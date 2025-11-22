mod support;

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use support::{
    PingPongServer, wait_for_port_ready, LOCALHOST,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use std::time::Duration;
use gsc_fq::config::loader::{ConfigFile, ServerSection, ReverseProxySection};
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};

/// 最简单的反向代理测试
#[tokio::test]
async fn test_simple_reverse_proxy() -> Result<()> {
    // 设置测试环境变量
    std::env::set_var("YAMUX_POOL_SIZE", "1"); // 最小连接池
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20"); // 宽松黑洞检测

    println!("🧪 开始简单反向代理测试");

    // 1. 启动本地服务器
    let local_server = PingPongServer::start().await?;
    let local_port = local_server.port();
    println!("📡 本地服务器启动在端口: {}", local_port);

    // 2. 选择可用端口
    let control_port = 59000; // 固定控制端口
    let proxy_port = 59001;   // 固定代理端口

    // 3. 启动反向代理服务器
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    tokio::spawn(async move {
        let mut server = ReverseProxyServer::new(bind_ip, control_port);
        if let Err(e) = server.start().await {
            eprintln!("反向代理服务器错误: {:?}", e);
        }
    });

    wait_for_port_ready(control_port, Duration::from_secs(5)).await?;
    println!("✅ 反向代理服务器启动，控制端口: {}", control_port);

    // 4. 配置并启动客户端
    let reverse_proxy_config = vec![ReverseProxySection {
        port: Some(proxy_port),      // 服务器和客户端使用相同端口
        server_port: None,
        local_port: Some(local_port), // 转发到本地服务器端口
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    }];

    let config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(false),
        }),
        proxies: vec![],
        reverse_proxies: reverse_proxy_config,
    };

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), control_port);
    tokio::spawn(async move {
        let mut client = ReverseProxyClient::new(server_addr, config);
        if let Err(e) = client.start().await {
            eprintln!("反向代理客户端错误: {:?}", e);
        }
    });

    // 等待客户端连接和代理准备就绪
    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(proxy_port, Duration::from_secs(5)).await?;
    println!("✅ 反向代理客户端准备就绪，代理端口: {}", proxy_port);

    // 5. 测试连接
    println!("🔗 测试连接到反向代理...");
    let mut stream = TcpStream::connect((LOCALHOST, proxy_port)).await?;
    println!("✅ 已连接到反向代理");

    // 6. 发送HTTP请求
    let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    stream.write_all(request.as_bytes()).await?;
    println!("✅ 已发送HTTP请求");

    // 7. 读取响应
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).await?;
    println!("📄 响应状态: {}", status_line.trim());

    // 8. 验证响应
    assert!(
        status_line.contains("200 OK"),
        "期望 200 OK，实际收到: {}",
        status_line
    );

    // 9. 读取响应体
    let mut response_body = String::new();
    let mut buf = [0u8; 1024];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                response_body.push_str(&String::from_utf8_lossy(&buf[..n]));
                if response_body.len() > 1000 { // 防止无限读取
                    break;
                }
            }
            Err(_) => break,
        }
    }

    println!("📄 响应体: {}", response_body.trim());
    assert!(response_body.contains("PONG"), "期望 PONG 响应，实际收到: {}", response_body);

    println!("🎉 反向代理测试成功！");
    Ok(())
}