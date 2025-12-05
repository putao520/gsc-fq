use crate::debug_println;
use crate::error::types::ProxyError;
use crate::proxy::ConnectionPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{copy, AsyncReadExt};
/// Stealth handler with blackhole mode for hiding protocol signatures
/// Detects server rejections and enters blackhole mode to confuse active probing
use tokio::net::TcpStream;

/// Generate a simple pseudo-random number using system time
fn pseudo_random_range(min: u64, max: u64) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    // Simple linear congruential generator
    let a: u64 = 1664525;
    let c: u64 = 1013904223;
    let m: u64 = 2_u64.pow(32);
    let result = (a.wrapping_mul(seed).wrapping_add(c)) % m;

    // Map to range
    min + (result % (max - min + 1))
}

/// Stealth handler that detects rejections and enters blackhole mode
pub struct StealthHandler;

impl StealthHandler {
    /// Handle connection with stealth capabilities
    pub async fn handle_stealth(
        client: TcpStream,
        remote_addr: std::net::SocketAddr,
        connection_pool: Option<Arc<ConnectionPool>>,
    ) -> Result<(), ProxyError> {
        // Try to acquire connection from pool first
        if let Some(pool) = connection_pool {
            match pool.acquire().await {
                Ok(remote) => {
                    debug_println!("✅ Acquired connection from pool");
                    return Self::normal_forwarding(client, remote).await;
                }
                Err(e) => {
                    debug_println!("⚠️  Pool acquisition failed: {}, falling back to direct connection", e);
                    // Fall through to direct connection
                }
            }
        }

        // Fallback: test the remote connection
        match Self::test_remote_connection(remote_addr).await {
            Ok(()) => {
                // Remote is responsive, use normal forwarding
                debug_println!("Remote server responsive, using normal forwarding");

                // Re-establish connection for forwarding
                let remote = TcpStream::connect(remote_addr).await.map_err(|e| {
                    ProxyError::ForwardingFailed(format!("Connection failed: {}", e))
                })?;
                Self::normal_forwarding(client, remote).await
            }
            Err(e) => {
                // Check if it's a rejection
                if Self::is_rejection(&e) {
                    debug_println!("🕳️  Server rejection detected, entering blackhole mode");
                    Self::enter_blackhole_mode(client).await
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Test if remote server accepts connections (without sending data to avoid WAF triggers)
    async fn test_remote_connection(remote_addr: std::net::SocketAddr) -> Result<(), ProxyError> {
        // 只建立连接，不发送任何数据（避免触发 WAF/IDS）
        let _test_stream =
            tokio::time::timeout(Duration::from_millis(500), TcpStream::connect(remote_addr))
                .await
                .map_err(|_| ProxyError::ForwardingFailed("Connection timeout".to_string()))?
                .map_err(|e| ProxyError::ForwardingFailed(format!("Connection failed: {}", e)))?;

        // 连接成功即可，不需要发送测试数据
        Ok(())
    }

    /// Check if error indicates server rejection
    fn is_rejection(err: &ProxyError) -> bool {
        match err {
            ProxyError::ForwardingFailed(msg)
                if msg.contains("refused")
                    || msg.contains("reset")
                    || msg.contains("rejected")
                    || msg.contains("timeout") =>
            {
                true
            }
            _ => false,
        }
    }

    /// Check if IO error indicates rejection
    fn is_io_rejection(err: &std::io::Error) -> bool {
        matches!(
            err.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
        )
    }

    /// Enter blackhole mode - keep client connected but discard all data
    async fn enter_blackhole_mode(mut client: TcpStream) -> Result<(), ProxyError> {
        use tokio::time;

        // Random delay between 2-30 minutes
        let delay_seconds = pseudo_random_range(120, 1800);
        let delay = Duration::from_secs(delay_seconds);

        let client_addr = client
            .peer_addr()
            .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
        debug_println!(
            "🕳️  Blackhole activated for {} - duration: {}s",
            client_addr,
            delay_seconds
        );

        let mut buffer = [0u8; 4096];
        let start_time = time::Instant::now();

        while start_time.elapsed() < delay {
            let remaining = delay - start_time.elapsed();

            tokio::select! {
                result = client.read(&mut buffer) => {
                    match result {
                        Ok(0) => {
                            debug_println!("Blackhole: client disconnected");
                            break;
                        }
                        Ok(n) => {
                            debug_println!("Blackhole: absorbed {} bytes", n);
                            // Data is automatically discarded
                        }
                        Err(e) if Self::is_io_rejection(&e) => {
                            debug_println!("Blackhole: client reset connection");
                            break;
                        }
                        Err(_) => break,
                    }
                }
                _ = time::sleep(remaining) => {
                    debug_println!("Blackhole: timeout reached");
                    break;
                }
            }
        }

        debug_println!("🕳️  Blackhole mode ended");
        Ok(())
    }

    /// Normal bidirectional forwarding
    async fn normal_forwarding(client: TcpStream, remote: TcpStream) -> Result<(), ProxyError> {
        // Get connection addresses for debug logging
        let client_addr = client.peer_addr().ok();
        let remote_addr = remote.peer_addr().ok();

        debug_println!(
            "📡 Starting forwarding: {} <-> {}",
            client_addr.map(|a| a.to_string()).unwrap_or_else(|| "unknown".to_string()),
            remote_addr.map(|a| a.to_string()).unwrap_or_else(|| "unknown".to_string())
        );

        let (mut client_read, mut client_write) = client.into_split();
        let (mut remote_read, mut remote_write) = remote.into_split();

        // Format addresses for logging
        let client_addr_str = client_addr.map(|a| a.to_string()).unwrap_or_else(|| "unknown".to_string());
        let remote_addr_str = remote_addr.map(|a| a.to_string()).unwrap_or_else(|| "unknown".to_string());

        let client_to_remote = {
            let client_addr = client_addr_str.clone();
            let remote_addr = remote_addr_str.clone();
            tokio::spawn(async move {
                match copy(&mut client_read, &mut remote_write).await {
                    Ok(bytes) => {
                        debug_println!("✅ {} → {} | {} bytes", client_addr, remote_addr, bytes);
                    }
                    Err(e) => {
                        debug_println!("❌ {} → {} | {}", client_addr, remote_addr, e);
                    }
                }
            })
        };

        let remote_to_client = {
            let client_addr = client_addr_str.clone();
            let remote_addr = remote_addr_str.clone();
            tokio::spawn(async move {
                match copy(&mut remote_read, &mut client_write).await {
                    Ok(bytes) => {
                        debug_println!("✅ {} ← {} | {} bytes", client_addr, remote_addr, bytes);
                    }
                    Err(e) => {
                        debug_println!("❌ {} ← {} | {}", client_addr, remote_addr, e);
                    }
                }
            })
        };

        // Wait for both directions
        let _ = tokio::join!(client_to_remote, remote_to_client);

        debug_println!(
            "📊 Connection closed: {} ↔ {}",
            client_addr_str,
            remote_addr_str
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejection_detection() {
        let reset_err = ProxyError::ForwardingFailed("connection refused".to_string());
        let timeout_err = ProxyError::ForwardingFailed("Server timeout".to_string());
        let other_err = ProxyError::ForwardingFailed("unknown error".to_string());

        assert!(StealthHandler::is_rejection(&reset_err));
        assert!(StealthHandler::is_rejection(&timeout_err));
        assert!(!StealthHandler::is_rejection(&other_err));
    }
}
