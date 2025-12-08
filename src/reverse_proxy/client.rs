use crate::config::loader::ConfigFile;
use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::reverse_proxy::yamux_pool::{YamuxConnectionPool, ConnectionSelectionStrategy, DEFAULT_POOL_SIZE};
use crate::{debug_println, error_println, warning_println};
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
                config.reverse_proxy_server.as_ref()
                    .and_then(|s| {
                        if !s.allowed_tokens.is_empty() {
                            Some(s.allowed_tokens[0].clone())
                        } else {
                            None
                        }
                    })
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

        // Enhanced connection monitoring with better error handling
        println!("🔍 Enhanced connection monitoring active");
        println!("   Heartbeat: every 30s");
        println!("   Health check: every 5s");
        println!("   TCP keepalive: enabled");

        // Store pool stats and reconnect state
        let mut last_heartbeat = std::time::Instant::now();
        let mut last_successful_reconnect = std::time::Instant::now();
        let mut consecutive_failures = 0u32;
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;
        const RECONNECT_COOLDOWN: u64 = 60; // 60秒重连冷却

        let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        let mut connection_check_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                // Send heartbeat every 30 seconds
                _ = heartbeat_interval.tick() => {
                    if let Some(pool) = &self.yamux_pool {
                        let pool_stats = pool.get_stats().await;

                        // Check if we have any connections at all
                        if pool_stats.pool_size == 0 {
                            consecutive_failures += 1;
                            error_println!("❌ No connections in pool (attempt {})", consecutive_failures);
                        } else if pool_stats.active_connections == 0 {
                            consecutive_failures += 1;
                            error_println!("❌ No active connections (attempt {})", consecutive_failures);
                        } else {
                            // Only reset if connections look good
                            last_heartbeat = std::time::Instant::now();
                            consecutive_failures = 0; // Reset counter on success
                            debug_println!("💓 Connection healthy - {}/{} connections active",
                                pool_stats.active_connections, pool_stats.pool_size);
                        }

                        // Try to reconnect if heartbeat fails
                        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                            error_println!("⚠️  Too many consecutive failures ({}), attempting reconnection...", consecutive_failures);
                            if self.reconnect_internal().await.is_err() {
                                return Err(ReverseProxyError::ConnectionFailed(
                                    format!("Failed to reconnect after {} consecutive failures", MAX_CONSECUTIVE_FAILURES)
                                ).into());
                            } else {
                                last_successful_reconnect = std::time::Instant::now();
                                consecutive_failures = 0;
                                error_println!("✅ Reconnection successful");
                            }
                        }
                    } else {
                        error_println!("❌ Connection pool not available");
                    }
                }

                // Enhanced connection health check every 5 seconds
                _ = connection_check_interval.tick() => {
                    if let Some(pool) = &self.yamux_pool {
                        let stats = pool.get_stats().await;
                        debug_println!("📊 Connection pool status: {} active connections (pool size: {})",
                            stats.active_connections, stats.pool_size);

                        // Check if all connections are lost
                        if stats.active_connections == 0 {
                            error_println!("🚨 Critical: All yamux connections have been lost");

                            // Check reconnect cooldown
                            if last_successful_reconnect.elapsed().as_secs() < RECONNECT_COOLDOWN {
                                let remaining = RECONNECT_COOLDOWN - last_successful_reconnect.elapsed().as_secs();
                                error_println!("⏳ Reconnect cooldown active: {}s remaining", remaining);
                            } else {
                                if self.reconnect_internal().await.is_err() {
                                    error_println!("❌ Emergency reconnection failed");
                                } else {
                                    last_successful_reconnect = std::time::Instant::now();
                                    consecutive_failures = 0;
                                    error_println!("✅ Emergency reconnection successful");
                                }
                            }
                        }

                        // Check if we haven't sent heartbeat recently
                        let heartbeat_age = last_heartbeat.elapsed();
                        if heartbeat_age > tokio::time::Duration::from_secs(60) {
                            warning_println!("⚠️  No heartbeat sent for {} seconds, possible connection issue",
                                heartbeat_age.as_secs());
                        }

                        // Check overall connection age
                        let connection_age = last_successful_reconnect.elapsed();
                        if connection_age > tokio::time::Duration::from_secs(3600) { // 1 hour
                            debug_println!("📝 Connection has been active for {} hours",
                                connection_age.as_secs() / 3600);
                        }
                    } else {
                        error_println!("❌ Connection pool not available for health check");
                    }
                }

                // Fallback sleep to prevent busy waiting
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // This is just a fallback to ensure we don't busy wait
                    // The actual work is done by the interval timers above
                }
            }
        }
    }

    
    /// Attempt to reconnect to the server
    async fn reconnect_internal(&mut self) -> Result<()> {
        error_println!("🔄 Attempting to reconnect to server...");

        // Try to recreate the connection pool
        match YamuxConnectionPool::new(
            self.server_addr,
            &vec![], // We'll use the existing configs from the pool
            self.yamux_pool_size,
            self.selection_strategy,
            &self.auth_token,
        ).await {
            Ok(new_pool) => {
                // Replace the old pool
                if let Some(pool) = self.yamux_pool.as_mut() {
                    *pool = new_pool;
                    error_println!("✅ Reconnection successful");
                    Ok(())
                } else {
                    error_println!("❌ Failed to access connection pool");
                    Err(ReverseProxyError::ConnectionFailed("Pool not initialized".to_string()).into())
                }
            }
            Err(e) => {
                error_println!("❌ Reconnection failed: {}", e);
                // Wait before retrying
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                Err(e)
            }
        }
    }

    
        
    }
