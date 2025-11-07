use crate::error::{ProxyError, Result};
use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::{Mutex, Semaphore};
use tokio::time;

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
    /// 连接池目标大小
    pool_size: usize,
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
    /// - `pool_size`: 连接池大小（建议 3-10）
    pub fn new(remote_addr: SocketAddr, source_ip: Option<IpAddr>, pool_size: usize) -> Self {
        Self {
            remote_addr,
            source_ip,
            pool_size,
            pool: Arc::new(Mutex::new(VecDeque::with_capacity(pool_size))),
            stats: Arc::new(PoolStats::default()),
            shutdown: Arc::new(AtomicBool::new(false)),
            semaphore: Arc::new(Semaphore::new(pool_size * 2)), // 允许2倍池大小的并发创建
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

    /// 预热连接池（初始化时建立连接）
    async fn preheat_pool(&self) -> Result<()> {
        let mut tasks = Vec::new();

        for _ in 0..self.pool_size {
            let remote_addr = self.remote_addr;
            let source_ip = self.source_ip;
            let stats = Arc::clone(&self.stats);
            let pool = Arc::clone(&self.pool);

            let task = tokio::spawn(async move {
                match Self::create_connection(remote_addr, source_ip).await {
                    Ok(stream) => {
                        pool.lock().await.push_back(stream);
                        stats.total_created.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("Failed to preheat connection: {}", e);
                        stats.connection_failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });

            tasks.push(task);
        }

        // 等待所有预热任务完成
        for task in tasks {
            let _ = task.await;
        }

        Ok(())
    }

    /// 从池中获取连接
    ///
    /// 如果池中有可用连接，立即返回；否则现场创建新连接。
    pub async fn acquire(&self) -> Result<TcpStream> {
        // 尝试从池中获取
        if let Some(stream) = self.pool.lock().await.pop_front() {
            self.stats.pool_hits.fetch_add(1, Ordering::Relaxed);

            // 验证连接是否仍然有效
            if Self::is_connection_alive(&stream).await {
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
                // 触发异步补充
                self.spawn_refill_task();
                Ok(stream)
            }
            Err(e) => {
                self.stats.connection_failures.fetch_add(1, Ordering::Relaxed);
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

    /// 启动后台维护任务
    fn spawn_maintenance_task(&self) {
        let pool = Arc::clone(&self.pool);
        let remote_addr = self.remote_addr;
        let source_ip = self.source_ip;
        let pool_size = self.pool_size;
        let stats = Arc::clone(&self.stats);
        let shutdown = Arc::clone(&self.shutdown);
        let semaphore = Arc::clone(&self.semaphore);

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(5)); // 每5秒检查一次

            loop {
                interval.tick().await;

                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                // 检查当前池大小
                let current_size = pool.lock().await.len();

                // 如果低于目标大小，补充连接
                if current_size < pool_size {
                    let needed = pool_size - current_size;

                    for _ in 0..needed {
                        if shutdown.load(Ordering::Relaxed) {
                            break;
                        }

                        // 获取信号量（限制并发）
                        if let Ok(_permit) = semaphore.try_acquire() {
                            let pool_clone = Arc::clone(&pool);
                            let stats_clone = Arc::clone(&stats);

                            tokio::spawn(async move {
                                match Self::create_connection(remote_addr, source_ip).await {
                                    Ok(stream) => {
                                        pool_clone.lock().await.push_back(stream);
                                        stats_clone.total_created.fetch_add(1, Ordering::Relaxed);
                                    }
                                    Err(_) => {
                                        stats_clone
                                            .connection_failures
                                            .fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                            });
                        }
                    }
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

                *pool_guard = valid_connections;
            }
        });
    }

    /// 触发异步补充任务（不阻塞）
    fn spawn_refill_task(&self) {
        let pool = Arc::clone(&self.pool);
        let remote_addr = self.remote_addr;
        let source_ip = self.source_ip;
        let pool_size = self.pool_size;
        let stats = Arc::clone(&self.stats);
        let shutdown = Arc::clone(&self.shutdown);
        let semaphore = Arc::clone(&self.semaphore);

        tokio::spawn(async move {
            if shutdown.load(Ordering::Relaxed) {
                return;
            }

            let current_size = pool.lock().await.len();

            // 只补充一个连接
            if current_size < pool_size {
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
