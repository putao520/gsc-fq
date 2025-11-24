//! Debug test to isolate yamux stream handling issues

use std::time::Duration;
use tokio::time::timeout;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use gsc_fq::reverse_proxy::{
    protocol::ReverseProxyConfig,
    server::ReverseProxyServer,
    client::ReverseProxyClient,
};
use gsc_fq::config::ConfigFile;

mod support;
use support::PingPongServer;

#[tokio::test]
async fn debug_yamux_stream_issue() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 Debug: Testing Yamux Stream Handling");

    // Set environment variable for authentication
    std::env::set_var("REVERSE_PROXY_TOKEN", "debug-token");

    // Start PingPong server
    let pingpong_server = PingPongServer::start().await?;
    let pingpong_addr = pingpong_server.addr();
    println!("   ✅ PingPong server started on {}", pingpong_addr);

    // Configure reverse proxy
    let proxy_config = ReverseProxyConfig {
        server_port: 9100,  // External port
        local_host: "127.0.0.1".to_string(),
        local_port: pingpong_addr.port(),
    };

    // Create config file for client
    let config = ConfigFile {
        server: None, // No server config needed for client
        proxies: vec![],
        reverse_proxies: vec![], // Will be populated with ReverseProxySection
        reverse_mode: Some("client".to_string()),
        reverse_target: Some("127.0.0.1:9101".to_string()),
    };

    // Start reverse proxy server
    let mut server = ReverseProxyServer::new("127.0.0.1".parse()?, 9101);
    let server_handle = server.start();

    println!("   ✅ Reverse proxy server started on control port 9101");

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Start reverse proxy client
    let mut client = ReverseProxyClient::new("127.0.0.1:9101".parse()?, config);
    let client_handle = client.start();

    println!("   ✅ Reverse proxy client started");

    // Give client time to connect and establish yamux
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test direct connection to proxy port
    println!("   🔍 Testing connection to proxy port 9100...");

    match timeout(Duration::from_secs(5), TcpStream::connect("127.0.0.1:9100")).await {
        Ok(Ok(mut stream)) => {
            println!("   ✅ Connected to proxy port 9100");

            // Send HTTP request
            let request = "GET /ping HTTP/1.1\r\nHost: 127.0.0.1:9100\r\nConnection: close\r\n\r\n";
            stream.write_all(request.as_bytes()).await?;
            stream.flush().await?;
            println!("   ✅ Sent HTTP request: {}", request.trim());

            // Read response
            let mut response = String::new();
            let mut buffer = [0u8; 1024];

            match timeout(Duration::from_secs(5), stream.read(&mut buffer)).await {
                Ok(Ok(n)) => {
                    response.push_str(&String::from_utf8_lossy(&buffer[..n]));
                    println!("   📥 Received response: {}", response.trim());

                    if response.contains("Pong") {
                        println!("   🎉 SUCCESS: Received PingPong response!");
                    } else {
                        println!("   ❌ FAILURE: Response doesn't contain expected 'Pong'");
                        return Err("Invalid response from PingPong server".into());
                    }
                }
                Ok(Err(e)) => {
                    println!("   ❌ Read error: {}", e);
                    return Err(e.into());
                }
                Err(_) => {
                    println!("   ❌ Read timeout");
                    return Err("Read timeout".into());
                }
            }
        }
        Ok(Err(e)) => {
            println!("   ❌ Failed to connect to proxy port 9100: {}", e);
            return Err(e.into());
        }
        Err(_) => {
            println!("   ❌ Connection timeout to proxy port 9100");
            return Err("Connection timeout".into());
        }
    }

    // Cleanup
    println!("   🧹 Cleaning up...");
    drop(client_handle);
    drop(server_handle);
    pingpong_server.shutdown().await?;

    println!("🎉 Debug test completed successfully!");
    Ok(())
}