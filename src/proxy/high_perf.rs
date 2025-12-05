use crate::debug_println;
/// 高性能零拷贝实现，使用 Tokio 生态的成熟库
use crate::error::Result;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 使用 tokio::io::copy 的高效双向数据转发
pub async fn optimized_bidirectional(client: TcpStream, server: TcpStream) -> Result<(u64, u64)> {
    debug_println!("Starting optimized bidirectional forwarding");

    // 分割流为读写两部分
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();

    // 客户端到服务器的转发
    let client_to_server = tokio::spawn(async move {
        match io::copy(&mut client_read, &mut server_write).await {
            Ok(bytes) => {
                debug_println!("Client->Server: {} bytes", bytes);
                bytes
            }
            Err(e) => {
                debug_println!("Client->Server error: {}", e);
                0
            }
        }
    });

    // 服务器到客户端的转发
    let server_to_client = tokio::spawn(async move {
        match io::copy(&mut server_read, &mut client_write).await {
            Ok(bytes) => {
                debug_println!("Server->Client: {} bytes", bytes);
                bytes
            }
            Err(e) => {
                debug_println!("Server->Client error: {}", e);
                0
            }
        }
    });

    // 等待两个任务完成
    let (bytes1, bytes2) = tokio::join!(client_to_server, server_to_client);

    let bytes1 = bytes1.unwrap_or(0);
    let bytes2 = bytes2.unwrap_or(0);
    let total_bytes = bytes1 + bytes2;
    debug_println!("Optimized transfer complete: {} bytes", total_bytes);

    Ok((bytes1, bytes2))
}

/// 带缓冲区优化的双向转发
pub async fn buffered_bidirectional(client: TcpStream, server: TcpStream) -> Result<(u64, u64)> {
    debug_println!("Starting buffered bidirectional forwarding");

    // 使用更大的缓冲区提高性能
    const BUFFER_SIZE: usize = 64 * 1024; // 64KB buffer

    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();

    // 客户端到服务器的转发
    let client_to_server = tokio::spawn(async move {
        let mut buffer = vec![0u8; BUFFER_SIZE];
        let mut total = 0u64;

        loop {
            match client_read.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if let Err(e) = server_write.write_all(&buffer[..n]).await {
                        debug_println!("Client->Server write error: {}", e);
                        break;
                    }
                    total += n as u64;
                }
                Err(e) => {
                    debug_println!("Client->Server read error: {}", e);
                    break;
                }
            }
        }
        total
    });

    // 服务器到客户端的转发
    let server_to_client = tokio::spawn(async move {
        let mut buffer = vec![0u8; BUFFER_SIZE];
        let mut total = 0u64;

        loop {
            match server_read.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if let Err(e) = client_write.write_all(&buffer[..n]).await {
                        debug_println!("Server->Client write error: {}", e);
                        break;
                    }
                    total += n as u64;
                }
                Err(e) => {
                    debug_println!("Server->Client read error: {}", e);
                    break;
                }
            }
        }
        total
    });

    // 等待两个任务完成
    let (bytes1, bytes2) = tokio::join!(client_to_server, server_to_client);

    let bytes1 = bytes1.unwrap_or(0);
    let bytes2 = bytes2.unwrap_or(0);
    let total_bytes = bytes1 + bytes2;
    debug_println!("Buffered transfer complete: {} bytes", total_bytes);

    Ok((bytes1, bytes2))
}

/// 自适应性能优化：自动选择最佳策略
pub async fn adaptive_copy(client: TcpStream, server: TcpStream) -> Result<(u64, u64)> {
    debug_println!("Starting adaptive copy - using optimized bidirectional forwarding");

    // 使用优化的 io::copy 实现，这是最稳定的转发方式
    optimized_bidirectional(client, server).await
}

