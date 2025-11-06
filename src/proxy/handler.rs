use crate::error::{NetworkError, ProxyError, Result};
use crate::{debug_println, error_println};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpSocket, TcpStream};

/// Connection handler for managing TCP proxy connections
pub struct ConnectionHandler {
    pub remote_addr: SocketAddr,
    pub source_ip: Option<IpAddr>,
    max_connections: Option<usize>,
}

impl ConnectionHandler {
    /// Create a new connection handler
    pub fn new(
        remote_addr: SocketAddr,
        source_ip: Option<IpAddr>,
        max_connections: Option<usize>,
    ) -> Self {
        Self {
            remote_addr,
            source_ip,
            max_connections,
        }
    }

    /// Get the remote address
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Handle incoming client connection
    pub async fn handle_connection(&self, client_stream: TcpStream) -> Result<()> {
        let client_addr = client_stream.peer_addr().map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to get client address: {}", e))
        })?;

        debug_println!(
            "New connection from {} to {}",
            client_addr,
            self.remote_addr
        );

        // Connect to remote server with source_ip if specified
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
            // Add connection timeout
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

        // Note: TCP keep-alive is handled by the OS by default
        // We rely on the timeout mechanisms to detect issues

        // Start bidirectional data forwarding
        debug_println!(
            "Starting data forwarding for {} <-> {}",
            client_addr,
            self.remote_addr
        );

        match self.forward_data(client_stream, remote_stream).await {
            Ok(()) => {
                debug_println!("Data forwarding completed for {}", client_addr);
            }
            Err(e) => {
                error_println!("Data forwarding failed for {}: {:?}", client_addr, e);
                return Err(e);
            }
        }

        Ok(())
    }

    /// Connect to remote server with specific source IP
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

        // Add connection timeout for source IP connections
        let stream = tokio::time::timeout(
            Duration::from_secs(10),
            socket.connect(self.remote_addr)
        ).await.map_err(|_| {
            // Timeout occurred
            NetworkError::ConnectionTimeout
        })?.map_err(|e| {
            NetworkError::ConnectionFailed(format!(
                "Failed to connect to remote {}: {}",
                self.remote_addr, e
            ))
        })?;

        Ok(stream)
    }

    /// Forward data between client and remote using Tokio's built-in bidirectional copying
    async fn forward_data(&self, mut client: TcpStream, mut remote: TcpStream) -> Result<()> {
        use tokio::io::copy_bidirectional;

        // Apply CODEX's performance optimizations: disable Nagle's algorithm for low latency
        client.set_nodelay(true).map_err(|e| {
            ProxyError::ForwardingFailed(format!("Failed to set client nodelay: {}", e))
        })?;
        remote.set_nodelay(true).map_err(|e| {
            ProxyError::ForwardingFailed(format!("Failed to set remote nodelay: {}", e))
        })?;

        // Use Tokio's built-in copy_bidirectional for efficient zero-copy data forwarding
        // Add timeout to prevent infinite blocking
        match tokio::time::timeout(
            Duration::from_secs(300), // 5 minutes timeout for data forwarding
            copy_bidirectional(&mut client, &mut remote)
        ).await {
            Ok(result) => {
                result.map_err(|e| {
                    ProxyError::ForwardingFailed(format!("Bidirectional copy failed: {}", e))
                })?;
            }
            Err(_) => {
                // Timeout occurred - one side likely stopped responding
                error_println!("Data forwarding timeout after 5 minutes, closing connection");
                return Err(ProxyError::ForwardingFailed(
                    "Data forwarding timeout".to_string()
                ).into());
            }
        }

        Ok(())
    }

    /// Get current connection statistics
    pub fn get_connection_stats(&self) -> ConnectionStats {
        let max_connections = self.max_connections.unwrap_or(usize::MAX);
        ConnectionStats {
            remote_addr: self.remote_addr,
            source_ip: self.source_ip,
            max_connections,
            active_connections: 0, // Not tracked in this implementation
            available_slots: max_connections.saturating_sub(0), // All slots available for now
        }
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub remote_addr: SocketAddr,
    pub source_ip: Option<IpAddr>,
    pub max_connections: usize,
    pub active_connections: usize,
    pub available_slots: usize,
}

