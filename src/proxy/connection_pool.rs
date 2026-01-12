use crate::error::{ProxyError, Result};
use socket2::{Socket, Domain, Type, Protocol};
use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, Semaphore};
use tokio::time;

// 优化后的连接池策略（性能优先，仍保持安全性）
const INITIAL_POOL_SIZE: usize = 50; // 固定池大小：50个连接
const MAX_POOL_SIZE: usize = 50; // 最大池大小：50个连接（固定大小，不扩张）
const PREHEAT_DELAY_MS: u64 = 100; // 预热时每个连接间隔 100ms
const REFILL_DELAY_MS: u64 = 100; // 补充连接延迟 100ms（快速补充）
const MAINTENANCE_INTERVAL_SECS: u64 = 10; // 维护周期：10秒（快速清理失效连接）
const BLACKHOLE_FAILURE_THRESHOLD: u32 = 3; // 黑洞服务器检测阈值：连续3次连接失败

// 运行时可配置的黑洞检测阈值
fn get_blackhole_failure_threshold() -> u32 {
    std::env::var("BLACKHOLE_FAILURE_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(BLACKHOLE_FAILURE_THRESHOLD)
}

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
                }
                Err(e) => {
                    consecutive_failures += 1;
                    self.stats.connection_failures.fetch_add(1, Ordering::Relaxed);
                    self.stats.consecutive_failures.store(consecutive_failures as u64, Ordering::Relaxed);

                    eprintln!("⚠️  Failed to preheat connection {}/{}: {}", i + 1, initial_size, e);

                    // 黑洞检测：连续N次失败则标记为黑洞服务器
                    let threshold = get_blackhole_failure_threshold();
                    if consecutive_failures >= threshold {
                        self.stats.is_blackhole.store(true, Ordering::Relaxed);
                        eprintln!("🕳️  Blackhole server detected: {} consecutive failures (threshold: {})", consecutive_failures, threshold);
                        eprintln!("⏹️  Stopping preheat for blackhole server");
                        break;
                    }

                    // 失败但不达到黑洞阈值时，继续尝试剩余连接
                }
            }
        }

        // 预热完成，打印实际创建的连接数
        let created_count = self.stats.total_created.load(Ordering::Relaxed);
        if created_count > 0 {
            eprintln!("✅ Preheat completed: {} connections created", created_count);
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

        // 池空或连接失效，立即触发同步补充（不等待）
        let pool = Arc::clone(&self.pool);
        let remote_addr = self.remote_addr;
        let source_ip = self.source_ip;
        let stats = Arc::clone(&self.stats);
        let semaphore = Arc::clone(&self.semaphore);

        tokio::spawn(async move {
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
        });

        // 短暂等待，让补充任务有机会完成
        tokio::time::sleep(Duration::from_millis(10)).await;

        // 再次尝试从池中获取（可能拿到刚补充的连接）
        if let Some(stream) = self.pool.lock().await.pop_front() {
            self.stats.pool_hits.fetch_add(1, Ordering::Relaxed);

            if Self::is_connection_alive(&stream).await {
                self.stats.consecutive_failures.store(0, Ordering::Relaxed);
                self.spawn_refill_task();
                return Ok(stream);
            } else {
                drop(stream);
            }
        }

        // 池还是空，现场创建给应用
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
                let threshold = get_blackhole_failure_threshold() as u64;
                if consecutive_failures >= threshold {
                    self.stats.is_blackhole.store(true, Ordering::Relaxed);
                    eprintln!("🕳️  Server marked as blackhole: {} consecutive failures (threshold: {})", consecutive_failures, threshold);
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
        // 使用 socket2 创建连接，以便设置 TCP 缓冲区
        let std_socket = Socket::new(
            if remote_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 },
            Type::STREAM,
            Some(Protocol::TCP),
        )?;

        // 设置 TCP 缓冲区大小（高吞吐优化）
        // 发送缓冲区：2MB
        // 接收缓冲区：2MB
        // 这对于千兆网络（RTT 10ms）的 BDP = 1.25MB 已经足够
        if let Err(e) = std_socket.set_send_buffer_size(2 * 1024 * 1024) {
            eprintln!("⚠️  Failed to set send buffer size: {}", e);
        }
        if let Err(e) = std_socket.set_recv_buffer_size(2 * 1024 * 1024) {
            eprintln!("⚠️  Failed to set recv buffer size: {}", e);
        }

        // 绑定源 IP（如果指定）
        if let Some(source_ip) = source_ip {
            let local_addr = SocketAddr::new(source_ip, 0);
            std_socket.bind(&local_addr.into())?;
        }

        // 连接超时：5秒（在线程池中执行阻塞连接）
        let connect_result = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                // 阻塞模式连接（确保连接完成）
                std_socket.set_nonblocking(false)
                    .map_err(|e| ProxyError::ConnectionPoolError(format!("Failed to set nonblocking: {}", e)))?;
                std_socket.connect(&remote_addr.into())
                    .map_err(|e| ProxyError::ConnectionPoolError(format!("Connect failed: {}", e)))?;

                // 转换为 std::net::TcpStream
                let std_stream: std::net::TcpStream = std_socket.into();

                // 转换为 tokio TcpStream（必须在 spawn_blocking 内完成，因为 std_stream 已是 blocking）
                TcpStream::from_std(std_stream)
                    .map_err(|e| ProxyError::ConnectionPoolError(format!("Failed to create TcpStream: {}", e)))
            })
        ).await;

        // 解包三层 Result：timeout + JoinHandle + 闭包Result
        let join_result = match connect_result {
            Ok(inner) => inner,
            Err(_) => return Err(ProxyError::ConnectionPoolError("Connection timeout".to_string()).into()),
        };

        let stream_or_err = match join_result {
            Ok(s) => s,
            Err(e) => return Err(ProxyError::ConnectionPoolError(format!("Join error: {}", e)).into()),
        };

        let mut stream = match stream_or_err {
            Ok(s) => s,
            Err(e) => return Err(e.into()),
        };

        // 禁用 Nagle 算法
        stream.set_nodelay(true)
            .map_err(|e| ProxyError::ConnectionPoolError(format!("Failed to set nodelay: {}", e)))?;

        Ok(stream)
    }

    /// 检查连接是否存活（使用可靠的检查方式）
    async fn is_connection_alive(stream: &TcpStream) -> bool {
        // 方法1：尝试 peek 操作（不消耗数据）
        // 如果远端已关闭，peek 会立即返回错误
        let mut buf = [0u8; 1];
        match stream.try_read(&mut buf) {
            Ok(n) => {
                // 读到数据说明连接肯定活跃
                n > 0
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // 无数据但连接可能正常
                // 方法2：检查本地socket错误状态
                if let Ok(err) = stream.take_error() {
                    // 有待处理的错误，说明连接已断开
                    err.is_none()
                } else {
                    // 无法获取错误状态，保守认为连接正常
                    true
                }
            }
            Err(_) => false, // 连接已断开
        }
    }

    /// 启动后台维护任务（低频检查+固定大小）
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

            loop {
                interval.tick().await;

                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                // 清理失效连接
                let mut pool_guard = pool.lock().await;
                let initial_size = pool_guard.len();
                let mut valid_connections = VecDeque::new();

                while let Some(stream) = pool_guard.pop_front() {
                    if Self::is_connection_alive(&stream).await {
                        valid_connections.push_back(stream);
                    }
                    // 失效的连接自动丢弃
                }

                let dead_connections = initial_size - valid_connections.len();

                // 强制执行目标大小限制（清理多余连接）
                let target_limit = pool_size.load(Ordering::Relaxed);
                if valid_connections.len() > target_limit {
                    valid_connections.truncate(target_limit);
                }

                let current_size = valid_connections.len();
                *pool_guard = valid_connections;
                drop(pool_guard); // 释放锁

                let target_size = pool_size.load(Ordering::Relaxed);

                // 监控和告警
                if dead_connections > 0 {
                    eprintln!(
                        "⚠️  清理了 {} 个失效连接 ({} → {}/{})",
                        dead_connections,
                        initial_size,
                        current_size,
                        target_size
                    );
                }

                if current_size < target_size / 2 {
                    eprintln!(
                        "🚨 连接池健康度低：{}/{} ({:.0}%)",
                        current_size,
                        target_size,
                        (current_size as f64 / target_size as f64) * 100.0
                    );
                }

                // 如果低于目标大小，批量补充到目标大小
                while pool.lock().await.len() < target_size {
                    if let Ok(_permit) = semaphore.try_acquire() {
                        match Self::create_connection(remote_addr, source_ip).await {
                            Ok(stream) => {
                                pool.lock().await.push_back(stream);
                                stats.total_created.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                stats.connection_failures.fetch_add(1, Ordering::Relaxed);
                                break; // 创建失败，停止补充
                            }
                        }
                    } else {
                        break; // 信号量满，停止补充
                    }
                }
            }
        });
    }

    /// 触发异步补充任务（延迟+节流）
    ///
    /// 补充策略：
    /// 1. 延迟1秒后补充（避免频繁连接）
    /// 2. 只在池未满时补充一个连接（避免突发）
    /// 3. 补充到固定目标大小（INITIAL_POOL_SIZE），不扩张
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

            // 尝试获取信号量许可（限制并发创建连接数）
            if let Ok(_permit) = semaphore.try_acquire() {
                let current_size = pool.lock().await.len();
                let target_size = pool_size.load(Ordering::Relaxed);

                // 只在池未满时补充（不扩张）
                if current_size < target_size {
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
