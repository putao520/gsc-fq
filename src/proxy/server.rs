use crate::config::loader::{ConfigLoader, ProxySection};
use crate::error::{NetworkError, Result};
use crate::proxy::{ConnectionPool, StealthConnectionHandler};
use crate::{debug_println, error_println};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpSocket};
use tokio::signal;
use tokio::sync::broadcast;

/// Proxy server that manages multiple proxy instances
pub struct ProxyServer {
    bind_ip: IpAddr,
    proxy_instances: Vec<ProxyInstance>,
    shutdown_tx: broadcast::Sender<()>,
}

impl ProxyServer {
    /// Create a new proxy server
    pub fn new(bind_ip: IpAddr) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            bind_ip,
            proxy_instances: Vec::new(),
            shutdown_tx,
        }
    }

    /// Add proxy configuration
    pub fn add_proxy(&mut self, proxy_config: &ProxySection) -> Result<()> {
        let remote_addr =
            ConfigLoader::create_socket_addr(&proxy_config.remote_host, proxy_config.remote_port)?;
        let source_ip = if let Some(ref source_ip_str) = proxy_config.source_ip {
            Some(ConfigLoader::parse_ip_address(source_ip_str)?)
        } else {
            None
        };

        // Connection pool configuration
        let pool_enabled = proxy_config.pool_enabled.unwrap_or(false);
        let pool_size = proxy_config.pool_size.unwrap_or(5).max(1).min(20);

        let instance = ProxyInstance::new(
            self.bind_ip,
            proxy_config.local_port,
            remote_addr,
            source_ip,
            pool_enabled,
            pool_size,
        )?;

        self.proxy_instances.push(instance);

        Ok(())
    }

    /// Start all proxy instances
    pub async fn start(&mut self) -> Result<()> {
        if self.proxy_instances.is_empty() {
            eprintln!(
                "ℹ️  No proxy instances configured - server running but no forwarding rules active"
            );
            // 不返回错误，允许服务器在没有代理实例的情况下运行
        }

        // Display startup information
        self.display_startup_info().await;

        // Start all proxy instances concurrently
        let mut handles = Vec::new();

        for instance in &mut self.proxy_instances {
            // Preserve the original handler by reusing its Arc instead of constructing a dummy instance.
            let placeholder = ProxyInstance::placeholder_from(instance);
            let mut instance = std::mem::replace(instance, placeholder);
            let shutdown_rx = self.shutdown_tx.subscribe();
            let handle = tokio::spawn(async move {
                let _ = instance.start(shutdown_rx).await;
            });
            handles.push(handle);
        }

        // Wait for shutdown signal
        self.wait_for_shutdown().await;

        // Send shutdown signal to all instances
        let _ = self.shutdown_tx.send(());

        // Wait for all instances to shutdown
        for handle in handles {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Wait for shutdown signal (Ctrl+C)
    async fn wait_for_shutdown(&self) {
        #[cfg(unix)]
        {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to setup SIGTERM handler");
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
                .expect("Failed to setup SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => {}
                _ = sigint.recv() => {}
            }
        }

        #[cfg(windows)]
        {
            let _ = signal::ctrl_c().await;
        }
    }

    /// Display startup information
    async fn display_startup_info(&self) {
        println!("🚀 GSC-FQ Proxy Server v{}", env!("CARGO_PKG_VERSION"));
        println!("================================");
        println!("📡 Server listening on: {}", self.bind_ip);
        println!();

        if self.proxy_instances.is_empty() {
            println!("⚠️  No proxy rules configured");
            println!();
            return;
        }

        println!("🔄 Checking remote server connectivity...");
        println!();

        // Test connectivity to remote servers
        for (i, instance) in self.proxy_instances.iter().enumerate() {
            let status = match self.test_remote_connectivity(instance).await {
                Ok(duration) => {
                    format!("✅ Connected ({}ms)", duration.as_millis())
                }
                Err(e) => {
                    format!("❌ Failed: {}", e)
                }
            };

            println!(
                "  {}. {}:{} -> {}:{}{}",
                i + 1,
                self.bind_ip,
                instance.bind_addr.port(),
                instance.remote_addr.ip(),
                instance.remote_addr.port(),
                if let Some(source_ip) = instance.source_ip {
                    format!(" (via {})", source_ip)
                } else {
                    String::new()
                }
            );
            println!("     Status: {}", status);
            println!();
        }

        println!("✅ Server started successfully");
        println!("🛑 Press Ctrl+C to stop the server");
        println!();
    }

    /// Test connectivity to a remote server
    async fn test_remote_connectivity(&self, instance: &ProxyInstance) -> Result<Duration> {
        let start = std::time::Instant::now();

        // Use tokio::time::timeout for the connection attempt
        let connect_result = tokio::time::timeout(Duration::from_secs(3), async {
            // If source IP is specified, we need to use TcpSocket to bind
            if let Some(source_ip) = instance.source_ip {
                let socket = match instance.remote_addr {
                    SocketAddr::V4(_) => TcpSocket::new_v4().map_err(|e| {
                        NetworkError::ConnectionFailed(format!(
                            "Failed to create IPv4 socket: {}",
                            e
                        ))
                    })?,
                    SocketAddr::V6(_) => TcpSocket::new_v6().map_err(|e| {
                        NetworkError::ConnectionFailed(format!(
                            "Failed to create IPv6 socket: {}",
                            e
                        ))
                    })?,
                };

                // Bind to source IP
                let local_addr = SocketAddr::new(source_ip, 0);
                socket.bind(local_addr).map_err(|e| {
                    NetworkError::ConnectionFailed(format!(
                        "Failed to bind to source IP {}: {}",
                        source_ip, e
                    ))
                })?;

                // Connect using the bound socket
                let _stream = socket.connect(instance.remote_addr).await.map_err(|e| {
                    NetworkError::ConnectionFailed(format!("Connection failed: {}", e))
                })?;
            } else {
                // No source IP specified, use TcpStream::connect
                let _stream = tokio::net::TcpStream::connect(instance.remote_addr)
                    .await
                    .map_err(|e| {
                        NetworkError::ConnectionFailed(format!("Connection failed: {}", e))
                    })?;
            }

            Ok::<(), NetworkError>(())
        })
        .await;

        match connect_result {
            Ok(Ok(())) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(NetworkError::ConnectionTimeout.into()),
        }
    }

    /// Get server status
    pub fn get_status(&self) -> ServerStatus {
        ServerStatus {
            bind_ip: self.bind_ip,
            total_instances: self.proxy_instances.len(),
            active_instances: self
                .proxy_instances
                .iter()
                .filter(|i| i.is_running())
                .count(),
        }
    }
}

/// Individual proxy instance
pub struct ProxyInstance {
    bind_addr: SocketAddr,
    remote_addr: SocketAddr,
    source_ip: Option<IpAddr>,
    connection_handler: Arc<StealthConnectionHandler>,
    connection_pool: Option<Arc<ConnectionPool>>,
    running: bool,
}

impl ProxyInstance {
    /// Create a placeholder proxy instance derived from an existing instance (used for moving instances)
    fn placeholder_from(original: &Self) -> Self {
        let bind_addr = SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
        Self {
            bind_addr,
            remote_addr: original.remote_addr,
            source_ip: original.source_ip,
            // Share the existing connection handler so remote metadata remains intact.
            connection_handler: Arc::clone(&original.connection_handler),
            connection_pool: original.connection_pool.clone(),
            running: false,
        }
    }

    /// Create a new proxy instance
    pub fn new(
        bind_ip: IpAddr,
        local_port: u16,
        remote_addr: SocketAddr,
        source_ip: Option<IpAddr>,
        pool_enabled: bool,
        pool_size: usize,
    ) -> Result<Self> {
        let bind_addr = SocketAddr::new(bind_ip, local_port);

        // Create connection pool if enabled
        let connection_pool = if pool_enabled {
            let pool = ConnectionPool::new(remote_addr, source_ip, pool_size);
            Some(Arc::new(pool))
        } else {
            None
        };

        // Create stealth connection handler with blackhole capabilities
        let connection_handler = Arc::new(StealthConnectionHandler::new(
            remote_addr,
            source_ip,
            connection_pool.clone(),
        ));

        Ok(Self {
            bind_addr,
            remote_addr,
            source_ip,
            connection_handler,
            connection_pool,
            running: false,
        })
    }

    /// Start the proxy instance
    pub async fn start(&mut self, mut shutdown_rx: broadcast::Receiver<()>) -> Result<()> {
        self.running = true;

        // Start connection pool if enabled
        if let Some(pool) = &self.connection_pool {
            debug_println!(
                "Starting connection pool for {}:{} (size: {})",
                self.bind_addr,
                pool.get_stats().total_created,
                ""
            );
            pool.start().await?;
            let stats = pool.get_stats();
            debug_println!(
                "Connection pool preheated: {} connections created",
                stats.total_created
            );
        }

        // Create optimized TCP listener
        let listener = self.create_optimized_listener().await?;
        let _local_addr = listener.local_addr().map_err(|e| {
            NetworkError::ListenFailed(format!("Failed to get local address: {}", e))
        })?;

        loop {
            tokio::select! {
                // Accept new connection
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let handler = self.connection_handler.clone();
                            let remote_addr = handler.remote_addr();

                            debug_println!("Accepted connection from {} on {} -> {}", addr, self.bind_addr, remote_addr);

                            // Spawn connection handler for unlimited concurrency (only hardware limits)
                            tokio::spawn(async move {
                                debug_println!("Starting connection handler for {} -> {}", addr, remote_addr);

                                // Ensure the connection handler runs with proper error handling
                                let result = handler.handle_connection(stream).await;
                                match result {
                                    Ok(()) => {
                                        debug_println!("Connection handler completed successfully for {}", addr);
                                    }
                                    Err(e) => {
                                        error_println!("Connection handler failed for {}: {:?}", addr, e);
                                    }
                                }
                            });
                        }
                        Err(err) => {
                            error_println!(
                                "Failed to accept incoming connection on {}: {}",
                                self.bind_addr,
                                err
                            );
                        }
                    }
                }

                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }

        self.running = false;
        Ok(())
    }

    /// Create optimized TCP listener
    async fn create_optimized_listener(&self) -> Result<TcpListener> {
        let domain = match self.bind_addr {
            SocketAddr::V4(_) => Domain::IPV4,
            SocketAddr::V6(_) => Domain::IPV6,
        };

        // Create standard socket
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

        // Enable address reuse before binding
        socket
            .set_reuse_address(true)
            .map_err(|e| NetworkError::InvalidSocketOption(format!("set_reuse_address: {}", e)))?;

        #[cfg(target_os = "linux")]
        {
            // Enable port reuse on Linux
            let _ = socket.set_reuse_port(true);
        }

        // Bind to address
        socket.bind(&self.bind_addr.into()).map_err(|e| {
            NetworkError::ListenFailed(format!("Failed to bind to {}: {}", self.bind_addr, e))
        })?;

        // Start listening
        socket
            .listen(1024) // Backlog of 1024 connections
            .map_err(|e| {
                NetworkError::ListenFailed(format!("Failed to listen on {}: {}", self.bind_addr, e))
            })?;

        // Set socket to non-blocking mode before converting to Tokio
        socket
            .set_nonblocking(true)
            .map_err(|e| NetworkError::ListenFailed(format!("set_nonblocking: {}", e)))?;

        // Convert to tokio TcpListener
        let listener = TcpListener::from_std(socket.into()).map_err(|e| {
            NetworkError::ListenFailed(format!("Failed to create TcpListener: {}", e))
        })?;

        Ok(listener)
    }

    /// Check if the instance is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get connection statistics
    pub async fn get_connection_stats(
        &self,
    ) -> crate::proxy::stealth_connection_handler::StealthConnectionStats {
        self.connection_handler.get_connection_stats().await
    }

    /// Get remote address
    pub fn get_remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Get source IP
    pub fn get_source_ip(&self) -> Option<IpAddr> {
        self.source_ip
    }

    /// Get bind address
    pub fn get_bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }
}

