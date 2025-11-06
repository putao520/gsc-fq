/// 增强版连接处理器 - 支持跨洲际传输优化
use crate::error::{NetworkError, ProxyError, Result};
use crate::{debug_println, error_println};
use crate::proxy::resilient::{ResilientConnectionManager, AdaptiveForwarder, ReconnectConfig};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpStream, TcpSocket};

/// 增强版连接处理器
pub struct EnhancedConnectionHandler {
    /// 远程地址
    pub remote_addr: SocketAddr,
    /// 源IP
    pub source_ip: Option<IpAddr>,
    /// 最大连接数
    max_connections: Option<usize>,
    /// 健壮连接管理器
    resilient_manager: Arc<ResilientConnectionManager>,
    /// 自适应转发器
    adaptive_forwarder: Arc<AdaptiveForwarder>,
    /// 是否启用健壮模式
    resilient_mode: bool,
}

impl EnhancedConnectionHandler {
    /// 创建新的增强连接处理器（仅使用优化模式）
    pub fn new(
        remote_addr: SocketAddr,
        source_ip: Option<IpAddr>,
        max_connections: Option<usize>,
    ) -> Self {
        // 跨洲际传输优化配置
        let reconnect_config = ReconnectConfig {
            max_retries: 8, // 增加重试次数，适应跨洲际网络抖动
            initial_delay: Duration::from_millis(50), // 更快的初始重试
            max_delay: Duration::from_secs(15), // 更长的最大等待
            backoff_factor: 1.5, // 更温和的退避策略
            connect_timeout: Duration::from_secs(15), // 更长的连接超时
        };

        Self {
            remote_addr,
            source_ip,
            max_connections,
            resilient_manager: Arc::new(ResilientConnectionManager::with_config(reconnect_config)),
            adaptive_forwarder: Arc::new(AdaptiveForwarder::new()),
            resilient_mode: true, // 强制启用优化模式
        }
    }

    /// 设置是否启用健壮模式
    pub fn with_resilient_mode(mut self, enabled: bool) -> Self {
        self.resilient_mode = enabled;
        self
    }

