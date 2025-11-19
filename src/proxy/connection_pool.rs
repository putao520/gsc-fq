use crate::error::{ProxyError, Result};
use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::{Mutex, Semaphore};
use tokio::time;

// 优化后的连接池策略（性能优先，仍保持安全性）
const INITIAL_POOL_SIZE: usize = 50; // 初始池大小：50个连接（提升3.3倍）
const MAX_POOL_SIZE: usize = 200; // 最大池大小：200个连接（提升6.7倍，支持高并发）
const PREHEAT_DELAY_MS: u64 = 100; // 预热时每个连接间隔 100ms（加速预热，5秒完成）
const REFILL_DELAY_MS: u64 = 1000; // 补充连接延迟 1s（降低延迟，更快响应）
const MAINTENANCE_INTERVAL_SECS: u64 = 30; // 维护周期：30秒（低频检查）
const IDLE_SHRINK_THRESHOLD: usize = 5; // 空闲收缩阈值：连续5次检查池满则收缩
const EXPANSION_MISS_THRESHOLD: f64 = 0.3; // 扩张阈值：miss率超过30%则考虑扩张
const BLACKHOLE_FAILURE_THRESHOLD: u32 = 3; // 黑洞服务器检测阈值：连续3次连接失败
const BLACKHOLE_DETECTION_TIMEOUT_SECS: u64 = 30; // 黑洞检测超时：30秒

/// 连接池统计信息
#[derive(Debug, Default)]
pub struct PoolStats {
    /// 总共创建的连接数
    pub total_created: AtomicU64,
    /// 从池中获取的连接数
    pub pool_hits: AtomicU64,
    /// 池空时现场创建的连接数
    pub pool_misses: AtomicU64,
    /// 连接失败次数
    pub connection_failures: AtomicU64,
    /// 连续连接失败次数（用于黑洞检测）
    pub consecutive_failures: AtomicU64,
    /// 服务器是否被标记为黑洞
    pub is_blackhole: AtomicBool,
}

/// 预热连接池
///
/// 在代理启动时预先建立到后端服务器的连接，减少客户端首次连接延迟。
/// 当客户端到来时，直接从池中获取已建立的连接，省去 TCP 握手时间。
pub struct ConnectionPool {
    /// 后端服务器地址
    remote_addr: SocketAddr,
    /// 可选的源 IP 地址
    source_ip: Option<IpAddr>,
    /// 连接池当前目标大小（可自适应扩张）
    pool_size: Arc<AtomicUsize>,
    /// 连接队列（FIFO）
    pool: Arc<Mutex<VecDeque<TcpStream>>>,
    /// 统计信息
    stats: Arc<PoolStats>,
    /// 关闭信号
    shutdown: Arc<AtomicBool>,
    /// 并发控制（防止过度创建连接）
    semaphore: Arc<Semaphore>,
}

