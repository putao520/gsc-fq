#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! tokio = { version = "1.0", features = ["full"] }
//! gsc-fq = { path = ".." }
//! ```

use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use gsc_fq::config::loader::ProxySection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 GSC-FQ Connection Analysis Tool");
    println!("=================================");

    // Step 1: Test direct TCP connection performance
    println!("\n1. Testing direct TCP connection performance...");
    test_direct_connection().await?;

    // Step 2: Test proxy connection with various scenarios
    println!("\n2. Testing proxy connection scenarios...");
    test_proxy_scenarios().await?;

    // Step 3: Test edge cases that might cause timeouts
    println!("\n3. Testing edge cases...");
    test_edge_cases().await?;

    Ok(())
}

async fn test_direct_connection() -> Result<(), Box<dyn std::error::Error>> {
    // Start a simple echo server
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let echo_port = listener.local_addr()?.port();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                if let Ok(n) = socket.read(&mut buffer).await {
                    let _ = socket.write_all(&buffer[..n]).await;
                }
            });
        }
    });

    // Test connection establishment time
    let start = Instant::now();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", echo_port)).await?;
    let connect_time = start.elapsed();
    println!("   Direct connection established in: {:?}", connect_time);

    // Test data transfer time
    let test_data = b"Hello, World!";
    let start = Instant::now();
    stream.write_all(test_data).await?;
    stream.flush().await?;

    let mut response = vec![0u8; test_data.len()];
    stream.read_exact(&mut response).await?;
    let transfer_time = start.elapsed();
    println!("   Data transfer completed in: {:?}", transfer_time);
    println!("   Response: {:?}", String::from_utf8_lossy(&response));

    Ok(())
}

async fn test_proxy_scenarios() -> Result<(), Box<dyn std::error::Error>> {
    // Test scenario 1: Normal operation
    println!("   Scenario 1: Normal proxy operation");
    test_proxy_with_config(
        "127.0.0.1:8080".parse()?,
        "127.0.0.1:9000".parse()?,
        Duration::from_secs(5),
    ).await?;

    // Test scenario 2: Slow remote server
    println!("   Scenario 2: Slow remote server simulation");
    test_proxy_with_slow_remote().await?;

    // Test scenario 3: Remote server that drops connections
    println!("   Scenario 3: Remote server that drops connections");
    test_proxy_with_dropping_remote().await?;

    Ok(())
}

async fn test_proxy_with_config(
    proxy_addr: SocketAddr,
    remote_addr: SocketAddr,
    test_timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    // Start echo server
    let listener = TcpListener::bind(&remote_addr).await?;

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                if let Ok(n) = socket.read(&mut buffer).await {
                    let _ = socket.write_all(&buffer[..n]).await;
                }
            });
        }
    });

    // Give echo server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Configure proxy
    let proxy_config = ProxySection {
        local_port: proxy_addr.port(),
        remote_host: remote_addr.ip().to_string(),
        remote_port: remote_addr.port(),
        source_ip: None,
    };

    let mut proxy_server = gsc_fq::proxy::ProxyServerBuilder::new()
        .bind_ip(proxy_addr.ip())
        .add_proxy(proxy_config)
        .build()?;

    // Start proxy in background
    let proxy_handle = tokio::spawn(async move {
        let _ = proxy_server.start().await;
    });

    // Give proxy time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test connection through proxy
    let test_result = timeout(test_timeout, async {
        let start = Instant::now();
        let mut stream = TcpStream::connect(proxy_addr).await?;
        let connect_time = start.elapsed();

        let test_data = b"Test message through proxy";
        stream.write_all(test_data).await?;
        stream.flush().await?;

        let mut response = vec![0u8; test_data.len()];
        stream.read_exact(&mut response).await?;
        let total_time = start.elapsed();

        Ok::<(Duration, Duration, Vec<u8>), Box<dyn std::error::Error>>((connect_time, total_time, response))
    }).await;

    match test_result {
        Ok(Ok((connect_time, total_time, response))) => {
            println!("     ✅ Proxy test successful:");
            println!("        Connect time: {:?}", connect_time);
            println!("        Total time: {:?}", total_time);
            println!("        Response: {:?}", String::from_utf8_lossy(&response));
        }
        Ok(Err(e)) => {
            println!("     ❌ Proxy test failed: {}", e);
        }
        Err(_) => {
            println!("     ⏰ Proxy test timed out after {:?}", test_timeout);
        }
    }

    // Stop proxy
    proxy_handle.abort();
    Ok(())
}

