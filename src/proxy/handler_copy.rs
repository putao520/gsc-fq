/// 使用两个单向 copy 的代理实现
use crate::error::{Result, ProxyError};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::Duration;

/// 使用两个独立的 copy 函数实现双向转发
pub async fn forward_data_with_two_copies(
    mut client: TcpStream,
    mut remote: TcpStream,
) -> Result<()> {
    // 分离读写
    let (client_read, mut client_write) = client.split();
    let (remote_read, mut remote_write) = remote.split();

    // 设置 nodelay 确保低延迟
    client_write.set_nodelay(true)
        .map_err(|e| ProxyError::ForwardingFailed(format!("Failed to set client nodelay: {}", e)))?;
    remote_write.set_nodelay(true)
        .map_err(|e| ProxyError::ForwardingFailed(format!("Failed to set remote nodelay: {}", e)))?;

    // 方向1：客户端 -> 服务器（使用单向 copy）
    let client_to_remote = tokio::spawn(async move {
        let bytes_copied = io::copy(&mut client_read, &mut remote_write).await
            .map_err(|e| ProxyError::ForwardingFailed(format!("Client to remote copy failed: {}", e)))?;

        // 复制完成后优雅关闭写入端
        let _ = remote_write.shutdown().await;

        Ok(bytes_copied)
    });

    // 方向2：服务器 -> 客户端（使用单向 copy）
    let remote_to_client = tokio::spawn(async move {
        let bytes_copied = io::copy(&mut remote_read, &mut client_write).await
            .map_err(|e| ProxyError::ForwardingFailed(format!("Remote to client copy failed: {}", e)))?;

        // 复制完成后优雅关闭写入端
        let _ = client_write.shutdown().await;

        Ok(bytes_copied)
    });

    // 等待两个方向完成
    match tokio::time::timeout(
        Duration::from_secs(30),
        tokio::join!(client_to_remote, remote_to_client)
    ).await {
        Ok((client_result, remote_result)) => {
            match (client_result, remote_result) {
                (Ok(client_bytes), Ok(remote_bytes)) => {
                    println!("✅ Forwarding completed - Client->Remote: {} bytes, Remote->Client: {} bytes",
                            client_bytes.unwrap_or(0), remote_bytes.unwrap_or(0));
                }
                (Err(e), _) | (_, Err(e)) => {
                    eprintln!("❌ Forwarding task failed: {:?}", e);
                    return Err(ProxyError::ForwardingFailed("Task failed".to_string()));
                }
            }
        }
        Err(_) => {
            eprintln!("⏰ Forwarding timeout after 30 seconds");
            // 超时不算错误，可能是长连接
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use std::time::Duration;

    #[tokio::test]
    async fn test_two_copy_solution() {
        // 创建一个测试服务器
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        // 启动服务器任务
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut read, mut write) = stream.into_split();

            // 立即发送欢迎消息
            write.write_all(b"Welcome from server!\n").await.unwrap();
            write.flush().await.unwrap();

            // Echo 服务器
            let mut buf = [0; 1024];
            loop {
                match read.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        write.write_all(&buf[..n]).await.unwrap();
                        write.flush().await.unwrap();
                    }
                    Err(_) => break,
                }
            }
        });

        // 客户端连接
        let client = TcpStream::connect(server_addr).await.unwrap();

        // 使用两个 copy 的方案
        let (client_read, mut client_write) = client.into_split();

        // 模拟另一个客户端
        let (fake_server_read, fake_server_write) = tokio::io::duplex(1024);

        let forward_task = tokio::spawn(async move {
            let client_stream = TcpStream::from_split(client_read);
            forward_data_with_two_copies(client_stream, TcpStream::from_split(fake_server_write)).await
        });

        // 读取服务器的欢迎消息
        let mut buf = [0; 1024];
        let n = fake_server_read.read(&mut buf).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&buf[..n]), "Welcome from server!\n");

        // 清理
        server_task.abort();
        forward_task.abort();
    }
}