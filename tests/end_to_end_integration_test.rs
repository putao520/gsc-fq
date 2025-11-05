use anyhow::{anyhow, Result};
use gsc_fq::proxy::ProxyServerBuilder;
use gsc_fq::ProxySection;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proxy_handles_concurrent_connections_with_library_startup() -> Result<()> {
    // Start a backend echo server that the proxy will forward to.
    let backend_listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let backend_addr = backend_listener.local_addr()?;
    let (backend_shutdown_tx, backend_shutdown_rx) = broadcast::channel(1);
    let backend_handle = tokio::spawn(run_backend_server(backend_listener, backend_shutdown_rx));
    let backend_guard = BackendGuard::new(backend_shutdown_tx, backend_handle);

    // Choose an available port for the proxy to listen on.
    let proxy_port = pick_unused_port()?;
    let proxy_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), proxy_port);

    // Build the proxy server directly via the library API.
    let proxy_config = ProxySection {
        local_port: proxy_port,
        remote_host: backend_addr.ip().to_string(),
        remote_port: backend_addr.port(),
        source_ip: None,
    };

    let mut proxy_server = ProxyServerBuilder::new()
        .bind_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .add_proxy(proxy_config)
        .build()?;

    let proxy_handle = tokio::spawn(async move { proxy_server.start().await });
    let proxy_guard = AbortOnDrop::new(proxy_handle);

    // Wait until the proxy port is accepting connections before running the test flow.
    wait_for_port(proxy_addr, Duration::from_secs(5)).await?;

    // Exercise the proxy with multiple concurrent clients.
    let messages: Vec<String> = (0..10).map(|i| format!("message-{i}")).collect();
    let mut client_handles = Vec::with_capacity(messages.len());
    for message in messages.iter().cloned() {
        client_handles.push(tokio::spawn(client_roundtrip(proxy_addr, message)));
    }

    let mut echoed_messages = Vec::with_capacity(messages.len());
    for handle in client_handles {
        echoed_messages.push(handle.await??);
    }

    let mut expected = messages.clone();
    expected.sort();
    let mut echoed_sorted = echoed_messages.clone();
    echoed_sorted.sort();
    assert_eq!(echoed_sorted, expected);

    // Stop the proxy server task now that verification is complete.
    let proxy_handle = proxy_guard.into_inner();
    proxy_handle.abort();
    match proxy_handle.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => return Err(err.into()),
        Err(join_err) if join_err.is_cancelled() => {}
        Err(join_err) => return Err(join_err.into()),
    }

    // Shut down the backend server gracefully.
    backend_guard.shutdown().await?;

    Ok(())
}

async fn client_roundtrip(addr: SocketAddr, message: String) -> Result<String> {
    let mut stream = TcpStream::connect(addr).await?;
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;

    let mut buffer = vec![0u8; message.len()];
    stream.read_exact(&mut buffer).await?;
    stream.shutdown().await?;

    Ok(String::from_utf8(buffer)?)
}

async fn run_backend_server(
    listener: TcpListener,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<()> {
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (socket, _) = accept_result?;
                tokio::spawn(async move {
                    if let Err(err) = handle_backend_connection(socket).await {
                        eprintln!("backend connection error: {err}");
                    }
                });
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
    Ok(())
}

async fn handle_backend_connection(mut socket: TcpStream) -> Result<()> {
    let mut buffer = vec![0u8; 1024];
    let bytes_read = socket.read(&mut buffer).await?;
    if bytes_read == 0 {
        return Ok(());
    }

    socket.write_all(&buffer[..bytes_read]).await?;
    socket.flush().await?;
    socket.shutdown().await?;
    Ok(())
}

async fn wait_for_port(addr: SocketAddr, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                let _ = stream.shutdown().await;
                return Ok(());
            }
            Err(err) => {
                if start.elapsed() > timeout {
                    return Err(anyhow!("timed out waiting for proxy on {addr}: {err}"));
                }
                sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

fn pick_unused_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    Ok(port)
}

struct AbortOnDrop<T> {
    handle: Option<JoinHandle<T>>,
}

impl<T> AbortOnDrop<T> {
    fn new(handle: JoinHandle<T>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    fn into_inner(mut self) -> JoinHandle<T> {
        self.handle.take().expect("join handle already taken")
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}

struct BackendGuard {
    sender: broadcast::Sender<()>,
    handle: Option<JoinHandle<Result<()>>>,
}

impl BackendGuard {
    fn new(sender: broadcast::Sender<()>, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            sender,
            handle: Some(handle),
        }
    }

    async fn shutdown(mut self) -> Result<()> {
        let _ = self.sender.send(());
        if let Some(handle) = self.handle.take() {
            handle.await??;
        }
        Ok(())
    }
}

impl Drop for BackendGuard {
    fn drop(&mut self) {
        let _ = self.sender.send(());
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}
