//! HTTP Test Client for E2E Testing
//!
//! This is a test client that makes HTTP requests to test GSC-FQ proxy functionality.
//! It supports multiple concurrent requests and various endpoints.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::timeout;

#[derive(Clone)]
struct TestRequest {
    id: usize,
    method: String,
    path: String,
    body: Option<String>,
}

#[derive(Clone)]
struct TestResult {
    request_id: usize,
    success: bool,
    response_time_ms: u64,
    response_status: Option<u16>,
    response_body: Option<String>,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 4 {
        eprintln!(
            "Usage: {} <proxy_host:port> <num_clients> <request_type> [options]",
            args[0]
        );
        eprintln!("  proxy_host:port - e.g., 127.0.0.1:8080");
        eprintln!("  num_clients - number of concurrent clients");
        eprintln!("  request_type - one of: GET, POST, MIXED");
        eprintln!("  options:");
        eprintln!("    --timeout-ms <ms> - request timeout (default: 5000)");
        eprintln!("    --delay-ms <ms> - delay between starting clients (default: 0)");
        eprintln!("");
        eprintln!("Examples:");
        eprintln!("  {} 127.0.0.1:8080 10 GET", args[0]);
        eprintln!("  {} 127.0.0.1:8080 20 POST", args[0]);
        eprintln!("  {} 127.0.0.1:8080 100 MIXED --timeout-ms 10000", args[0]);
        std::process::exit(1);
    }

    let proxy_addr = &args[1];
    let num_clients: usize = args[2].parse()?;
    let request_type = &args[3].to_uppercase();

    let mut timeout_ms = 5000u64;
    let mut delay_ms = 0u64;

    // Parse optional arguments
    let mut i = 4;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout-ms" => {
                if i + 1 < args.len() {
                    timeout_ms = args[i + 1].parse()?;
                    i += 2;
                } else {
                    eprintln!("Error: --timeout-ms requires a value");
                    std::process::exit(1);
                }
            }
            "--delay-ms" => {
                if i + 1 < args.len() {
                    delay_ms = args[i + 1].parse()?;
                    i += 2;
                } else {
                    eprintln!("Error: --delay-ms requires a value");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Error: Unknown option {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    println!("==> Starting E2E Test Client");
    println!("   Proxy: {}", proxy_addr);
    println!("   Clients: {}", num_clients);
    println!("   Request Type: {}", request_type);
    println!("   Timeout: {}ms", timeout_ms);
    println!("   Delay: {}ms between clients", delay_ms);
    println!();

    // Generate test requests
    let requests = generate_requests(num_clients, request_type)?;

    // Shared counters
    let success_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));
    let total_time = Arc::new(AtomicUsize::new(0));

    // Semaphore to limit concurrent connections
    let semaphore = Arc::new(Semaphore::new(num_clients));

    let start_time = Instant::now();
    let mut handles = Vec::new();

    // Spawn clients
    for request in requests {
        let permit = semaphore.clone().acquire_owned().await?; // Owned permit can move into spawned task
        let proxy_addr = proxy_addr.clone();
        let success_count = success_count.clone();
        let error_count = error_count.clone();
        let total_time = total_time.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit; // Keep permit until task completes

            let result = execute_request(&proxy_addr, &request, timeout_ms).await;

            match result {
                Ok(test_result) => {
                    if test_result.success {
                        success_count.fetch_add(1, Ordering::Relaxed);
                        total_time
                            .fetch_add(test_result.response_time_ms as usize, Ordering::Relaxed);
                        println!(
                            "[OK] Client {}: {} ms - {}",
                            request.id,
                            test_result.response_time_ms,
                            test_result.response_status.unwrap_or(0)
                        );
                    } else {
                        error_count.fetch_add(1, Ordering::Relaxed);
                        let error_msg = test_result.error.as_deref().unwrap_or("Unknown error");
                        println!("[ERR] Client {}: Failed - {}", request.id, error_msg);
                    }
                    test_result
                }
                Err(e) => {
                    error_count.fetch_add(1, Ordering::Relaxed);
                    println!("[ERR] Client {}: Error - {}", request.id, e);
                    TestResult {
                        request_id: request.id,
                        success: false,
                        response_time_ms: 0,
                        response_status: None,
                        response_body: None,
                        error: Some(e.to_string()),
                    }
                }
            }
        });

        handles.push(handle);

        // Delay between starting clients if specified
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    // Wait for all clients to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await?);
    }

    let total_elapsed = start_time.elapsed();

    // Print summary
    println!();
    println!("Summary: Test Results");
    println!("======================");
    println!("Total clients: {}", num_clients);
    println!("Successful: {}", success_count.load(Ordering::Relaxed));
    println!("Failed: {}", error_count.load(Ordering::Relaxed));
    println!(
        "Success rate: {:.2}%",
        (success_count.load(Ordering::Relaxed) as f64 / num_clients as f64) * 100.0
    );

    if success_count.load(Ordering::Relaxed) > 0 {
        let avg_time = total_time.load(Ordering::Relaxed) / success_count.load(Ordering::Relaxed);
        println!("Average response time: {}ms", avg_time);
    }

    println!("Total test time: {:?}", total_elapsed);
    println!(
        "Requests per second: {:.2}",
        num_clients as f64 / total_elapsed.as_secs_f64()
    );

    // Exit with error code if any requests failed
    if error_count.load(Ordering::Relaxed) > 0 {
        println!();
        println!("[WARN] Test completed with errors");
        std::process::exit(1);
    } else {
        println!();
        println!("[OK] All tests passed successfully!");
    }

    Ok(())
}

