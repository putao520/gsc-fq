//! Real End-to-End Integration Test for GSC-FQ Proxy System
//!
//! This test performs a complete workflow testing the actual CLI behavior:
//! 1. Starts a mock HTTP backend server using tokio
//! 2. Starts GSC-FQ proxy via CLI
//! 3. Tests proxy functionality with multiple concurrent clients
//!
//! This tests the actual CLI behavior, not just library functions.

use scopeguard;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

/// Get an available port
fn get_available_port() -> u16 {
    use std::net::TcpListener as StdTcpListener;
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Wait for a port to be ready
async fn wait_for_port_ready(port: u16, timeout_ms: u64) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        match TcpStream::connect(format!("127.0.0.1:{}", port)).await {
            Ok(mut stream) => {
                let _ = stream.shutdown().await;
                return Ok(());
            }
            Err(_) => {
                sleep(Duration::from_millis(50)).await;
            }
        }
    }

    Err(format!("Port {} not ready within {}ms", port, timeout_ms).into())
}

/// Generate test configuration file for GSC-FQ proxy
fn generate_proxy_config(
    proxy_port: u16,
    backend_port: u16,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let config_content = format!(
        r#"[server]
bind_ip = "127.0.0.1"

[[proxies]]
local_port = {}
remote_host = "127.0.0.1"
remote_port = {}
"#,
        proxy_port, backend_port
    );

    let config_path = PathBuf::from("e2e_test_proxy_config.toml");
    fs::write(&config_path, config_content)?;
    println!("[FILE] Generated proxy config: {:?}", config_path);
    Ok(config_path)
}

/// Simple HTTP mock server for testing
async fn run_mock_http_server(
    listener: TcpListener,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (socket, _) = accept_result?;
                tokio::spawn(async move {
                    if let Err(e) = handle_http_connection(socket).await {
                        eprintln!("[HTTP] Connection error: {e}");
                    }
                });
            }
            _ = &mut shutdown => {
                break;
            }
        }
    }
    Ok(())
}

/// Handle HTTP connection for mock server
async fn handle_http_connection(
    mut socket: TcpStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = [0; 1024];
    let bytes_read = socket.read(&mut buffer).await?;

    if bytes_read > 0 {
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        if request.contains("GET") || request.contains("POST") {
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
            socket.write_all(response.as_bytes()).await?;
        }
    }

    Ok(())
}

/// Start the mock HTTP backend server
async fn start_backend_server(
    port: u16,
) -> Result<
    (
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    ),
    Box<dyn std::error::Error>,
> {
    println!("==> Starting mock HTTP backend server on port {}", port);

    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let handle = tokio::spawn(run_mock_http_server(listener, shutdown_rx));

    println!("[OK] Backend server started on port {}", port);
    Ok((shutdown_tx, handle))
}

/// Start GSC-FQ proxy server via CLI
fn start_proxy_server(config_path: &PathBuf) -> Result<Child, Box<dyn std::error::Error>> {
    println!("==> Starting GSC-FQ proxy with config: {:?}", config_path);

    let child = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "gsc-fq",
            "--",
            "--config",
            config_path.to_str().unwrap(),
            "--debug",
            "127.0.0.1", // Add required BIND_IP parameter
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(".") // Ensure we're in the project root
        .spawn()?;

    println!("[OK] Proxy server started (PID: {})", child.id());
    Ok(child)
}

/// Run simple HTTP client test
async fn run_client_test(
    proxy_port: u16,
    num_clients: u16,
) -> Result<bool, Box<dyn std::error::Error>> {
    println!(
        "==> Running client test with {} concurrent clients",
        num_clients
    );

    let proxy_addr = format!("127.0.0.1:{}", proxy_port);
    let mut tasks = Vec::new();

    for i in 0..num_clients {
        let proxy_addr = proxy_addr.clone();
        let task = tokio::spawn(async move {
            match test_single_http_client(&proxy_addr, i).await {
                Ok(success) => success,
                Err(e) => {
                    eprintln!("[CLIENT-{}] Error: {}", i, e);
                    false
                }
            }
        });
        tasks.push(task);
    }

    // Wait for all clients to complete
    let mut success_count = 0;
    for task in tasks {
        match task.await? {
            true => success_count += 1,
            false => {}
        }
    }

    let success_rate = success_count as f32 / num_clients as f32;
    println!(
        "[RESULT] {}/{} clients successful ({:.1}%)",
        success_count,
        num_clients,
        success_rate * 100.0
    );

    Ok(success_rate >= 0.8) // 80% success rate is acceptable
}

