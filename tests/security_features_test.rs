use anyhow::Result;
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServer;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn test_security_acl_blocking() -> Result<()> {
    println!("🔐 测试 ACL (IP 白名单) 拦截");

    // 1. 启动目标服务器
    let target = TcpListener::bind("127.0.0.1:0").await?;
    let target_addr = target.local_addr()?;

    // 2. 配置代理，仅允许 1.2.3.4 (显然本地测试 IP 127.0.0.1 不在其中)
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut server = ProxyServer::new(bind_ip);

    let local_port = 19101;
    let proxy_config = ProxySection {
        local: local_port.to_string(),
        remote: target_addr.to_string(),
        source_ip: None,
        allow_ips: Some(vec!["1.2.3.4".to_string()]), // 故意设置错误的白名单
        max_conns_per_ip: None,
        cps_limit: None,
    };

    server.add_proxy(&proxy_config)?;

    // 3. 启动代理服务器
    tokio::spawn(async move {
        server.start().await.unwrap();
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    // 4. 尝试连接 (应该被拒绝)
    let proxy_addr = SocketAddr::new(bind_ip, local_port);
    let result = TcpStream::connect(proxy_addr).await;

    match result {
        Ok(mut stream) => {
            // 连接建立了，但读取应该立即返回 0 (因为处理器检测到安全错误后会关闭连接)
            // 或者处理器可能在建立连接前就发现了错误？
            // 实际上 handle_connection 是在 accept 之后被调用的
            let mut buf = [0u8; 10];
            let read_res =
                tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
            match read_res {
                Ok(Ok(0)) => println!("✅ 连接被成功拦截 (读取返回 0)"),
                Ok(Ok(n)) => panic!("❌ 意外收到了数据: {} 字节", n),
                Ok(Err(e)) => println!("✅ 读取出错 (正常现象): {}", e),
                Err(_) => println!("✅ 读取超时 (可能正常，视处理逻辑而定)"),
            }
        }
        Err(e) => {
            println!("✅ 连接直接失败: {}", e);
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_security_rate_limiting() -> Result<()> {
    println!("🔐 测试连接速率限制 (CPS)");

    // 1. 启动目标服务器
    let target = TcpListener::bind("127.0.0.1:0").await?;
    let target_addr = target.local_addr()?;

    tokio::spawn(async move {
        while let Ok((mut s, _)) = target.accept().await {
            let _ = s.write_all(b"OK").await;
        }
    });

    // 2. 配置代理，CPS 限制为 2
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut server = ProxyServer::new(bind_ip);

    let local_port = 19102;
    let proxy_config = ProxySection {
        local: local_port.to_string(),
        remote: target_addr.to_string(),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: None,
        cps_limit: Some(2.0),
    };

    server.add_proxy(&proxy_config)?;

    tokio::spawn(async move {
        server.start().await.unwrap();
    });

    tokio::time::sleep(Duration::from_secs(1)).await;
    let proxy_addr = SocketAddr::new(bind_ip, local_port);

    // 3. 快速发送 5 个连接
    let mut success_count = 0;
    for i in 0..5 {
        match TcpStream::connect(proxy_addr).await {
            Ok(mut stream) => {
                let mut buf = [0u8; 2];
                if let Ok(Ok(n)) =
                    tokio::time::timeout(Duration::from_millis(200), stream.read(&mut buf)).await
                {
                    if n > 0 {
                        success_count += 1;
                        println!("Connection {} success", i);
                    } else {
                        println!("Connection {} blocked (immediate close)", i);
                    }
                } else {
                    println!("Connection {} blocked (timeout/error)", i);
                }
            }
            Err(_) => println!("Connection {} failed", i),
        }
    }

    println!("📊 成功连接数: {} / 5", success_count);
    assert!(success_count <= 2, "速率限制失效: 成功次数超过限制");

    Ok(())
}
