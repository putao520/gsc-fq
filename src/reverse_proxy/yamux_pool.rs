use crate::error::Result;
use crate::reverse_proxy::protocol::{ControlMessage, HandshakeStatus, ReverseProxyConfig};
use crate::{debug_println, error_println};
use futures::StreamExt;
use sha2::Digest;
use std::marker::Unpin;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::Mutex;
use tokio_util::compat::TokioAsyncReadCompatExt;
use yamux::{Config, Connection, Mode};

/// Yamux连接池大小配置
pub const DEFAULT_POOL_SIZE: usize = 32; // 默认32个连接，支持500MB/s+

/// 连接选择策略
#[derive(Debug, Clone, Copy)]
pub enum ConnectionSelectionStrategy {
    /// 轮询（默认）
    RoundRobin,
    /// 随机
    Random,
    /// 最少负载（预留）
    LeastLoaded,
}

impl Default for ConnectionSelectionStrategy {
    fn default() -> Self {
        ConnectionSelectionStrategy::RoundRobin
    }
}

/// 单个Yamux连接
pub struct YamuxConnection {
    /// Yamux控制器
    control: yamux::Control,

    /// 连接ID
    pub id: usize,

    /// 连接状态（true = 活跃）
    state: Arc<AtomicBool>,

    /// 流计数
    stream_count: Arc<AtomicUsize>,
}

impl YamuxConnection {
    /// 打开新流
    pub async fn open_stream(&mut self) -> std::result::Result<yamux::Stream, yamux::ConnectionError> {
        let stream = self.control.open_stream().await?;
        self.stream_count.fetch_add(1, Ordering::Relaxed);
        Ok(stream)
    }

    /// 获取流计数
    pub fn stream_count(&self) -> usize {
        self.stream_count.load(Ordering::Relaxed)
    }

    /// 检查是否活跃
    pub fn is_active(&self) -> bool {
        self.state.load(Ordering::Relaxed)
    }
}

/// Yamux连接池
pub struct YamuxConnectionPool {
    /// 连接池
    connections: Vec<Arc<Mutex<YamuxConnection>>>,
    
    /// 连接池大小
    pool_size: usize,
    
    /// 负载均衡策略
    selection_strategy: ConnectionSelectionStrategy,
    
    /// 轮询索引
    round_robin_index: AtomicUsize,
}

impl YamuxConnectionPool {
    /// 创建连接池
    pub async fn new(
        server_addr: SocketAddr,
        config: &[ReverseProxyConfig],
        pool_size: usize,
        strategy: ConnectionSelectionStrategy,
        auth_token: &Option<String>,
    ) -> Result<Self> {
        println!("🔄 创建Yamux连接池: {} 个连接...", pool_size);
        
        let mut connections = Vec::with_capacity(pool_size);
        
        // 并行创建所有连接
        let mut tasks = Vec::new();
        for id in 0..pool_size {
            let addr = server_addr;
            let cfg = config.to_vec();
            let token = auth_token.clone();

            tasks.push(tokio::spawn(async move {
                Self::create_yamux_connection(addr, &cfg, id, &token).await
            }));
        }
        
        // 等待所有连接建立
        let mut success_count = 0;
        for (idx, task) in tasks.into_iter().enumerate() {
            match task.await {
                Ok(Ok(conn)) => {
                    connections.push(Arc::new(Mutex::new(conn)));
                    success_count += 1;
                    if (idx + 1) % 8 == 0 {
                        println!("  ✅ 已建立 {}/{} 个连接", idx + 1, pool_size);
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("  ⚠️  连接 {} 失败: {}", idx, e);
                }
                Err(e) => {
                    eprintln!("  ⚠️  任务 {} 失败: {}", idx, e);
                }
            }
        }
        
        if success_count == 0 {
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                "无法建立任何Yamux连接".to_string()
            ).into());
        }
        
        println!("✅ Yamux连接池已建立: {}/{} 个连接", success_count, pool_size);
        
