mod support;

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use support::{
    pick_available_port, PingPongServer, ReverseProxyClientHandle, ReverseProxyServerHandle,
    wait_for_port_ready, LOCALHOST,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use std::time::Duration;
use gsc_fq::config::loader::ReverseProxySection;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_reverse_proxy_server_client_ping_pong() -> Result<()> {
    let local_http_server = PingPongServer::start().await?;
    let local_port = local_http_server.port();
    println!("Local HTTP server started on port {}", local_port);

    let control_port = pick_available_port()?;
    let server_port = pick_available_port()?;
    println!("Control port: {}, Server port: {}", control_port, server_port);

    let proxy_server = ReverseProxyServerHandle::start(control_port).await?;
    println!("Reverse proxy server started");

    let reverse_proxy_config = vec![ReverseProxySection {
        port: None,
        server_port: Some(server_port),
        local_port: Some(local_port),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    }];

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), control_port);
    let proxy_client = ReverseProxyClientHandle::start(server_addr, reverse_proxy_config).await?;
    println!("Reverse proxy client started and connected");

    wait_for_port_ready(server_port, Duration::from_secs(5)).await?;
    println!("Server port {} is ready", server_port);

    let mut stream = TcpStream::connect((LOCALHOST, server_port)).await?;
    println!("Connected to server port {}", server_port);

    let request = format!(
        "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await?;
    println!("Sent HTTP request");

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).await?;
    println!("Response status: {}", status_line.trim());

    assert!(
        status_line.contains("200 OK"),
        "Expected 200 OK, got: {}",
        status_line
    );

    let mut headers_done = false;
    let mut line = String::new();
    while !headers_done {
        line.clear();
        reader.read_line(&mut line).await?;
        if line == "\r\n" || line == "\n" {
            headers_done = true;
        }
    }

    let mut body = String::new();
    reader.read_to_string(&mut body).await?;
    println!("Response body: {}", body);

    assert_eq!(body, "PONG", "Expected PONG response, got: {}", body);

    proxy_client.shutdown().await;
    proxy_server.shutdown().await;
    local_http_server.shutdown().await?;

    println!("Test completed successfully");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_reverse_proxy_multiple_ports() -> Result<()> {
    let local_http_server1 = PingPongServer::start().await?;
    let local_port1 = local_http_server1.port();

    let local_http_server2 = PingPongServer::start().await?;
    let local_port2 = local_http_server2.port();

    println!(
        "Local HTTP servers started on ports {} and {}",
        local_port1, local_port2
    );

    let control_port = pick_available_port()?;
    let server_port1 = pick_available_port()?;
    let server_port2 = pick_available_port()?;

    let proxy_server = ReverseProxyServerHandle::start(control_port).await?;

    let reverse_proxy_config = vec![
        ReverseProxySection {
            port: None,
            server_port: Some(server_port1),
            local_port: Some(local_port1),
            local_host: Some("127.0.0.1".to_string()),
            source_ip: None,
        },
        ReverseProxySection {
            port: None,
            server_port: Some(server_port2),
            local_port: Some(local_port2),
            local_host: Some("127.0.0.1".to_string()),
            source_ip: None,
        },
    ];

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), control_port);
    let proxy_client = ReverseProxyClientHandle::start(server_addr, reverse_proxy_config).await?;

    wait_for_port_ready(server_port1, Duration::from_secs(5)).await?;
    wait_for_port_ready(server_port2, Duration::from_secs(5)).await?;

    for (port, name) in [(server_port1, "first"), (server_port2, "second")] {
        let mut stream = TcpStream::connect((LOCALHOST, port)).await?;
        let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        stream.write_all(request.as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).await?;
        assert!(
            status_line.contains("200 OK"),
            "{} port failed",
            name
        );

        let mut headers_done = false;
        let mut line = String::new();
        while !headers_done {
            line.clear();
            reader.read_line(&mut line).await?;
            if line == "\r\n" || line == "\n" {
                headers_done = true;
            }
        }

        let mut body = String::new();
        reader.read_to_string(&mut body).await?;
        assert_eq!(body, "PONG", "{} port got wrong response", name);

        println!("Successfully tested {} port {}", name, port);
    }

    proxy_client.shutdown().await;
    proxy_server.shutdown().await;
    local_http_server1.shutdown().await?;
    local_http_server2.shutdown().await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_reverse_proxy_multiple_connections() -> Result<()> {
    let local_http_server = PingPongServer::start().await?;
    let local_port = local_http_server.port();

    let control_port = pick_available_port()?;
    let server_port = pick_available_port()?;

    let proxy_server = ReverseProxyServerHandle::start(control_port).await?;

    let reverse_proxy_config = vec![ReverseProxySection {
        port: None,
        server_port: Some(server_port),
        local_port: Some(local_port),
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    }];

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), control_port);
    let proxy_client = ReverseProxyClientHandle::start(server_addr, reverse_proxy_config).await?;

    wait_for_port_ready(server_port, Duration::from_secs(5)).await?;

    let mut handles = vec![];
    for i in 0..5 {
        let handle = tokio::spawn(async move {
            let mut stream = TcpStream::connect((LOCALHOST, server_port)).await?;
            let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            stream.write_all(request.as_bytes()).await?;

            let mut reader = BufReader::new(stream);
            let mut status_line = String::new();
            reader.read_line(&mut status_line).await?;

            assert!(
                status_line.contains("200 OK"),
                "Connection {} failed",
                i
            );

            let mut headers_done = false;
            let mut line = String::new();
            while !headers_done {
                line.clear();
                reader.read_line(&mut line).await?;
                if line == "\r\n" || line == "\n" {
                    headers_done = true;
                }
            }

            let mut body = String::new();
            reader.read_to_string(&mut body).await?;
            assert_eq!(body, "PONG");

            Ok::<_, anyhow::Error>(())
        });
        handles.push(handle);
    }

    for (i, handle) in handles.into_iter().enumerate() {
        handle.await??;
        println!("Connection {} completed successfully", i);
    }

    proxy_client.shutdown().await;
    proxy_server.shutdown().await;
    local_http_server.shutdown().await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_reverse_proxy_with_port_shorthand() -> Result<()> {
    let local_http_server = PingPongServer::start().await?;
    let local_port = local_http_server.port();

    let control_port = pick_available_port()?;

    let proxy_server = ReverseProxyServerHandle::start(control_port).await?;

    let reverse_proxy_config = vec![ReverseProxySection {
        port: Some(local_port),
        server_port: None,
        local_port: None,
        local_host: Some("127.0.0.1".to_string()),
        source_ip: None,
    }];

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), control_port);
    let proxy_client = ReverseProxyClientHandle::start(server_addr, reverse_proxy_config).await?;

    wait_for_port_ready(local_port, Duration::from_secs(5)).await?;

    let mut stream = TcpStream::connect((LOCALHOST, local_port)).await?;
    let request = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    stream.write_all(request.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).await?;

    assert!(status_line.contains("200 OK"));

    let mut headers_done = false;
    let mut line = String::new();
    while !headers_done {
        line.clear();
        reader.read_line(&mut line).await?;
        if line == "\r\n" || line == "\n" {
            headers_done = true;
        }
    }

    let mut body = String::new();
    reader.read_to_string(&mut body).await?;
    assert_eq!(body, "PONG");

    proxy_client.shutdown().await;
    proxy_server.shutdown().await;
    local_http_server.shutdown().await?;

    Ok(())
}