impl std::fmt::Display for ConnectionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Connection Stats for {}:\n\
                  - Source IP: {:?}\n\
                  - Max Connections: {}\n\
                  - Active Connections: {}\n\
                  - Available Slots: {}",
            self.remote_addr,
            self.source_ip,
            self.max_connections,
            self.active_connections,
            self.available_slots
        )
    }
}

/// Connection pool manager for handling multiple concurrent connections
pub struct ConnectionPool {
    handlers: Vec<Arc<ConnectionHandler>>,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Add a connection handler to the pool
    pub fn add_handler(&mut self, handler: ConnectionHandler) {
        self.handlers.push(Arc::new(handler));
    }

    /// Get connection handler by index
    pub fn get_handler(&self, index: usize) -> Option<&Arc<ConnectionHandler>> {
        self.handlers.get(index)
    }

    /// Get all handlers
    pub fn get_handlers(&self) -> &[Arc<ConnectionHandler>] {
        &self.handlers
    }

    /// Get total pool statistics
    pub fn get_pool_stats(&self) -> PoolStats {
        let total_handlers = self.handlers.len();

        // Calculate total max connections from all handlers
        let total_max_connections = self
            .handlers
            .iter()
            .map(|h| h.get_connection_stats().max_connections)
            .reduce(|acc, val| acc.saturating_add(val))
            .unwrap_or(usize::MAX);

        PoolStats {
            total_handlers,
            total_max_connections,
            total_active_connections: 0, // Not tracked in this implementation
            total_available_slots: total_max_connections, // All slots available for now
        }
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_handlers: usize,
    pub total_max_connections: usize,
    pub total_active_connections: usize,
    pub total_available_slots: usize,
}

impl std::fmt::Display for PoolStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Connection Pool Stats:\n\
                  - Total Handlers: {}\n\
                  - Total Max Connections: {}\n\
                  - Total Active Connections: {}\n\
                  - Total Available Slots: {}",
            self.total_handlers,
            self.total_max_connections,
            self.total_active_connections,
            self.total_available_slots
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_connection_handler_creation() {
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080);
        let source_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 50));

        let handler = ConnectionHandler::new(remote_addr, Some(source_ip), Some(100));

        assert_eq!(handler.remote_addr, remote_addr);
        assert_eq!(handler.source_ip, Some(source_ip));
    }

    #[test]
    fn test_connection_stats() {
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080);
        let source_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 50));

        let handler = ConnectionHandler::new(remote_addr, Some(source_ip), Some(100));
        let stats = handler.get_connection_stats();

        assert_eq!(stats.remote_addr, remote_addr);
        assert_eq!(stats.source_ip, Some(source_ip));
        assert_eq!(stats.max_connections, 100);
    }

    #[test]
    fn test_connection_pool() {
        let mut pool = ConnectionPool::new();

        let handler1 = ConnectionHandler::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080),
            None,
            None,
        );

        let handler2 = ConnectionHandler::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101)), 8081),
            None,
            None,
        );

        pool.add_handler(handler1);
        pool.add_handler(handler2);

        assert_eq!(pool.get_handlers().len(), 2);
        assert!(pool.get_handler(0).is_some());
        assert!(pool.get_handler(1).is_some());
        assert!(pool.get_handler(2).is_none());
    }

    #[test]
    fn test_pool_stats() {
        let mut pool = ConnectionPool::new();

        let handler = ConnectionHandler::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080),
            None,
            Some(100),
        );

        pool.add_handler(handler);
        let stats = pool.get_pool_stats();

        assert_eq!(stats.total_handlers, 1);
        assert_eq!(stats.total_max_connections, 100);
        assert_eq!(stats.total_active_connections, 0);
        assert_eq!(stats.total_available_slots, 100);
    }
}
