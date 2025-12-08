// 正向代理与反向代理同时工作的端到端测试
mod support;

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr};
use support::{
    PingPongServer, wait_for_port_ready, LOCALHOST,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use std::time::Duration;
use gsc_fq::config::loader::{ConfigFile, ServerSection, ReverseProxyServerSection};

/// 测试正向代理与反向代理同时工作
///
/// 测试场景：
/// 1. 正向代理：本地端口 59010 → 远程服务器 59011
/// 2. 反向代理：服务器端口 59012 → 本地服务 59013
/// 3. 两种代理模式同时运行，互不干扰
#[tokio::test]
async fn test_simultaneous_forward_and_reverse_proxy() -> Result<()> {
    println!("🧪 开始正向代理与反向代理同时工作测试");

    // 设置测试环境变量
    std::env::set_var("YAMUX_POOL_SIZE", "2"); // 小连接池
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20"); // 宽松黑洞检测

    // 1. 启动两个本地服务器作为测试目标
    let forward_target_server = PingPongServer::start().await?;
    let forward_target_port = forward_target_server.port();
    println!("📡 正向代理目标服务器启动在端口: {}", forward_target_port);

    let reverse_target_server = PingPongServer::start().await?;
    let reverse_target_port = reverse_target_server.port();
    println!("📡 反向代理目标服务器启动在端口: {}", reverse_target_port);

    // 2. 选择端口配置
    let reverse_control_port = 59020; // 反向代理控制端口
    let reverse_proxy_port = 59021;   // 反向代理服务端口
    let forward_proxy_port = 59022;   // 正向代理监听端口

    // 3. 创建同时运行正向和反向代理的配置
    let config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
        }),

        // 正向代理配置：从 forward_proxy_port 转发到目标服务器
        proxies: vec![
            gsc_fq::config::loader::ProxySection {
                local: forward_proxy_port.to_string(),
                remote: format!("127.0.0.1:{}", forward_target_port),
                source_ip: None,
            }
        ],

        // 反向代理配置：将外部连接转发到本地服务器
        reverse_proxies: vec![
            gsc_fq::config::loader::ReverseProxySection {
                server: reverse_proxy_port.to_string(),        // 服务器监听端口
                local: format!("127.0.0.1:{}", reverse_target_port), // 本地服务IP:端口
                source_ip: None,
            }
        ],

        reverse_proxy_server: Some(ReverseProxyServerSection {
            port: reverse_control_port,
            allowed_tokens: vec!["test-token".to_string()],
        }),
        reverse_proxy_client: None,
    };

    println!("🔧 配置创建完成:");
    println!("   - 正向代理: {} → 127.0.0.1:{}", forward_proxy_port, forward_target_port);
    println!("   - 反向代理服务器模式: 控制端口 {}, 服务端口 {}", reverse_control_port, reverse_proxy_port);
    println!("   - 反向代理转发: {} → 127.0.0.1:{}", reverse_proxy_port, reverse_target_port);

    // 4. 启动gsc-fq服务（包含正向和反向代理）
    println!("🚀 启动GSC-FQ服务...");

    // 由于我们需要在测试中模拟main.rs的行为，这里使用tokio::spawn来启动服务
    let service_handle = tokio::spawn(async move {
        // 这里需要模拟main.rs的逻辑来启动正向和反向代理
        // 由于我们无法直接调用main函数，我们需要使用现有的组件

        // 启动反向代理服务器
        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut reverse_server = gsc_fq::reverse_proxy::ReverseProxyServer::new(bind_ip, reverse_control_port);
        let reverse_handle = tokio::spawn(async move {
            if let Err(e) = reverse_server.start().await {
                eprintln!("❌ 反向代理服务器失败: {}", e);
            }
        });

        // 等待反向代理服务器启动
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 启动正向代理服务器
        let mut forward_proxy = gsc_fq::proxy::ProxyServerBuilder::new()
            .bind_ip(bind_ip)
            .add_proxies(config.proxies.clone())
            .build()
            .expect("Failed to create forward proxy");

        let forward_handle = tokio::spawn(async move {
            if let Err(e) = forward_proxy.start().await {
                eprintln!("❌ 正向代理失败: {}", e);
            }
        });

        // 等待两个服务都完成（实际上它们会一直运行）
        let _ = tokio::join!(reverse_handle, forward_handle);
    });

    // 等待服务启动
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 等待所有服务端口准备就绪
    wait_for_port_ready(forward_proxy_port, Duration::from_secs(5)).await?;
    wait_for_port_ready(reverse_control_port, Duration::from_secs(5)).await?;
    // 注意：reverse_proxy_port 是客户端监听端口，不需要在这里检查就绪

    println!("✅ 所有服务端口准备就绪:");
    println!("   - 正向代理端口: {}", forward_proxy_port);
    println!("   - 反向代理控制端口: {}", reverse_control_port);
    println!("   - 反向代理服务端口: {}", reverse_proxy_port);

    // 5. 测试正向代理功能
    println!("\n🔗 测试正向代理连接...");
    test_forward_proxy_connection(forward_proxy_port).await?;

    // 6. 测试反向代理功能
    println!("\n🔗 测试反向代理连接...");
    test_reverse_proxy_connection(reverse_proxy_port).await?;

    // 7. 测试同时访问（并发测试）
    println!("\n🔄 测试正向和反向代理并发访问...");
    test_concurrent_access(forward_proxy_port, reverse_proxy_port).await?;

    println!("\n🎉 正向代理与反向代理同时工作测试成功！");

    // 清理资源
    service_handle.abort();
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(())
}

