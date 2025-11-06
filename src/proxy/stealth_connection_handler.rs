use crate::debug_println;
use crate::error::types::ProxyError;
use crate::error::Result;
use crate::proxy::stealth_handler::StealthHandler;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
/// Stealth connection handler with blackhole mode
/// Replaces EnhancedConnectionHandler to enable blackhole functionality
use tokio::net::TcpStream;

/// Stealth connection handler that includes blackhole mode
#[derive(Clone)]
#[allow(dead_code)] // source_ip field reserved for future functionality
pub struct StealthConnectionHandler {
    remote_addr: SocketAddr,
    #[allow(dead_code)] // Reserved for future source IP binding functionality
    source_ip: Option<std::net::IpAddr>,
    stats: Arc<StealthConnectionCounters>,
}

impl StealthConnectionHandler {
    /// Create a new stealth connection handler
    pub fn new(
        remote_addr: SocketAddr,
        source_ip: Option<std::net::IpAddr>,
        _extra_options: Option<&str>,
    ) -> Self {
        Self {
            remote_addr,
            source_ip,
            stats: Arc::new(StealthConnectionCounters::new()),
        }
    }

    /// Handle incoming connection with stealth capabilities
    pub async fn handle_connection(&self, client: TcpStream) -> Result<()> {
        // Update connection statistics
        self.stats.total_connections.fetch_add(1, Ordering::Relaxed);
        self.stats
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        let client_addr = client.peer_addr().ok();

        // Apply TCP optimizations if needed
        let client = self.apply_tcp_optimizations(client).await?;

        // Use StealthHandler which includes blackhole mode
        match StealthHandler::handle_stealth(client, self.remote_addr).await {
            Ok(()) => {
                debug_println!(
                    "Stealth handler completed successfully for {:?}",
                    client_addr
                );
                self.stats
                    .successful_connections
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                debug_println!("Stealth handler failed for {:?}: {:?}", client_addr, e);
                self.stats
                    .failed_connections
                    .fetch_add(1, Ordering::Relaxed);
                return Err(e.into());
            }
        }

        // Decrement active connections
        self.stats
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);

        Ok(())
    }

    /// Apply TCP optimizations to the connection
    async fn apply_tcp_optimizations(&self, stream: TcpStream) -> Result<TcpStream> {
        // Set TCP_NODELAY for better latency
        stream
            .set_nodelay(true)
            .map_err(|e| ProxyError::ForwardingFailed(format!("Failed to set nodelay: {}", e)))?;

        // Set keepalive
        #[cfg(unix)]
        {
            use socket2::{SockRef, TcpKeepalive};
            use std::time::Duration;

            let socket = SockRef::from(&stream);
            let keepalive = TcpKeepalive::new()
                .with_time(Duration::from_secs(60))
                .with_interval(Duration::from_secs(10))
                .with_retries(3);

            let _ = socket.set_tcp_keepalive(&keepalive);
        }

        Ok(stream)
    }

    /// Get remote address
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Get connection statistics
    pub async fn get_connection_stats(&self) -> StealthConnectionStats {
        self.stats.snapshot()
    }
}

/// Connection statistics for stealth handler
#[derive(Debug, Default)]
struct StealthConnectionCounters {
    total_connections: AtomicU64,
    active_connections: AtomicU64,
    successful_connections: AtomicU64,
    failed_connections: AtomicU64,
    bytes_transferred: AtomicU64,
}

impl StealthConnectionCounters {
    /// Create new statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total connections
    pub fn total_connections(&self) -> u64 {
        self.total_connections.load(Ordering::Relaxed)
    }

    /// Get active connections
    pub fn active_connections(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Get successful connections
    pub fn successful_connections(&self) -> u64 {
        self.successful_connections.load(Ordering::Relaxed)
    }

    /// Get failed connections
    pub fn failed_connections(&self) -> u64 {
        self.failed_connections.load(Ordering::Relaxed)
    }

    /// Get bytes transferred
    pub fn bytes_transferred(&self) -> u64 {
        self.bytes_transferred.load(Ordering::Relaxed)
    }
}

impl StealthConnectionCounters {
    /// Create a snapshot of the current statistics
    fn snapshot(&self) -> StealthConnectionStats {
        StealthConnectionStats {
            total_connections: self.total_connections(),
            active_connections: self.active_connections(),
            successful_connections: self.successful_connections(),
            failed_connections: self.failed_connections(),
            bytes_transferred: self.bytes_transferred(),
        }
    }
}

/// Snapshot of stealth connection statistics with plain counters
#[derive(Debug, Clone, Copy, Default)]
pub struct StealthConnectionStats {
    pub total_connections: u64,
    pub active_connections: u64,
    pub successful_connections: u64,
    pub failed_connections: u64,
    pub bytes_transferred: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stealth_handler_creation() {
        let remote_addr = "127.0.0.1:8080".parse().unwrap();
        let handler = StealthConnectionHandler::new(remote_addr, None, None);

        assert_eq!(handler.remote_addr(), remote_addr);
        assert_eq!(handler.stats.total_connections(), 0);
    }

    #[test]
    fn test_connection_stats() {
        let stats = StealthConnectionCounters::new();

        stats.total_connections.store(10, Ordering::Relaxed);
        stats.successful_connections.store(8, Ordering::Relaxed);
        stats.failed_connections.store(2, Ordering::Relaxed);

        let snapshot = stats.snapshot();

        assert_eq!(snapshot.total_connections, 10);
        assert_eq!(snapshot.successful_connections, 8);
        assert_eq!(snapshot.failed_connections, 2);
    }
}
