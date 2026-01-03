use anyhow::Result;
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServer;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

#[tokio::test]
async fn test_forward_proxy_udp_forwarding() -> Result<()> {
    println!("🔄 测试正向代理 UDP 转发");

    // 1. 启动 UDP Echo Server (目标服务器)
    let echo_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let echo_addr = echo_socket.local_addr()?;
    println!("📡 目标 UDP Echo 服务器启动在: {}", echo_addr);

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            if let Ok((n, peer)) = echo_socket.recv_from(&mut buf).await {
                println!("Echo Server received: {:?} from {}", &buf[..n], peer);
                // 回显给客户端
                echo_socket.send_to(&buf[..n], peer).await.unwrap();
            }
        }
    });

    // 2. 配置正向代理
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut server = ProxyServer::new(bind_ip);

    let local_port = 19001;
    let proxy_config = ProxySection {
        local: local_port.to_string(),
        remote: format!("{}:{}", echo_addr.ip(), echo_addr.port()),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: None,
        cps_limit: None,
    };

    server.add_proxy(&proxy_config)?;

    // 3. 启动代理服务器
    tokio::spawn(async move {
        server.start().await.unwrap();
    });

    // 等待服务器启动 (连接性检查默认 3 秒超时，所以等待 4 秒)
    tokio::time::sleep(Duration::from_secs(4)).await;

    // 4. 发送测试数据到代理端口
    let client_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let proxy_addr = SocketAddr::new(bind_ip, local_port);

    let test_msg = b"Hello Forward Proxy UDP";
    println!("📤 发送数据到代理: {}", proxy_addr);
    client_socket.send_to(test_msg, proxy_addr).await?;

    // 5. 接收回显
    let mut buf = [0u8; 1024];
    let result =
        tokio::time::timeout(Duration::from_secs(2), client_socket.recv_from(&mut buf)).await;

    match result {
        Ok(Ok((n, _))) => {
            let received = &buf[..n];
            println!("📥 收到回显: {:?}", received);
            assert_eq!(received, test_msg);
        }
        Ok(Err(e)) => panic!("接收错误: {}", e),
        Err(_) => panic!("接收超时 - UDP 转发可能失败了"),
    }

    println!("✅ 正向代理 UDP 转发验证成功");
    Ok(())
}
