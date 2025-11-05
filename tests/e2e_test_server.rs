//! HTTP Test Server for E2E Testing
//!
//! This is a simple HTTP server that acts as a backend for testing GSC-FQ proxy.
//! It supports various endpoints to test different scenarios.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <port>", args[0]);
        std::process::exit(1);
    }

    let port: u16 = args[1].parse().map_err(|_| "Invalid port number")?;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    println!("==> Starting E2E Test Server on http://127.0.0.1:{}", port);

    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, addr) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, addr).await {
                eprintln!("[ERR] Connection handling failed: {}", e);
            }
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = vec![0u8; 8192];
    let mut request_data = Vec::new();
    let mut header_len: Option<usize> = None;
    let mut content_length: Option<usize> = None;

    // Read until we consume the full request body (if any).
    loop {
        let bytes_read = stream.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }

        request_data.extend_from_slice(&buffer[..bytes_read]);

        if header_len.is_none() {
            if let Some(pos) = request_data
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
            {
                header_len = Some(pos + 4);
                if let Ok(header_text) = std::str::from_utf8(&request_data[..pos]) {
                    for line in header_text.lines().skip(1) {
                        if let Some((name, value)) = line.split_once(':') {
                            if name.trim().eq_ignore_ascii_case("Content-Length") {
                                if let Ok(len) = value.trim().parse::<usize>() {
                                    content_length = Some(len);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(h_len) = header_len {
            if let Some(expected) = content_length {
                let body_bytes = request_data.len().saturating_sub(h_len);
                if body_bytes >= expected {
                    break;
                }
            } else {
                break;
            }
        }
    }

    if request_data.is_empty() {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&request_data).into_owned();
    println!(
        "[RX] Received request from {}: {}",
        addr,
        request.lines().next().unwrap_or("")
    );

    // Parse request to get method and path
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return Ok(());
    }

    let request_line = lines[0];
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0];
    let raw_target = parts[1];
    let (path, query) = match raw_target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (raw_target, ""),
    };

    // Route the request
    let (status_code, content_type, body) = match (method, path) {
        ("GET", "/") => (
            200,
            "application/json",
            format!(
                r#"{{"status":"ok","server":"e2e-test","timestamp":{}}}"#,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        ),
        ("GET", "/health") => (
            200,
            "application/json",
            r#"{"status":"healthy"}"#.to_string(),
        ),
        ("GET", "/echo" | "/test") => {
            let response_body = format!(
                r#"{{"method":"{}","path":"{}","query":"{}","client_ip":"{}","timestamp":{}}}"#,
                method,
                raw_target,
                query,
                addr,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            );
            (200, "application/json", response_body)
        }
        ("POST", "/echo") => {
            // Find body after headers
            if let Some(body_start) = request.find("\r\n\r\n") {
                let body = &request[body_start + 4..];
                let response = format!(
                    r#"{{"method":"POST","path":"/echo","body":"{}","client_ip":"{}","timestamp":{}}}"#,
                    body.replace('"', "\\\""),
                    addr,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                );
                (200, "application/json", response)
            } else {
                (400, "text/plain", "Bad Request".to_string())
            }
        }
        ("GET", path) if path.starts_with("/delay/") => {
            // Extract delay milliseconds
            if let Some(ms_str) = path.strip_prefix("/delay/") {
                if let Ok(ms) = ms_str.parse::<u64>() {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    (
                        200,
                        "application/json",
                        format!(
                            r#"{{"delayed":{},"timestamp":{}}}"#,
                            ms,
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs()
                        ),
                    )
                } else {
                    (400, "text/plain", "Invalid delay value".to_string())
                }
            } else {
                (404, "text/plain", "Not Found".to_string())
            }
        }
        ("GET", "/status") => {
            let status = r#"{
                "server": "e2e-test-server",
                "uptime": 0,
                "connections": 1,
                "version": "1.0.0"
            }"#;
            (200, "application/json", status.to_string())
        }
        _ => (404, "text/plain", "Not Found".to_string()),
    };

    // Build HTTP response
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_code,
        match status_code {
            200 => "OK",
            404 => "Not Found",
            400 => "Bad Request",
            _ => "Unknown",
        },
        content_type,
        body.len(),
        body
    );

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;

    println!(
        "[TX] Sent response to {}: {} {}",
        addr,
        status_code,
        match status_code {
            200 => "OK",
            404 => "Not Found",
            400 => "Bad Request",
            _ => "Unknown",
        }
    );

    Ok(())
}
