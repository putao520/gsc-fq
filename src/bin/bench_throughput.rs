use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use std::time::{Instant, Duration};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).expect("Usage: bench_throughput <server|client> <port>");
    let port = std::env::args().nth(2).expect("Usage: bench_throughput <server|client> <port>");
    let addr = format!("127.0.0.1:{}", port);

    match mode.as_str() {
        "server" => run_server(&addr).await?,
        "client" => run_client(&addr).await?,
        _ => panic!("Invalid mode"),
    }

    Ok(())
}

async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    println!("🚀 Benchmark Server (Sink) listening on {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;
        println!("New connection received");
        
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024]; // 64KB buffer
            let total_bytes = Arc::new(AtomicUsize::new(0));
            let total_bytes_clone = total_bytes.clone();
            let start_time = Instant::now();

            // Stats printer
            tokio::spawn(async move {
                let mut last_bytes = 0;
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let current_bytes = total_bytes_clone.load(Ordering::Relaxed);
                    let bytes_diff = current_bytes - last_bytes;
                    let mb_per_sec = bytes_diff as f64 / 1024.0 / 1024.0;
                    let total_mb = current_bytes as f64 / 1024.0 / 1024.0;
                    
                    println!("Speed: {:.2} MB/s | Total: {:.2} MB", mb_per_sec, total_mb);
                    last_bytes = current_bytes;
                }
            });

            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        total_bytes.fetch_add(n, Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("Read error: {}", e);
                        break;
                    }
                }
            }
            println!("Connection closed");
        });
    }
}

async fn run_client(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Benchmark Client (Source) connecting to {}", addr);
    let mut socket = TcpStream::connect(addr).await?;
    println!("Connected! Sending data...");

    let buf = vec![1u8; 64 * 1024]; // 64KB buffer of ones
    let total_bytes = Arc::new(AtomicUsize::new(0));
    let total_bytes_clone = total_bytes.clone();

    // Stats printer
    tokio::spawn(async move {
        let mut last_bytes = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let current_bytes = total_bytes_clone.load(Ordering::Relaxed);
            let bytes_diff = current_bytes - last_bytes;
            let mb_per_sec = bytes_diff as f64 / 1024.0 / 1024.0;
            
            println!("Sending Speed: {:.2} MB/s", mb_per_sec);
            last_bytes = current_bytes;
        }
    });

    loop {
        match socket.write_all(&buf).await {
            Ok(_) => {
                total_bytes.fetch_add(buf.len(), Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("Write error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
