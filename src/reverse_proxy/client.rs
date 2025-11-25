use crate::config::loader::ConfigFile;
use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::reverse_proxy::yamux_pool::{YamuxConnectionPool, ConnectionSelectionStrategy, DEFAULT_POOL_SIZE};
use crate::{debug_println, error_println};
use std::net::SocketAddr;

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
        #[allow(dead_code)] const MIN_BACKOFF: u64 = 1;  // Currently unused but kept for future retry logic
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
            let server_host = rproxy.get_server_ip();  // 获取服务器绑定IP
            let local_port = rproxy.get_local_port().map_err(|e| {
                ReverseProxyError::HandshakeFailed(format!("Invalid local config: {}", e))
            })?;
            let local_host = rproxy.get_local_host().unwrap_or_else(|| "localhost".to_string());

            println!("🔧 Client sending config: server_port={}, server_host={:?}, local={}:{}",
                server_port, server_host, local_host, local_port);

            proxy_configs.push(ReverseProxyConfig {
                server_port,
                server_host,
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

        let pool = self.yamux_pool.as_mut()
            .ok_or_else(|| ReverseProxyError::ConnectionFailed("Yamux pool not initialized".to_string()))?;

        loop {
            // Check connection health
            let stats = pool.get_stats().await;
            debug_println!("Connection pool status: {} active connections", stats.active_connections);

            if stats.active_connections == 0 {
                return Err(ReverseProxyError::ConnectionFailed(
                    "All yamux connections have been lost".to_string()
                ).into());
            }

            // Keep the client alive and handle potential incoming streams
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    
        
    }
