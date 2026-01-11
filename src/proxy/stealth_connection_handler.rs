use crate::debug_println;
use crate::error::types::ProxyError;
use crate::error::Result;
use crate::proxy::stealth_handler::StealthHandler;
use crate::proxy::ConnectionPool;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
/// Stealth connection handler with blackhole mode
/// Replaces EnhancedConnectionHandler to enable blackhole functionality
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;

/// Stealth connection handler that includes blackhole mode
#[derive(Clone)]
#[allow(dead_code)] // source_ip field reserved for future functionality
pub struct StealthConnectionHandler {
    remote_addr: SocketAddr,
    #[allow(dead_code)] // Reserved for future source IP binding functionality
    source_ip: Option<std::net::IpAddr>,
    stats: Arc<StealthConnectionCounters>,
    connection_pool: Option<Arc<ConnectionPool>>,
    udp_sessions: Arc<Mutex<HashMap<SocketAddr, Arc<UdpSocket>>>>,
    allow_ips: Option<Vec<String>>,
    max_conns_per_ip: Option<usize>,
    cps_limit: Option<f64>,
    // Per-IP rate limiting and connection tracking
    ip_stats: Arc<Mutex<HashMap<IpAddr, IpSecurityStats>>>,
}

struct IpSecurityStats {
    active_connections: usize,
    // For simple CPS limiting
    recent_connections: Vec<std::time::Instant>,
}

impl IpSecurityStats {
    fn new() -> Self {
        Self {
            active_connections: 0,
            recent_connections: Vec::new(),
        }
    }
}