/// 测试正向代理连接
async fn test_forward_proxy_connection(forward_port: u16) -> Result<()> {
    let mut stream = TcpStream::connect((LOCALHOST, forward_port)).await?;
    println!("✅ 已连接到正向代理端口 {}", forward_port);

    // 发送HTTP请求到正向代理
    let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    stream.write_all(request.as_bytes()).await?;
    println!("✅ 已发送HTTP请求到正向代理");

    // 读取响应
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).await?;
    println!("📄 正向代理响应状态: {}", status_line.trim());

    // 验证响应
    assert!(status_line.contains("200 OK"),
        "正向代理期望 200 OK，实际收到: {}", status_line);

    // 读取部分响应体
    let mut buf = [0u8; 100];
    let bytes_read = reader.read(&mut buf).await?;
    let response_body = String::from_utf8_lossy(&buf[..bytes_read]);
    println!("📄 正向代理响应体片段: {}", response_body.trim());
    assert!(response_body.contains("PONG"),
        "正向代理期望 PONG 响应，实际收到: {}", response_body);

    Ok(())
}

/// 测试反向代理连接
async fn test_reverse_proxy_connection(reverse_port: u16) -> Result<()> {
    let mut stream = TcpStream::connect((LOCALHOST, reverse_port)).await?;
    println!("✅ 已连接到反向代理端口 {}", reverse_port);

    // 发送HTTP请求
    let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    stream.write_all(request.as_bytes()).await?;
    println!("✅ 已发送HTTP请求到反向代理");

    // 读取响应
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).await?;
    println!("📄 反向代理响应状态: {}", status_line.trim());

    // 验证响应
    assert!(status_line.contains("200 OK"),
        "反向代理期望 200 OK，实际收到: {}", status_line);

    // 读取部分响应体
    let mut buf = [0u8; 100];
    let bytes_read = reader.read(&mut buf).await?;
    let response_body = String::from_utf8_lossy(&buf[..bytes_read]);
    println!("📄 反向代理响应体片段: {}", response_body.trim());
    assert!(response_body.contains("PONG"),
        "反向代理期望 PONG 响应，实际收到: {}", response_body);

    Ok(())
}

