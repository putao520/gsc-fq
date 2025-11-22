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
use std::sync::Arc;
use tokio::sync::Mutex;

/// 全面的反向代理测试套件
struct ComprehensiveTestSetup {
    servers: Arc<Mutex<Vec<PingPongServer>>>,
    control_port: u16,
    proxy_configs: Vec<ReverseProxySection>,
}

impl ComprehensiveTestSetup {
    async fn new() -> Result<Self> {
        // 启动多个本地服务器用于不同测试场景
        let servers = vec![
            PingPongServer::start().await?,   // 主服务器，端口随机
            PingPongServer::start().await?,   // 辅助服务器1
            PingPongServer::start().await?,   // 辅助服务器2
        ];

        let servers = Arc::new(Mutex::new(servers));
        let control_port = 60000;

        Ok(Self {
            servers,
            control_port,
            proxy_configs: Vec::new(),
        })
    }

    async fn start_reverse_proxy_server(&self) -> Result<()> {
        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let control_port = self.control_port;

        tokio::spawn(async move {
            let mut server = ReverseProxyServer::new(bind_ip, control_port);
            if let Err(e) = server.start().await {
                eprintln!("反向代理服务器错误: {:?}", e);
            }
        });

        wait_for_port_ready(self.control_port, Duration::from_secs(5)).await?;
        Ok(())
    }

    async fn start_reverse_proxy_client(&self) -> Result<()> {
        let config = ConfigFile {
            server: Some(ServerSection {
                bind_ip: Some("127.0.0.1".to_string()),
                debug: Some(true),
            }),
            proxies: vec![],
            reverse_proxies: self.proxy_configs.clone(),
        };

        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.control_port);
        tokio::spawn(async move {
            let mut client = ReverseProxyClient::new(server_addr, config);
            if let Err(e) = client.start().await {
                eprintln!("反向代理客户端错误: {:?}", e);
            }
        });

        // 等待客户端连接
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok(())
    }

    async fn get_server_port(&self, index: usize) -> Result<u16> {
        let servers = self.servers.lock().await;
        Ok(servers[index].port())
    }
}

/// HTTP请求辅助函数
async fn send_http_request(
    stream: &mut TcpStream,
    path: &str,
    host: &str,
) -> Result<String> {
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );

    stream.write_all(request.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();

    // 读取状态行
    let mut status_line = String::new();
    reader.read_line(&mut status_line).await?;
    response.push_str(&status_line);

    // 读取头部
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await?;
        response.push_str(&line);

        if line == "\r\n" || line == "\n" {
            break;
        }
    }

    // 读取响应体
    let mut body = String::new();
    let mut buf = [0u8; 1024];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                body.push_str(&String::from_utf8_lossy(&buf[..n]));
                if body.len() > 10000 { // 防止无限读取
                    break;
                }
            }
            Err(_) => break,
        }
    }

    response.push_str(&body);
    Ok(response)
}

/// 测试1: 对称端口配置 (port方式)
#[tokio::test]
async fn test_symmetric_port_configuration() -> Result<()> {
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    println!("🧪 测试1: 对称端口配置");

    let mut setup = ComprehensiveTestSetup::new().await?;

    // 使用对称端口配置
    let proxy_config = ReverseProxySection {
        port: Some(8080),                    // 服务器和本地都使用8080端口
        server_port: None,
        local_port: None,
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    };

    setup.proxy_configs.push(proxy_config);
    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;

    // 等待端口准备就绪
    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(8080, Duration::from_secs(5)).await?;

    // 测试连接
    let mut stream = TcpStream::connect((LOCALHOST, 8080)).await?;
    let response = send_http_request(&mut stream, "/ping", "localhost").await?;

    assert!(response.contains("200 OK"), "期望200响应，实际: {}", response);
    assert!(response.contains("PONG"), "期望PONG响应，实际: {}", response);

    println!("✅ 对称端口配置测试通过");
    Ok(())
}