/// Server status information
#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub bind_ip: IpAddr,
    pub total_instances: usize,
    pub active_instances: usize,
}

impl std::fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Proxy Server Status:\n\
                  - Bind IP: {}\n\
                  - Total Instances: {}\n\
                  - Active Instances: {}",
            self.bind_ip, self.total_instances, self.active_instances
        )
    }
}

/// Proxy server builder for easy configuration
pub struct ProxyServerBuilder {
    bind_ip: Option<IpAddr>,
    proxy_configs: Vec<ProxySection>,
}

impl ProxyServerBuilder {
    /// Create a new proxy server builder
    pub fn new() -> Self {
        Self {
            bind_ip: None,
            proxy_configs: Vec::new(),
        }
    }

    /// Set bind IP address
    pub fn bind_ip(mut self, ip: IpAddr) -> Self {
        self.bind_ip = Some(ip);
        self
    }

    /// Add proxy configuration
    pub fn add_proxy(mut self, config: ProxySection) -> Self {
        self.proxy_configs.push(config);
        self
    }

    /// Add multiple proxy configurations
    pub fn add_proxies(mut self, configs: Vec<ProxySection>) -> Self {
        self.proxy_configs.extend(configs);
        self
    }

    /// Build the proxy server
    pub fn build(self) -> Result<ProxyServer> {
        let bind_ip = self.bind_ip.unwrap_or_else(|| {
            // Default to loopback when configuration omits the bind address.
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        });

        let mut server = ProxyServer::new(bind_ip);

        for config in self.proxy_configs {
            server.add_proxy(&config)?;
        }

        Ok(server)
    }
}