async fn test_proxy_with_slow_remote() -> Result<(), Box<dyn std::error::Error>> {
    // Start a slow echo server
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let remote_addr = listener.local_addr()?;

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                if let Ok(n) = socket.read(&mut buffer).await {
                    // Add delay before responding
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    let _ = socket.write_all(&buffer[..n]).await;
                }
            });
        }
    });

    test_proxy_with_config(
        "127.0.0.1:8081".parse()?,
        remote_addr,
        Duration::from_secs(10),
    ).await?;

    Ok(())
}

async fn test_proxy_with_dropping_remote() -> Result<(), Box<dyn std::error::Error>> {
    // Start a malicious echo server that drops connections
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let _remote_addr = listener.local_addr()?;

    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                // Immediately drop the connection
                drop(socket);
            });
        }
    });

    println!("     Testing proxy with remote server that drops connections...");

    let test_result = timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect("127.0.0.1:8082").await?;
        stream.write_all(b"test").await?;
        stream.flush().await?;

        let mut response = vec![0u8; 4];
        stream.read_exact(&mut response).await?;

        Ok::<(), Box<dyn std::error::Error>>(())
    }).await;

    match test_result {
        Ok(Ok(_)) => println!("     ⚠️  Unexpected success with dropping remote"),
        Ok(Err(e)) => println!("     ✅ Expected error with dropping remote: {}", e),
        Err(_) => println!("     ✅ Expected timeout with dropping remote"),
    }

    Ok(())
}

async fn test_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
    // Test 1: Large data transfer
    println!("   Edge case 1: Large data transfer");
    test_large_data_transfer().await?;

    // Test 2: Multiple concurrent connections
    println!("   Edge case 2: Multiple concurrent connections");
    test_concurrent_connections().await?;

    // Test 3: Connection that sends data but doesn't read
    println!("   Edge case 3: Write-only connection");
    test_write_only_connection().await?;

    Ok(())
}

async fn test_large_data_transfer() -> Result<(), Box<dyn std::error::Error>> {
    let large_data = vec![0u8; 1024 * 1024]; // 1MB

    let test_result = timeout(Duration::from_secs(30), async {
        let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
        let start = Instant::now();

        stream.write_all(&large_data).await?;
        stream.flush().await?;

        let mut response = vec![0u8; large_data.len()];
        stream.read_exact(&mut response).await?;

        let transfer_time = start.elapsed();
        println!("     1MB transfer completed in: {:?}", transfer_time);

        Ok::<(), Box<dyn std::error::Error>>(())
    }).await;

    match test_result {
        Ok(Ok(_)) => println!("     ✅ Large data transfer successful"),
        Ok(Err(e)) => println!("     ❌ Large data transfer failed: {}", e),
        Err(_) => println!("     ⏰ Large data transfer timed out"),
    }

    Ok(())
}

async fn test_concurrent_connections() -> Result<(), Box<dyn std::error::Error>> {
    let mut handles = Vec::new();

    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let test_result = timeout(Duration::from_secs(10), async {
                let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
                let message = format!("Concurrent test {}", i);
                stream.write_all(message.as_bytes()).await?;
                stream.flush().await?;

                let mut response = vec![0u8; message.len()];
                stream.read_exact(&mut response).await?;

                Ok::<(), Box<dyn std::error::Error>>(())
            }).await;

            match test_result {
                Ok(Ok(_)) => println!("     ✅ Concurrent connection {} successful", i),
                Ok(Err(e)) => println!("     ❌ Concurrent connection {} failed: {}", i, e),
                Err(_) => println!("     ⏰ Concurrent connection {} timed out", i),
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn test_write_only_connection() -> Result<(), Box<dyn std::error::Error>> {
    let test_result = timeout(Duration::from_secs(10), async {
        let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
        stream.write_all(b"Write-only test").await?;
        stream.flush().await?;

        // Don't try to read, just wait to see if connection times out
        tokio::time::sleep(Duration::from_secs(5)).await;

        Ok::<(), Box<dyn std::error::Error>>(())
    }).await;

    match test_result {
        Ok(Ok(_)) => println!("     ✅ Write-only connection completed without timeout"),
        Ok(Err(e)) => println!("     ❌ Write-only connection failed: {}", e),
        Err(_) => println!("     ⏰ Write-only connection timed out"),
    }

    Ok(())
}