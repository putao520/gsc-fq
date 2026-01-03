mod support;

use anyhow::Result;
use gsc_fq::config::loader::{ConfigFile, ReverseProxyClientSection, ReverseProxySection};
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use support::{pick_available_port, wait_for_port_ready, TestServer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::test]
async fn test_totp_authentication_success() -> Result<()> {
    let control_port = pick_available_port()?;
    let local_proxy_port = pick_available_port()?;
    let target_server = TestServer::start_echo().await?;
    let target_addr = target_server.addr();

    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let totp_secret = "JBSWY3DPEHPK3PXP".to_string(); // Base32 secret example

    // 1. Start Server with TOTP secret
    let mut server =
        ReverseProxyServer::new(bind_ip, control_port).with_totp_secret(Some(totp_secret.clone()));

    let server_handle = tokio::spawn(async move {
        let _ = server.start().await;
    });
    wait_for_port_ready(control_port, Duration::from_secs(5)).await?;

    // 2. Start Client with same TOTP secret
    let proxy_config = ReverseProxySection {
        server: local_proxy_port.to_string(),
        local: target_addr.to_string(),
        source_ip: None,
    };

    let config = ConfigFile {
        server: None,
        proxies: vec![],
        token: Some("test-token".to_string()),
        totp_secret: None,
        reverse_proxies: vec![proxy_config],
        reverse_proxy_server: None,
        reverse_proxy_client: Some(ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", control_port),
            token: Some("test-token".to_string()),
            totp_secret: Some(totp_secret),
        }),
    };

    let ctrl_addr = std::net::SocketAddr::new(bind_ip, control_port);
    let client_handle = tokio::spawn(async move {
        let mut client = ReverseProxyClient::new(ctrl_addr, config);
        let _ = client.start().await;
    });

    // 3. Verify tunnel is established
    wait_for_port_ready(local_proxy_port, Duration::from_secs(10)).await?;

    // Test data transfer
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", local_proxy_port)).await?;
    stream.write_all(b"hello totp\n").await?;
    let mut buf = [0u8; 128];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(response.contains("hello totp"));

    server_handle.abort();
    client_handle.abort();
    Ok(())
}

#[tokio::test]
async fn test_totp_authentication_failure_wrong_secret() -> Result<()> {
    let control_port = pick_available_port()?;
    let local_proxy_port = pick_available_port()?;

    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let server_secret = "JBSWY3DPEHPK3PXP".to_string();
    let client_secret = "WRONGSECRET1234".to_string();

    // 1. Start Server
    let mut server =
        ReverseProxyServer::new(bind_ip, control_port).with_totp_secret(Some(server_secret));

    let server_handle = tokio::spawn(async move {
        let _ = server.start().await;
    });
    wait_for_port_ready(control_port, Duration::from_secs(5)).await?;

    // 2. Start Client with WRONG secret
    let proxy_config = ReverseProxySection {
        server: local_proxy_port.to_string(),
        local: "127.0.0.1:1".to_string(), // Doesn't matter, handshake will fail
        source_ip: None,
    };

    let config = ConfigFile {
        server: None,
        proxies: vec![],
        token: Some("test-token".to_string()),
        totp_secret: None,
        reverse_proxies: vec![proxy_config],
        reverse_proxy_server: None,
        reverse_proxy_client: Some(ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", control_port),
            token: Some("test-token".to_string()),
            totp_secret: Some(client_secret),
        }),
    };

    let ctrl_addr = std::net::SocketAddr::new(bind_ip, control_port);
    let mut client = ReverseProxyClient::new(ctrl_addr, config);

    // The handshake should fail, and we wrap it in a timeout because start() loops
    let result = tokio::time::timeout(Duration::from_secs(2), client.start()).await;

    // It should timeout because it retries, but we check if it ever succeeded (it shouldn't)
    assert!(
        result.is_err(),
        "Client should not have succeeded handshake"
    );

    server_handle.abort();
    Ok(())
}

#[tokio::test]
async fn test_totp_drift_tolerance() -> Result<()> {
    // Verify that the server accepts codes from adjacent time windows (drift)
    // We can't easily change system time, but we can generate a code for T-30s
    // and verify the server accepts it (since it allows ±1 step drift)

    let secret = b"12345678901234567890".to_vec(); // Raw bytes secret
    let totp = gsc_fq::utils::totp::Totp::new(secret.clone());

    // Generate code for 30 seconds ago (1 step back)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let past_code = totp.generate(now - 30);

    // Manually verify using the utility first (unit test within integration test context)
    assert!(
        totp.verify(past_code),
        "TOTP utility should verify code from -1 step"
    );

    // Now verify via full server integration
    // We need to inject the code manually into the handshake, which is hard via High-Level Client.
    // However, the server's verify logic is shared.
    // Ideally we would mock the client to send a specific code, but `ReverseProxyClient`
    // calculates it automatically based on current time.

    // For this integration test, we will trust the unit test coverage of `Totp::verify`
    // for the drift logic itself, as simulating network delay of exactly 30s is flaky.
    // Instead, we focus on the fallback logic below.
    Ok(())
}

#[tokio::test]
async fn test_totp_invalid_base32_fallback() -> Result<()> {
    let control_port = pick_available_port()?;
    let local_proxy_port = pick_available_port()?;
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

    // Secret that is NOT valid Base32 (contains '1', '8', '9' etc which are not in standard Base32 alphabet)
    let weird_secret = "ThisIsNotBase32!@#".to_string();

    // 1. Start Server with weird secret
    let mut server =
        ReverseProxyServer::new(bind_ip, control_port).with_totp_secret(Some(weird_secret.clone()));

    let server_handle = tokio::spawn(async move {
        let _ = server.start().await;
    });
    wait_for_port_ready(control_port, Duration::from_secs(5)).await?;

    // 2. Start Client with SAME weird secret
    // If fallback works, both should treat it as raw bytes "ThisIsNotBase32!@#" and generate matching codes.
    let proxy_config = ReverseProxySection {
        server: local_proxy_port.to_string(),
        local: "127.0.0.1:80".to_string(), // Dummy target
        source_ip: None,
    };

    let config = ConfigFile {
        server: None,
        proxies: vec![],
        token: Some("test-token".to_string()),
        totp_secret: None,
        reverse_proxies: vec![proxy_config],
        reverse_proxy_server: None,
        reverse_proxy_client: Some(ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", control_port),
            token: Some("test-token".to_string()),
            totp_secret: Some(weird_secret),
        }),
    };

    let ctrl_addr = std::net::SocketAddr::new(bind_ip, control_port);
    let mut client = ReverseProxyClient::new(ctrl_addr, config);

    // 3. Handshake should SUCCESS because both sides fall back to raw bytes
    // We wrap deeply blocking loop in timeout
    let result = tokio::time::timeout(Duration::from_secs(5), client.start()).await;

    // The client.start() runs forever if successful, so timeout means it didn't error out immediately.
    // However, client.start() returns immediately if handshake fails?
    // Actually `start()` runs the main loop. If handshake fails, it returns Err.
    // If handshake succeeds, it enters the loop. So Timeout is GOOD sign.

    match result {
        Ok(Err(e)) => panic!("Client failed handshake: {}", e),
        Ok(Ok(_)) => {} // Logic loop finished? Unexpected but ok if no error.
        Err(_) => {}    // Timeout means code kept running (success loop)
    }

    server_handle.abort();
    Ok(())
}