impl Default for ProxyServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_proxy_server_creation() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let server = ProxyServer::new(ip);
        assert_eq!(server.get_status().bind_ip, ip);
        assert_eq!(server.get_status().total_instances, 0);
    }

    #[test]
    fn test_proxy_instance_creation() {
        let bind_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let remote_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080);
        let source_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 50));

        let instance = ProxyInstance::new(bind_ip, 8080, remote_addr, Some(source_ip), false, 5).unwrap();

        assert_eq!(instance.get_bind_addr().ip(), bind_ip);
        assert_eq!(instance.get_bind_addr().port(), 8080);
        assert_eq!(instance.get_remote_addr(), remote_addr);
        assert_eq!(instance.get_source_ip(), Some(source_ip));
        assert!(!instance.is_running());
    }

    #[test]
    fn test_proxy_server_builder() {
        let bind_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let proxy_config = ProxySection {
            local_port: 8080,
            remote_host: "192.168.1.100".to_string(),
            remote_port: 8080,
            source_ip: None,
            pool_enabled: None,
            pool_size: None,
        };

        let server = ProxyServerBuilder::new()
            .bind_ip(bind_ip)
            .add_proxy(proxy_config)
            .build()
            .unwrap();

        let status = server.get_status();
        assert_eq!(status.bind_ip, bind_ip);
        assert_eq!(status.total_instances, 1);
    }

    #[test]
    fn test_server_status_display() {
        let status = ServerStatus {
            bind_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            total_instances: 3,
            active_instances: 2,
        };

        let display = format!("{}", status);
        assert!(display.contains("127.0.0.1"));
        assert!(display.contains("3"));
        assert!(display.contains("2"));
    }

    #[test]
    fn test_proxy_instance_ipv6() {
        let bind_ip = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
        let remote_addr = SocketAddr::new(
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            8080,
        );

        let instance = ProxyInstance::new(bind_ip, 8080, remote_addr, None, false, 5).unwrap();

        assert_eq!(instance.get_bind_addr().ip(), bind_ip);
        assert_eq!(instance.get_remote_addr(), remote_addr);
    }

    #[test]
    fn test_proxy_server_builder_default() {
        let builder = ProxyServerBuilder::default();
        assert!(builder.bind_ip.is_none());
        assert!(builder.proxy_configs.is_empty());
    }

    #[test]
    fn test_proxy_server_without_bind_ip() {
        let server = ProxyServerBuilder::new()
            .build()
            .expect("Builder should default to localhost bind");
        assert_eq!(server.get_status().bind_ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
}