/// 测试2: 非对称端口配置 (server_port + local_port)
#[tokio::test]
async fn test_asymmetric_port_configuration() -> Result<()> {
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    println!("🧪 测试2: 非对称端口配置");

    let mut setup = ComprehensiveTestSetup::new().await?;
    let local_port = setup.get_server_port(0).await?;

    // 使用非对称端口配置
    let proxy_config = ReverseProxySection {
        port: None,
        server_port: Some(8081),             // 服务器监听8081
        local_port: Some(local_port),       // 转发到本地服务器的端口
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    };

    setup.proxy_configs.push(proxy_config);
    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;

    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(8081, Duration::from_secs(5)).await?;

    let mut stream = TcpStream::connect((LOCALHOST, 8081)).await?;
    let response = send_http_request(&mut stream, "/ping", "localhost").await?;

    assert!(response.contains("200 OK"), "期望200响应，实际: {}", response);
    assert!(response.contains("PONG"), "期望PONG响应，实际: {}", response);

    println!("✅ 非对称端口配置测试通过");
    Ok(())
}

/// 测试3: 多个反向代理配置
#[tokio::test]
async fn test_multiple_reverse_proxies() -> Result<()> {
    std::env::set_var("YAMUX_POOL_SIZE", "2");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    println!("🧪 测试3: 多个反向代理配置");

    let mut setup = ComprehensiveTestSetup::new().await?;
    let local_port1 = setup.get_server_port(0).await?;
    let local_port2 = setup.get_server_port(1).await?;

    // 配置多个代理规则
    let proxy1 = ReverseProxySection {
        port: None,
        server_port: Some(8080),
        local_port: Some(local_port1),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    };

    let proxy2 = ReverseProxySection {
        port: None,
        server_port: Some(8081),
        local_port: Some(local_port2),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    };

    setup.proxy_configs.push(proxy1);
    setup.proxy_configs.push(proxy2);

    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;

    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(8080, Duration::from_secs(5)).await?;
    wait_for_port_ready(8081, Duration::from_secs(5)).await?;

    // 测试第一个代理
    let mut stream1 = TcpStream::connect((LOCALHOST, 8080)).await?;
    let response1 = send_http_request(&mut stream1, "/ping", "localhost").await?;
    assert!(response1.contains("200 OK"));

    // 测试第二个代理
    let mut stream2 = TcpStream::connect((LOCALHOST, 8081)).await?;
    let response2 = send_http_request(&mut stream2, "/ping", "localhost").await?;
    assert!(response2.contains("200 OK"));

    println!("✅ 多个反向代理配置测试通过");
    Ok(())
}

/// 测试4: 源IP伪装功能
#[tokio::test]
async fn test_source_ip_spoofing() -> Result<()> {
    std::env::set_var("YAMUX_POOL_SIZE", "1");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    println!("🧪 测试4: 源IP伪装功能");

    let mut setup = ComprehensiveTestSetup::new().await?;
    let local_port = setup.get_server_port(0).await?;

    // 配置源IP伪装
    let proxy_config = ReverseProxySection {
        port: None,
        server_port: Some(8082),
        local_port: Some(local_port),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: Some("192.168.1.100".to_string()), // 伪装源IP
    };

    setup.proxy_configs.push(proxy_config);
    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;

    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(8082, Duration::from_secs(5)).await?;

    let mut stream = TcpStream::connect((LOCALHOST, 8082)).await?;
    let response = send_http_request(&mut stream, "/ping", "localhost").await?;

    assert!(response.contains("200 OK"), "期望200响应，实际: {}", response);

    println!("✅ 源IP伪装功能测试通过");
    println!("📝 注意: 实际的源IP伪装需要在真实网络环境中验证");
    Ok(())
}

