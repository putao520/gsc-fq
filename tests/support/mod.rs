use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use gsc_fq::config::loader::{ConfigFile, ReverseProxySection, ServerSection, ReverseProxyClientSection};
use gsc_fq::proxy::ProxyInstance;
use gsc_fq::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

pub const LOCALHOST: &str = "127.0.0.1";

pub enum RemoteTarget {
    Socket(SocketAddr),
}

impl RemoteTarget {
    pub fn socket(addr: SocketAddr) -> Self {
        Self::Socket(addr)
    }

    pub fn localhost(port: u16) -> Self {
        Self::socket(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }

    async fn resolve(self) -> Result<SocketAddr> {
        match self {
            RemoteTarget::Socket(addr) => Ok(addr),
        }
    }
}

pub struct ProxyHandle {
    local_addr: SocketAddr,
    shutdown_tx: broadcast::Sender<()>,
    join_handle: Option<JoinHandle<()>>,
}

impl ProxyHandle {
    pub async fn start(local_port: u16, target: RemoteTarget) -> Result<Self> {
        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let remote_addr = target.resolve().await?;

        let instance = ProxyInstance::new(bind_ip, local_port, remote_addr, None)
            .context("failed to create proxy instance")?;
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        let join_handle = tokio::spawn(async move {
            let mut instance = instance;
            let _ = instance.start(shutdown_rx).await;
        });

        wait_for_port_ready(local_port, Duration::from_secs(5))
            .await
            .context("proxy failed to bind within timeout")?;

        Ok(Self {
            local_addr: SocketAddr::new(bind_ip, local_port),
            shutdown_tx,
            join_handle: Some(join_handle),
        })
    }

    pub fn local_port(&self) -> u16 {
        self.local_addr.port()
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.join_handle.take() {
            let _ = self.shutdown_tx.send(());
            handle.abort();
        }
    }
}

pub struct TestServer {
    addr: SocketAddr,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    pub async fn start_echo() -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut socket, _)) => {
                        tokio::spawn(async move {
                            let mut buf = [0u8; 1024];
                            loop {
                                match socket.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        if socket.write_all(&buf[..n]).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            addr,
            handle: Some(handle),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub async fn shutdown(mut self) -> io::Result<()> {
        if let Some(mut handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        Ok(())
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

pub async fn wait_for_port_ready(port: u16, timeout: Duration) -> io::Result<()> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let deadline = Instant::now() + timeout;

    loop {
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                let _ = stream.shutdown().await;
                return Ok(());
            }
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("port {} did not become ready: {}", port, err),
                    ));
                }
            }
        }
    }
}

pub fn pick_available_port() -> Result<u16> {
    // Try multiple times to get a truly available port
    for _ in 0..3 {
        let listener =
            std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("failed to allocate port")?;
        let port = listener
            .local_addr()
            .context("failed to read allocated port")?
            .port();
        drop(listener);

        // Quick check to see if port is still available
        match std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(_) => {
                // Port is available, use it
                return Ok(port);
            }
            Err(_) => {
                // Port was taken, try again
                continue;
            }
        }
    }

    // Fallback to original behavior
    let listener =
        std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("failed to allocate port")?;
    let port = listener
        .local_addr()
        .context("failed to read allocated port")?
        .port();
    drop(listener);
    Ok(port)
}

pub struct PingPongServer {
    addr: SocketAddr,
    handle: Option<JoinHandle<()>>,
}

impl PingPongServer {
    pub async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        
        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, _)) => {
                        tokio::spawn(async move {
                            Self::handle_connection(socket).await;
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            addr,
            handle: Some(handle),
        })
    }

    async fn handle_connection(mut socket: TcpStream) {
        let (reader, mut writer) = socket.split();
        let mut reader = BufReader::new(reader);
        let mut request_line = String::new();
        
        if reader.read_line(&mut request_line).await.is_err() {
            return;
        }

        let mut headers_done = false;
        let mut line = String::new();
        while !headers_done {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    if line == "\r\n" || line == "\n" {
                        headers_done = true;
                    }
                }
            }
        }

        let response = if request_line.contains("/ping") {
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nPONG"
        } else {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nNot Found"
        };

        let _ = writer.write_all(response.as_bytes()).await;
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub async fn shutdown(mut self) -> io::Result<()> {
        if let Some(mut handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        Ok(())
    }
}

impl Drop for PingPongServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

pub struct ReverseProxyServerHandle {
    control_port: u16,
    handle: Option<JoinHandle<()>>,
}

impl ReverseProxyServerHandle {
    pub async fn start(control_port: u16) -> Result<Self> {
        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        
        let handle = tokio::spawn(async move {
            let mut server = ReverseProxyServer::new(bind_ip, control_port);
            let _ = server.start().await;
        });

        wait_for_port_ready(control_port, Duration::from_secs(5))
            .await
            .context("reverse proxy server failed to bind within timeout")?;

        Ok(Self {
            control_port,
            handle: Some(handle),
        })
    }

    pub fn control_port(&self) -> u16 {
        self.control_port
    }

    pub async fn shutdown(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for ReverseProxyServerHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

pub struct ReverseProxyClientHandle {
    handle: Option<JoinHandle<()>>,
}

impl ReverseProxyClientHandle {
    pub async fn start(
        server_addr: SocketAddr,
        reverse_proxies: Vec<ReverseProxySection>,
    ) -> Result<Self> {
        let config = ConfigFile {
            server: Some(ServerSection {
                bind_ip: Some("127.0.0.1".to_string()),
                debug: Some(false),
                auth_token: None,
                allowed_tokens: Vec::new(),
            }),
            proxies: Vec::new(),
            reverse_proxies,
            reverse_proxy_server: None,
            reverse_proxy_client: Some(ReverseProxyClientSection {
                server: server_addr.to_string(),
            }),
        };

        let handle = tokio::spawn(async move {
            let mut client = ReverseProxyClient::new(server_addr, config);
            let _ = client.start().await;
        });

        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(Self {
            handle: Some(handle),
        })
    }

    pub async fn shutdown(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for ReverseProxyClientHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}
