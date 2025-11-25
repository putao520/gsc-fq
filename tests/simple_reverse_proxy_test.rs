mod support;

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use support::wait_for_port_ready;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use std::time::Duration;
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use gsc_fq::config::loader::ConfigFile;

// 固定端口配置
const PROXY_SERVER_PORT: u16 = 9001;
const PROXY_CLIENT_LISTEN_PORT: u16 = 9000;

#[tokio::test]
async fn test_basic_reverse_proxy_functionality() -> Result<()> {
    println!("🧪 基础反向代理功能测试");

    // 设置环境变量
    std::env::set_var("YAMUX_POOL_SIZE", "1");

    // 1. 启动简单的echo服务器作为目标
    println!("📡 启动目标echo服务器");
    let echo_server = support::TestServer::start_echo().await?;
    let echo_addr = echo_server.addr();
    println!("✅ Echo服务器启动在: {}", echo_addr);

    // 2. 配置反向代理
    let proxy_config = gsc_fq::config::loader::ReverseProxySection {
        server: PROXY_CLIENT_LISTEN_PORT.to_string(),
        local: format!("{}:{}", echo_addr.ip(), echo_addr.port()),
        source_ip: None,
    };

    let config = ConfigFile {
        server: None,
        proxies: vec![],
        reverse_proxies: vec![proxy_config],
        reverse_proxy_server: None,
        reverse_proxy_client: Some(gsc_fq::config::loader::ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", PROXY_SERVER_PORT),
        }),
    };

    // 3. 启动反向代理服务端
    println!("🔄 启动反向代理服务端");
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut server = ReverseProxyServer::new(bind_ip, PROXY_SERVER_PORT);
    let server_handle = tokio::spawn(async move {
        let _ = server.start().await;
    });

    wait_for_port_ready(PROXY_SERVER_PORT, Duration::from_secs(5)).await?;
    println!("✅ 服务端已启动");

    // 4. 启动反向代理客户端
    println!("🔗 启动反向代理客户端");
    let server_addr = SocketAddr::new(bind_ip, PROXY_SERVER_PORT);
    let client_handle = tokio::spawn(async move {
        let mut client = ReverseProxyClient::new(server_addr, config);
        let _ = client.start().await;
    });

    // 等待代理设置完成
    tokio::time::sleep(Duration::from_secs(2)).await;
    wait_for_port_ready(PROXY_CLIENT_LISTEN_PORT, Duration::from_secs(5)).await?;
    println!("✅ 代理已就绪");

    // 5. 测试通过代理连接
    println!("🌐 测试代理连接...");
    let mut stream = timeout(
        Duration::from_secs(5),
        TcpStream::connect(format!("127.0.0.1:{}", PROXY_CLIENT_LISTEN_PORT))
    ).await??;

    // 6. 按照协议发送端口头部 (服务端口9000)
    let server_port_bytes = (PROXY_CLIENT_LISTEN_PORT as u16).to_be_bytes();
    stream.write_all(&server_port_bytes).await?;
    stream.flush().await?;

    // 7. 发送测试数据
    let test_data = b"Hello, Proxy!";
    stream.write_all(test_data).await?;
    stream.flush().await?;

    // 8. 读取回显数据
    let mut response = vec![0u8; test_data.len()];
    timeout(Duration::from_secs(5), stream.read_exact(&mut response)).await??;

    // 9. 验证数据
    assert_eq!(test_data, &response[..], "代理应该回显相同的数据");
    println!("✅ 数据回显测试通过");

    // 10. 清理资源
    println!("🧹 清理资源");
    client_handle.abort();
    server_handle.abort();
    echo_server.shutdown().await?;

    let _ = client_handle.await;
    let _ = server_handle.await;

    println!("✅ 基础反向代理功能测试完成");
    Ok(())
}