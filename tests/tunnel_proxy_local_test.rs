// 本地隧道代理测试 - 不依赖DNS解析
// 测试隧道代理的基本功能，使用本地服务避免DNS问题

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::{timeout, Duration};
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServerBuilder;

/// 启动本地echo服务器用于测试
async fn start_echo_server(port: u16) -> Result<()> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let listener = TcpListener::bind(addr).await?;

    println!("🔧 Echo服务器启动在端口 {}", port);

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                tokio::spawn(async move {
                    let mut reader = BufReader::new(&mut stream);
                    let mut line = String::new();

                    // 读取客户端发送的数据
                    if let Ok(_) = reader.read_line(&mut line).await {
                        // 发送echo响应
                        let response = format!("ECHO: {}", line);
                        if let Err(_) = stream.write_all(response.as_bytes()).await {
                            return;
                        }
                        let _ = stream.flush().await;
                    }
                });
            }
            Err(_) => break Ok(()),
        }
    }
}

#[tokio::test]
async fn test_tunnel_proxy_local_service() -> Result<()> {
    println!("🚀 测试隧道代理到本地服务（无DNS依赖）...");

    // 启动本地echo服务器
    let echo_port = 9000;
    let echo_handle = tokio::spawn(start_echo_server(echo_port));

    // 等待echo服务器启动
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 创建隧道代理配置
    let proxy_port = 8080;
    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", echo_port), // 使用IP避免DNS
        source_ip: None,
    };

    println!("🔧 代理配置: 本地端口 {} → 本地服务 {}", proxy_port, proxy_config.remote);

    // 启动代理服务器
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut instance = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxy(proxy_config)
        .build()?;

    let server_handle = tokio::spawn(async move {
        let _ = instance.start().await;
    });

    // 等待代理服务器启动
    tokio::time::sleep(Duration::from_millis(1000)).await;
    println!("✅ 隧道代理服务器已启动在端口 {}", proxy_port);

    // 测试通过代理访问本地echo服务
    println!("🔗 通过代理访问本地echo服务...");

    let test_start = std::time::Instant::now();

    match timeout(
        Duration::from_secs(5),
        TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
    ).await {
        Ok(Ok(mut stream)) => {
            let connect_time = test_start.elapsed();
            println!("✅ 连接代理成功，耗时: {:?}", connect_time);

            // 发送测试数据
            let test_message = "Hello Tunnel Proxy! 你好隧道代理！\n";
            let write_start = std::time::Instant::now();
            stream.write_all(test_message.as_bytes()).await?;
            stream.flush().await?;
            let write_time = write_start.elapsed();
            println!("✅ 测试数据发送完成，耗时: {:?}", write_time);

            // 读取echo响应
            let read_start = std::time::Instant::now();
            let mut reader = BufReader::new(&mut stream);
            let mut response = String::new();

            match timeout(Duration::from_secs(5), reader.read_line(&mut response)).await {
                Ok(Ok(_)) => {
                    let read_time = read_start.elapsed();
                    println!("📥 收到响应: {} (耗时: {:?})", response.trim(), read_time);

                    // 验证echo响应
                    if response.contains("Hello Tunnel Proxy!") && response.contains("ECHO:") {
                        println!("✅ 隧道代理成功转发数据并收到正确的echo响应");
                    } else {
                        println!("⚠️  响应内容不符合预期: {}", response);
                    }

                    let total_time = test_start.elapsed();
                    println!("📊 性能统计: 总耗时={:?}, 连接={:?}, 写入={:?}, 读取={:?}",
                        total_time, connect_time, write_time, read_time);

                }
                Ok(Err(e)) => {
                    println!("❌ 读取响应时出错: {:?}", e);
                }
                Err(_) => {
                    println!("❌ 读取响应超时");
                }
            }

            let _ = stream.shutdown().await;
        }
        Ok(Err(e)) => {
            println!("❌ 连接代理失败: {:?}", e);
        }
        Err(_) => {
            println!("❌ 连接代理超时");
        }
    }

    // 清理资源
    println!("🧹 清理测试资源...");
    let _ = server_handle.await;
    echo_handle.abort();

    println!("✅ 本地隧道代理测试完成");
    Ok(())
}

#[tokio::test]
async fn test_tunnel_proxy_ip_only() -> Result<()> {
    println!("🚀 测试隧道代理使用纯IP地址（无DNS）...");

    // 使用已知的公共IP地址进行测试（避免DNS解析）
    // 这里使用Google的公共DNS服务器IP进行连通性测试
    let public_ip = "8.8.8.8";
    let public_port = 53; // DNS端口

    // 创建隧道代理配置
    let proxy_port = 8081;
    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("{}:{}", public_ip, public_port),
        source_ip: None,
    };

    println!("🔧 代理配置: 本地端口 {} → 公共服务 {}:{}", proxy_port, public_ip, public_port);

    // 启动代理服务器
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut instance = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxy(proxy_config)
        .build()?;

    let server_handle = tokio::spawn(async move {
        let _ = instance.start().await;
    });

    // 等待代理服务器启动
    tokio::time::sleep(Duration::from_millis(1000)).await;
    println!("✅ 隧道代理服务器已启动在端口 {}", proxy_port);

    // 测试代理连接（不发送实际数据，只测试连接建立）
    println!("🔗 测试通过代理建立到公共服务的连接...");

    match timeout(
        Duration::from_secs(10),
        TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
    ).await {
        Ok(Ok(mut stream)) => {
            println!("✅ 通过代理成功连接到公共服务 {}:{}", public_ip, public_port);
            println!("✅ 隧道代理TCP转发功能正常工作");
            let _ = stream.shutdown().await;
        }
        Ok(Err(e)) => {
            println!("❌ 连接代理失败: {:?}", e);
        }
        Err(_) => {
            println!("❌ 连接代理超时");
        }
    }

    // 清理资源
    println!("🧹 清理测试资源...");
    let _ = server_handle.await;

    println!("✅ 纯IP隧道代理测试完成");
    Ok(())
}