impl ConnectionPool {
    /// 创建新的连接池
    ///
    /// # 参数
    /// - `remote_addr`: 后端服务器地址
    /// - `source_ip`: 可选的源 IP 地址
    ///
    /// # 安全策略
    /// 使用自适应策略，平衡安全与性能：
    /// - 初始：15个连接（缓慢建立）
    /// - 最大：30个连接（运行时可自适应扩张）
    /// - 预热间隔：500ms（避免被识别为端口扫描）
    /// - 扩张延迟：2秒（避免连接风暴）
    /// - 自适应扩张：根据 miss 率自动调整池大小
    pub fn new(remote_addr: SocketAddr, source_ip: Option<IpAddr>) -> Self {
        Self {
            remote_addr,
            source_ip,
            pool_size: Arc::new(AtomicUsize::new(INITIAL_POOL_SIZE)),
            pool: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_POOL_SIZE))),
            stats: Arc::new(PoolStats::default()),
            shutdown: Arc::new(AtomicBool::new(false)),
            semaphore: Arc::new(Semaphore::new(MAX_POOL_SIZE * 2)),
        }
    }

    /// 启动连接池
    ///
    /// 开始预热连接并启动后台维护任务
    pub async fn start(&self) -> Result<()> {
        // 预热连接池
        self.preheat_pool().await?;

        // 启动后台维护任务
        self.spawn_maintenance_task();

        Ok(())
    }

    /// 预热连接池（串行+延迟+黑洞检测，避免触发防火墙）
    ///
    /// 安全策略：
    /// 1. 串行创建连接（避免并发连接风暴）
    /// 2. 每个连接之间延迟 500ms（缓慢建立，模拟正常流量模式）
    /// 3. 失败时不重试（避免反复触发防火墙）
    /// 4. 黑洞检测：连续3次失败则标记为黑洞服务器，停止预热
    async fn preheat_pool(&self) -> Result<()> {
        let initial_size = INITIAL_POOL_SIZE;
        let mut consecutive_failures = 0u32;

        for i in 0..initial_size {
            // 添加延迟（第一个连接除外）
            if i > 0 {
                time::sleep(Duration::from_millis(PREHEAT_DELAY_MS)).await;
            }

            match Self::create_connection(self.remote_addr, self.source_ip).await {
                Ok(stream) => {
                    self.pool.lock().await.push_back(stream);
                    self.stats.total_created.fetch_add(1, Ordering::Relaxed);
                    // 成功连接，重置连续失败计数
                    consecutive_failures = 0;
                    self.stats.consecutive_failures.store(0, Ordering::Relaxed);

                    // 如果已经有成功连接，且成功率达到一定比例，可以提前结束预热
                    let created_count = self.stats.total_created.load(Ordering::Relaxed);
                    if created_count >= 3 { // 至少有3个成功连接就足够测试了
                        eprintln!("✅ Preheat completed: {} connections created", created_count);
                        break;
                    }
                }
                Err(e) => {
                    consecutive_failures += 1;
                    self.stats.connection_failures.fetch_add(1, Ordering::Relaxed);
                    self.stats.consecutive_failures.store(consecutive_failures as u64, Ordering::Relaxed);

                    eprintln!("⚠️  Failed to preheat connection {}/{}: {}", i + 1, initial_size, e);

                    // 黑洞检测：连续3次失败则标记为黑洞服务器
                    if consecutive_failures >= BLACKHOLE_FAILURE_THRESHOLD {
                        self.stats.is_blackhole.store(true, Ordering::Relaxed);
                        eprintln!("🕳️  Blackhole server detected: {} consecutive failures", consecutive_failures);
                        eprintln!("⏹️  Stopping preheat for blackhole server");
                        break;
                    }

                    // 失败但不达到黑洞阈值时，继续尝试剩余连接
                }
            }
        }

        Ok(())
    }

    /// 从池中获取连接
    ///
    /// 如果池中有可用连接，立即返回；否则现场创建新连接。
    /// 如果服务器被标记为黑洞，则直接返回黑洞错误。
    pub async fn acquire(&self) -> Result<TcpStream> {
        // 检查是否为黑洞服务器
        if self.stats.is_blackhole.load(Ordering::Relaxed) {
            return Err(crate::error::ProxyError::ConnectionPoolError(
                "Target server is marked as blackhole (unreachable)".to_string()
            ).into());
        }

        // 尝试从池中获取
        if let Some(stream) = self.pool.lock().await.pop_front() {
            self.stats.pool_hits.fetch_add(1, Ordering::Relaxed);

            // 验证连接是否仍然有效
            if Self::is_connection_alive(&stream).await {
                // 成功获取连接，重置连续失败计数
                self.stats.consecutive_failures.store(0, Ordering::Relaxed);
                // 触发异步补充（不等待）
                self.spawn_refill_task();
                return Ok(stream);
            } else {
                // 连接已断开，丢弃并继续
                drop(stream);
            }
        }

        // 池空或连接失效，现场创建
        self.stats.pool_misses.fetch_add(1, Ordering::Relaxed);

        match Self::create_connection(self.remote_addr, self.source_ip).await {
            Ok(stream) => {
                self.stats.total_created.fetch_add(1, Ordering::Relaxed);
                // 成功创建连接，重置连续失败计数
                self.stats.consecutive_failures.store(0, Ordering::Relaxed);
                // 触发异步补充
                self.spawn_refill_task();
                Ok(stream)
            }
            Err(e) => {
                self.stats.connection_failures.fetch_add(1, Ordering::Relaxed);

                // 更新连续失败次数
                let consecutive_failures = self.stats.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;

                // 检查是否达到黑洞阈值
                if consecutive_failures >= BLACKHOLE_FAILURE_THRESHOLD as u64 {
                    self.stats.is_blackhole.store(true, Ordering::Relaxed);
                    eprintln!("🕳️  Server marked as blackhole: {} consecutive failures", consecutive_failures);
                }

                Err(e)
            }
        }
    }

    /// 创建到后端的 TCP 连接
    async fn create_connection(
        remote_addr: SocketAddr,
        source_ip: Option<IpAddr>,
    ) -> Result<TcpStream> {
        let stream = if let Some(source_ip) = source_ip {
            // 使用指定的源 IP
            let socket = if source_ip.is_ipv4() {
                TcpSocket::new_v4()?
            } else {
                TcpSocket::new_v6()?
            };

            let local_addr = SocketAddr::new(source_ip, 0);
            socket.bind(local_addr)?;

            // 连接超时：5秒
            tokio::time::timeout(Duration::from_secs(5), socket.connect(remote_addr))
                .await
                .map_err(|_| ProxyError::ConnectionPoolError("Connection timeout".to_string()))??
        } else {
            // 使用默认源 IP
            tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(remote_addr))
                .await
                .map_err(|_| ProxyError::ConnectionPoolError("Connection timeout".to_string()))??
        };

        // 应用 TCP 优化
        stream.set_nodelay(true)?;

        Ok(stream)
    }

    /// 检查连接是否存活
    async fn is_connection_alive(stream: &TcpStream) -> bool {
        // 尝试 peek 操作（不消耗数据）
        // 如果连接已断开，peek 会立即返回错误
        let mut buf = [0u8; 1];
        match stream.try_read(&mut buf) {
            Ok(_) => true,  // 连接活跃
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => true, // 无数据但连接正常
            Err(_) => false, // 连接已断开
        }
    }

    /// 启动后台维护任务（低频检查+自适应收缩）
    fn spawn_maintenance_task(&self) {
        let pool = Arc::clone(&self.pool);
        let remote_addr = self.remote_addr;
        let source_ip = self.source_ip;
        let pool_size = Arc::clone(&self.pool_size);
        let stats = Arc::clone(&self.stats);
        let shutdown = Arc::clone(&self.shutdown);
        let semaphore = Arc::clone(&self.semaphore);

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(MAINTENANCE_INTERVAL_SECS));
            let mut idle_full_count = 0; // 连续池满计数

            loop {
                interval.tick().await;

                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                // 清理失效连接
                let mut pool_guard = pool.lock().await;
                let mut valid_connections = VecDeque::new();

                while let Some(stream) = pool_guard.pop_front() {
                    if Self::is_connection_alive(&stream).await {
                        valid_connections.push_back(stream);
                    }
                    // 失效的连接自动丢弃
                }

                let current_size = valid_connections.len();
                *pool_guard = valid_connections;
                drop(pool_guard); // 释放锁

                let target_size = pool_size.load(Ordering::Relaxed);

                // 如果低于目标大小，只补充一个连接（避免批量创建）
                if current_size < target_size {
                    if let Ok(_permit) = semaphore.try_acquire() {
                        match Self::create_connection(remote_addr, source_ip).await {
                            Ok(stream) => {
                                pool.lock().await.push_back(stream);
                                stats.total_created.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                stats.connection_failures.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    idle_full_count = 0; // 重置计数
                } else if current_size >= target_size {
                    // 池满时，记录空闲状态
                    idle_full_count += 1;

                    // 连续多次池满且无使用，考虑收缩
                    if idle_full_count >= IDLE_SHRINK_THRESHOLD && target_size > INITIAL_POOL_SIZE {
                        pool_size.store(target_size - 1, Ordering::Relaxed);
                        idle_full_count = 0;
                    }
                }
            }
        });
    }

    /// 触发异步补充任务（延迟+节流+自适应扩张）
    ///
    /// 自适应扩张策略：
    /// 1. 延迟2秒后补充（避免频繁连接）
    /// 2. 只补充一个连接（避免突发）
    /// 3. 根据 miss 率智能扩张：
    ///    - miss率 > 30%: 考虑扩张池大小
    ///    - 池已满: 增加目标大小
    ///    - 最大限制: MAX_POOL_SIZE (30)
    fn spawn_refill_task(&self) {
        let pool = Arc::clone(&self.pool);
        let remote_addr = self.remote_addr;
        let source_ip = self.source_ip;
        let pool_size = Arc::clone(&self.pool_size);
        let stats = Arc::clone(&self.stats);
        let shutdown = Arc::clone(&self.shutdown);
        let semaphore = Arc::clone(&self.semaphore);

        tokio::spawn(async move {
            if shutdown.load(Ordering::Relaxed) {
                return;
            }

            // 延迟补充（避免频繁连接）
            time::sleep(Duration::from_millis(REFILL_DELAY_MS)).await;

            if shutdown.load(Ordering::Relaxed) {
                return;
            }

            let current_size = pool.lock().await.len();
            let target_size = pool_size.load(Ordering::Relaxed);

            // 计算 miss 率，决定是否需要扩张
            let hits = stats.pool_hits.load(Ordering::Relaxed);
            let misses = stats.pool_misses.load(Ordering::Relaxed);
            let total = hits + misses;
            let miss_rate = if total > 0 {
                misses as f64 / total as f64
            } else {
                0.0
            };

            // 只补充一个连接
            if current_size < target_size {
                if let Ok(_permit) = semaphore.try_acquire() {
                    match Self::create_connection(remote_addr, source_ip).await {
                        Ok(stream) => {
                            pool.lock().await.push_back(stream);
                            stats.total_created.fetch_add(1, Ordering::Relaxed);

                            // 自适应扩张：根据 miss 率和池状态决定是否扩张
                            if current_size + 1 >= target_size
                                && target_size < MAX_POOL_SIZE
                                && miss_rate > EXPANSION_MISS_THRESHOLD
                            {
                                let new_size = target_size + 1;
                                pool_size.store(new_size, Ordering::Relaxed);
                                eprintln!(
                                    "📈 Pool expanding: {} -> {} (miss rate: {:.1}%)",
                                    target_size,
                                    new_size,
                                    miss_rate * 100.0
                                );
                            }
                        }
                        Err(_) => {
                            stats.connection_failures.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        });
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> PoolStatsSnapshot {
        PoolStatsSnapshot {
            total_created: self.stats.total_created.load(Ordering::Relaxed),
            pool_hits: self.stats.pool_hits.load(Ordering::Relaxed),
            pool_misses: self.stats.pool_misses.load(Ordering::Relaxed),
            connection_failures: self.stats.connection_failures.load(Ordering::Relaxed),
            consecutive_failures: self.stats.consecutive_failures.load(Ordering::Relaxed),
            is_blackhole: self.stats.is_blackhole.load(Ordering::Relaxed),
        }
    }

    /// 关闭连接池
    pub async fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);

        // 清空连接池
        let mut pool = self.pool.lock().await;
        pool.clear();
    }
}

/// 连接池统计快照
#[derive(Debug, Clone)]
pub struct PoolStatsSnapshot {
    pub total_created: u64,
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub connection_failures: u64,
    pub consecutive_failures: u64,
    pub is_blackhole: bool,
}

impl PoolStatsSnapshot {
    /// 计算缓存命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.pool_hits + self.pool_misses;
        if total == 0 {
            0.0
        } else {
            (self.pool_hits as f64 / total as f64) * 100.0
        }
    }
}
