// 快速修复补丁：解决 GSC-FQ 代理服务器卡住问题
//
// 使用方法：将 src/proxy/handler.rs 中的 forward_data 函数替换为以下版本

use crate::error::{NetworkError, ProxyError, Result};
use crate::{debug_println, error_println};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::select;

// 将原来的 forward_data 函数替换为这个版本：
async fn forward_data(&self, mut client: TcpStream, mut remote: TcpStream) -> Result<()> {
    // 使用较短的超时时间，避免连接卡住太久
    const READ_TIMEOUT: Duration = Duration::from_secs(30);  // 30秒读超时
    const WRITE_TIMEOUT: Duration = Duration::from_secs(10); // 10秒写超时
    const IDLE_TIMEOUT: Duration = Duration::from_secs(60);  // 60秒空闲超时

    // 禁用 Nagle 算法以降低延迟
    client.set_nodelay(true).map_err(|e| {
        ProxyError::ForwardingFailed(format!("Failed to set client nodelay: {}", e))
    })?;
    remote.set_nodelay(true).map_err(|e| {
        ProxyError::ForwardingFailed(format!("Failed to set remote nodelay: {}", e))
    })?;

    let (mut client_read, mut client_write) = client.split();
    let (mut remote_read, mut remote_write) = remote.split();

    let mut buffer = vec![0u8; 8192]; // 8KB 缓冲区
    let mut last_activity = std::time::Instant::now();

    debug_println!("Starting improved data forwarding with timeouts");

    loop {
        select! {
            // 客户端到远程服务器的数据传输
            result = tokio::time::timeout(READ_TIMEOUT, client_read.read(&mut buffer)) => {
                match result {
                    Ok(Ok(0)) => {
                        debug_println!("Client closed connection");
                        break;
                    }
                    Ok(Ok(n)) => {
                        last_activity = std::time::Instant::now();
                        debug_println!("Read {} bytes from client", n);

                        match tokio::time::timeout(WRITE_TIMEOUT, remote_write.write_all(&buffer[..n])).await {
                            Ok(Ok(())) => {
                                debug_println!("Wrote {} bytes to remote", n);
                            }
                            Ok(Err(e)) => {
                                error_println!("Failed to write to remote: {}", e);
                                return Err(ProxyError::ForwardingFailed(format!("Remote write failed: {}", e)).into());
                            }
                            Err(_) => {
                                error_println!("Write to remote timed out after {:?}", WRITE_TIMEOUT);
                                return Err(ProxyError::ForwardingFailed("Remote write timeout".to_string()).into());
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
                            debug_println!("Client read timeout, checking idle time");
                            if last_activity.elapsed() > IDLE_TIMEOUT {
                                error_println!("Connection idle timeout after {:?}", IDLE_TIMEOUT);
                                break;
                            }
                            continue;
                        } else {
                            error_println!("Client read error: {}", e);
                            break;
                        }
                    }
                    Err(_) => {
                        error_println!("Client read timed out after {:?}", READ_TIMEOUT);
                        if last_activity.elapsed() > IDLE_TIMEOUT {
                            error_println!("Connection idle timeout after {:?}", IDLE_TIMEOUT);
                            break;
                        }
                        continue;
                    }
                }
            }

            // 远程服务器到客户端的数据传输
            result = tokio::time::timeout(READ_TIMEOUT, remote_read.read(&mut buffer)) => {
                match result {
                    Ok(Ok(0)) => {
                        debug_println!("Remote closed connection");
                        break;
                    }
                    Ok(Ok(n)) => {
                        last_activity = std::time::Instant::now();
                        debug_println!("Read {} bytes from remote", n);

                        match tokio::time::timeout(WRITE_TIMEOUT, client_write.write_all(&buffer[..n])).await {
                            Ok(Ok(())) => {
                                debug_println!("Wrote {} bytes to client", n);
                            }
                            Ok(Err(e)) => {
                                error_println!("Failed to write to client: {}", e);
                                return Err(ProxyError::ForwardingFailed(format!("Client write failed: {}", e)).into());
                            }
                            Err(_) => {
                                error_println!("Write to client timed out after {:?}", WRITE_TIMEOUT);
                                return Err(ProxyError::ForwardingFailed("Client write timeout".to_string()).into());
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
                            debug_println!("Remote read timeout, checking idle time");
                            if last_activity.elapsed() > IDLE_TIMEOUT {
                                error_println!("Connection idle timeout after {:?}", IDLE_TIMEOUT);
                                break;
                            }
                            continue;
                        } else {
                            error_println!("Remote read error: {}", e);
                            break;
                        }
                    }
                    Err(_) => {
                        error_println!("Remote read timed out after {:?}", READ_TIMEOUT);
                        if last_activity.elapsed() > IDLE_TIMEOUT {
                            error_println!("Connection idle timeout after {:?}", IDLE_TIMEOUT);
                            break;
                        }
                        continue;
                    }
                }
            }
        }

        // 检查空闲超时
        if last_activity.elapsed() > IDLE_TIMEOUT {
            error_println!("Connection idle timeout after {:?}", IDLE_TIMEOUT);
            break;
        }
    }

    debug_println!("Data forwarding completed successfully");
    Ok(())
}

// 更简单的快速修复：只需要修改超时时间
async fn forward_data_quick_fix(&self, mut client: TcpStream, mut remote: TcpStream) -> Result<()> {
    use tokio::io::copy_bidirectional;

    // 禁用 Nagle 算法
    client.set_nodelay(true).map_err(|e| {
        ProxyError::ForwardingFailed(format!("Failed to set client nodelay: {}", e))
    })?;
    remote.set_nodelay(true).map_err(|e| {
        ProxyError::ForwardingFailed(format!("Failed to set remote nodelay: {}", e))
    })?;

    // 使用较短的超时时间（60秒而不是5分钟）
    match tokio::time::timeout(
        Duration::from_secs(60), // 60秒超时，而不是原来的300秒
        copy_bidirectional(&mut client, &mut remote)
    ).await {
        Ok(result) => {
            result.map_err(|e| {
                ProxyError::ForwardingFailed(format!("Bidirectional copy failed: {}", e))
            })?;
        }
        Err(_) => {
            error_println!("Data forwarding timeout after 60 seconds, closing connection");
            return Err(ProxyError::ForwardingFailed(
                "Data forwarding timeout".to_string()
            ).into());
        }
    }

    Ok(())
}

// 使用说明：
//
// 选项1 - 快速修复（推荐）：
// 在 handler.rs 的 forward_data 函数中，将第171行的
// Duration::from_secs(300) 改为 Duration::from_secs(60)
//
// 选项2 - 完整修复：
// 将整个 forward_data 函数替换为上面实现的改进版本
//
// 选项3 - 配置化修复：
// 添加超时配置参数，让用户可以根据需要调整超时时间