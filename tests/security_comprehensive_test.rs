mod support;

use anyhow::Result;
use gsc_fq::config::loader::ProxySection;
use gsc_fq::proxy::ProxyServerBuilder;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use support::{pick_available_port, PingPongServer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Security Test 1: Positive ACL (Allowed IP)
/// Configures allow_ips with "127.0.0.1", expects connection success.
#[tokio::test]
async fn test_acl_positive_allow_ip() -> Result<()> {
    println!("🛡️ Security Test: Positive ACL (Allow 127.0.0.1)");

    // 1. Start Target Server
    let target_server = PingPongServer::start().await?;
    let target_port = target_server.port();

    // 2. Configure Proxy with ACL
    let proxy_port = pick_available_port()?;
    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", target_port),
        source_ip: None,
        allow_ips: Some(vec!["127.0.0.1".to_string()]), // Explicitly allow localhost
        max_conns_per_ip: None,
        cps_limit: None,
    };

    // 3. Start Proxy
    let mut instance = ProxyServerBuilder::new()
        .bind_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .add_proxy(proxy_config)
        .build()?;

    let server_handle = tokio::spawn(async move {
        let _ = instance.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // 4. Test Connection (Should Succeed)
    let result = timeout(
        Duration::from_secs(5),
        TcpStream::connect(format!("127.0.0.1:{}", proxy_port)),
    )
    .await;

    match result {
        Ok(Ok(mut stream)) => {
            println!("✅ Connection allowed as expected.");
            // Verify data flow - use HTTP format for PingPongServer
            let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            stream.write_all(request.as_bytes()).await?;

            let mut buf = [0u8; 128];
            let n = stream.read(&mut buf).await?;
            let response = String::from_utf8_lossy(&buf[..n]);
            assert!(
                response.contains("200") || response.contains("PONG"),
                "Expected valid response"
            );
            println!("✅ Data exchange successful.");
        }
        _ => panic!("❌ Connection failed but should have been allowed."),
    }

    server_handle.abort();
    Ok(())
}

/// Security Test 2: Negative ACL (Denied IP)
/// Configures allow_ips with "192.168.1.1", expects connection failure from 127.0.0.1.
#[tokio::test]
async fn test_acl_negative_deny_ip() -> Result<()> {
    println!("🛡️ Security Test: Negative ACL (Deny 127.0.0.1)");

    let target_server = PingPongServer::start().await?;
    let target_port = target_server.port();
    let proxy_port = pick_available_port()?;

    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", target_port),
        source_ip: None,
        allow_ips: Some(vec!["192.168.1.1".to_string()]), // Only allow 192.168.1.1
        max_conns_per_ip: None,
        cps_limit: None,
    };

    let mut instance = ProxyServerBuilder::new()
        .bind_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .add_proxy(proxy_config)
        .build()?;

    let server_handle = tokio::spawn(async move {
        let _ = instance.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // 4. Test Connection (Should Fail or Close Immediately)
    // The implementation might accept and then immediately close, or fail to handshake.
    // Let's see what happens.
    let result = timeout(
        Duration::from_secs(2),
        TcpStream::connect(format!("127.0.0.1:{}", proxy_port)),
    )
    .await;

    match result {
        Ok(Ok(mut stream)) => {
            // Connection might be accepted at TCP level but closed by application layer immediately.
            // Try to read/write.
            let write_res = stream.write_all(b"PING").await;
            if write_res.is_err() {
                println!("✅ Connection write failed as expected (Connection closed).");
                return Ok(());
            }

            let mut buf = [0u8; 1];
            let read_res = stream.read(&mut buf).await;
            match read_res {
                Ok(0) => println!("✅ Connection closed by remote (EOF) as expected."), // EOF
                Ok(_) => panic!("❌ Connection stayed open and received data! Security Fail!"),
                Err(_) => println!("✅ Connection read error as expected."),
            }
        }
        Ok(Err(_)) => println!("✅ Connection refused as expected."),
        Err(_) => println!("✅ Connection timed out (filtered) as expected."),
    }

    server_handle.abort();
    Ok(())
}

/// Security Test 3: Concurrency Limit (max_conns_per_ip)
/// Configures max_conns_per_ip = 1.
/// Establishes Connection A (Held open).
/// Attempts Connection B (Should fail/block).
#[tokio::test]
async fn test_concurrency_limit() -> Result<()> {
    println!("🛡️ Security Test: Concurrency Limit (Max 1)");

    let target_server = PingPongServer::start().await?;
    let target_port = target_server.port();
    let proxy_port = pick_available_port()?;

    let proxy_config = ProxySection {
        local: proxy_port.to_string(),
        remote: format!("127.0.0.1:{}", target_port),
        source_ip: None,
        allow_ips: None,
        max_conns_per_ip: Some(1), // Strict Limit
        cps_limit: None,
    };

    let mut instance = ProxyServerBuilder::new()
        .bind_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .add_proxy(proxy_config)
        .build()?;

    let server_handle = tokio::spawn(async move {
        let _ = instance.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // 1. Establish First Connection (Keep it open)
    let mut conn1 = TcpStream::connect(format!("127.0.0.1:{}", proxy_port)).await?;
    println!("✅ Connection 1 established.");
    conn1.write_all(b"PING").await?; // Send something to verify it's active

    // 2. Attempt Second Connection (Should Fail or Close immediately)
    println!("🔄 Attempting Connection 2 (Should be rejected)...");
    let conn2_result = timeout(
        Duration::from_secs(2),
        TcpStream::connect(format!("127.0.0.1:{}", proxy_port)),
    )
    .await;

    match conn2_result {
        Ok(Ok(mut conn2)) => {
            // Connection 2 connected, but should be closed immediately.
            let mut buf = [0u8; 1];
            match conn2.read(&mut buf).await {
                Ok(0) => println!("✅ Connection 2 closed by server (EOF) as expected."),
                Ok(_) => panic!("❌ Connection 2 received data! Concurrency Limit Fail!"),
                Err(_) => println!("✅ Connection 2 read error as expected."),
            }
        }
        Ok(Err(_)) => println!("✅ Connection 2 refused properly."),
        Err(_) => println!("✅ Connection 2 timed out properly."),
    }

    // cleanup
    server_handle.abort();
    Ok(())
}
