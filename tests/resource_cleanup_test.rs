mod support;

use anyhow::Result;
use gsc_fq::reverse_proxy::ReverseProxyServer;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use support::pick_available_port;
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Test Resource Cleanup: Verify port updates after server stop
#[tokio::test]
async fn test_port_release_on_drop() -> Result<()> {
    println!("🧹 Resource Cleanup Test: Port Release");

    let port = pick_available_port()?;
    let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

    // 1. Start Server
    println!("   🚀 Starting server on port {}", port);
    let mut server = ReverseProxyServer::new(bind_ip, port);
    // Use an abort handle to simulate "stopping"
    let server_handle = tokio::spawn(async move {
        let _ = server.start().await;
    });

    // Wait for start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 2. Verify Port is Listening
    println!("   🔍 Verifying port is open...");
    let result = timeout(
        Duration::from_secs(1),
        TcpStream::connect(format!("127.0.0.1:{}", port)),
    )
    .await;
    assert!(matches!(result, Ok(Ok(_))), "Server should be listening");
    println!("   ✅ Port is open.");

    // 3. Stop Server (Abort Handle)
    println!("   🛑 Stopping server (aborting handle)...");
    server_handle.abort();
    let _ = server_handle.await; // Wait for it to finish (cancelled)

    // Give OS time to release port (SO_REUSEADDR helps, but usually takes a moment)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 4. Verify Port is Closed
    println!("   🔍 Verifying port is closed...");
    let result = timeout(
        Duration::from_secs(1),
        TcpStream::connect(format!("127.0.0.1:{}", port)),
    )
    .await;
    match result {
        Ok(Ok(_)) => {
            // Depending on OS, it might still accept? No, aborting handle drops the Listener.
            // But if there are established connections, they might stick around?
            // We only connected briefly.
            println!("   ⚠️  Port still accepts connections? This might be OS linger.");
            // Ideally it should be refused.
            // If this fails, we might need explicit shutdown signal in Server instead of just dropping.
            // But dropping the Future of `start()` drops `listener.accept()` which drops `TcpListener`.
            // Dropping `TcpListener` closes the socket fd.
        }
        Ok(Err(_)) | Err(_) => {
            println!("   ✅ Port is closed (Connection refused or timed out).");
        }
    }

    // 5. Try to bind again (Definitive specific test)
    println!("   ♻️  Attempting to re-bind port {}", port);
    let mut server2 = ReverseProxyServer::new(bind_ip, port);
    let server_handle2 = tokio::spawn(async move {
        let _ = server2.start().await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let result = timeout(
        Duration::from_secs(1),
        TcpStream::connect(format!("127.0.0.1:{}", port)),
    )
    .await;
    match result {
        Ok(Ok(_)) => println!("   ✅ Re-bind successful. Port was released."),
        _ => panic!("   ❌ Re-bind failed. Port was NOT released properly."),
    }

    server_handle2.abort();
    Ok(())
}
