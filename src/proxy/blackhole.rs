use crate::debug_println;
use crate::error::types::ProxyError;
use std::io::ErrorKind;
/// Blackhole mode for handling server rejections
/// When server resets connection, keep client connection open for 2-30 minutes
/// to confuse active probing attempts
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::tcp::{ReadHalf, WriteHalf};
use tokio::time;

/// Check if an error is a connection reset
pub fn is_connection_reset(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::ConnectionReset | ErrorKind::ConnectionRefused | ErrorKind::ConnectionAborted
    )
}

/// Generate a simple pseudo-random number using system time
pub fn pseudo_random_range(min: u64, max: u64) -> u64 {
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

/// Enter blackhole mode - keep connection alive but discard all data
/// Returns Ok(()) so the handler doesn't return an error
pub async fn enter_blackhole_mode(
    mut client_read: ReadHalf<'_>,
    _client_write: WriteHalf<'_>,
    client_addr: std::net::SocketAddr,
) -> Result<(), ProxyError> {
    // Random delay between 2-30 minutes (120-1800 seconds)
    let delay_seconds = pseudo_random_range(120, 1800);
    let delay = Duration::from_secs(delay_seconds);

    debug_println!(
        "🕳️  Entering blackhole mode for {} - will close after {} seconds",
        client_addr,
        delay_seconds
    );

    let mut buffer = vec![0u8; 8192];
    let deadline = time::Instant::now() + delay;

    // Keep reading and discarding data until timeout
    while time::Instant::now() < deadline {
        // Set a read timeout to check deadline periodically
        let read_timeout = time::sleep_until(deadline);

        tokio::select! {
            result = client_read.read(&mut buffer) => {
                match result {
                    Ok(0) => {
                        // Client closed connection
                        debug_println!("Blackhole: client closed connection");
                        break;
                    }
                    Ok(n) => {
                        // Data received and discarded
                        debug_println!("Blackhole: discarded {} bytes from {}", n, client_addr);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        // Continue
                    }
                    Err(_) => {
                        // Read error, exit blackhole
                        break;
                    }
                }
            }
            _ = read_timeout => {
                // Timeout reached
                debug_println!("⏱️  Blackhole timeout for {} - closing connection", client_addr);
                break;
            }
        }
    }

    debug_println!("🕳️  Blackhole mode ended for {}", client_addr);

    // Return success so the overall handler doesn't log an error
    Ok(())
}

/// Enhanced version with better resource management
pub async fn enter_blackhole_mode_enhanced(
    client_stream: &mut tokio::net::TcpStream,
    client_addr: std::net::SocketAddr,
) -> Result<(), ProxyError> {
    // Random delay between 2-30 minutes
    let delay_seconds = pseudo_random_range(120, 1800);
    let delay = Duration::from_secs(delay_seconds);

    debug_println!(
        "🕳️  Enhanced blackhole activated for {} - duration: {}s",
        client_addr,
        delay_seconds
    );

    let mut buffer = [0u8; 4096];
    let start_time = time::Instant::now();

    while start_time.elapsed() < delay {
        let remaining = delay - start_time.elapsed();

        tokio::select! {
            result = client_stream.read(&mut buffer) => {
                match result {
                    Ok(0) => {
                        debug_println!("Blackhole: client disconnected");
                        break;
                    }
                    Ok(n) => {
                        debug_println!("Blackhole: absorbed {} bytes", n);
                        // Data is automatically discarded
                    }
                    Err(e) if is_connection_reset(&e) => {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_reset_detection() {
        let reset_err = std::io::Error::new(ErrorKind::ConnectionReset, "test");
        let refused_err = std::io::Error::new(ErrorKind::ConnectionRefused, "test");
        let other_err = std::io::Error::new(ErrorKind::Other, "test");

        assert!(is_connection_reset(&reset_err));
        assert!(is_connection_reset(&refused_err));
        assert!(!is_connection_reset(&other_err));
    }
}