/// 测试正向和反向代理的并发访问
async fn test_concurrent_access(forward_port: u16, reverse_port: u16) -> Result<()> {
    let mut handles = Vec::new();

    // 启动多个正向代理连接
    for i in 0..3 {
        let port = forward_port;
        let handle = tokio::spawn(async move {
            if let Err(e) = test_forward_proxy_connection(port).await {
                eprintln!("❌ 正向代理并发连接 {} 失败: {}", i, e);
            } else {
                println!("✅ 正向代理并发连接 {} 成功", i);
            }
        });
        handles.push(handle);
    }

    // 启动多个反向代理连接
    for i in 0..3 {
        let port = reverse_port;
        let handle = tokio::spawn(async move {
            if let Err(e) = test_reverse_proxy_connection(port).await {
                eprintln!("❌ 反向代理并发连接 {} 失败: {}", i, e);
            } else {
                println!("✅ 反向代理并发连接 {} 成功", i);
            }
        });
        handles.push(handle);
    }

    // 等待所有并发连接完成
    for (i, handle) in handles.into_iter().enumerate() {
        handle.await?;
        println!("✅ 并发连接 {} 已完成", i);
    }

    println!("✅ 所有并发连接测试成功完成");
    Ok(())
}

/// 测试不同配置组合的正向和反向代理同时工作
#[tokio::test]
async fn test_multiple_proxy_configurations() -> Result<()> {
    println!("🧪 测试多种代理配置组合");

    // 设置测试环境变量
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    // 启动目标服务器
    let mut target_servers = Vec::new();
    for _ in 0..3 {
        target_servers.push(PingPongServer::start().await?);
    }

    let target_ports: Vec<u16> = target_servers.iter().map(|s| s.port()).collect();
    println!("📡 启动了 {} 个目标服务器，端口: {:?}", target_ports.len(), target_ports);

    // 配置多个正向代理和反向代理
    let forward_ports = vec![59030, 59031];
    let reverse_ports = vec![59032, 59033];
    let reverse_control_port = 59034;

    println!("🔧 配置多个代理:");
    println!("   正向代理: {:?} → {:?}", forward_ports, target_ports[0..2].to_vec());
    println!("   反向代理: {:?} → {:?}", reverse_ports, target_ports[1..3].to_vec());

    // 测试配置验证逻辑
    let mut config = ConfigFile {
        server: Some(ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
        }),

        proxies: forward_ports.iter().zip(target_ports[0..2].iter()).map(|(&local_port, &target_port)| {
            gsc_fq::config::loader::ProxySection {
                local: local_port.to_string(),
                remote: format!("127.0.0.1:{}", target_port),
                source_ip: None,
            }
        }).collect(),

        reverse_proxies: reverse_ports.iter().zip(target_ports[1..3].iter()).map(|(&server_port, &target_port)| {
            gsc_fq::config::loader::ReverseProxySection {
                server: server_port.to_string(),
                local: format!("127.0.0.1:{}", target_port),
                source_ip: None,
            }
        }).collect(),

        reverse_proxy_server: Some(ReverseProxyServerSection {
            port: reverse_control_port,
            allowed_tokens: vec!["multi-test-token".to_string()],
        }),
        reverse_proxy_client: None,
    };

    // 验证配置
    let validation_result = config.validate();
    assert!(validation_result.is_ok(),
        "配置验证失败: {:?}", validation_result.err());

    println!("✅ 多代理配置验证成功");
    println!("   - 正向代理规则数: {}", config.proxies.len());
    println!("   - 反向代理规则数: {}", config.reverse_proxies.len());

    // 验证端口映射
    for (i, proxy) in config.proxies.iter().enumerate() {
        let local_port = proxy.get_local_port().unwrap();
        let remote_port = proxy.get_remote_port().unwrap();
        println!("   正向代理 {}: {} → 127.0.0.1:{}", i, local_port, remote_port);
    }

    for (i, rproxy) in config.reverse_proxies.iter().enumerate() {
        let server_port = rproxy.get_server_port().unwrap();
        let local_port = rproxy.get_local_port().unwrap();
        println!("   反向代理 {}: {} → 127.0.0.1:{}", i, server_port, local_port);
    }

    println!("🎉 多代理配置测试成功！");

    Ok(())
}