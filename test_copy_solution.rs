// 测试使用两个 copy 的方案
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy};
use std::time::Duration;

async fn test_copy_solution() {
    println!("Testing two-copy solution...");

    // 模拟客户端和服务器的读写半
    let (mut client_read, mut client_write) = tokio::io::duplex(1024);
    let (mut server_read, mut server_write) = tokio::io::duplex(1024);

    // 方案1：两个独立的 copy
    let client_to_server = tokio::spawn(async move {
        let result = copy(&mut client_read, &mut server_write).await;
        println!("Client->Server: {} bytes", result.unwrap_or(0));
    });

    let server_to_client = tokio::spawn(async move {
        // 服务器立即发送数据
        tokio::time::sleep(Duration::from_millis(100)).await;
        server_write.write_all(b"Hello from server!").await.unwrap();
        server_write.flush().await.unwrap();

        // 然后开始转发
        let result = copy(&mut server_read, &mut client_write).await;
        println!("Server->Client: {} bytes", result.unwrap_or(0));
    });

    // 客户端等待接收数据
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut buf = [0; 100];
    let n = client_read.read(&mut buf).await.unwrap();
    println!("Client received: {}", String::from_utf8_lossy(&buf[..n]));

    // 清理
    tokio::time::sleep(Duration::from_secs(1)).await;
}

#[tokio::main]
async fn main() {
    test_copy_solution().await;
}