mod support;

use anyhow::Result;
use gsc_fq::config::loader::ConfigFile;
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use support::{pick_available_port, wait_for_port_ready};
use tokio::net::UdpSocket;

const TIMEOUT: Duration = Duration::from_secs(5);

async fn setup_udp_reverse_proxy() -> Result<(
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
    u16,
    u16,
    u16,
)> {
    let control_port = pick_available_port()?;
    let proxy_server_udp_port = pick_available_port()?; // Public facing UDP port on server
    let local_target_udp_port = pick_available_port()?; // The actual local service port

    // 1. Start Echo Server (Local Target)
    let echo_socket = UdpSocket::bind(format!("127.0.0.1:{}", local_target_udp_port)).await?;
    tokio::spawn(async move {
        let mut buf = [0u8; 65535]; // Max UDP size
        loop {
            if let Ok((n, peer)) = echo_socket.recv_from(&mut buf).await {
                // simple echo
                let _ = echo_socket.send_to(&buf[..n], peer).await;
            }
        }
    });

    // 2. Start Reverse Proxy Server
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    // Note: Reusing the same port for TCP control and UDP listening might be the design,
    // or separate. Based on implementation, `ReverseProxyServer` opens a UDP listener
    // on `control_port`? No, wait.
    // `ReverseProxyServer` listens on `control_port` for TCP.
    // The "Public" mapping is defined in the Client's config, telling the Server
    // to listen on a specific port.
    // Actually, looking at `ReverseProxyServer`, it doesn't dynamically open ports
    // requested by client in the current code (it might, but let's check `ReverseProxyServer` impl).
    // Ah, `ReverseProxyServer` acts as the bridge.
    // The `Client` says: "Please expose my local service on ServerPort X".
    // Wait, looking at `udp_over_tcp_test.rs`, configuration is:
    // `server: PROXY_CLIENT_UDP_PORT.to_string()` in `ReverseProxySection`.
    // This tells the Server to listen on `PROXY_CLIENT_UDP_PORT`.

    // So we need to start the Server first.
    // But `ReverseProxyServer` constructor takes `control_port`.
    // Does it listen on other ports?
    // The `ReverseProxyServer` handles client connections.
    // when a client connects and completes handshake, the server starts listening on
    // the requested ports (TCP/UDP).

    let mut server = ReverseProxyServer::new(bind_ip, control_port);
    // We need to enable TOTP or Token? Config says none for simplicity unless enforced.

    let server_handle = tokio::spawn(async move {
        let _ = server.start().await;
    });
    wait_for_port_ready(control_port, Duration::from_secs(5)).await?;

    // 3. Start Client
    let proxy_config = gsc_fq::config::loader::ReverseProxySection {
        server: proxy_server_udp_port.to_string(),
        local: format!("127.0.0.1:{}", local_target_udp_port),
        source_ip: None,
    };

    let config = ConfigFile {
        server: None,
        proxies: vec![],
        token: Some("test-token".to_string()),
        totp_secret: None,
        reverse_proxies: vec![proxy_config],
        reverse_proxy_server: None,
        reverse_proxy_client: Some(gsc_fq::config::loader::ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", control_port),
            token: Some("test-token".to_string()),
            totp_secret: None,
        }),
    };

    let ctrl_addr = std::net::SocketAddr::new(bind_ip, control_port);
    // Clone config because `new` takes ownership? No, `new` takes config.
    let mut client = ReverseProxyClient::new(ctrl_addr, config);
    let client_handle = tokio::spawn(async move {
        let _ = client.start().await;
    });

    // Wait for the tunnel to establish and the UDP port to open on the server
    tokio::time::sleep(Duration::from_secs(2)).await;

    Ok((
        server_handle,
        client_handle,
        control_port,
        proxy_server_udp_port,
        local_target_udp_port,
    ))
}

#[tokio::test]
async fn test_udp_integrity_echo() -> Result<()> {
    let (server_h, client_h, _, proxy_port, _) = setup_udp_reverse_proxy().await?;

    // Client to test the proxy
    let app_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let target = format!("127.0.0.1:{}", proxy_port);
    app_socket.connect(&target).await?;

    // Verify Integrity
    let payloads = vec![
        b"Small packet".to_vec(),
        b"Medium packet with more data including 1234567890".to_vec(),
        vec![0xAA; 1000], // 1KB
    ];

    let mut buf = [0u8; 65535];
    for payload in payloads {
        app_socket.send(&payload).await?;

        let res = tokio::time::timeout(TIMEOUT, app_socket.recv(&mut buf)).await;
        match res {
            Ok(Ok(n)) => {
                assert_eq!(n, payload.len(), "Echoed length mismatch");
                assert_eq!(&buf[..n], &payload[..], "Echoed consistency mismatch");
            }
            Ok(Err(e)) => panic!("Recv error: {}", e),
            Err(_) => panic!("Recv timeout for payload len {}", payload.len()),
        }
    }

    server_h.abort();
    client_h.abort();
    Ok(())
}

#[tokio::test]
async fn test_udp_fragmentation_large_packet() -> Result<()> {
    let (server_h, client_h, _, proxy_port, _) = setup_udp_reverse_proxy().await?;

    let app_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let target = format!("127.0.0.1:{}", proxy_port);
    app_socket.connect(&target).await?;

    // Verify Large Packet (Fragmentation handling)
    // Note: UDP over Internet is unreliable > MTU (1500), but generic UDP supports up to 64KB.
    // Our tunnel encapsulates UDP in Yamux frames.
    // If our implementation doesn't handle framing correctly, large packets might be truncated.
    let large_payload = vec![0xBB; 8192]; // 8KB, definitely > Ethernet MTU

    app_socket.send(&large_payload).await?;

    let mut buf = [0u8; 65535];
    let res = tokio::time::timeout(TIMEOUT, app_socket.recv(&mut buf)).await;

    match res {
        Ok(Ok(n)) => {
            assert_eq!(n, large_payload.len(), "Large packet length mismatch");
            assert_eq!(
                &buf[..n],
                &large_payload[..],
                "Large packet content mismatch"
            );
        }
        Ok(Err(e)) => panic!("Recv error: {}", e),
        Err(_) => panic!("Recv timeout for large packet"),
    }

    server_h.abort();
    client_h.abort();
    Ok(())
}

#[tokio::test]
async fn test_udp_zero_length() -> Result<()> {
    // Some protocols use 0-len UDP for keep-alives or probing
    let (server_h, client_h, _, proxy_port, _) = setup_udp_reverse_proxy().await?;

    let app_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let target = format!("127.0.0.1:{}", proxy_port);
    app_socket.connect(&target).await?;

    let empty_payload = b"";
    app_socket.send(empty_payload).await?;

    let mut buf = [0u8; 1024];
    let res = tokio::time::timeout(TIMEOUT, app_socket.recv(&mut buf)).await;

    // Depending on impl, 0-byte might be swallowed or echoed.
    // A robust proxy SHOULD forward 0-byte packets.
    match res {
        Ok(Ok(n)) => {
            assert_eq!(n, 0, "Expected 0 bytes echoed");
        }
        Ok(Err(e)) => panic!("Recv error: {}", e),
        Err(_) => panic!("Recv timeout for 0-byte packet (Proxy might have dropped it)"),
    }

    server_h.abort();
    client_h.abort();
    Ok(())
}