impl StealthConnectionHandler {
    /// Create a new stealth connection handler
    pub fn new(
        remote_addr: SocketAddr,
        source_ip: Option<std::net::IpAddr>,
        connection_pool: Option<Arc<ConnectionPool>>,
        allow_ips: Option<Vec<String>>,
        max_conns_per_ip: Option<usize>,
        cps_limit: Option<f64>,
    ) -> Self {
        Self {
            remote_addr,
            source_ip,
            stats: Arc::new(StealthConnectionCounters::new()),
            connection_pool,
            udp_sessions: Arc::new(Mutex::new(HashMap::new())),
            allow_ips,
            max_conns_per_ip,
            cps_limit,
            ip_stats: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Handle incoming connection with stealth capabilities
    pub async fn handle_connection(&self, client: TcpStream) -> Result<()> {
        let client_addr = client.peer_addr().ok();

        // 1. ACL and Rate Limiting Check
        if let Some(addr) = client_addr {
            self.check_security(addr.ip()).await?;
        }

        // Update connection statistics
        self.stats.total_connections.fetch_add(1, Ordering::Relaxed);
        self.stats
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        let client_addr = client.peer_addr().ok();

        // Apply TCP optimizations if needed
        let client = self.apply_tcp_optimizations(client).await?;

        // Use StealthHandler which includes blackhole mode
        match StealthHandler::handle_stealth(client, self.remote_addr, self.connection_pool.clone())
            .await
        {
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
                if let Some(addr) = client_addr {
                    self.register_disconnection(addr.ip()).await;
                }
                self.stats
                    .failed_connections
                    .fetch_add(1, Ordering::Relaxed);
                return Err(e.into());
            }
        }

        // Decrement active connections
        if let Some(addr) = client_addr {
            self.register_disconnection(addr.ip()).await;
        }
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

    /// Check security rules (ACL and Rate Limiting)
    async fn check_security(&self, ip: IpAddr) -> Result<()> {
        // 1. Check ACL
        if let Some(ref allowed) = self.allow_ips {
            let ip_str = ip.to_string();
            let mut match_found = false;
            for pattern in allowed {
                // Simple string match or CIDR?
                // For now, simple string match or prefix (simplified)
                if ip_str == *pattern || pattern == "0.0.0.0/0" || pattern == "::/0" {
                    match_found = true;
                    break;
                }
            }
            if !match_found {
                return Err(
                    ProxyError::ForwardingFailed(format!("IP {} not in allow list", ip)).into(),
                );
            }
        }

        // 2. Check Rate Limiting and Max Connections
        if self.max_conns_per_ip.is_some() || self.cps_limit.is_some() {
            let mut ip_stats_map = self.ip_stats.lock().await;
            let stats = ip_stats_map.entry(ip).or_insert_with(IpSecurityStats::new);

            // Max connections check
            if let Some(max_active) = self.max_conns_per_ip {
                if stats.active_connections >= max_active {
                    return Err(ProxyError::ForwardingFailed(format!(
                        "Max connections per IP reached for {}",
                        ip
                    ))
                    .into());
                }
            }

            // CPS check
            if let Some(limit) = self.cps_limit {
                let now = std::time::Instant::now();
                // Clean up old entries (older than 1s)
                stats
                    .recent_connections
                    .retain(|&t| now.duration_since(t) < std::time::Duration::from_secs(1));

                if stats.recent_connections.len() >= limit as usize {
                    return Err(ProxyError::ForwardingFailed(format!(
                        "Connection rate limit exceeded for {}",
                        ip
                    ))
                    .into());
                }
                stats.recent_connections.push(now);
            }

            stats.active_connections += 1;
        }

        Ok(())
    }

    /// Register connection closure for rate limiting stats
    async fn register_disconnection(&self, ip: IpAddr) {
        if self.max_conns_per_ip.is_some() {
            let mut ip_stats_map = self.ip_stats.lock().await;
            if let Some(stats) = ip_stats_map.get_mut(&ip) {
                stats.active_connections = stats.active_connections.saturating_sub(1);
            }
        }
    }

    /// Handle UDP packet forwarding
    pub async fn handle_udp_packet(
        &self,
        data: Vec<u8>,
        client_addr: SocketAddr,
        server_socket: Arc<UdpSocket>,
    ) -> Result<()> {
        // 1. ACL and Rate Limiting Check
        self.check_security(client_addr.ip()).await?;

        let mut sessions = self.udp_sessions.lock().await;

        // Get or create session for this client
        let backend_socket = if let Some(socket) = sessions.get(&client_addr) {
            socket.clone()
        } else {
            // Bind a new local socket for this client to talk to the remote
            let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await.map_err(|e| {
                ProxyError::ForwardingFailed(format!("Failed to bind backend UDP socket: {}", e))
            })?);

            // Connect to the target
            socket.connect(self.remote_addr).await.map_err(|e| {
                ProxyError::ForwardingFailed(format!(
                    "Failed to connect to remote UDP {}: {}",
                    self.remote_addr, e
                ))
            })?;

            let socket_clone = socket.clone();
            let sessions_clone = self.udp_sessions.clone();
            let remote_addr = self.remote_addr;

            // Spawn relay task for responses from remote to client
            tokio::spawn(async move {
                let mut buf = [0u8; 65535];
                loop {
                    match tokio::time::timeout(Duration::from_secs(60), socket_clone.recv(&mut buf))
                        .await
                    {
                        Ok(Ok(n)) => {
                            if let Err(e) = server_socket.send_to(&buf[..n], client_addr).await {
                                crate::error_println!(
                                    "Failed to send UDP response back to {}: {}",
                                    client_addr,
                                    e
                                );
                                break;
                            }
                        }
                        Ok(Err(e)) => {
                            crate::error_println!(
                                "UDP backend recv error from {}: {}",
                                remote_addr,
                                e
                            );
                            break;
                        }
                        Err(_) => {
                            debug_println!("UDP session for {} timed out", client_addr);
                            break;
                        }
                    }
                }

                // Cleanup session
                let mut sessions = sessions_clone.lock().await;
                sessions.remove(&client_addr);
            });

            sessions.insert(client_addr, socket.clone());
            socket
        };

        // Forward data to remote
        backend_socket.send(&data).await.map_err(|e| {
            ProxyError::ForwardingFailed(format!("Failed to forward UDP packet: {}", e))
        })?;

        Ok(())
    }
}

use std::time::Duration;

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
        let handler = StealthConnectionHandler::new(remote_addr, None, None, None, None, None);

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
