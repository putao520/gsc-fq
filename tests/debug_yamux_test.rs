//! Debug test to isolate yamux stream handling issues

use std::time::Duration;
use tokio::time::timeout;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use gsc_fq::reverse_proxy::{
    server::ReverseProxyServer,
    client::ReverseProxyClient,
};
use gsc_fq::config::loader::{ReverseProxySection, ReverseProxyServerSection, ReverseProxyClientSection};
use gsc_fq::config::ConfigFile;

mod support;
use support::PingPongServer;

/// Allocate a unique port for testing
async fn allocate_port() -> Result<u16, Box<dyn std::error::Error>> {
    // Use random port allocation to avoid conflicts
    let mut attempts = 0;
    while attempts < 100 {
        let port = 9000 + rand::random::<u16>() % 1000;
        match std::net::TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => {
                drop(listener); // Close listener immediately
                println!("   🔧 Allocated port: {}", port);
                return Ok(port);
            }
            Err(_) => {
                attempts += 1;
                continue;
            }
        }
    }
    Err("Failed to allocate port after 100 attempts".into())
}

#[tokio::test]
async fn debug_yamux_stream_issue() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 Debug: Testing Yamux Stream Handling");

    // Start PingPong server
    let pingpong_server = PingPongServer::start().await?;
    let pingpong_addr = pingpong_server.addr();
    println!("   ✅ PingPong server started on {}", pingpong_addr);

    // Use dynamic port allocation to avoid conflicts
    let external_port = allocate_port().await?;

    // Add small delay to ensure port is fully released before second allocation
    tokio::time::sleep(Duration::from_millis(10)).await;

    let control_port = allocate_port().await?;

    // Configure reverse proxy
    let reverse_proxy_config = vec![ReverseProxySection {
        server: external_port.to_string(),  // External port only
        local: format!("127.0.0.1:{}", pingpong_addr.port()),
        source_ip: None,
    }];

    // Create config file for client
    let config = ConfigFile {
        server: Some(gsc_fq::config::loader::ServerSection {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(true),
        }),
        proxies: vec![],
        reverse_proxies: reverse_proxy_config, // ✅ 正确传递配置
        reverse_proxy_server: Some(ReverseProxyServerSection {
            port: control_port,
            allowed_tokens: vec![], // No tokens for testing
        }),
        reverse_proxy_client: Some(ReverseProxyClientSection {
            server: format!("127.0.0.1:{}", control_port),
        }),
    };
    let mut server = ReverseProxyServer::new("127.0.0.1".parse()?, control_port);
    let server_handle = server.start();

    println!("   ✅ Reverse proxy server started on control port {}", control_port);

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Start reverse proxy client
    let mut client = ReverseProxyClient::new(format!("127.0.0.1:{}", control_port).parse()?, config);
    let client_handle = tokio::spawn(async move {
        if let Err(e) = client.start().await {
            eprintln!("❌ Client error: {}", e);
        }
    });

    println!("   ✅ Reverse proxy client started");

    // Give client time to connect and establish yamux
    println!("   ⏳ Waiting for client to connect and establish yamux...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check if the external port is actually listening
    println!("   🔍 Checking if proxy port {} is listening...", external_port);
    match timeout(Duration::from_secs(3), TcpStream::connect(format!("127.0.0.1:{}", external_port))).await {
        Ok(Ok(_stream)) => {
            println!("   ✅ Proxy port {} is listening!", external_port);

            // Test direct connection to proxy port
            println!("   🔍 Testing connection to proxy port {}...", external_port);
            drop(_stream); // Drop the first connection, create a new one for test

            match timeout(Duration::from_secs(5), TcpStream::connect(format!("127.0.0.1:{}", external_port))).await {
                Ok(Ok(mut stream)) => {
                    println!("   ✅ Connected to proxy port {}", external_port);

                    // Send HTTP request
                    let request = format!("GET /ping HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", external_port);
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
                    println!("   ❌ Failed to connect to proxy port {}: {}", external_port, e);
                    return Err(e.into());
                }
                Err(_) => {
                    println!("   ❌ Connection timeout to proxy port {}", external_port);
                    return Err("Connection timeout".into());
                }
            }
        }
        Ok(Err(e)) => {
            println!("   ❌ Failed to connect to proxy port {}: {}", external_port, e);
            return Err(e.into());
        }
        Err(_) => {
            println!("   ❌ Connection timeout to proxy port {}", external_port);
            return Err("Connection timeout".into());
        }
    }

    // Cleanup
    println!("   🧹 Cleaning up...");
    client_handle.abort();
    drop(client_handle);
    drop(server_handle);
    pingpong_server.shutdown().await?;

    println!("🎉 Debug test completed successfully!");
    Ok(())
}