/// Test a single HTTP client connection
async fn test_single_http_client(
    proxy_addr: &str,
    client_id: u16,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(proxy_addr).await?;

    let request = format!(
        "GET /test{} HTTP/1.1\r\nHost: test.example.com\r\nConnection: close\r\n\r\n",
        client_id
    );

    stream.write_all(request.as_bytes()).await?;
    stream.shutdown().await?;

    let mut response = [0u8; 1024];
    let bytes_read = stream.read(&mut response).await?;

    let response_str = String::from_utf8_lossy(&response[..bytes_read]);
    Ok(response_str.contains("200 OK") && response_str.contains("Hello, World!"))
}

/// Main end-to-end integration test
#[tokio::test]
async fn test_real_e2e_proxy_functionality() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n==> Starting Real End-to-End Integration Test");
    println!("==========================================");

    // Phase 1: Setup - get dynamic ports
    let backend_port = get_available_port();
    let proxy_port = get_available_port();

    println!("[INFO] Port allocation:");
    println!("  Backend server: {}", backend_port);
    println!("  Proxy server: {}", proxy_port);

    // Phase 2: Generate proxy configuration
    let config_path = generate_proxy_config(proxy_port, backend_port)?;

    // Ensure cleanup at the end
    let _cleanup_guard = scopeguard::guard((), |_| {
        println!("\n==> Cleaning up...");
        let _ = fs::remove_file("e2e_test_proxy_config.toml");
    });

    // Phase 3: Start backend server
    let (backend_shutdown, _backend_handle) = start_backend_server(backend_port).await?;

    // Wait for backend to start
    sleep(Duration::from_millis(1000)).await;
    wait_for_port_ready(backend_port, 5000).await?;
    println!("[OK] Backend server is ready");

    // Phase 4: Start proxy server
    let mut proxy_server = start_proxy_server(&config_path)?;

    // Wait for proxy to start
    sleep(Duration::from_millis(3000)).await;
    wait_for_port_ready(proxy_port, 10000).await?;
    println!("[OK] Proxy server is ready");

    // Phase 5: Run client tests
    println!("\n==> Starting client tests...");

    // Test 1: Small number of concurrent clients
    let test1_success = run_client_test(proxy_port, 10).await?;
    assert!(test1_success, "Test 1 with 10 clients failed");

    // Wait a bit between tests
    sleep(Duration::from_millis(2000)).await;

    // Test 2: Higher concurrency
    let test2_success = run_client_test(proxy_port, 50).await?;
    assert!(test2_success, "Test 2 with 50 clients failed");

    // Phase 6: Cleanup
    println!("\n==> Shutting down servers...");

    // Stop backend server gracefully
    let _ = backend_shutdown.send(());

    // Stop proxy server
    let _ = proxy_server.kill();
    let _ = proxy_server.wait();

    println!("\n[OK] Real End-to-End Integration Test completed successfully!");
    println!("[OK] All tests passed - GSC-FQ proxy is working correctly in CLI mode!");

    Ok(())
}

/// Test with different configurations
#[tokio::test]
async fn test_e2e_with_different_configs() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n==> Testing with different proxy configurations");

    let backend_port = get_available_port();
    let proxy_port = get_available_port();

    // Test with source_ip configured
    let config_content = format!(
        r#"[server]
bind_ip = "127.0.0.1"

[[proxies]]
local_port = {}
remote_host = "127.0.0.1"
remote_port = {}
source_ip = "127.0.0.1"
"#,
        proxy_port, backend_port
    );

    let config_path = PathBuf::from("e2e_test_proxy_config_with_source_ip.toml");
    fs::write(&config_path, config_content)?;

    let _cleanup = scopeguard::guard((), |_| {
        let _ = fs::remove_file("e2e_test_proxy_config_with_source_ip.toml");
    });

    // Run a quick test with this configuration
    let (backend_shutdown, _backend_handle) = start_backend_server(backend_port).await?;
    sleep(Duration::from_millis(1000)).await;

    let mut proxy = start_proxy_server(&config_path)?;
    sleep(Duration::from_millis(3000)).await;

    wait_for_port_ready(proxy_port, 5000).await?;

    let success = run_client_test(proxy_port, 5).await?;

    // Cleanup
    let _ = backend_shutdown.send(());
    let _ = proxy.kill();
    let _ = proxy.wait();

    assert!(success, "Test with source_ip configuration failed");
    println!("[OK] Configuration test passed!");

    Ok(())
}
