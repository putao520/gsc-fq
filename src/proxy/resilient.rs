use crate::debug_println;
/// 跨洲际传输增强模块 - 提供健壮性和自动恢复能力
use crate::error::{NetworkError, ProxyError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

/// 连接质量监控器
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    /// RTT（往返时间）历史记录
    pub rtt_history: Arc<RwLock<Vec<Duration>>>,
    /// 丢包率
    pub packet_loss: Arc<RwLock<f64>>,
    /// 带宽测量
    pub bandwidth: Arc<RwLock<Option<u64>>>,
    /// 最后活跃时间
    pub last_active: Arc<RwLock<Instant>>,
    /// 连接错误计数
    pub error_count: Arc<RwLock<u32>>,
}

impl ConnectionQuality {
    pub fn new() -> Self {
        Self {
            rtt_history: Arc::new(RwLock::new(Vec::new())),
            packet_loss: Arc::new(RwLock::new(0.0)),
            bandwidth: Arc::new(RwLock::new(None)),
            last_active: Arc::new(RwLock::new(Instant::now())),
            error_count: Arc::new(RwLock::new(0)),
        }
    }

    /// 记录RTT
    pub async fn record_rtt(&self, rtt: Duration) {
        let mut history = self.rtt_history.write().await;
        history.push(rtt);
        // 只保留最近20次测量
        if history.len() > 20 {
            history.remove(0);
        }
        *self.last_active.write().await = Instant::now();
    }

    /// 获取平均RTT
    pub async fn get_average_rtt(&self) -> Option<Duration> {
        let history = self.rtt_history.read().await;
        if history.is_empty() {
            return None;
        }
        let sum: Duration = history.iter().sum();
        Some(sum / history.len() as u32)
    }

    /// 记录错误
    pub async fn record_error(&self) {
        let mut count = self.error_count.write().await;
        *count += 1;
        debug_println!("Connection error recorded, total errors: {}", *count);
    }

    /// 重置错误计数
    pub async fn reset_errors(&self) {
        *self.error_count.write().await = 0;
    }

    /// 检查连接健康状态
    pub async fn is_healthy(&self) -> bool {
        let errors = *self.error_count.read().await;
        let last_active = *self.last_active.read().await;

        // 如果错误超过5次或30秒无活动，认为不健康
        errors < 5 && last_active.elapsed() < Duration::from_secs(30)
    }
}

/// 会话信息
#[derive(Debug)]
pub struct SessionInfo {
    /// 会话ID
    pub id: Uuid,
    /// 客户端地址
    pub client_addr: String,
    /// 目标地址
    pub target_addr: String,
    /// 创建时间
    pub created_at: Instant,
    /// 最后更新时间
    pub last_updated: Arc<RwLock<Instant>>,
    /// 缓冲的数据
    pub pending_data: Arc<RwLock<Vec<Vec<u8>>>>,
}

impl SessionInfo {
    pub fn new(client_addr: String, target_addr: String) -> Self {
        let now = Instant::now();
        Self {
            id: Uuid::new_v4(),
            client_addr,
            target_addr,
            created_at: now,
            last_updated: Arc::new(RwLock::new(now)),
            pending_data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 添加待发送数据
    pub async fn add_pending_data(&self, data: Vec<u8>) {
        let mut pending = self.pending_data.write().await;
        pending.push(data);
        *self.last_updated.write().await = Instant::now();
    }

    /// 获取并清空待发送数据
    pub async fn take_pending_data(&self) -> Vec<Vec<u8>> {
        let mut pending = self.pending_data.write().await;
        let data = std::mem::take(&mut *pending);
        *self.last_updated.write().await = Instant::now();
        data
    }

    /// 检查会话是否过期
    pub async fn is_expired(&self, timeout: Duration) -> bool {
        let last_updated = *self.last_updated.read().await;
        last_updated.elapsed() > timeout
    }
}

/// 增强的连接管理器
pub struct ResilientConnectionManager {
    /// 会话存储
    sessions: Arc<RwLock<HashMap<Uuid, Arc<SessionInfo>>>>,
    /// 连接质量监控
    quality_monitor: Arc<ConnectionQuality>,
    /// 重连配置
    reconnect_config: ReconnectConfig,
    /// 健康检查间隔
    health_check_interval: Duration,
}

/// 重连配置
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始重试延迟
    pub initial_delay: Duration,
    /// 最大重试延迟
    pub max_delay: Duration,
    /// 指数退避因子
    pub backoff_factor: f64,
    /// 连接超时
    pub connect_timeout: Duration,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_factor: 2.0,
            connect_timeout: Duration::from_secs(10),
        }
    }
}

