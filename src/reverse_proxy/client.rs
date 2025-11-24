use crate::config::loader::ConfigFile;
use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::reverse_proxy::yamux_pool::{YamuxConnectionPool, ConnectionSelectionStrategy, DEFAULT_POOL_SIZE};
use crate::{debug_println, error_println};
use std::net::SocketAddr;
use tokio::io::{copy_bidirectional, AsyncReadExt};
use tokio::net::TcpStream;
use tokio_util::compat::FuturesAsyncReadCompatExt;

/// Reverse proxy client
pub struct ReverseProxyClient {
    server_addr: SocketAddr,
    config: ConfigFile,

    /// Yamux连接池
    yamux_pool: Option<YamuxConnectionPool>,

    /// 连接池大小（默认32）
    yamux_pool_size: usize,

    /// 负载均衡策略
    selection_strategy: ConnectionSelectionStrategy,

    /// 认证TOKEN
    auth_token: Option<String>,
}

impl ReverseProxyClient {
    /// Create new reverse proxy client
    pub fn new(server_addr: SocketAddr, config: ConfigFile) -> Self {
        // 从环境变量读取auth_token
        let auth_token = std::env::var("REVERSE_PROXY_TOKEN")
            .ok()
            .or_else(|| {
                // 从配置文件读取token（如果指定）
                config.server.as_ref()
                    .and_then(|s| s.auth_token.clone())
            });

        // 从环境变量或配置读取pool_size，默认32
        let yamux_pool_size = std::env::var("YAMUX_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_POOL_SIZE);

        Self {
            server_addr,
            config,
            yamux_pool: None,
            yamux_pool_size,
            selection_strategy: ConnectionSelectionStrategy::RoundRobin,
            auth_token,
        }
    }

    /// Create new reverse proxy client with custom auth token
    pub fn new_with_token(server_addr: SocketAddr, config: ConfigFile, auth_token: String) -> Self {
        // 从环境变量或配置读取pool_size，默认32
        let yamux_pool_size = std::env::var("YAMUX_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_POOL_SIZE);

        Self {
            server_addr,
            config,
            yamux_pool: None,
            yamux_pool_size,
            selection_strategy: ConnectionSelectionStrategy::RoundRobin,
            auth_token: Some(auth_token),
        }
    }
    
    /// Start the reverse proxy client
    pub async fn start(&mut self) -> Result<()> {
        let mut retry_count = 0u64;
        let mut backoff_seconds = 1u64;
        const MIN_BACKOFF: u64 = 1;
        const MAX_BACKOFF: u64 = 60;
        const MAX_RETRIES: u64 = 10;  // Limit retry attempts to prevent infinite loops

        loop {
            // Check if we've exceeded maximum retry attempts
            if retry_count >= MAX_RETRIES {
                error_println!("Maximum retry attempts ({}) exceeded, giving up", MAX_RETRIES);
                return Err(ReverseProxyError::ConnectionFailed(
                    format!("Failed after {} retry attempts", MAX_RETRIES)
                ).into());
            }

            match self.try_connect_and_run().await {
                Ok(_) => {
                    // Connection completed successfully, exit cleanly
                    println!("✅ Connection completed successfully");
                    return Ok(());
                }
                Err(e) => {
                    retry_count += 1;
                    error_println!("Connection failed (attempt {}): {}", retry_count, e);

                    if retry_count < MAX_RETRIES {
                        println!("🔄 Reconnecting in {} seconds... (attempt {}/{})",
                            backoff_seconds, retry_count, MAX_RETRIES);
                        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)).await;

                        // Exponential backoff with max limit
                        backoff_seconds = (backoff_seconds * 2).min(MAX_BACKOFF);
                    }
                }
            }
        }
    }
    
    
    /// Try to connect to server and run the main loop (extracted for retry logic)
    async fn try_connect_and_run(&mut self) -> Result<()> {
        println!("🔄 Creating Yamux connection pool ({} connections) to {}", 
            self.yamux_pool_size, self.server_addr);
        
        // Convert ReverseProxySection to ReverseProxyConfig
        let mut proxy_configs = Vec::new();
        for rproxy in &self.config.reverse_proxies {
            let server_port = rproxy.get_server_port().map_err(|e| {
                ReverseProxyError::HandshakeFailed(format!("Invalid server config: {}", e))
            })?;
            let local_port = rproxy.get_local_port().map_err(|e| {
                ReverseProxyError::HandshakeFailed(format!("Invalid local config: {}", e))
            })?;
            let local_host = rproxy.get_local_host().unwrap_or_else(|| "localhost".to_string());

            proxy_configs.push(ReverseProxyConfig {
                server_port,
                local_host,
                local_port,
            });
        }
        
        if proxy_configs.is_empty() {
            return Err(ReverseProxyError::HandshakeFailed(
                "No valid reverse proxy configurations".to_string()
            ).into());
        }
        
        // Create Yamux connection pool
        let pool = YamuxConnectionPool::new(
            self.server_addr,
            &proxy_configs,
            self.yamux_pool_size,
            self.selection_strategy,
            &self.auth_token,
        ).await?;
        
        // Display active reverse proxies
        println!("\n📡 Active Reverse Proxies:");
        for config in &proxy_configs {
            println!("   Server:{} → Local:{}:{}",
                config.server_port,
                config.local_host,
                config.local_port
            );
        }
        
        // Display pool stats
        let stats = pool.get_stats().await;
        println!("\n🔗 Connection Pool:");
        println!("   Total connections: {}", stats.pool_size);
        println!("   Active connections: {}", stats.active_connections);
        println!("   Strategy: {:?}", self.selection_strategy);
        println!();
        
        self.yamux_pool = Some(pool);
        
        self.run_session(proxy_configs).await
    }
    
    
    /// Run a single session (separated for cleaner code)
    async fn run_session(&mut self, _proxy_configs: Vec<ReverseProxyConfig>) -> Result<()> {
        println!("✅ Client connected and waiting for data forwarding through reverse proxy");

        // The yamux pool is already handling incoming streams in its background tasks
        // We just need to keep the main client connection alive
        // When the server opens streams to forward external connections,
        // the pool's background tasks will handle them automatically

        loop {
            // Keep the client alive and check connection health
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            // Check if the pool is still active
            let pool = self.yamux_pool.as_ref()
                .ok_or_else(|| ReverseProxyError::ConnectionFailed("Yamux pool not initialized".to_string()))?;

            let stats = pool.get_stats().await;
            debug_println!("Connection pool status: {} active connections", stats.active_connections);

            if stats.active_connections == 0 {
                return Err(ReverseProxyError::ConnectionFailed(
                    "All yamux connections have been lost".to_string()
                ).into());
            }
        }
    }

    
    /// Handle an incoming yamux stream from the server
    async fn handle_incoming_stream(
        yamux_stream: yamux::Stream,
        proxy_configs: Vec<ReverseProxyConfig>,
    ) -> Result<()> {
        let mut yamux_tokio = yamux_stream.compat();

        // Read port header (first 2 bytes)
        let mut port_bytes = [0u8; 2];
        debug_println!("Reading port header from incoming stream...");

        if let Err(e) = yamux_tokio.read_exact(&mut port_bytes).await {
            error_println!("Failed to read port header: {}", e);
            return Err(ReverseProxyError::ConnectionFailed(
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
            return Err(ReverseProxyError::ConnectionFailed(
                format!("Unknown server port: {}", server_port)
            ).into());
        };

        // Handle the stream data forwarding
        Self::handle_stream(yamux_tokio, target).await
    }
    
    /// Handle a single yamux stream
    async fn handle_stream(
        mut yamux_stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        target: ReverseProxyConfig,
    ) -> Result<()> {
        // Connect to local service
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let mut local_stream = TcpStream::connect(&local_addr).await.map_err(|e| {
            ReverseProxyError::ConnectionFailed(format!(
                "Failed to connect to local service {}: {}",
                local_addr, e
            ))
        })?;
        
        debug_println!("Connected to local service: {}", local_addr);
        
        // Bidirectional copy
        match copy_bidirectional(&mut yamux_stream, &mut local_stream).await {
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