    /// 获取远程地址
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// 处理传入的客户端连接（使用优化模式）
    pub async fn handle_connection(&self, client_stream: TcpStream) -> Result<()> {
        let client_addr = client_stream.peer_addr().map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to get client address: {}", e))
        })?;

        debug_println!(
            "New optimized connection from {} to {}",
            client_addr,
            self.remote_addr
        );

        // 直接使用优化模式处理
        self.handle_resilient_connection(client_stream, client_addr.to_string()).await
    }

    /// 处理健壮模式连接
    async fn handle_resilient_connection(&self, client_stream: TcpStream, client_addr: String) -> Result<()> {
        // 创建带会话保持的连接
        let (session_id, remote_stream) = self.resilient_manager
            .create_resilient_connection(
                client_addr.clone(),
                &self.remote_addr.to_string(),
            )
            .await?;

        debug_println!("Created resilient session {} for {}", session_id, client_addr);

        // 启动自适应转发
        let forwarder = self.adaptive_forwarder.clone();
        let manager = self.resilient_manager.clone();
        let target_addr = self.remote_addr.to_string();
        let session_id_clone = session_id;

        let handle = tokio::spawn(async move {
            match forwarder.forward_adaptive(client_stream, remote_stream, Some(session_id)).await {
                Ok((bytes1, bytes2)) => {
                    debug_println!("Session {} completed: {} bytes transferred",
                        session_id, bytes1 + bytes2);
                }
                Err(e) => {
                    debug_println!("Session {} forwarding error: {}", session_id, e);

                    // 尝试恢复连接
                    debug_println!("Attempting to recover session {}", session_id);
                    match manager.recover_connection(session_id, &target_addr).await {
                        Ok((new_stream, pending_data)) => {
                            debug_println!("Successfully recovered session {}", session_id);
                            // 这里可以将pending_data发送出去
                        }
                        Err(recover_err) => {
                            debug_println!("Failed to recover session {}: {}",
                                session_id, recover_err);
                        }
                    }
                }
            }
        });

        // 等待处理完成
        handle.await.map_err(|e| {
            ProxyError::ForwardingFailed(format!("Task join error: {}", e))
        })?;

        Ok(())
    }

    /// 处理标准连接（向后兼容）
    async fn handle_standard_connection(&self, client_stream: TcpStream) -> Result<()> {
        // 使用原有的处理逻辑
        let remote_stream = if let Some(source_ip) = self.source_ip {
            debug_println!(
                "Connecting to {} using source IP {}",
                self.remote_addr,
                source_ip
            );
            match self.connect_with_source_ip(source_ip).await {
                Ok(stream) => stream,
                Err(e) => {
                    error_println!(
                        "Failed to connect to remote {} using source IP {}: {:?}",
                        self.remote_addr,
                        source_ip,
                        e
                    );
                    return Err(e);
                }
            }
        } else {
            debug_println!("Connecting to {}", self.remote_addr);
            match tokio::time::timeout(
                Duration::from_secs(10),
                TcpStream::connect(self.remote_addr)
            ).await {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    error_println!("Failed to connect to remote {}: {}", self.remote_addr, e);
                    return Err(NetworkError::ConnectionFailed(format!(
                        "Failed to connect to remote {}: {}",
                        self.remote_addr, e
                    ))
                    .into());
                }
                Err(_) => {
                    error_println!("Connection timeout to remote {} after 10 seconds", self.remote_addr);
                    return Err(NetworkError::ConnectionTimeout.into());
                }
            }
        };

        debug_println!("Successfully connected to remote {}", self.remote_addr);

        // 使用自适应转发
        match self.adaptive_forwarder.forward_adaptive(client_stream, remote_stream, None).await {
            Ok((bytes1, bytes2)) => {
                debug_println!("Adaptive forwarding completed: {} bytes total", bytes1 + bytes2);
            }
            Err(e) => {
                error_println!("Adaptive forwarding failed: {:?}", e);
                return Err(e);
            }
        }

        Ok(())
    }

    /// 使用特定源IP连接到远程服务器
    async fn connect_with_source_ip(&self, source_ip: IpAddr) -> Result<TcpStream> {
        let local_addr = SocketAddr::new(source_ip, 0);

        let socket = match (source_ip, self.remote_addr) {
            (IpAddr::V4(_), SocketAddr::V4(_)) => TcpSocket::new_v4().map_err(|e| {
                NetworkError::ConnectionFailed(format!("Failed to create IPv4 socket: {}", e))
            })?,
            (IpAddr::V6(_), SocketAddr::V6(_)) => TcpSocket::new_v6().map_err(|e| {
                NetworkError::ConnectionFailed(format!("Failed to create IPv6 socket: {}", e))
            })?,
            _ => {
                return Err(NetworkError::ConnectionFailed(
                    "Source IP family does not match remote address family".to_string(),
                )
                .into());
            }
        };

        socket.bind(local_addr).map_err(|e| {
            NetworkError::ConnectionFailed(format!(
                "Failed to bind to source IP {}: {}",
                source_ip, e
            ))
        })?;

        // 使用优化的TCP设置
        let stream = tokio::time::timeout(
            Duration::from_secs(10),
            socket.connect(self.remote_addr)
        ).await.map_err(|_| {
            NetworkError::ConnectionTimeout
        })?.map_err(|e| {
            NetworkError::ConnectionFailed(format!(
                "Failed to connect to remote {}: {}",
                self.remote_addr, e
            ))
        })?;

        Ok(stream)
    }

    /// 获取连接统计
    pub async fn get_connection_stats(&self) -> EnhancedConnectionStats {
        let max_connections = self.max_connections.unwrap_or(usize::MAX);
        let quality_stats = self.resilient_manager.get_quality_stats().await;

        EnhancedConnectionStats {
            remote_addr: self.remote_addr,
            source_ip: self.source_ip,
            max_connections,
            active_connections: 0, // 可以通过其他方式跟踪
            available_slots: max_connections,
            resilient_mode: self.resilient_mode,
            quality_stats,
        }
    }

    /// 启动健康检查后台任务
    pub async fn start_health_monitor(self: Arc<Self>) {
        if self.resilient_mode {
            debug_println!("Starting health monitor for {}", self.remote_addr);
            self.resilient_manager.clone().start_health_check().await;
        }
    }
}

/// 增强版连接统计
#[derive(Debug, Clone)]
pub struct EnhancedConnectionStats {
    pub remote_addr: SocketAddr,
    pub source_ip: Option<IpAddr>,
    pub max_connections: usize,
    pub active_connections: usize,
    pub available_slots: usize,
    pub resilient_mode: bool,
    pub quality_stats: crate::proxy::resilient::QualityStats,
}