fn generate_requests(
    count: usize,
    request_type: &str,
) -> Result<Vec<TestRequest>, Box<dyn std::error::Error>> {
    let mut requests = Vec::new();

    for i in 0..count {
        let request = match request_type {
            "GET" => TestRequest {
                id: i,
                method: "GET".to_string(),
                path: format!("/echo?client_id={}&message=test_message_{}", i, i),
                body: None,
            },
            "POST" => TestRequest {
                id: i,
                method: "POST".to_string(),
                path: "/echo".to_string(),
                body: Some(format!(
                    r#"{{"client_id":{},"message":"test_message_{}","data":"{}"}}"#,
                    i,
                    i,
                    "x".repeat(100)
                )),
            },
            "MIXED" => {
                if i % 3 == 0 {
                    TestRequest {
                        id: i,
                        method: "GET".to_string(),
                        path: "/".to_string(),
                        body: None,
                    }
                } else if i % 3 == 1 {
                    TestRequest {
                        id: i,
                        method: "GET".to_string(),
                        path: format!("/test?client_id={}&rand={}", i, i * 12345),
                        body: None,
                    }
                } else {
                    TestRequest {
                        id: i,
                        method: "POST".to_string(),
                        path: "/echo".to_string(),
                        body: Some(format!(
                            r#"{{"client_id":{},"timestamp":{}}}"#,
                            i,
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs()
                        )),
                    }
                }
            }
            _ => {
                return Err(format!("Unknown request type: {}", request_type).into());
            }
        };

        requests.push(request);
    }

    Ok(requests)
}

async fn execute_request(
    proxy_addr: &str,
    request: &TestRequest,
    timeout_ms: u64,
) -> Result<TestResult, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();

    let request_future = async {
        let mut stream = TcpStream::connect(proxy_addr).await?;

        // Build HTTP request
        let mut http_request = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\n",
            request.method, request.path, proxy_addr
        );

        if let Some(body) = &request.body {
            http_request.push_str(&format!("Content-Type: application/json\r\n"));
            http_request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }

        http_request.push_str("Connection: close\r\n");
        http_request.push_str("\r\n");

        if let Some(body) = &request.body {
            http_request.push_str(body);
        }

        // Send request
        stream.write_all(http_request.as_bytes()).await?;
        stream.flush().await?;

        // Read response
        let mut buffer = vec![0u8; 8192];
        let mut response_data = Vec::new();

        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            response_data.extend_from_slice(&buffer[..n]);
        }

        let response_str = String::from_utf8_lossy(&response_data);

        // Parse status line
        let status_code = if let Some(line) = response_str.lines().next() {
            if let Some(status_str) = line.split_whitespace().nth(1) {
                status_str.parse::<u16>().unwrap_or(500)
            } else {
                500
            }
        } else {
            500
        };

        // Find body after headers
        let body = if let Some(body_start) = response_str.find("\r\n\r\n") {
            Some(response_str[body_start + 4..].to_string())
        } else {
            None
        };

        let mut success = status_code == 200;
        let mut error_message = if success {
            None
        } else {
            Some(format!("Unexpected status code {}", status_code))
        };

        if success {
            match &body {
                Some(body_text) if !body_text.trim().is_empty() => {
                    // Basic validation to ensure the response reflects the original request.
                    if !body_text.contains(&format!("\"method\":\"{}\"", request.method)) {
                        success = false;
                        error_message = Some("Response missing expected method field".to_string());
                    } else if let Some((_, query)) = request.path.split_once('?') {
                        if !query.is_empty() && !body_text.contains(query) {
                            success = false;
                            error_message =
                                Some("Response missing expected query string".to_string());
                        }
                    } else if let Some(request_body) = &request.body {
                        let sanitized = request_body.replace('"', "\\\"");
                        let fragment: String = sanitized.chars().take(32).collect();
                        if !fragment.is_empty() && !body_text.contains(&fragment) {
                            success = false;
                            error_message =
                                Some("Response missing expected payload fragment".to_string());
                        }
                    }
                }
                _ => {
                    success = false;
                    error_message = Some("Empty response body".to_string());
                }
            }
        }

        Ok(TestResult {
            request_id: request.id,
            success,
            response_time_ms: start_time.elapsed().as_millis() as u64,
            response_status: Some(status_code),
            response_body: body,
            error: error_message,
        })
    };

    // Apply timeout
    match timeout(Duration::from_millis(timeout_ms), request_future).await {
        Ok(result) => result,
        Err(_) => Ok(TestResult {
            request_id: request.id,
            success: false,
            response_time_ms: timeout_ms,
            response_status: None,
            response_body: None,
            error: Some("Request timeout".to_string()),
        }),
    }
}
