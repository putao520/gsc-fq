use crate::error::{NetworkError, ProxyError, Result};
use crate::{debug_println, error_println};
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::select;

/// Improved Connection handler with better timeout handling
pub struct ConnectionHandler {
    pub remote_addr: SocketAddr,
    pub source_ip: Option<IpAddr>,
    max_connections: Option<usize>,
    read_timeout: Duration,
    write_timeout: Duration,
    idle_timeout: Duration,
}

impl ConnectionHandler {
    /// Create a new connection handler with configurable timeouts
    pub fn new(
        remote_addr: SocketAddr,
        source_ip: Option<IpAddr>,
        max_connections: Option<usize>,
    ) -> Self {
        Self {
            remote_addr,
            source_ip,
            max_connections,
            read_timeout: Duration::from_secs(30),   // 30 seconds read timeout
            write_timeout: Duration::from_secs(10),  // 10 seconds write timeout
            idle_timeout: Duration::from_secs(60),   // 60 seconds idle timeout
        }
    }

    /// Create a connection handler with custom timeouts
    pub fn new_with_timeouts(
        remote_addr: SocketAddr,
        source_ip: Option<IpAddr>,
        max_connections: Option<usize>,
        read_timeout: Duration,
        write_timeout: Duration,
        idle_timeout: Duration,
    ) -> Self {
        Self {
            remote_addr,
            source_ip,
            max_connections,
            read_timeout,
            write_timeout,
            idle_timeout,
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

        // Start bidirectional data forwarding with improved timeout handling
        debug_println!(
            "Starting data forwarding for {} <-> {}",
            client_addr,
            self.remote_addr
        );

        match self.forward_data_improved(client_stream, remote_stream).await {
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

    /// Improved data forwarding with proper timeout handling
    /// This solves the "hanging connection" problem by implementing:
    /// 1. Separate read/write timeouts
    /// 2. Idle connection detection
    /// 3. Graceful error handling
    async fn forward_data_improved(&self, mut client: TcpStream, mut remote: TcpStream) -> Result<()> {
        // Apply TCP optimizations
        client.set_nodelay(true).map_err(|e| {
            ProxyError::ForwardingFailed(format!("Failed to set client nodelay: {}", e))
        })?;
        remote.set_nodelay(true).map_err(|e| {
            ProxyError::ForwardingFailed(format!("Failed to set remote nodelay: {}", e))
        })?;

        // Set socket read timeouts
        client.set_read_timeout(Some(self.read_timeout)).map_err(|e| {
            ProxyError::ForwardingFailed(format!("Failed to set client read timeout: {}", e))
        })?;
        remote.set_read_timeout(Some(self.read_timeout)).map_err(|e| {
            ProxyError::ForwardingFailed(format!("Failed to set remote read timeout: {}", e))
        })?;

        let (mut client_read, mut client_write) = client.split();
        let (mut remote_read, mut remote_write) = remote.split();

        let mut buffer = vec![0u8; 8192]; // 8KB buffer
        let mut last_activity = std::time::Instant::now();

        loop {
            select! {
                // Client to Remote data transfer
                result = tokio::time::timeout(self.read_timeout, client_read.read(&mut buffer)) => {
                    match result {
                        Ok(Ok(0)) => {
                            debug_println!("Client closed connection");
                            break;
                        }
                        Ok(Ok(n)) => {
                            last_activity = std::time::Instant::now();
                            debug_println!("Read {} bytes from client", n);

                            match tokio::time::timeout(self.write_timeout, remote_write.write_all(&buffer[..n])).await {
                                Ok(Ok(())) => {
                                    debug_println!("Wrote {} bytes to remote", n);
                                }
                                Ok(Err(e)) => {
                                    error_println!("Failed to write to remote: {}", e);
                                    return Err(ProxyError::ForwardingFailed(format!("Remote write failed: {}", e)).into());
                                }
                                Err(_) => {
                                    error_println!("Write to remote timed out after {:?}", self.write_timeout);
                                    return Err(ProxyError::ForwardingFailed("Remote write timeout".to_string()).into());
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                                debug_println!("Client read timeout, checking idle time");
                                if last_activity.elapsed() > self.idle_timeout {
                                    error_println!("Connection idle timeout after {:?}", self.idle_timeout);
                                    break;
                                }
                                continue;
                            } else {
                                error_println!("Client read error: {}", e);
                                break;
                            }
                        }
                        Err(_) => {
                            error_println!("Client read timed out after {:?}", self.read_timeout);
                            if last_activity.elapsed() > self.idle_timeout {
                                error_println!("Connection idle timeout after {:?}", self.idle_timeout);
                                break;
                            }
                            continue;
                        }
                    }
                }

                // Remote to Client data transfer
                result = tokio::time::timeout(self.read_timeout, remote_read.read(&mut buffer)) => {
                    match result {
                        Ok(Ok(0)) => {
                            debug_println!("Remote closed connection");
                            break;
                        }
                        Ok(Ok(n)) => {
                            last_activity = std::time::Instant::now();
                            debug_println!("Read {} bytes from remote", n);

                            match tokio::time::timeout(self.write_timeout, client_write.write_all(&buffer[..n])).await {
                                Ok(Ok(())) => {
                                    debug_println!("Wrote {} bytes to client", n);
                                }
                                Ok(Err(e)) => {
                                    error_println!("Failed to write to client: {}", e);
                                    return Err(ProxyError::ForwardingFailed(format!("Client write failed: {}", e)).into());
                                }
                                Err(_) => {
                                    error_println!("Write to client timed out after {:?}", self.write_timeout);
                                    return Err(ProxyError::ForwardingFailed("Client write timeout".to_string()).into());
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                                debug_println!("Remote read timeout, checking idle time");
                                if last_activity.elapsed() > self.idle_timeout {
                                    error_println!("Connection idle timeout after {:?}", self.idle_timeout);
                                    break;
                                }
                                continue;
                            } else {
                                error_println!("Remote read error: {}", e);
                                break;
                            }
                        }
                        Err(_) => {
                            error_println!("Remote read timed out after {:?}", self.read_timeout);
                            if last_activity.elapsed() > self.idle_timeout {
                                error_println!("Connection idle timeout after {:?}", self.idle_timeout);
                                break;
                            }
                            continue;
                        }
                    }
                }

                // Idle timeout check
                _ = tokio::time::sleep(self.idle_timeout) => {
                    if last_activity.elapsed() > self.idle_timeout {
                        error_println!("Connection idle timeout after {:?}", self.idle_timeout);
                        break;
                    }
                }
            }
        }

        debug_println!("Data forwarding completed successfully");
        Ok(())
    }
}

// Configuration for connection timeouts
pub struct TimeoutConfig {
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub idle_timeout: Duration,
    pub connect_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            read_timeout: Duration::from_secs(30),   // 30 seconds for read operations
            write_timeout: Duration::from_secs(10),  // 10 seconds for write operations
            idle_timeout: Duration::from_secs(60),   // 60 seconds idle timeout
            connect_timeout: Duration::from_secs(10), // 10 seconds connection timeout
        }
    }
}

impl TimeoutConfig {
    /// Create configuration for interactive connections (shorter timeouts)
    pub fn interactive() -> Self {
        Self {
            read_timeout: Duration::from_secs(10),   // 10 seconds for interactive use
            write_timeout: Duration::from_secs(5),   // 5 seconds for write operations
            idle_timeout: Duration::from_secs(30),   // 30 seconds idle timeout
            connect_timeout: Duration::from_secs(5),  // 5 seconds connection timeout
        }
    }

    /// Create configuration for long-running connections (longer timeouts)
    pub fn persistent() -> Self {
        Self {
            read_timeout: Duration::from_secs(300),  // 5 minutes for read operations
            write_timeout: Duration::from_secs(30),  // 30 seconds for write operations
            idle_timeout: Duration::from_secs(600),  // 10 minutes idle timeout
            connect_timeout: Duration::from_secs(30), // 30 seconds connection timeout
        }
    }
}