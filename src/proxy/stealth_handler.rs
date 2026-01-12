use crate::debug_println;
use crate::error::types::ProxyError;
use crate::proxy::ConnectionPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
/// Stealth handler with blackhole mode for hiding protocol signatures
/// Detects server rejections and enters blackhole mode to confuse active probing
use tokio::net::TcpStream;

use crate::proxy::zero_copy::zero_copy_bidirectional;

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
                    debug_println!(
                        "⚠️  Pool acquisition failed: {}, falling back to direct connection",
                        e
                    );
                    // Fall through to direct connection
                }
            }
        }

        // Fallback: 直接建立连接（不测试，避免建立2次连接）
        match TcpStream::connect(remote_addr).await {
            Ok(remote) => {
                debug_println!("Direct connection established to {}", remote_addr);
                Self::normal_forwarding(client, remote).await
            }
            Err(e) => {
                // 检查是否是拒绝连接
                let error_msg = e.to_string();
                if Self::is_io_rejection(&e) || Self::is_connection_error_rejection(&error_msg) {
                    debug_println!("🕳️  Server rejection detected, entering blackhole mode");
                    Self::enter_blackhole_mode(client).await
                } else {
                    Err(ProxyError::ForwardingFailed(format!("Connection failed: {}", e)))
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

    /// 检查连接错误消息是否表示拒绝
    fn is_connection_error_rejection(error_msg: &str) -> bool {
        error_msg.contains("refused")
            || error_msg.contains("reset")
            || error_msg.contains("rejected")
            || error_msg.contains("timeout")
            || error_msg.contains("connection")
    }

    /// Enter blackhole mode - keep client connected but discard all data
    async fn enter_blackhole_mode(mut client: TcpStream) -> Result<(), ProxyError> {
        use tokio::time;

        // Random delay between 5-30 seconds (reduced from minutes to prevent resource exhaustion)
        let delay_seconds = pseudo_random_range(5, 30);
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
            client_addr
                .map(|a| a.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            remote_addr
                .map(|a| a.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );

        // TCP 优化在连接创建时已应用（socket2 层面）

        // 使用平台特定的自适应零拷贝优化
        // - Linux: splice() 系统调用（内核空间零拷贝，预期 +30%）
        // - macOS: bulk_copy (256KB, Benchmark 验证最优: 4.02x)
        // - Windows: bulk_copy (256KB, Benchmark 优化: 512KB 性能差 -21%)
        // - 其他: bulk_copy (256KB 通用优化)
        #[cfg(target_os = "linux")]
        debug_println!("🚀 平台优化: Linux splice() 内核零拷贝 (预期 +30%)");

        #[cfg(target_os = "macos")]
        debug_println!("🚀 平台优化: macOS bulk_copy (256KB, Benchmark: 4.02x)");

        #[cfg(target_os = "windows")]
        debug_println!("🚀 平台优化: Windows bulk_copy (256KB, Benchmark 优化)");

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        debug_println!("🚀 平台优化: 通用 bulk_copy (256KB)");

        let (bytes1, bytes2) = zero_copy_bidirectional(client, remote)
            .await
            .map_err(|e| ProxyError::ForwardingFailed(format!("Zero-copy failed: {}", e)))?;

        debug_println!(
            "📊 转发完成: {} 字节 ↓ / {} 字节 ↑",
            bytes1,
            bytes2
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