impl std::fmt::Display for EnhancedConnectionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Enhanced Connection Stats for {}:\n\
                  - Source IP: {:?}\n\
                  - Max Connections: {}\n\
                  - Active Connections: {}\n\
                  - Available Slots: {}\n\
                  - Resilient Mode: {}\n\
                  - Healthy: {}\n\
                  - Active Sessions: {}",
            self.remote_addr,
            self.source_ip,
            self.max_connections,
            self.active_connections,
            self.available_slots,
            self.resilient_mode,
            self.quality_stats.is_healthy,
            self.quality_stats.active_sessions
        )
    }
}

/// 连接池管理器（增强版）
pub struct EnhancedConnectionPool {
    handlers: Vec<Arc<EnhancedConnectionHandler>>,
}

impl EnhancedConnectionPool {
    /// 创建新的增强连接池
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// 添加处理器到池
    pub fn add_handler(&mut self, handler: EnhancedConnectionHandler) {
        self.handlers.push(Arc::new(handler));
    }

    /// 获取处理器
    pub fn get_handler(&self, index: usize) -> Option<&Arc<EnhancedConnectionHandler>> {
        self.handlers.get(index)
    }

    /// 获取所有处理器
    pub fn get_handlers(&self) -> &[Arc<EnhancedConnectionHandler>] {
        &self.handlers
    }

    /// 启动所有处理器的健康监控
    pub async fn start_all_health_monitors(&self) {
        for handler in &self.handlers {
            let handler_clone = handler.clone();
            tokio::spawn(async move {
                handler_clone.start_health_monitor().await;
            });
        }
    }

    /// 获取池统计
    pub async fn get_pool_stats(&self) -> EnhancedPoolStats {
        let total_handlers = self.handlers.len();
        let mut total_max_connections = 0;
        let mut total_active_connections = 0;
        let mut total_available_slots = 0;
        let mut all_healthy = true;
        let mut total_sessions = 0;

        for handler in &self.handlers {
            let stats = handler.get_connection_stats().await;
            total_max_connections += stats.max_connections;
            total_active_connections += stats.active_connections;
            total_available_slots += stats.available_slots;
            total_sessions += stats.quality_stats.active_sessions;

            if !stats.quality_stats.is_healthy {
                all_healthy = false;
            }
        }

        EnhancedPoolStats {
            total_handlers,
            total_max_connections,
            total_active_connections,
            total_available_slots,
            all_healthy,
            total_sessions,
        }
    }
}

/// 增强版池统计
#[derive(Debug, Clone)]
pub struct EnhancedPoolStats {
    pub total_handlers: usize,
    pub total_max_connections: usize,
    pub total_active_connections: usize,
    pub total_available_slots: usize,
    pub all_healthy: bool,
    pub total_sessions: usize,
}

impl std::fmt::Display for EnhancedPoolStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Enhanced Pool Stats:\n\
                  - Total Handlers: {}\n\
                  - Total Max Connections: {}\n\
                  - Total Active Connections: {}\n\
                  - Total Available Slots: {}\n\
                  - All Healthy: {}\n\
                  - Total Active Sessions: {}",
            self.total_handlers,
            self.total_max_connections,
            self.total_active_connections,
            self.total_available_slots,
            self.all_healthy,
            self.total_sessions
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_enhanced_handler_creation() {
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080);
        let source_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 50));

        let handler = EnhancedConnectionHandler::new(remote_addr, Some(source_ip), Some(100));

        assert_eq!(handler.remote_addr, remote_addr);
        assert_eq!(handler.source_ip, Some(source_ip));
        assert!(handler.resilient_mode);
    }

    #[test]
    fn test_resilient_mode_toggle() {
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080);

        let handler = EnhancedConnectionHandler::new(remote_addr, None, None)
            .with_resilient_mode(false);

        assert!(!handler.resilient_mode);
    }

    #[test]
    fn test_enhanced_pool() {
        let mut pool = EnhancedConnectionPool::new();

        let handler1 = EnhancedConnectionHandler::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080),
            None,
            None,
        );

        let handler2 = EnhancedConnectionHandler::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101)), 8081),
            None,
            None,
        );

        pool.add_handler(handler1);
        pool.add_handler(handler2);

        assert_eq!(pool.get_handlers().len(), 2);
    }
}