impl ResilientConnectionManager {
    pub fn new() -> Self {
        Self::with_config(ReconnectConfig::default())
    }

    pub fn with_config(config: ReconnectConfig) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            quality_monitor: Arc::new(ConnectionQuality::new()),
            reconnect_config: config,
            health_check_interval: Duration::from_secs(5),
        }
    }

    /// 创建具有会话保持的连接
    pub async fn create_resilient_connection(
        &self,
        client_addr: String,
        target_addr: &str,
    ) -> Result<(Uuid, TcpStream)> {
        let session = SessionInfo::new(client_addr.clone(), target_addr.to_string());
        let session_id = session.id;

        // 存储会话
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, Arc::new(session));
        }

        // 创建连接
        let remote_stream = self.connect_with_retry(target_addr).await?;

        debug_println!(
            "Created resilient connection {} for {} -> {}",
            session_id,
            client_addr,
            target_addr
        );

        Ok((session_id, remote_stream))
    }

    /// 使用重试机制连接到远程服务器
    async fn connect_with_retry(&self, target_addr: &str) -> Result<TcpStream> {
        let mut delay = self.reconnect_config.initial_delay;

        for attempt in 1..=self.reconnect_config.max_retries {
            match timeout(
                self.reconnect_config.connect_timeout,
                TcpStream::connect(target_addr),
            )
            .await
            {
                Ok(Ok(stream)) => {
                    debug_println!(
                        "Successfully connected to {} on attempt {}",
                        target_addr,
                        attempt
                    );
                    self.quality_monitor.reset_errors().await;
                    return Ok(stream);
                }
                Ok(Err(e)) => {
                    debug_println!(
                        "Failed to connect to {} on attempt {}: {}",
                        target_addr,
                        attempt,
                        e
                    );
                    self.quality_monitor.record_error().await;
                }
                Err(_) => {
                    debug_println!(
                        "Connection timeout to {} on attempt {}",
                        target_addr,
                        attempt
                    );
                    self.quality_monitor.record_error().await;
                }
            }

            // 如果不是最后一次尝试，等待后重试
            if attempt < self.reconnect_config.max_retries {
                debug_println!("Waiting {:?} before retry...", delay);
                sleep(delay).await;

                // 指数退避
                delay = std::cmp::min(
                    Duration::from_millis(
                        (delay.as_millis() as f64 * self.reconnect_config.backoff_factor) as u64,
                    ),
                    self.reconnect_config.max_delay,
                );
            }
        }

        Err(NetworkError::ConnectionFailed(format!(
            "Failed to connect after {} attempts",
            self.reconnect_config.max_retries
        ))
        .into())
    }

    /// 恢复断开的连接
    pub async fn recover_connection(
        &self,
        session_id: Uuid,
        target_addr: &str,
    ) -> Result<(TcpStream, Vec<Vec<u8>>)> {
        debug_println!(
            "Attempting to recover connection for session {}",
            session_id
        );

        // 获取会话信息和待发送数据
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(&session_id).cloned()
        };

        if let Some(session) = session {
            let pending_data = session.take_pending_data().await;

            // 尝试重新连接
            let new_stream = self.connect_with_retry(target_addr).await?;

            debug_println!(
                "Successfully recovered connection for session {} with {} pending packets",
                session_id,
                pending_data.len()
            );

            Ok((new_stream, pending_data))
        } else {
            Err(NetworkError::ConnectionFailed("Session not found".to_string()).into())
        }
    }

    /// 缓存数据用于断线重连
    pub async fn cache_data(&self, session_id: Uuid, data: Vec<u8>) -> Result<()> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(&session_id) {
            session.add_pending_data(data).await;
            Ok(())
        } else {
            Err(NetworkError::ConnectionFailed("Session not found".to_string()).into())
        }
    }

    /// 清理过期会话
    pub async fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        let initial_count = sessions.len();

        sessions.retain(|_, session| {
            // 异步检查会话是否过期（这里简化处理）
            session.created_at.elapsed() <= Duration::from_secs(300) // 5分钟超时
        });

        let removed = initial_count - sessions.len();
        if removed > 0 {
            debug_println!("Cleaned up {} expired sessions", removed);
        }
    }

    /// 获取连接质量统计
    pub async fn get_quality_stats(&self) -> QualityStats {
        QualityStats {
            average_rtt: self.quality_monitor.get_average_rtt().await,
            packet_loss: *self.quality_monitor.packet_loss.read().await,
            bandwidth: *self.quality_monitor.bandwidth.read().await,
            error_count: *self.quality_monitor.error_count.read().await,
            active_sessions: self.sessions.read().await.len(),
            is_healthy: self.quality_monitor.is_healthy().await,
        }
    }

    /// 启动后台健康检查任务
    pub async fn start_health_check(self: Arc<Self>) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(manager.health_check_interval);

            loop {
                interval.tick().await;

                // 清理过期会话
                manager.cleanup_expired_sessions().await;

                // 输出健康状态
                let stats = manager.get_quality_stats().await;
                if !stats.is_healthy {
                    debug_println!("Connection health degraded: {:?}", stats);
                }
            }
        });
    }
}

