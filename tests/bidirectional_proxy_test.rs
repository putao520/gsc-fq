mod support;

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr};
use support::wait_for_port_ready;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use std::time::Duration;
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use gsc_fq::config::loader::ConfigFile;

const PROXY_SERVER_PORT: u16 = 9001;
const PROXY_CLIENT_LISTEN_PORT: u16 = 9000;

#[tokio::test]
async fn test_bidirectional_data_transfer() -> Result<()> {
    println!("🔄 测试双向数据传输");

    std::env::set_var("YAMUX_POOL_SIZE", "1");

    // 1. 启动echo服务器（它会回显收到的所有数据）
    println!("📡 启动echo服务器");
    let echo_server = support::TestServer::start_echo().await?;
    let echo_addr = echo_server.addr();
    println!("✅ Echo服务器启动在: {}", echo_addr);

    // 2. 配置反向代理指向echo服务器
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

    // 4. 启动反向代理客户端
    println!("🔗 启动反向代理客户端");
    let server_addr = std::net::SocketAddr::new(bind_ip, PROXY_SERVER_PORT);
    let client_handle = tokio::spawn(async move {
        let mut client = ReverseProxyClient::new(server_addr, config);
        let _ = client.start().await;
    });

    tokio::time::sleep(Duration::from_secs(2)).await;
    wait_for_port_ready(PROXY_CLIENT_LISTEN_PORT, Duration::from_secs(5)).await?;
    println!("✅ 代理已就绪");

    // 5. 建立多个连接进行双向测试
    for round in 1..=3 {
        println!("🔄 第{}轮双向传输测试", round);

        let mut stream = timeout(
            Duration::from_secs(5),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", PROXY_CLIENT_LISTEN_PORT))
        ).await??;

        // 发送端口头部
        let port_bytes = (PROXY_CLIENT_LISTEN_PORT as u16).to_be_bytes();
        stream.write_all(&port_bytes).await?;
        stream.flush().await?;

        // 客户端发送多个消息
        let messages = vec![
            format!("Message {} from client", round),
            "Another message".to_string(),
            "Final message".to_string(),
        ];

        for (i, msg) in messages.iter().enumerate() {
            println!("📤 客户端发送消息 {}: {}", i+1, msg);
            stream.write_all(msg.as_bytes()).await?;
            stream.flush().await?;

            // 读取echo响应
            let mut response = vec![0u8; msg.len()];
            timeout(Duration::from_secs(3), stream.read_exact(&mut response)).await??;
            let response_str = String::from_utf8_lossy(&response);
            println!("📥 客户端收到响应: {}", response_str.trim());

            // 验证echo正确
            assert_eq!(msg.trim(), response_str.trim(), "Echo应该完全匹配");
        }

        // 发送一些二进制数据测试
        let binary_data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        println!("📤 客户端发送二进制数据: {:?}", binary_data);
        stream.write_all(&binary_data).await?;
        stream.flush().await?;

        let mut binary_response = vec![0u8; binary_data.len()];
        timeout(Duration::from_secs(3), stream.read_exact(&mut binary_response)).await??;
        println!("📥 客户端收到二进制响应: {:?}", binary_response);
        assert_eq!(binary_data, binary_response, "二进制数据应该完全匹配");

        println!("✅ 第{}轮双向传输测试完成", round);
    }

    // 6. 测试并发连接
    println!("🔄 测试并发连接...");
    let mut handles = vec![];

    for i in 1..=3 {
        let handle = tokio::spawn(async move {
            let mut stream = timeout(
                Duration::from_secs(3),
                tokio::net::TcpStream::connect(format!("127.0.0.1:{}", PROXY_CLIENT_LISTEN_PORT))
            ).await.unwrap().unwrap();

            // 发送端口头部
            let port_bytes = (PROXY_CLIENT_LISTEN_PORT as u16).to_be_bytes();
            stream.write_all(&port_bytes).await.unwrap();
            stream.flush().await.unwrap();

            let msg = format!("Concurrent message {}", i);
            stream.write_all(msg.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();

            let mut response = vec![0u8; msg.len()];
            timeout(Duration::from_secs(3), stream.read_exact(&mut response)).await.unwrap().unwrap();

            let response_str = String::from_utf8_lossy(&response);
            println!("并发连接{} 收到: {}", i, response_str.trim());
            assert_eq!(msg, response_str.trim());
        });
        handles.push(handle);
    }

    // 等待所有并发连接完成
    for handle in handles {
        handle.await?;
    }
    println!("✅ 并发连接测试完成");

    // 7. 清理资源
    println!("🧹 清理资源");
    client_handle.abort();
    server_handle.abort();
    echo_server.shutdown().await?;

    let _ = client_handle.await;
    let _ = server_handle.await;

    println!("✅ 双向数据传输测试完成");
    Ok(())
}