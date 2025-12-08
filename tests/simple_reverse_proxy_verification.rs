mod support;

use anyhow::Result;
use support::PingPongServer;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use std::time::Duration;
use gsc_fq::config::loader::{ConfigFile, ServerSection, ReverseProxySection, ReverseProxyClientSection};
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};

/// 验证最简单的反向代理：客户端和服务器使用相同端口
#[tokio::test]
async fn test_simplest_reverse_proxy() -> Result<()> {
    // 设置最小化配置
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "50");

    println!("🧪 验证最简单的反向代理配置");
    println!("配置：客户端和服务器使用相同端口");

    // 1. 启动本地服务器（端口随机）
    let local_server = PingPongServer::start().await?;
    let local_port = local_server.port();
    println!("📡 本地服务器启动在端口: {}", local_port);

    // 2. 使用固定端口进行反向代理（客户端和服务器都使用这个端口）
    let proxy_port = 9000;
    let control_port = 9001;

    // 3. 启动反向代理服务器
    tokio::spawn(async move {
        let mut server = ReverseProxyServer::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            control_port,
        );
        if let Err(e) = server.start().await {
            eprintln!("反向代理服务器错误: {:?}", e);
        }
    });

    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("✅ 反向代理服务器已启动，控制端口: {}", control_port);

    // 4. 配置最简单的反向代理：客户端和服务器都使用相同端口
    let reverse_proxy_config = vec![ReverseProxySection {
        server: proxy_port.to_string(),        // 服务器监听端口
        local: format!("127.0.0.1:{}", local_port), // 本地服务IP:端口
        source_ip: None,
    }];

    let config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
        }),
        proxies: vec![],
        reverse_proxies: reverse_proxy_config,
        reverse_proxy_server: None,
        reverse_proxy_client: Some(ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", control_port),
        }),
    };

    // 5. 启动反向代理客户端
    let server_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        control_port,
    );
    tokio::spawn(async move {
        let mut client = ReverseProxyClient::new(server_addr, config);
        if let Err(e) = client.start().await {
            eprintln!("反向代理客户端错误: {:?}", e);
        }
    });

    // 等待客户端连接
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("✅ 反向代理客户端已连接");

    // 6. 测试连接
    println!("🔗 测试连接到反向代理端口 {}", proxy_port);

    match TcpStream::connect(("127.0.0.1", proxy_port)).await {
        Ok(mut stream) => {
            println!("✅ 成功连接到反向代理");

            // 发送HTTP请求
            let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            stream.write_all(request.as_bytes()).await?;
            println!("✅ 已发送HTTP请求");

            // 读取响应
            let mut reader = BufReader::new(stream);
            let mut status_line = String::new();

            match reader.read_line(&mut status_line).await {
                Ok(_) => {
                    println!("📄 响应状态: {}", status_line.trim());

                    // 验证响应
                    if status_line.contains("200 OK") {
                        println!("🎉 测试成功！反向代理正常工作");

                        // 读取更多响应内容
                        let mut response_body = String::new();
                        let mut buf = [0u8; 1024];

                        loop {
                            match reader.read(&mut buf).await {
                                Ok(0) => break, // EOF
                                Ok(n) => {
                                    response_body.push_str(&String::from_utf8_lossy(&buf[..n]));
                                    if response_body.contains("PONG") {
                                        println!("✅ 收到PONG响应");
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }

                        if response_body.contains("PONG") {
                            println!("✅ 反向代理成功转发了数据");
                        }

                    } else {
                        println!("❌ 响应异常: {}", status_line);
                    }
                }
                Err(e) => {
                    println!("❌ 读取响应失败: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ 连接失败: {:?}", e);
            return Err(anyhow::anyhow!("无法连接到反向代理"));
        }
    }

    println!("🏁 最简单的反向代理验证完成");
    Ok(())
}