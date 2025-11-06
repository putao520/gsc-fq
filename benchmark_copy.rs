// 性能对比测试
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

// 方案1：copy_bidirectional
async fn bidirectional_copy(mut a: tokio::net::TcpStream, mut b: tokio::net::TcpStream) -> std::io::Result<(u64, u64)> {
    io::copy_bidirectional(&mut a, &mut b).await
}

// 方案2：两个独立的 copy
async fn two_copies(mut a: tokio::net::TcpStream, mut b: tokio::net::TcpStream) -> std::io::Result<(u64, u64)> {
    let (a_read, a_write) = a.split();
    let (b_read, b_write) = b.split();

    let a_to_b = tokio::spawn(async move {
        io::copy(&mut a_read, &mut b_write).await
    });

    let b_to_a = tokio::spawn(async move {
        io::copy(&mut b_read, &mut a_write).await
    });

    let (a_to_b_result, b_to_a_result) = tokio::join!(a_to_b, b_to_a);
    Ok((a_to_b_result.unwrap()?, b_to_a_result.unwrap()?))
}

// 方案3：手动实现
async fn manual_copy(mut a: tokio::net::TcpStream, mut b: tokio::net::TcpStream) -> std::io::Result<(u64, u64)> {
    let (a_read, a_write) = a.split();
    let (b_read, b_write) = b.split();

    let a_to_b = tokio::spawn(async move {
        let mut buf = vec![0; 65536];
        let mut total = 0;
        loop {
            match a_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    a_write.write_all(&buf[..n]).await?;
                    a_write.flush().await?;
                    total += n as u64;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    });

    let b_to_a = tokio::spawn(async move {
        let mut buf = vec![0; 65536];
        let mut total = 0;
        loop {
            match b_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    a_write.write_all(&buf[..n]).await?;
                    a_write.flush().await?;
                    total += n as u64;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    });

    let (a_to_b_result, b_to_a_result) = tokio::join!(a_to_b, b_to_a);
    Ok((a_to_b_result.unwrap()?, b_to_a_result.unwrap()?))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    const DATA_SIZE: usize = 100 * 1024 * 1024; // 100MB
    const BUFFER_SIZE: usize = 64 * 1024; // 64KB

    // 创建测试数据
    let test_data = vec![0u8; DATA_SIZE];

    println!("Testing performance with {}MB data...\n", DATA_SIZE / 1024 / 1024);

    // 测试 copy_bidirectional
    println!("1. Testing copy_bidirectional...");
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0; BUFFER_SIZE];
        let mut total = 0;
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => break,
            }
        }
        total as u64
    });

    let start = Instant::now();
    let client = TcpStream::connect(("127.0.0.1", port)).await?;
    let server = TcpStream::connect(("127.0.0.1", port)).await?;

    // 发送数据
    let mut client2 = client.clone();
    tokio::spawn(async move {
        let mut written = 0;
        while written < DATA_SIZE {
            let to_write = std::cmp::min(BUFFER_SIZE, DATA_SIZE - written);
            client2.write_all(&test_data[written..written+to_write]).await.unwrap();
            written += to_write;
        }
    });

    let (bytes_a, bytes_b) = bidirectional_copy(client, server).await?;
    let duration = start.elapsed();

    server_task.abort();
    println!("   Duration: {:?}, Throughput: {:.2} MB/s", duration, (bytes_a + bytes_b) as f64 / duration.as_secs_f64() / 1024.0 / 1024.0);

    // 测试两个 copy
    println!("\n2. Testing two copies...");
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0; BUFFER_SIZE];
        let mut total = 0;
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => break,
            }
        }
        total as u64
    });

    let start = Instant::now();
    let client = TcpStream::connect(("127.0.0.1", port)).await?;
    let server = TcpStream::connect(("127.0.0.1", port)).await?;

    // 发送数据
    let mut client2 = client.clone();
    tokio::spawn(async move {
        let mut written = 0;
        while written < DATA_SIZE {
            let to_write = std::cmp::min(BUFFER_SIZE, DATA_SIZE - written);
            client2.write_all(&test_data[written..written+to_write]).await.unwrap();
            written += to_write;
        }
    });

    let (bytes_a, bytes_b) = two_copies(client, server).await?;
    let duration = start.elapsed();

    server_task.abort();
    println!("   Duration: {:?}, Throughput: {:.2} MB/s", duration, (bytes_a + bytes_b) as f64 / duration.as_secs_f64() / 1024.0 / 1024.0);

    // 测试手动实现
    println!("\n3. Testing manual copy...");
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0; BUFFER_SIZE];
        let mut total = 0;
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => break,
            }
        }
        total as u64
    });

    let start = Instant::now();
    let client = TcpStream::connect(("127.0.0.1", port)).await?;
    let server = TcpStream::connect(("127.0.0.1", port)).await?;

    // 发送数据
    let mut client2 = client.clone();
    tokio::spawn(async move {
        let mut written = 0;
        while written < DATA_SIZE {
            let to_write = std::cmp::min(BUFFER_SIZE, DATA_SIZE - written);
            client2.write_all(&test_data[written..written+to_write]).await.unwrap();
            written += to_write;
        }
    });

    let (bytes_a, bytes_b) = manual_copy(client, server).await?;
    let duration = start.elapsed();

    server_task.abort();
    println!("   Duration: {:?}, Throughput: {:.2} MB/s", duration, (bytes_a + bytes_b) as f64 / duration.as_secs_f64() / 1024.0 / 1024.0);

    Ok(())
}