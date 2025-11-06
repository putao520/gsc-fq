// 代理性能测试
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// 方法1：手动实现（当前方案）
async fn manual_copy(
    mut reader: tokio::net::tcp::ReadHalf<'_>,
    mut writer: tokio::net::tcp::WriteHalf<'_>,
) -> tokio::io::Result<u64> {
    let mut buf = [0; 8192];
    let mut total = 0;

    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                writer.write_all(&buf[..n]).await?;
                writer.flush().await?;
                total += n as u64;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(total)
}

// 方法2：使用 tokio::io::copy
async fn optimized_copy(
    mut reader: tokio::net::tcp::ReadHalf<'_>,
    mut writer: tokio::net::tcp::WriteHalf<'_>,
) -> tokio::io::Result<u64> {
    tokio::io::copy(&mut reader, &mut writer).await
}

// 方法3：减少 flush
async fn buffered_copy(
    mut reader: tokio::net::tcp::ReadHalf<'_>,
    mut writer: tokio::net::tcp::WriteHalf<'_>,
) -> tokio::io::Result<u64> {
    let mut buf = [0; 65536];
    let mut total = 0;
    let mut need_flush = false;

    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                writer.write_all(&buf[..n]).await?;
                need_flush = true;

                // 累积足够数据再flush
                if n < buf.len() || total % (64 * 1024) < n as u64 {
                    writer.flush().await?;
                    need_flush = false;
                }

                total += n as u64;
            }
            Err(e) => return Err(e),
        }
    }

    if need_flush {
        writer.flush().await?;
    }

    Ok(total)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    const TEST_SIZE: usize = 10 * 1024 * 1024; // 10MB

    println!("Testing proxy implementations with {}MB data...\n", TEST_SIZE / 1024 / 1024);

    // 创建测试服务器
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let server_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (mut read, mut write) = stream.into_split();

        // Echo 服务器
        let mut buf = [0; 65536];
        let mut total = 0;
        loop {
            match read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    write.write_all(&buf[..n]).await.unwrap();
                    total += n;
                }
                Err(_) => break,
            }
        }
        total
    });

    // 测试数据
    let test_data = vec![42u8; TEST_SIZE];

    // 测试方法1：手动实现
    println!("1. Testing manual implementation...");
    let start = Instant::now();
    let client = TcpStream::connect(("127.0.0.1", port)).await?;
    let (read, write) = client.into_split();

    let (bytes1, bytes2) = tokio::join!(
        async {
            let mut written = 0;
            let mut w = write;
            while written < TEST_SIZE {
                let chunk = std::cmp::min(4096, TEST_SIZE - written);
                w.write_all(&test_data[written..written+chunk]).await?;
                w.flush().await?;
                written += chunk;
            }
            Ok(written as u64)
        },
        manual_copy(read, write)
    );

    let duration1 = start.elapsed();
    println!("   Duration: {:?}, Throughput: {:.2} MB/s",
             duration1,
             (bytes1.unwrap() + bytes2.unwrap()) as f64 / duration1.as_secs_f64() / 1024.0 / 1024.0);

    // TODO: 测试其他方法...

    Ok(())
}