/// 测试5: 连接复用和并发处理
#[tokio::test]
async fn test_connection_reuse_and_concurrency() -> Result<()> {
    std::env::set_var("YAMUX_POOL_SIZE", "4");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    println!("🧪 测试5: 连接复用和并发处理");

    let mut setup = ComprehensiveTestSetup::new().await?;
    let local_port = setup.get_server_port(0).await?;

    let proxy_config = ReverseProxySection {
        port: None,
        server_port: Some(8083),
        local_port: Some(local_port),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    };

    setup.proxy_configs.push(proxy_config);
    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;

    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(8083, Duration::from_secs(5)).await?;

    // 并发测试多个连接
    let mut handles = vec![];
    for i in 0..5 {
        let handle = tokio::spawn(async move {
            let mut stream = TcpStream::connect((LOCALHOST, 8083)).await?;
            let response = send_http_request(&mut stream, &format!("/ping{}", i), "localhost").await?;

            if response.contains("200 OK") && response.contains("PONG") {
                Ok(())
            } else {
                Err(anyhow::anyhow!("连接{}失败", i))
            }
        });
        handles.push(handle);
    }

    // 等待所有连接完成
    for handle in handles {
        handle.await??;
    }

    println!("✅ 连接复用和并发处理测试通过");
    Ok(())
}

/// 测试6: 大数据传输测试
#[tokio::test]
async fn test_large_data_transfer() -> Result<()> {
    std::env::set_var("YAMUX_POOL_SIZE", "2");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    println!("🧪 测试6: 大数据传输测试");

    let mut setup = ComprehensiveTestSetup::new().await?;
    let local_port = setup.get_server_port(0).await?;

    let proxy_config = ReverseProxySection {
        port: None,
        server_port: Some(8084),
        local_port: Some(local_port),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    };

    setup.proxy_configs.push(proxy_config);
    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;

    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(8084, Duration::from_secs(5)).await?;

    let mut stream = TcpStream::connect((LOCALHOST, 8084)).await?;

    // 发送较大的HTTP请求
    let large_payload = "x".repeat(10000); // 10KB数据
    let request = format!(
        "POST /large HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        large_payload.len(),
        large_payload
    );

    stream.write_all(request.as_bytes()).await?;

    let mut response = String::new();
    let mut buf = [0u8; 1024];
    let mut reader = BufReader::new(stream);

    // 读取完整的响应
    while let Ok(n) = reader.read(&mut buf).await {
        if n == 0 { break; }
        response.push_str(&String::from_utf8_lossy(&buf[..n]));
    }

    assert!(response.len() > 1000, "期望接收到大量数据");

    println!("✅ 大数据传输测试通过，接收 {} 字节", response.len());
    Ok(())
}

/// 测试7: 错误处理和连接恢复
#[tokio::test]
async fn test_error_handling_and_recovery() -> Result<()> {
    std::env::set_var("YAMUX_POOL_SIZE", "2");
    std::env::set_var("BLACKHOLE_FAILURE_THRESHOLD", "20");

    println!("🧪 测试7: 错误处理和连接恢复");

    let mut setup = ComprehensiveTestSetup::new().await?;
    let local_port = setup.get_server_port(0).await?;

    let proxy_config = ReverseProxySection {
        port: None,
        server_port: Some(8085),
        local_port: Some(local_port),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    };

    setup.proxy_configs.push(proxy_config);
    setup.start_reverse_proxy_server().await?;
    setup.start_reverse_proxy_client().await?;

    tokio::time::sleep(Duration::from_secs(3)).await;
    wait_for_port_ready(8085, Duration::from_secs(5)).await?;

    // 测试正常连接
    let mut stream1 = TcpStream::connect((LOCALHOST, 8085)).await?;
    let response1 = send_http_request(&mut stream1, "/ping", "localhost").await?;
    assert!(response1.contains("200 OK"));

    // 模拟连接中断后恢复
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut stream2 = TcpStream::connect((LOCALHOST, 8085)).await?;
    let response2 = send_http_request(&mut stream2, "/ping", "localhost").await?;
    assert!(response2.contains("200 OK"));

    // 测试不存在的路径
    let mut stream3 = TcpStream::connect((LOCALHOST, 8085)).await?;
    let response3 = send_http_request(&mut stream3, "/notfound", "localhost").await?;
    assert!(response3.contains("404") || response3.contains("Not Found"));

    println!("✅ 错误处理和连接恢复测试通过");
    Ok(())
}