/// 连接质量统计
#[derive(Debug, Clone)]
pub struct QualityStats {
    pub average_rtt: Option<Duration>,
    pub packet_loss: f64,
    pub bandwidth: Option<u64>,
    pub error_count: u32,
    pub active_sessions: usize,
    pub is_healthy: bool,
}

/// 自适应数据转发器
#[allow(dead_code)] // Fields are used in adjust_buffer_size function
pub struct AdaptiveForwarder {
    /// 质量监控
    quality: Arc<ConnectionQuality>,
    /// 缓冲区大小（根据网络质量调整）
    buffer_size: Arc<RwLock<usize>>,
    /// 最小缓冲区
    #[allow(dead_code)] // Used in adjust_buffer_size function
    min_buffer_size: usize,
    /// 最大缓冲区
    #[allow(dead_code)] // Used in adjust_buffer_size function
    max_buffer_size: usize,
}

impl AdaptiveForwarder {
    pub fn new() -> Self {
        Self {
            quality: Arc::new(ConnectionQuality::new()),
            buffer_size: Arc::new(RwLock::new(64 * 1024)), // 默认64KB
            min_buffer_size: 8 * 1024,                     // 8KB
            max_buffer_size: 512 * 1024,                   // 512KB
        }
    }

    /// 自适应双向数据转发
    pub async fn forward_adaptive(
        &self,
        mut client: TcpStream,
        mut remote: TcpStream,
        session_id: Option<Uuid>,
    ) -> Result<(u64, u64)> {
        // 设置TCP选项优化
        self.optimize_tcp_socket(&mut client).await?;
        self.optimize_tcp_socket(&mut remote).await?;

        // 分割流
        let (mut client_read, mut client_write) = client.into_split();
        let (mut remote_read, mut remote_write) = remote.into_split();

        let quality = self.quality.clone();
        let buffer_size = self.buffer_size.clone();
        let _session_id_clone = session_id;

        // 客户端到远程的转发
        let client_to_remote = tokio::spawn(async move {
            let mut total_bytes = 0u64;
            let mut buffer = vec![0; *buffer_size.read().await];

            loop {
                let start = Instant::now();
                match client_read.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _send_start = Instant::now();
                        if let Err(e) = client_write.write_all(&buffer[..n]).await {
                            debug_println!("Failed to forward data: {}", e);
                            break;
                        }

                        // 记录RTT
                        let rtt = start.elapsed();
                        quality.record_rtt(rtt).await;

                        total_bytes += n as u64;

                        // 自适应调整缓冲区大小
                        Self::adjust_buffer_size(&buffer_size, &quality, 8 * 1024, 512 * 1024)
                            .await;
                    }
                    Err(e) => {
                        debug_println!("Read error: {}", e);
                        quality.record_error().await;
                        break;
                    }
                }
            }

            total_bytes
        });

        let quality = self.quality.clone();
        let buffer_size = self.buffer_size.clone();

        // 远程到客户端的转发
        let remote_to_client = tokio::spawn(async move {
            let mut total_bytes = 0u64;
            let mut buffer = vec![0; *buffer_size.read().await];

            loop {
                let start = Instant::now();
                match remote_read.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(e) = remote_write.write_all(&buffer[..n]).await {
                            debug_println!("Failed to forward data: {}", e);
                            break;
                        }

                        // 记录RTT
                        let rtt = start.elapsed();
                        quality.record_rtt(rtt).await;

                        total_bytes += n as u64;

                        // 自适应调整缓冲区大小
                        Self::adjust_buffer_size(&buffer_size, &quality, 8 * 1024, 512 * 1024)
                            .await;
                    }
                    Err(e) => {
                        debug_println!("Read error: {}", e);
                        quality.record_error().await;
                        break;
                    }
                }
            }

            total_bytes
        });

        // 等待两个方向完成
        let (bytes1, bytes2) = tokio::join!(client_to_remote, remote_to_client);

        Ok((bytes1.unwrap_or(0), bytes2.unwrap_or(0)))
    }

    /// 优化TCP套接字设置
    async fn optimize_tcp_socket(&self, stream: &mut TcpStream) -> Result<()> {
        // 禁用Nagle算法以减少延迟
        stream
            .set_nodelay(true)
            .map_err(|e| ProxyError::ForwardingFailed(format!("Failed to set nodelay: {}", e)))?;

        // TODO: 实现TCP keepalive设置（需要更复杂的处理）
        debug_println!("TCP socket optimized for cross-continental transmission");

        Ok(())
    }

    /// 根据网络质量自适应调整缓冲区大小
    async fn adjust_buffer_size(
        buffer_size: &Arc<RwLock<usize>>,
        quality: &ConnectionQuality,
        min_buffer_size: usize,
        max_buffer_size: usize,
    ) {
        if let Some(rtt) = quality.get_average_rtt().await {
            let current_size = *buffer_size.read().await;
            let mut new_size = current_size;

            // RTT小于50ms：使用大缓冲区提高吞吐量
            if rtt < Duration::from_millis(50) {
                new_size = std::cmp::min(current_size * 2, max_buffer_size);
            }
            // RTT大于200ms：使用小缓冲区减少延迟
            else if rtt > Duration::from_millis(200) {
                new_size = std::cmp::max(current_size / 2, min_buffer_size);
            }

            if new_size != current_size {
                debug_println!(
                    "Adjusting buffer size: {} -> {} (RTT: {:?})",
                    current_size,
                    new_size,
                    rtt
                );
                *buffer_size.write().await = new_size;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_quality() {
        let quality = ConnectionQuality::new();

        quality.record_rtt(Duration::from_millis(100)).await;
        quality.record_rtt(Duration::from_millis(200)).await;

        let avg_rtt = quality.get_average_rtt().await;
        assert!(avg_rtt.is_some());
        assert_eq!(avg_rtt.unwrap(), Duration::from_millis(150));
    }

    #[tokio::test]
    async fn test_session_info() {
        let session = SessionInfo::new("127.0.0.1:12345".to_string(), "example.com:80".to_string());

        session.add_pending_data(b"test data".to_vec()).await;
        let pending = session.take_pending_data().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0], b"test data");
    }

    #[test]
    fn test_reconnect_config() {
        let config = ReconnectConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
    }
}
