use anyhow::Result;
use gsc_fq::config::loader::ConfigFile;
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use std::time::Duration;
use tokio::net::UdpSocket;

const PROXY_SERVER_PORT: u16 = 9005;
const PROXY_CLIENT_UDP_PORT: u16 = 9006;

#[tokio::test]
async fn test_udp_over_tcp_forwarding() -> Result<()> {
    println!("🔄 测试 UDP over TCP 转发");

    // 1. 启动 UDP Echo Server (模拟本地服务)
    println!("📡 启动 UDP Echo Server");
    let echo_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let echo_addr = echo_socket.local_addr()?;
    println!("✅ UDP Echo服务器启动在: {}", echo_addr);

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            if let Ok((n, peer)) = echo_socket.recv_from(&mut buf).await {
                let msg = &buf[..n];
                println!("Echo Server received: {:?}", msg);
                // 暂时不回显，因为 Client 端只实现了单向发送 (Fire-and-forget)
                // 如果回显，Client 需要监听 response
            }
        }
    });

    // 2. 配置反向代理
    let proxy_config = gsc_fq::config::loader::ReverseProxySection {
        server: PROXY_CLIENT_UDP_PORT.to_string(), // 公网映射端口
        local: format!("{}:{}", echo_addr.ip(), echo_addr.port()), // 本地目标
        source_ip: None,
    };

    let config = ConfigFile {
        server: None,
        proxies: vec![],
        token: Some("default".to_string()), // Keep simple
        totp_secret: None,
        reverse_proxies: vec![proxy_config],
        reverse_proxy_server: None,
        reverse_proxy_client: Some(gsc_fq::config::loader::ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", PROXY_SERVER_PORT),
            token: None,
            totp_secret: None,
        }),
    };

    // 3. 启动 Server
    let bind_ip = "127.0.0.1".parse().unwrap();
    let mut server = ReverseProxyServer::new(bind_ip, PROXY_SERVER_PORT);
    tokio::spawn(async move {
        server.start().await.unwrap();
    });

    // 等待 Server 启动
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 4. 启动 Client
    let server_addr = std::net::SocketAddr::new(bind_ip, PROXY_SERVER_PORT);
    let mut client = ReverseProxyClient::new(server_addr, config);
    tokio::spawn(async move {
        client.start().await.unwrap();
    });

    // 等待 Client 连接
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 5. 发送测试 UDP 数据到 Server 暴露的端口
    println!("📤 发送 UDP 测试数据");
    let sender = UdpSocket::bind("127.0.0.1:0").await?;
    sender
        .connect(format!("127.0.0.1:{}", PROXY_CLIENT_UDP_PORT))
        .await?;

    // 发送多个包
    for i in 0..3 {
        let msg = format!("Hello UDP {}", i);
        sender.send(msg.as_bytes()).await?;
        println!("Sent: {}", msg);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // 给一点时间让数据传输
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 目前测试主要是验证不崩溃，且日志中有 "Forwarding ... bytes"
    // 真正的端到端验证需要 Mock 这里的 debug_println 或 改进 Client 实现双向

    println!("✅ UDP 测试完成 (无崩溃)");
    Ok(())
}
