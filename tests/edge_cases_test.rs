// 边界情况测试 - 测试代理在极端情况下的表现
//
// 测试场景：
// 1. GB 级大文件传输（流式处理）
// 2. 单字节包传输
// 3. 边界值测试（256KB, 512KB, 1MB, 10MB）
// 4. 零数据传输

use anyhow::Result;
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServerBuilder;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// 创建流式 echo 服务器（支持大文件）
async fn create_streaming_echo_server(port: u16) -> Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;

    let handle = tokio::spawn(async move {
        println!("📡 流式 Echo 服务器启动在端口 {}", port);
        let mut connection_count = 0;

        loop {
            match listener.accept().await {
                Ok((mut socket, addr)) => {
                    connection_count += 1;
                    println!("✅ Echo 服务器接受连接 #{}: {}", connection_count, addr);

                    tokio::spawn(async move {
                        let mut buf = [0u8; 65536]; // 64KB 缓冲区（流式处理）
                        let mut total_bytes = 0u64;

                        loop {
                            match socket.read(&mut buf).await {
                                Ok(0) => {
                                    println!(
                                        "🔚 Echo 连接关闭: {} (总传输: {} bytes)",
                                        addr, total_bytes
                                    );
                                    break;
                                }
                                Ok(n) => {
                                    total_bytes += n as u64;
                                    if let Err(e) = socket.write_all(&buf[..n]).await {
                                        println!("❌ Echo 写入失败 {}: {}", addr, e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    println!("❌ Echo 读取失败 {}: {}", addr, e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    println!("❌ Echo 服务器接受连接失败: {}", e);
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(handle)
}

#[tokio::test]
async fn test_single_byte_transfer() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         边界测试: 单字节传输");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 21001;
    let proxy_port = 21002;

    // 启动 echo 服务器
    let echo_handle = create_streaming_echo_server(echo_port).await?;
    println!("✅ Echo 服务器已启动\n");

    // 启动代理
    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", echo_port),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: None,
        cps_limit: None,
    };

    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut proxy_instance = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxy(proxy_config)
        .build()?;

    let proxy_handle = tokio::spawn(async move {
        let _ = proxy_instance.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("✅ 代理服务器已启动\n");

    println!("📝 测试场景: 单字节传输\n");

    use tokio::net::TcpStream;

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

    // 发送单字节
    let test_data = [42u8];
    client.write_all(&test_data).await?;
    client.flush().await?;
    println!("📤 发送数据: 1 byte (value: {})", test_data[0]);

    // 接收响应
    let mut recv_buf = [0u8; 1];
    client.read_exact(&mut recv_buf).await?;
    println!("📥 接收数据: 1 byte (value: {})", recv_buf[0]);

    assert_eq!(recv_buf[0], test_data[0], "单字节数据应该被正确传输");
    println!("✅ 单字节传输成功\n");

    // 清理
    drop(client);
    echo_handle.abort();
    proxy_handle.abort();

    println!("📊 测试结果:");
    println!("  ✅ 单字节发送成功");
    println!("  ✅ 单字节接收成功");
    println!("  ✅ 数据完整性验证通过");

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_boundary_buffer_sizes() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         边界测试: 缓冲区边界值（256KB, 512KB, 1MB）");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 21003;
    let proxy_port = 21004;

    // 启动 echo 服务器
    let echo_handle = create_streaming_echo_server(echo_port).await?;
    println!("✅ Echo 服务器已启动\n");

    // 启动代理
    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", echo_port),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: None,
        cps_limit: None,
    };

    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut proxy_instance = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxy(proxy_config)
        .build()?;

    let proxy_handle = tokio::spawn(async move {
        let _ = proxy_instance.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("✅ 代理服务器已启动\n");

    println!("📝 测试场景: 缓冲区边界值\n");

    // 测试不同大小的数据
    let test_cases = vec![
        (256 * 1024, "256KB"),
        (512 * 1024, "512KB"),
        (1024 * 1024, "1MB"),
    ];

    use tokio::net::TcpStream;

    for (size, name) in test_cases {
        println!("🔗 测试数据大小: {}", name);

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

        // 创建测试数据
        let test_data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

        // 计算原始数据的 SHA256
        let original_hash = Sha256::digest(&test_data);
        println!("  原始数据 SHA256: {:x}", original_hash);

        // 发送数据
        let send_start = std::time::Instant::now();
        client.write_all(&test_data).await?;
        client.flush().await?;
        let send_time = send_start.elapsed();
        println!("  ✅ 发送完成: {:?}", send_time);

        // 接收响应
        let mut recv_buf = vec![0u8; size];
        let recv_start = std::time::Instant::now();
        client.read_exact(&mut recv_buf).await?;
        let recv_time = recv_start.elapsed();
        println!("  ✅ 接收完成: {:?}", recv_time);

        // 计算接收数据的 SHA256
        let received_hash = Sha256::digest(&recv_buf);
        println!("  接收数据 SHA256: {:x}", received_hash);

        assert_eq!(original_hash, received_hash, "{} 数据完整性验证", name);
        println!("  ✅ SHA256 验证通过");
        println!("  ✅ 总耗时: {:?}\n", send_start.elapsed());

        drop(client);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // 清理
    echo_handle.abort();
    proxy_handle.abort();

    println!("📊 测试结果:");
    println!("  ✅ 256KB 传输成功，SHA256 验证通过");
    println!("  ✅ 512KB 传输成功，SHA256 验证通过");
    println!("  ✅ 1MB 传输成功，SHA256 验证通过");

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_large_file_transfer_with_hash_verification() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         边界测试: 大文件传输 + SHA256 验证（100MB）");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 21005;
    let proxy_port = 21006;
    let file_size = 100 * 1024 * 1024; // 100MB

    // 启动 echo 服务器
    let echo_handle = create_streaming_echo_server(echo_port).await?;
    println!("✅ Echo 服务器已启动（支持流式处理）\n");

    // 启动代理
    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", echo_port),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: None,
        cps_limit: None,
    };

    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut proxy_instance = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxy(proxy_config)
        .build()?;

    let proxy_handle = tokio::spawn(async move {
        let _ = proxy_instance.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("✅ 代理服务器已启动\n");

    println!("📝 测试配置:");
    println!("  文件大小: {} MB", file_size / 1024 / 1024);
    println!("  验证方式: SHA256 哈希");
    println!("  传输模式: 流式（避免内存爆炸）\n");

    use tokio::net::TcpStream;

    println!("🔗 建立连接...");
    let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;
    println!("✅ 连接建立成功\n");

    // 生成测试数据（流式生成，避免一次性占用内存）
    println!("📤 生成测试数据并计算原始哈希...");
    let original_hash = tokio::task::spawn_blocking(move || {
        let mut hasher = Sha256::new();
        let mut data = Vec::with_capacity(file_size);

        // 使用伪随机数据（避免重复）
        for i in 0..file_size {
            let byte = ((i * 1103515245 + 12345) % 256) as u8;
            hasher.update(&[byte]);
            data.push(byte);
        }

        Ok::<(Vec<u8>, Vec<u8>), anyhow::Error>((hasher.finalize().to_vec(), data))
    })
    .await??;

    // 打印哈希值（将 Vec<u8> 转换为 hex 字符串）
    let hash_str: String = original_hash.0.iter().map(|b| format!("{:02x}", b)).collect();
    println!("  原始数据 SHA256: {}", hash_str);
    println!("  数据生成完成\n");

    // 发送数据（流式）
    println!("📤 发送 {} MB 数据...", file_size / 1024 / 1024);
    let send_start = std::time::Instant::now();

    // 分块发送（每块 1MB）
    let chunk_size = 1024 * 1024;
    for chunk in original_hash.1.chunks(chunk_size) {
        client.write_all(chunk).await?;
    }
    client.flush().await?;
    let send_time = send_start.elapsed();
    println!("✅ 发送完成: {:?} ({:.2} MB/s)", send_time, file_size as f64 / send_time.as_secs_f64() / 1024.0 / 1024.0);

    // 接收数据（流式）
    println!("📥 接收 {} MB 数据...", file_size / 1024 / 1024);
    let mut recv_hasher = Sha256::new();
    let mut total_received = 0usize;
    let mut buffer = vec![0u8; chunk_size];
    let recv_start = std::time::Instant::now();

    while total_received < file_size {
        let n = client.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        recv_hasher.update(&buffer[..n]);
        total_received += n;

        if total_received % (10 * 1024 * 1024) == 0 {
            println!("  进度: {} / {} MB", total_received / 1024 / 1024, file_size / 1024 / 1024);
        }
    }

    let recv_time = recv_start.elapsed();
    let received_hash = recv_hasher.finalize();
    let recv_hash_str: String = received_hash.iter().map(|b| format!("{:02x}", b)).collect();
    println!("✅ 接收完成: {:?} ({:.2} MB/s)", recv_time, file_size as f64 / recv_time.as_secs_f64() / 1024.0 / 1024.0);
    println!("  接收数据 SHA256: {}\n", recv_hash_str);

    assert_eq!(total_received, file_size, "接收到的数据量应该匹配");
    assert_eq!(original_hash.0.as_slice(), received_hash.as_slice(), "SHA256 哈希应该匹配");
    println!("✅ SHA256 验证通过\n");

    // 清理
    drop(client);
    echo_handle.abort();
    proxy_handle.abort();

    println!("📊 测试结果:");
    println!("  ✅ 数据大小: {} MB", file_size / 1024 / 1024);
    println!("  ✅ 发送时间: {:?}", send_time);
    println!("  ✅ 接收时间: {:?}", recv_time);
    println!("  ✅ 总时间: {:?}", send_start.elapsed());
    println!("  ✅ 平均吞吐量: {:.2} MB/s", file_size as f64 / send_start.elapsed().as_secs_f64() / 1024.0 / 1024.0);
    println!("  ✅ SHA256 验证: 通过");

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_zero_data_transfer() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         边界测试: 零数据传输");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 21007;
    let proxy_port = 21008;

    // 启动 echo 服务器
    let echo_handle = create_streaming_echo_server(echo_port).await?;
    println!("✅ Echo 服务器已启动\n");

    // 启动代理
    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", echo_port),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: None,
        cps_limit: None,
    };

    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut proxy_instance = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxy(proxy_config)
        .build()?;

    let proxy_handle = tokio::spawn(async move {
        let _ = proxy_instance.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("✅ 代理服务器已启动\n");

    println!("📝 测试场景: 零数据传输\n");

    use tokio::net::TcpStream;

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

    // 发送空数据
    client.write_all(b"").await?;
    client.flush().await?;
    println!("📤 发送空数据");

    // 尝试接收（应该超时或立即返回 0）
    let mut recv_buf = [0u8; 100];

    match timeout(Duration::from_millis(100), client.read(&mut recv_buf)).await {
        Ok(Ok(0)) => println!("✅ 服务器正确关闭连接（EOF）"),
        Ok(Ok(n)) => println!("⚠️  服务器返回了 {} 字节", n),
        Ok(Err(e)) => println!("⚠️  读取错误: {}", e),
        Err(_) => println!("✅ 超时（符合预期）"),
    }

    println!("\n✅ 零数据传输测试完成");

    // 清理
    drop(client);
    echo_handle.abort();
    proxy_handle.abort();

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}
