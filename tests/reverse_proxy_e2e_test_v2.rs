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

/// 简化的反向代理测试架构
struct ReverseProxyTestSetup {
    local_server: PingPongServer,
    server_port: u16,
    control_port: u16,
}

impl ReverseProxyTestSetup {
    async fn new() -> Result<Self> {
        // 启动本地服务器
        let local_server = PingPongServer::start().await?;

        // 选择可用端口
        let server_port = Self::pick_available_port_range(30000, 31000)?;
        let control_port = Self::pick_available_port_range(31000, 32000)?;

        Ok(Self {
            local_server,
            server_port,
            control_port,
        })
    }

    /// 在指定范围内选择可用端口
    fn pick_available_port_range(start: u16, end: u16) -> Result<u16> {
        for port in start..end {
            if let Ok(listener) = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
                drop(listener);
                return Ok(port);
            }
        }
        Err(anyhow::anyhow!("No available port in range {}-{}", start, end))
    }

    async fn start_reverse_proxy_server(&self) -> Result<()> {
        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut server = ReverseProxyServer::new(bind_ip, self.control_port);

        tokio::spawn(async move {
            if let Err(e) = server.start().await {
                eprintln!("ReverseProxyServer error: {:?}", e);
            }
        });

        wait_for_port_ready(self.control_port, Duration::from_secs(10)).await?;
        Ok(())
    }

    async fn start_reverse_proxy_client(&self) -> Result<()> {
        let reverse_proxy_config = vec![ReverseProxySection {
            server: self.server_port.to_string(),
            local: format!("127.0.0.1:{}", self.local_server.port()),
            source_ip: None,
        }];

        let config = ConfigFile {
            server: Some(ServerSection {
                bind_ip: Some("127.0.0.1".to_string()),
                debug: Some(false),
                auth_token: None,
                allowed_tokens: Vec::new(),
            }),
            proxies: vec![],
            reverse_proxies: reverse_proxy_config,
            reverse_mode: Some("client".to_string()),
            reverse_target: Some(format!("127.0.0.1:{}", self.control_port)),
        };

        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.control_port);
        let mut client = ReverseProxyClient::new(server_addr, config);

        tokio::spawn(async move {
            if let Err(e) = client.start().await {
                eprintln!("ReverseProxyClient error: {:?}", e);
            }
        });

        // 等待代理客户端连接并准备就绪
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok(())
    }

    async fn wait_for_reverse_proxy_ready(&self) -> Result<()> {
        wait_for_port_ready(self.server_port, Duration::from_secs(10)).await?;
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_reverse_proxy_single_connection() -> Result<()> {
    // 为测试环境设置最小的连接池（只有1个连接）
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    // 为测试环境设置非常宽松的黑洞检测阈值
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    let setup = ReverseProxyTestSetup::new().await?;
    println!("Local server: {}", setup.local_server.port());
    println!("Control port: {}", setup.control_port);
    println!("Server port: {}", setup.server_port);

    // 启动反向代理服务器
    setup.start_reverse_proxy_server().await?;
    println!("✅ Reverse proxy server started");

    // 启动反向代理客户端
    setup.start_reverse_proxy_client().await?;
    println!("✅ Reverse proxy client started");

    // 等待反向代理准备就绪
    setup.wait_for_reverse_proxy_ready().await?;
    println!("✅ Reverse proxy ready");

    // 测试连接
    let mut stream = TcpStream::connect((LOCALHOST, setup.server_port)).await?;
    println!("✅ Connected to reverse proxy");

    let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    stream.write_all(request.as_bytes()).await?;
    println!("✅ Sent HTTP request");

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).await?;
    println!("✅ Response status: {}", status_line.trim());

    assert!(
        status_line.contains("200 OK"),
        "Expected 200 OK, got: {}",
        status_line
    );

    // 读取响应体
    let mut body = String::new();
    let mut content_length = 0;
    let mut line = String::new();

    loop {
        line.clear();
        reader.read_line(&mut line).await?;

        if line.starts_with("Content-Length:") {
            if let Some(len_str) = line.split(':').nth(1) {
                content_length = len_str.trim().parse().unwrap_or(0);
            }
        }

        if line == "\r\n" || line == "\n" {
            break;
        }
    }

    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).await?;
        body = String::from_utf8_lossy(&buf).to_string();
    } else {
        let mut buf = String::new();
        reader.read_to_string(&mut buf).await?;
        body = buf;
    }

    println!("✅ Response body: {}", body.trim());
    assert!(body.trim().contains("pong") || body.trim().contains("PONG"), "Expected pong response, got: {}", body);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_reverse_proxy_multiple_connections() -> Result<()> {
    // 为测试环境设置最小的连接池（只有1个连接）
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    // 为测试环境设置非常宽松的黑洞检测阈值
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    let setup = ReverseProxyTestSetup::new().await?;

    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;
    setup.wait_for_reverse_proxy_ready().await?;

    // 测试多个串行连接（避免端口冲突）
    let mut handles = vec![];
    for i in 0..2 {
        let server_port = setup.server_port;
        let handle = tokio::spawn(async move {
            let mut stream = TcpStream::connect((LOCALHOST, server_port)).await?;
            let request = format!("GET /ping{} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n", i);
            stream.write_all(request.as_bytes()).await?;

            let mut reader = BufReader::new(stream);
            let mut status_line = String::new();
            reader.read_line(&mut status_line).await?;

            if status_line.contains("200 OK") {
                Ok::<_, anyhow::Error>(format!("Connection {} success", i))
            } else {
                Err(anyhow::anyhow!("Connection {} failed: {}", i, status_line))
            }
        });
        handles.push(handle);
    }

    // 等待所有连接完成
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await??;
        println!("✅ {}", result);
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_reverse_proxy_connection_reuse() -> Result<()> {
    // 为测试环境设置最小的连接池（只有1个连接）
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    // 为测试环境设置非常宽松的黑洞检测阈值
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    let setup = ReverseProxyTestSetup::new().await?;

    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;
    setup.wait_for_reverse_proxy_ready().await?;

    // 测试连接重用
    for i in 0..3 {
        let mut stream = TcpStream::connect((LOCALHOST, setup.server_port)).await?;
        let request = format!("GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).await?;

        assert!(
            status_line.contains("200 OK"),
            "Request {} failed: {}",
            i,
            status_line
        );

        println!("✅ Request {} success", i);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}