        Ok(Self {
            connections,
            pool_size: success_count,
            selection_strategy: strategy,
            round_robin_index: AtomicUsize::new(0),
        })
    }
    
    /// 创建单个Yamux连接
    async fn create_yamux_connection(
        server_addr: SocketAddr,
        config: &[ReverseProxyConfig],
        id: usize,
        auth_token: &Option<String>,
    ) -> Result<YamuxConnection> {
        // 1. 创建优化的TCP连接
        let mut stream = Self::create_optimized_tcp(server_addr).await?;
        
        // 2. 执行握手
        Self::do_handshake(&mut stream, config, auth_token).await?;
        
        // 3. 升级到Yamux
        let compat_stream = stream.compat();
        let yamux_config = Self::create_optimized_yamux_config();
        let conn = Connection::new(compat_stream, yamux_config, Mode::Client);
        let control = conn.control();
        
        // 4. 启动Yamux驱动任务
        let state = Arc::new(AtomicBool::new(true));
        let state_clone = state.clone();
        
        // Store proxy configs for the connection handler
        let proxy_configs_for_handler = config.to_vec();

        tokio::spawn(async move {
            // Yamux驱动循环 - 处理incoming流
            let stream = yamux::into_stream(conn);
            tokio::pin!(stream);

            while let Some(stream_result) = stream.next().await {
                match stream_result {
                    Ok(incoming_yamux_stream) => {
                        debug_println!("📥 Client received incoming yamux stream from server");

                        // Handle the incoming stream in a separate task
                        let proxy_configs_clone = proxy_configs_for_handler.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_incoming_stream_from_pool(incoming_yamux_stream, proxy_configs_clone).await {
                                error_println!("Failed to handle incoming stream: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        debug_println!("Yamux stream error: {}", e);
                        break;
                    }
                }
            }

            state_clone.store(false, Ordering::Relaxed);
        });
        
        Ok(YamuxConnection {
            control,
            id,
            state,
            stream_count: Arc::new(AtomicUsize::new(0)),
        })
    }
    
    /// 创建优化的TCP连接
    async fn create_optimized_tcp(addr: SocketAddr) -> Result<TcpStream> {
        let socket = TcpSocket::new_v4()?;
        
        // TCP优化 - 关键性能提升
        socket.set_nodelay(true)?;  // 禁用Nagle算法
        socket.set_recv_buffer_size(4 * 1024 * 1024)?;  // 4MB接收缓冲
        socket.set_send_buffer_size(4 * 1024 * 1024)?;  // 4MB发送缓冲
        
        // 连接超时
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            socket.connect(addr)
        )
        .await
        .map_err(|_| crate::error::ReverseProxyError::ConnectionFailed(
            "连接超时".to_string()
        ))??;
        
        Ok(stream)
    }
    
    /// 创建优化的Yamux配置
    fn create_optimized_yamux_config() -> Config {
        let mut config = Config::default();
        
        // 关键性能优化
        config.set_receive_window(4 * 1024 * 1024);  // 4MB窗口 -> 20MB/s per connection
        config.set_max_num_streams(1024);            // 每个连接最多1024个流
        
        config
    }
    
    /// 执行握手
    async fn do_handshake(
        stream: &mut TcpStream,
        config: &[ReverseProxyConfig],
        auth_token: &Option<String>,
    ) -> Result<()> {
        // 计算配置哈希
        let config_json = serde_json::to_string(config)
            .map_err(|e| crate::error::ReverseProxyError::SerializationFailed(e.to_string()))?;
        let config_hash = format!("{:x}", sha2::Sha256::digest(config_json.as_bytes()));

        // 获取认证令牌
        let token = auth_token.clone().unwrap_or_default();

        // 发送ClientHello
        let hello = ControlMessage::ClientHello {
            version: 1,
            token,
            proxies: config.to_vec(),
            config_hash,
        };
        hello.write_to(stream).await?;

        // 接收ServerHello
        let response = ControlMessage::read_from(stream).await?;

        match response {
            ControlMessage::ServerHello { status, message, .. } => {
                match status {
                    HandshakeStatus::Ok => Ok(()),
                    _ => {
                        Err(crate::error::ReverseProxyError::HandshakeFailed(message).into())
                    }
                }
            }
            _ => Err(crate::error::ReverseProxyError::HandshakeFailed(
                "无效的服务器响应".to_string()
            ).into()),
        }
    }
    
    /// 获取一个连接
    pub async fn acquire(&self) -> Result<Arc<Mutex<YamuxConnection>>> {
        if self.connections.is_empty() {
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                "连接池为空".to_string()
            ).into());
        }
        
        let conn = match self.selection_strategy {
            ConnectionSelectionStrategy::RoundRobin => {
                let idx = self.round_robin_index.fetch_add(1, Ordering::Relaxed);
                self.connections[idx % self.pool_size].clone()
            }
            ConnectionSelectionStrategy::Random => {
                let idx = rand::random::<usize>() % self.pool_size;
                self.connections[idx].clone()
            }
            ConnectionSelectionStrategy::LeastLoaded => {
                // 选择流数量最少的连接
                self.select_least_loaded().await
            }
        };
        
        Ok(conn)
    }
    
    /// 选择负载最少的连接
    async fn select_least_loaded(&self) -> Arc<Mutex<YamuxConnection>> {
        let mut min_load = usize::MAX;
        let mut best_idx = 0;
        
        for (idx, conn) in self.connections.iter().enumerate() {
            let load = conn.lock().await.stream_count();
            if load < min_load {
                min_load = load;
                best_idx = idx;
            }
        }
        
        self.connections[best_idx].clone()
    }
    
    /// 打开新流（便捷方法）
    pub async fn open_stream(&self) -> Result<yamux::Stream> {
        let conn = self.acquire().await?;
        let mut conn_guard = conn.lock().await;
        
        Ok(conn_guard.open_stream().await.map_err(|e| {
            crate::error::ReverseProxyError::ConnectionFailed(
                format!("打开Yamux流失败: {}", e)
            )
        })?)
    }
    
    /// 获取池统计信息
    pub async fn get_stats(&self) -> PoolStats {
        let mut total_streams = 0;
        let mut active_connections = 0;
        
        for conn in &self.connections {
            let conn_guard = conn.lock().await;
            total_streams += conn_guard.stream_count();
            if conn_guard.is_active() {
                active_connections += 1;
            }
        }
        
        PoolStats {
            pool_size: self.pool_size,
            active_connections,
            total_streams,
        }
    }
}

