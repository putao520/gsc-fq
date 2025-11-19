use crate::error::Result;
use crate::reverse_proxy::protocol::{ControlMessage, HandshakeStatus, ReverseProxyConfig};
use futures::StreamExt;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::Mutex;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
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
    ) -> Result<Self> {
        println!("🔄 创建Yamux连接池: {} 个连接...", pool_size);
        
        let mut connections = Vec::with_capacity(pool_size);
        
        // 并行创建所有连接
        let mut tasks = Vec::new();
        for id in 0..pool_size {
            let addr = server_addr;
            let cfg = config.to_vec();
            
            tasks.push(tokio::spawn(async move {
                Self::create_yamux_connection(addr, &cfg, id).await
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
    ) -> Result<YamuxConnection> {
        // 1. 创建优化的TCP连接
        let mut stream = Self::create_optimized_tcp(server_addr).await?;
        
        // 2. 执行握手
        Self::do_handshake(&mut stream, config).await?;
        
        // 3. 升级到Yamux
        let compat_stream = stream.compat();
        let yamux_config = Self::create_optimized_yamux_config();
        let conn = Connection::new(compat_stream, yamux_config, Mode::Client);
        let control = conn.control();
        
        // 4. 启动Yamux驱动任务
        let state = Arc::new(AtomicBool::new(true));
        let state_clone = state.clone();
        
        tokio::spawn(async move {
            // Yamux驱动循环
            let stream = yamux::into_stream(conn);
            tokio::pin!(stream);
            
            while let Some(_) = stream.next().await {
                // 处理incoming流（如果需要）
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
    ) -> Result<()> {
        // 发送ClientHello
        let hello = ControlMessage::ClientHello {
            version: 1,
            proxies: config.to_vec(),
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_strategy_default() {
        let strategy = ConnectionSelectionStrategy::default();
        assert!(matches!(strategy, ConnectionSelectionStrategy::RoundRobin));
    }
}
