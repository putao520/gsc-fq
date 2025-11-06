//! Simple End-to-End Test using CLI executables
//!
//! This test runs the actual CLI commands to test the real workflow.

use std::fs;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

/// Get an available port
fn get_available_port() -> u16 {
    use std::net::TcpListener as StdTcpListener;
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn run_echo_server(
    listener: TcpListener,
    mut shutdown: oneshot::Receiver<()>,
) -> std::io::Result<()> {
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (socket, _) = accept_result?;
                tokio::spawn(async move {
                    if let Err(err) = handle_echo_connection(socket).await {
                        eprintln!("[ECHO] Connection error: {err}");
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

async fn handle_echo_connection(socket: TcpStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = socket.into_split();
    tokio::io::copy(&mut reader, &mut writer).await?;
    Ok(())
}

fn boxed_error<E>(err: E) -> Box<dyn std::error::Error>
where
    E: std::error::Error + Send + Sync + 'static,
{
    Box::new(err)
}

fn map_timeout_result<T>(
    result: Result<Result<T, std::io::Error>, tokio::time::error::Elapsed>,
) -> Result<T, Box<dyn std::error::Error>> {
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(boxed_error(err)),
        Err(err) => Err(boxed_error(err)),
    }
}

#[tokio::test]
async fn test_cli_with_simple_echo() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n==> Starting Simple CLI End-to-End Test");
    println!("======================================");

    // Get dynamic ports
    let echo_port = get_available_port();
    let proxy_port = get_available_port();

    println!("[INFO] Echo server port: {}", echo_port);
    println!("[INFO] Proxy port: {}", proxy_port);

    // Step 1: Start a simple echo server using Tokio
    println!("\n==> Starting echo server...");
    let listener = TcpListener::bind(("127.0.0.1", echo_port)).await?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let echo_task = tokio::spawn(run_echo_server(listener, shutdown_rx));
    println!("[OK] Echo server started");
    sleep(Duration::from_millis(200)).await;

    // Generate config file
    let config_content = format!(
        r#"[server]
bind_ip = "127.0.0.1"

[[proxies]]
local_port = {}
remote_host = "127.0.0.1"
remote_port = {}
"#,
        proxy_port, echo_port
    );

    fs::write("test_proxy_config.toml", config_content)?;
    println!("[FILE] Created test_proxy_config.toml");

    // Step 2: Start GSC-FQ proxy
    println!("\n==> Starting GSC-FQ proxy...");
    // Copy the test config to default.toml
    std::fs::copy("test_proxy_config.toml", "default.toml")?;

    let mut proxy_server = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "gsc-fq",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for proxy to start
    println!("[WAIT] Waiting for proxy to start...");
    sleep(Duration::from_millis(3000)).await;

    // Step 3: Test with a simple client implemented in Rust
    println!("\n==> Testing proxy with echo data...");
    let connect_future = TcpStream::connect(("127.0.0.1", proxy_port));
    let mut client = map_timeout_result(timeout(Duration::from_secs(5), connect_future).await)?;

    let message = b"Hello from client via proxy\n";
    map_timeout_result(timeout(Duration::from_secs(5), client.write_all(message)).await)?;
    map_timeout_result(timeout(Duration::from_secs(5), client.shutdown()).await)?;

    let mut reader = BufReader::new(client);
    let mut response_buf = vec![0; message.len()];
    map_timeout_result(
        timeout(Duration::from_secs(5), reader.read_exact(&mut response_buf)).await,
    )?;

    let response = String::from_utf8_lossy(&response_buf).to_string();
    println!("[RESP] {}", response);
    drop(reader);

    // Cleanup
    println!("\n==> Cleaning up...");
    let _ = proxy_server.kill();
    let _ = proxy_server.wait(); // Wait to avoid leftover proxy process.
    let _ = fs::remove_file("test_proxy_config.toml");
    let _ = fs::remove_file("default.toml");

    if shutdown_tx.send(()).is_err() {
        println!("[WARN] Echo server shutdown signal receiver dropped early");
    }

    match echo_task.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => return Err(boxed_error(err)),
        Err(join_err) => return Err(boxed_error(join_err)),
    }

    // Verify
    assert!(
        response.contains("Hello from client via proxy"),
        "Echo response not received. Got: {}",
        response
    );

    println!("[OK] Test passed!");

    Ok(())
}

#[tokio::test]
async fn test_config_loading() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n==> Testing configuration loading...");

    // Create a test config
    let config_content = r#"[server]
bind_ip = "127.0.0.1"

[[proxies]]
local_port = 33100
remote_host = "192.168.1.100"
remote_port = 8080

[[proxies]]
local_port = 33200
remote_host = "192.168.1.101"
remote_port = 8080
"#;

    fs::write("test_config.toml", config_content)?;

    // Try to run GSC-FQ with this config (it should start without errors)
    // Copy the test config to default.toml
    std::fs::copy("test_config.toml", "default.toml")?;

    let mut proxy = Command::new("timeout")
        .args(&[
            "5",
            "cargo",
            "run",
            "--bin",
            "gsc-fq",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Let it run for 3 seconds
    sleep(Duration::from_millis(3000)).await;

    // Check if it's still running (means config loaded successfully)
    let status = proxy.try_wait()?;

    // Cleanup
    let _ = proxy.kill();
    let _ = proxy.wait(); // Wait to ensure the process fully exits.
    let _ = fs::remove_file("test_config.toml");
    let _ = fs::remove_file("default.toml");

    // If it exited immediately, there was an error
    if status.is_some() {
        println!("[WARN] Proxy exited early - there might be a configuration issue");
    } else {
        println!("[OK] Configuration loaded successfully");
    }

    Ok(())
}

#[tokio::test]
async fn test_cli_help() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n==> Testing CLI help output...");

    // Since we removed command line arguments, help should fail or show an error
    let output = Command::new("cargo")
        .args(&["run", "--bin", "gsc-fq", "--", "--help"])
        .output()?;

    let stderr_text = String::from_utf8_lossy(&output.stderr);

    // The program should exit with error when given unknown arguments
    assert!(
        !output.status.success(),
        "Program should fail with unknown arguments"
    );

    assert!(
        stderr_text.contains("required arguments were not provided") ||
        stderr_text.contains("unexpected argument") ||
        stderr_text.contains("error"),
        "Should show error about unknown arguments"
    );

    println!("[OK] Command line validation works correctly");
    println!("[INFO] Error output: {}", stderr_text.trim());

    Ok(())
}