/// 连接池统计信息
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub pool_size: usize,
    pub active_connections: usize,
    pub total_streams: usize,
}

impl YamuxConnectionPool {
    /// 处理来自连接池的incoming流
    async fn handle_incoming_stream_from_pool(
        yamux_stream: yamux::Stream,
        proxy_configs: Vec<ReverseProxyConfig>,
    ) -> Result<()> {
        use tokio_util::compat::FuturesAsyncReadCompatExt;

        let mut yamux_tokio = yamux_stream.compat();

        // Read port header (first 2 bytes)
        let mut port_bytes = [0u8; 2];
        debug_println!("Reading port header from incoming stream...");

        if let Err(e) = yamux_tokio.read_exact(&mut port_bytes).await {
            error_println!("Failed to read port header: {}", e);
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                format!("Failed to read port header: {}", e)
            ).into());
        }

        let server_port = u16::from_be_bytes(port_bytes);
        debug_println!("Received incoming stream for server port {}", server_port);

        // Find the corresponding local target
        let local_target = proxy_configs.iter()
            .find(|c| c.server_port == server_port)
            .cloned();

        let Some(target) = local_target else {
            error_println!("Unknown server port: {}", server_port);
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                format!("Unknown server port: {}", server_port)
            ).into());
        };

        // Handle the stream data forwarding
        Self::handle_stream_forwarding(yamux_tokio, target).await
    }

    /// 处理流数据转发
    async fn handle_stream_forwarding(
        mut yamux_stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        target: ReverseProxyConfig,
    ) -> Result<()> {
        // Connect to local service
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let mut local_stream = TcpStream::connect(&local_addr).await.map_err(|e| {
            crate::error::ReverseProxyError::ConnectionFailed(format!(
                "Failed to connect to local service {}: {}",
                local_addr, e
            ))
        })?;

        debug_println!("Connected to local service: {}", local_addr);

        // Bidirectional copy
        match tokio::io::copy_bidirectional(&mut yamux_stream, &mut local_stream).await {
            Ok((from_yamux, to_yamux)) => {
                debug_println!(
                    "Stream closed. Transferred: {} bytes from server, {} bytes to server",
                    from_yamux, to_yamux
                );
            }
            Err(e) => {
                debug_println!("Copy error: {}", e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_strategy_default() {
        let strategy = ConnectionSelectionStrategy::default();
        assert!(matches!(strategy, ConnectionSelectionStrategy::RoundRobin));
    }
}
