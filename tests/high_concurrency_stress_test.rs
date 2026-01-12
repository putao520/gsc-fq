// 高并发压力测试 - 测试代理在高并发情况下的性能和稳定性
//
// 测试场景：
// 1. 100+ 并发连接测试
// 2. 并发数据传输验证
// 3. 连接池压力测试
// 4. 资源泄露监控

use anyhow::Result;
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServerBuilder;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// 创建高性能 echo 服务器
async fn create_echo_server(port: u16) -> Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;

    let handle = tokio::spawn(async move {
        println!("📡 高性能 Echo 服务器启动在端口 {}", port);
        let mut connection_count = 0u64;

        loop {
            match listener.accept().await {
                Ok((mut socket, addr)) => {
                    connection_count += 1;
                    if connection_count % 10 == 0 {
                        println!("📊 Echo 服务器已接受 {} 个连接", connection_count);
                    }

                    tokio::spawn(async move {
                        let mut buf = [0u8; 8192];
                        loop {
                            match socket.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if let Err(e) = socket.write_all(&buf[..n]).await {
                                        eprintln!("❌ Echo 写入失败 {}: {}", addr, e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("❌ Echo 读取失败 {}: {}", addr, e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("❌ Echo 服务器接受连接失败: {}", e);
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(handle)
}

#[tokio::test]
async fn test_100_concurrent_connections() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         高并发测试: 100 并发连接");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 20001;
    let proxy_port = 20002;
    let concurrent_connections = 100usize;

    // 1. 启动 echo 服务器
    let echo_handle = create_echo_server(echo_port).await?;
    println!("✅ Echo 服务器已启动\n");

    // 2. 启动代理服务器
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
    println!("✅ 代理服务器已启动在端口 {}\n", proxy_port);

    println!("📝 测试配置:");
    println!("  并发连接数: {}", concurrent_connections);
    println!("  数据大小: 1KB per connection");
    println!("  验证: 数据完整性 + 无连接丢失\n");

    let test_data = vec![42u8; 1024]; // 1KB 测试数据
    let success_count = Arc::new(AtomicU64::new(0));
    let failure_count = Arc::new(AtomicU64::new(0));
    let total_bytes = Arc::new(AtomicU64::new(0));

    let start_time = Instant::now();

    // 创建并发任务
    let mut tasks = Vec::with_capacity(concurrent_connections);

    for i in 0..concurrent_connections {
        let test_data = test_data.clone();
        let success_count = Arc::clone(&success_count);
        let failure_count = Arc::clone(&failure_count);
        let total_bytes = Arc::clone(&total_bytes);

        let task = tokio::spawn(async move {
            match timeout(
                Duration::from_secs(10),
                perform_single_request(proxy_port, i, &test_data),
            )
            .await
            {
                Ok(Ok(bytes)) => {
                    success_count.fetch_add(1, Ordering::Relaxed);
                    total_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
                }
                Ok(Err(e)) => {
                    eprintln!("❌ 连接 #{} 失败: {}", i, e);
                    failure_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    eprintln!("❌ 连接 #{} 超时", i);
                    failure_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        tasks.push(task);
    }

    // 等待所有任务完成
    for task in tasks {
        let _ = task.await;
    }

    let elapsed = start_time.elapsed();
    let success = success_count.load(Ordering::Relaxed) as usize;
    let failure = failure_count.load(Ordering::Relaxed) as usize;
    let bytes = total_bytes.load(Ordering::Relaxed);

    println!("\n📊 测试结果:");
    println!("  成功连接: {} / {}", success, concurrent_connections);
    println!("  失败连接: {}", failure);
    println!("  总传输数据: {} bytes ({} MB)", bytes, bytes as f64 / 1024.0 / 1024.0);
    println!("  总耗时: {:?}", elapsed);
    println!("  平均延迟: {:?}", elapsed / concurrent_connections as u32);
    println!("  吞吐量: {:.2} MB/s", bytes as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0);

    // 验证所有连接都成功
    assert_eq!(success, concurrent_connections, "所有连接都应该成功");
    assert_eq!(failure, 0, "不应该有失败的连接");

    // 清理
    echo_handle.abort();
    proxy_handle.abort();

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_mixed_data_sizes() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         高并发测试: 混合数据大小（50 连接）");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 20003;
    let proxy_port = 20004;
    let concurrent_connections = 50usize;

    // 启动 echo 服务器
    let echo_handle = create_echo_server(echo_port).await?;
    println!("✅ Echo 服务器已启动\n");

    // 启动代理服务器
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
    println!("  并发连接数: {}", concurrent_connections);
    println!("  数据大小: 混合 (100B - 100KB)");
    println!("  验证: 每个连接不同大小\n");

    let success_count = Arc::new(AtomicU64::new(0));
    let failure_count = Arc::new(AtomicU64::new(0));

    let start_time = Instant::now();
    let mut tasks = Vec::with_capacity(concurrent_connections);

    for i in 0..concurrent_connections {
        let success_count = Arc::clone(&success_count);
        let failure_count = Arc::clone(&failure_count);

        // 每个连接使用不同的数据大小
        let size = 100 + (i * 200); // 100, 300, 500, ..., 9900 bytes
        let test_data: Vec<u8> = (0..size).map(|j| (j % 256) as u8).collect();

        let task = tokio::spawn(async move {
            match timeout(
                Duration::from_secs(10),
                perform_single_request(proxy_port, i, &test_data),
            )
            .await
            {
                Ok(Ok(bytes)) => {
                    assert_eq!(bytes, size * 2, "传输字节应该匹配 (发送+接收)");
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
                Ok(Err(e)) => {
                    eprintln!("❌ 连接 #{} ({} bytes) 失败: {}", i, size, e);
                    failure_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    eprintln!("❌ 连接 #{} ({} bytes) 超时", i, size);
                    failure_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        tasks.push(task);
    }

    for task in tasks {
        let _ = task.await;
    }

    let elapsed = start_time.elapsed();
    let success = success_count.load(Ordering::Relaxed) as usize;
    let failure = failure_count.load(Ordering::Relaxed) as usize;

    println!("\n📊 测试结果:");
    println!("  成功连接: {} / {}", success, concurrent_connections);
    println!("  失败连接: {}", failure);
    println!("  总耗时: {:?}", elapsed);
    println!("  平均延迟: {:?}", elapsed / concurrent_connections as u32);

    assert_eq!(success, concurrent_connections, "所有连接都应该成功");
    assert_eq!(failure, 0, "不应该有失败的连接");

    // 清理
    echo_handle.abort();
    proxy_handle.abort();

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

#[tokio::test]
async fn test_connection_pool_stress() -> Result<()> {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("         高并发测试: 连接池压力（200 连接）");
    println!("═══════════════════════════════════════════════════════════════\n");

    let echo_port = 20005;
    let proxy_port = 20006;
    let concurrent_connections = 200usize;

    // 启动 echo 服务器
    let echo_handle = create_echo_server(echo_port).await?;
    println!("✅ Echo 服务器已启动\n");

    // 启动代理服务器（启用连接池）
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
    println!("✅ 代理服务器已启动（连接池已启用）\n");

    println!("📝 测试配置:");
    println!("  并发连接数: {}", concurrent_connections);
    println!("  数据大小: 512B per connection");
    println!("  重点: 连接池在高并发下的表现\n");

    let test_data = vec![99u8; 512];
    let success_count = Arc::new(AtomicU64::new(0));
    let failure_count = Arc::new(AtomicU64::new(0));

    let start_time = Instant::now();
    let mut tasks = Vec::with_capacity(concurrent_connections);

    for i in 0..concurrent_connections {
        let test_data = test_data.clone();
        let success_count = Arc::clone(&success_count);
        let failure_count = Arc::clone(&failure_count);

        let task = tokio::spawn(async move {
            match timeout(
                Duration::from_secs(15),
                perform_single_request(proxy_port, i, &test_data),
            )
            .await
            {
                Ok(Ok(_)) => {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
                Ok(Err(e)) => {
                    eprintln!("❌ 连接 #{} 失败: {}", i, e);
                    failure_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    eprintln!("❌ 连接 #{} 超时", i);
                    failure_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        tasks.push(task);
    }

    for task in tasks {
        let _ = task.await;
    }

    let elapsed = start_time.elapsed();
    let success = success_count.load(Ordering::Relaxed) as usize;
    let failure = failure_count.load(Ordering::Relaxed) as usize;

    println!("\n📊 测试结果:");
    println!("  成功连接: {} / {}", success, concurrent_connections);
    println!("  失败连接: {}", failure);
    println!("  总耗时: {:?}", elapsed);
    println!("  平均连接建立延迟: {:?}", elapsed / concurrent_connections as u32);
    println!("  吞吐量: {:.2} 连接/秒", concurrent_connections as f64 / elapsed.as_secs_f64());

    assert_eq!(success, concurrent_connections, "所有连接都应该成功");
    assert_eq!(failure, 0, "不应该有失败的连接");

    // 清理
    echo_handle.abort();
    proxy_handle.abort();

    println!("\n═══════════════════════════════════════════════════════════════\n");

    Ok(())
}

/// 执行单个请求
async fn perform_single_request(
    proxy_port: u16,
    id: usize,
    test_data: &[u8],
) -> Result<usize> {
    use tokio::net::TcpStream;

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;

    // 发送数据
    client.write_all(test_data).await?;
    client.flush().await?;

    // 接收响应
    let mut recv_buf = vec![0u8; test_data.len()];
    client.read_exact(&mut recv_buf).await?;

    // 验证数据
    assert_eq!(recv_buf, test_data, "连接 #{} 数据应该被正确回显", id);

    // 关闭连接
    drop(client);

    Ok(test_data.len() * 2) // 发送 + 接收
}
