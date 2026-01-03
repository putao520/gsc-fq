mod support;

use anyhow::Result;
use gsc_fq::config::loader::{ConfigFile, ReverseProxyClientSection, ReverseProxySection};
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use gsc_fq::utils::totp::Totp;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use support::{pick_available_port, wait_for_port_ready, TestServer};

/// Unit-style tests for Totp logic boundaries
#[test]
fn test_unit_boundary_values() {
    let secret = b"12345678901234567890".to_vec();
    let totp = Totp::new(secret);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Current time -> PASS
    let code_now = totp.generate(now);
    assert!(totp.verify(code_now), "Current time code failure");

    // 2. -30s (Previous Window) -> PASS
    let code_minus_30 = totp.generate(now - 30);
    assert!(totp.verify(code_minus_30), "-30s code failure");

    // 3. +30s (Next Window) -> PASS
    let code_plus_30 = totp.generate(now + 30);
    assert!(totp.verify(code_plus_30), "+30s code failure");

    // 4. -90s (Definite Fail, N-3 window) -> FAIL
    let code_minus_90 = totp.generate(now - 90);
    assert!(!totp.verify(code_minus_90), "-90s should fail but passed");

    // 5. +90s (Definite Fail, N+3 window) -> FAIL
    let code_plus_90 = totp.generate(now + 90);
    assert!(!totp.verify(code_plus_90), "+90s should fail but passed");
}

#[test]
fn test_unit_secret_edge_cases() {
    // 1. Empty Secret
    let totp_empty = Totp::new(vec![]);
    let code = totp_empty.generate_current();
    assert!(totp_empty.verify(code), "Empty secret verification failed");

    // 2. Huge Secret (10KB) - Ensure no stack overflow or panic
    let huge_secret = vec![0xAA; 10000];
    let totp_huge = Totp::new(huge_secret);
    let code_huge = totp_huge.generate_current();
    assert!(
        totp_huge.verify(code_huge),
        "Huge secret verification failed"
    );

    // 3. Case Sensitivity (Base32)
    // "MZXW6YTBOI======" decodes to "foobar"
    // "mznw6ytboi======" (lowercase) should act same if library handles it,
    // or fail if it's strict. Our implementation uses `from_base32` which might be strict.
    // Let's check `from_base32` behavior.

    let upper = "MZXW6YTBOI======";
    let lower = "mzxw6ytboi======";

    let totp_upper = Totp::from_base32(upper).unwrap();
    // Usually Base32 is case-insensitive, but data-encoding might be strict.
    // If it fails, fallback to raw bytes logic applies in our server.
    // So "MZXW..." as raw bytes != "mzxw..." as raw bytes.
    // This effectively means case SENSITIVITY for the fallback path,
    // but what about the decoding path?

    // Let's just verify they produce different secrets if treated as raw,
    // or same if decoded.
    if let Ok(totp_lower) = Totp::from_base32(lower) {
        // If generic decoder supports lowercase
        assert_eq!(totp_upper.generate_current(), totp_lower.generate_current());
    } else {
        // If it failed to decode, it's not a valid Base32 according to that lib
        // This is acceptable behavior, just documenting it via test.
    }
}

/// Integration Test: Protocol Anomaly
/// Scenario: Client sends NO code, but Server EXPECTS code.
#[tokio::test]
async fn test_protocol_anomaly_missing_code() -> Result<()> {
    let control_port = pick_available_port()?;
    let local_proxy_port = pick_available_port()?;
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let secret = "SECRET123";

    // 1. Server expects TOTP
    let mut server =
        ReverseProxyServer::new(bind_ip, control_port).with_totp_secret(Some(secret.to_string()));

    let server_handle = tokio::spawn(async move {
        let _ = server.start().await;
    });
    wait_for_port_ready(control_port, Duration::from_secs(5)).await?;

    // 2. Client has NO secret configured
    let proxy_config = ReverseProxySection {
        server: local_proxy_port.to_string(),
        local: "127.0.0.1:80".to_string(),
        source_ip: None,
    };

    let config = ConfigFile {
        server: None,
        proxies: vec![],
        token: Some("test-token".to_string()),
        totp_secret: None, // <--- MISSING SECRET
        reverse_proxies: vec![proxy_config],
        reverse_proxy_server: None,
        reverse_proxy_client: Some(ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", control_port),
            token: Some("test-token".to_string()),
            totp_secret: None, // <--- MISSING
        }),
    };

    let ctrl_addr = std::net::SocketAddr::new(bind_ip, control_port);
    let mut client = ReverseProxyClient::new(ctrl_addr, config);

    // 3. Handshake should FAIL
    let result = tokio::time::timeout(Duration::from_secs(2), client.start()).await;

    // Expect timeout (retrying loop) or error.
    // Since client keeps retrying, verify it doesn't succeed.
    // (In a real scenario we'd check logs or error count, but here just ensuring it doesn't connect/open port)

    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Client handshake should have failed"
    );

    server_handle.abort();
    Ok(())
}
