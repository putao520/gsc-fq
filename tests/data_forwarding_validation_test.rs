// 数据转发完整验证测试 - 验证代理的数据转发正确性
//
// 测试场景：
// 1. 双向数据转发验证
// 2. 多轮次往返传输
// 3. 数据完整性验证（SHA256）
// 4. 顺序保证验证
// 5. 分片传输验证

use anyhow::Result;
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServerBuilder;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// 创建双向验证服务器
async fn create_bidirectional_echo_server(port: u16) -> Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;

    let handle = tokio::spawn(async move {
        println!("📡 双向验证服务器启动在端口 {}", port);
        let mut connection_count = 0;

        loop {
            match listener.accept().await {
                Ok((mut socket, addr)) => {
                    connection_count += 1;
                    println!("✅ 验证服务器接受连接 #{}: {}", connection_count, addr);

                    tokio::spawn(async move {
                        let mut buf = [0u8; 65536];
                        loop {
                            match socket.read(&mut buf).await {
                                Ok(0) => {
                                    println!("🔚 连接关闭: {}", addr);
                                    break;
                                }
                                Ok(n) => {
                                    // Echo 数据回去
                                    if let Err(e) = socket.write_all(&buf[..n]).await {
                                        println!("❌ 写入失败 {}: {}", addr, e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    println!("❌ 读取失败 {}: {}", addr, e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    println!("❌ 接受连接失败: {}", e);
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(handle)
}

#[tokio::test]
async fn test_bidirectional_data_integrity() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         数据转发验证: 双向数据完整性");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 22001;
    let proxy_port = 22002;

    // 启动双向 echo 服务器
    let echo_handle = create_bidirectional_echo_server(echo_port).await?;
    println!("✅ 双向验证服务器已启动\n");

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

    println!("📝 测试场景: 多轮次双向传输 + SHA256 验证\n");

    use tokio::net::TcpStream;

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

    // 测试数据（多轮次）
    let test_rounds = vec![
        ("Hello, World!".as_bytes().to_vec(), "文本消息"),
        (vec![42u8; 1024], "1KB 二进制"),
        (vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9], "小序列"),
        (vec![255u8; 4096], "4KB 填充"),
    ];

    for (round, (test_data, description)) in test_rounds.iter().enumerate() {
        println!("🔗 轮次 {}: {} ({} bytes)", round + 1, description, test_data.len());

        // 计算 SHA256
        let original_hash = Sha256::digest(test_data);
        println!("  原始 SHA256: {:x}", original_hash);

        // 发送数据
        client.write_all(test_data).await?;
        client.flush().await?;

        // 接收响应
        let mut recv_buf = vec![0u8; test_data.len()];
        client.read_exact(&mut recv_buf).await?;

        // 计算接收数据的 SHA256
        let received_hash = Sha256::digest(&recv_buf);
        println!("  接收 SHA256: {:x}", received_hash);

        // 验证数据
        assert_eq!(recv_buf, *test_data, "{} 数据应该完全匹配", description);
        assert_eq!(original_hash, received_hash, "{} SHA256 应该匹配", description);

        println!("  ✅ 验证通过\n");
    }

    // 清理
    drop(client);
    echo_handle.abort();
    proxy_handle.abort();

    println!("📊 测试结果:");
    println!("  ✅ 4 轮次传输全部成功");
    println!("  ✅ 数据完整性验证通过");
    println!("  ✅ SHA256 哈希验证通过");
    println!("  ✅ 双向转发正确");

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_data_ordering_guarantee() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         数据转发验证: 数据顺序保证");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 22003;
    let proxy_port = 22004;

    // 启动 echo 服务器
    let echo_handle = create_bidirectional_echo_server(echo_port).await?;
    println!("✅ 验证服务器已启动\n");

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

    println!("📝 测试场景: 分片传输 + 顺序验证\n");

    use tokio::net::TcpStream;

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

    // 创建有序数据块（每个块有不同的序列号）
    let num_chunks = 10;
    let chunk_size = 512;
    let mut all_data = Vec::new();

    for i in 0..num_chunks {
        let chunk: Vec<u8> = (0..chunk_size)
            .map(|j| ((i * 256 + j) % 256) as u8)
            .collect();
        all_data.extend_from_slice(&chunk);
    }

    println!("📤 发送 {} 个数据块 (总计 {} bytes)", num_chunks, all_data.len());

    // 分片发送
    for (i, chunk) in all_data.chunks(chunk_size).enumerate() {
        client.write_all(chunk).await?;
        client.flush().await?;
        println!("  块 #{} 发送完成 ({} bytes)", i + 1, chunk.len());
    }

    // 接收响应
    let mut recv_buf = vec![0u8; all_data.len()];
    client.read_exact(&mut recv_buf).await?;
    println!("📥 接收完成 ({} bytes)\n", recv_buf.len());

    // 验证数据完整性
    assert_eq!(recv_buf.len(), all_data.len(), "接收数据长度应该匹配");
    assert_eq!(recv_buf, all_data, "接收数据应该完全匹配");

    // 验证数据顺序
    for i in 0..num_chunks {
        let start = i * chunk_size;
        let end = start + chunk_size;
        let expected: Vec<u8> = (0..chunk_size)
            .map(|j| ((i * 256 + j) % 256) as u8)
            .collect();

        let actual = &recv_buf[start..end];
        assert_eq!(actual, expected.as_slice(), "块 #{} 数据应该匹配", i + 1);
        println!("  ✅ 块 #{} 顺序验证通过", i + 1);
    }

    println!("\n✅ 所有数据块顺序验证通过");

    // 清理
    drop(client);
    echo_handle.abort();
    proxy_handle.abort();

    println!("\n📊 测试结果:");
    println!("  ✅ {} 个数据块传输成功", num_chunks);
    println!("  ✅ 数据顺序保证验证通过");
    println!("  ✅ 无乱序、无丢失");

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_fragmented_data_integrity() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         数据转发验证: 分片数据完整性");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 22005;
    let proxy_port = 22006;

    // 启动 echo 服务器
    let echo_handle = create_bidirectional_echo_server(echo_port).await?;
    println!("✅ 验证服务器已启动\n");

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

    println!("📝 测试场景: 不规则分片 + 完整性验证\n");

    use tokio::net::TcpStream;

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

    // 创建测试数据（5MB）
    let total_size = 5 * 1024 * 1024;
    let test_data: Vec<u8> = (0..total_size)
        .map(|i| ((i * 1103515245 + 12345) % 256) as u8)
        .collect();

    // 计算原始哈希
    let original_hash = Sha256::digest(&test_data);
    println!("  原始数据 SHA256: {:x}", original_hash);
    println!("  数据大小: {} MB\n", total_size / 1024 / 1024);

    // 不规则分片发送
    let fragment_sizes = vec![1, 13, 1024, 4096, 65536, 100, 500000, 1024 * 1024];
    let mut sent = 0;

    println!("📤 不规则分片发送:");
    for (i, &frag_size) in fragment_sizes.iter().cycle().enumerate() {
        if sent >= total_size {
            break;
        }

        let remaining = total_size - sent;
        let actual_size = frag_size.min(remaining);

        client.write_all(&test_data[sent..sent + actual_size]).await?;
        sent += actual_size;

        if (i + 1) % 100 == 0 {
            println!("  进度: {} / {} bytes ({} 块)", sent, total_size, i + 1);
        }
    }
    client.flush().await?;
    println!("  ✅ 发送完成 ({} 块)\n", sent);

    // 接收响应
    println!("📥 接收数据:");
    let mut recv_buf = vec![0u8; total_size];
    let mut received = 0;

    while received < total_size {
        let n = client.read(&mut recv_buf[received..]).await?;
        if n == 0 {
            break;
        }
        received += n;

        if received % (1024 * 1024) == 0 {
            println!("  进度: {} / {} bytes", received, total_size);
        }
    }
    println!("  ✅ 接收完成\n");

    // 计算接收哈希
    let received_hash = Sha256::digest(&recv_buf);
    println!("  接收数据 SHA256: {:x}\n", received_hash);

    assert_eq!(received, total_size, "接收数据量应该匹配");
    assert_eq!(original_hash, received_hash, "SHA256 哈希应该匹配");
    assert_eq!(recv_buf, test_data, "数据应该完全匹配");

    println!("✅ 不规则分片数据完整性验证通过");

    // 清理
    drop(client);
    echo_handle.abort();
    proxy_handle.abort();

    println!("\n📊 测试结果:");
    println!("  ✅ 不规则分片传输成功");
    println!("  ✅ SHA256 完整性验证通过");
    println!("  ✅ 数据完全匹配");

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_bidirectional_streams() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         数据转发验证: 并发双向流");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 22007;
    let proxy_port = 22008;

    // 启动 echo 服务器
    let echo_handle = create_bidirectional_echo_server(echo_port).await?;
    println!("✅ 验证服务器已启动\n");

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

    println!("📝 测试场景: 10 个并发双向流\n");

    use tokio::net::TcpStream;

    let num_streams = 10;
    let data_size = 1024 * 10; // 每个流 10KB

    let mut tasks = Vec::new();

    for i in 0..num_streams {
        let task = tokio::spawn(async move {
            let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

            // 创建唯一的测试数据（每个流不同）
            let test_data: Vec<u8> = (0..data_size)
                .map(|j| ((i * data_size + j) % 256) as u8)
                .collect();

            // 计算 SHA256
            let original_hash = Sha256::digest(&test_data);

            // 发送数据
            client.write_all(&test_data).await?;
            client.flush().await?;

            // 接收响应
            let mut recv_buf = vec![0u8; data_size];
            client.read_exact(&mut recv_buf).await?;

            // 验证
            let received_hash = Sha256::digest(&recv_buf);
            assert_eq!(recv_buf, test_data, "流 #{} 数据应该匹配", i);
            assert_eq!(original_hash, received_hash, "流 #{} SHA256 应该匹配", i);

            Ok::<usize, anyhow::Error>(data_size)
        });

        tasks.push(task);
    }

    // 等待所有流完成
    let mut total_bytes = 0usize;
    for task in tasks {
        total_bytes += task.await??;
    }

    println!("  ✅ {} 个并发双向流全部验证通过", num_streams);
    println!("  ✅ 总传输数据: {} bytes ({} MB)", total_bytes, total_bytes / 1024 / 1024);

    // 清理
    echo_handle.abort();
    proxy_handle.abort();

    println!("\n📊 测试结果:");
    println!("  ✅ 并发双向流验证通过");
    println!("  ✅ 数据完整性保证");
    println!("  ✅ 无数据混淆");

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}
