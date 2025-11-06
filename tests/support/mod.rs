use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use gsc_fq::proxy::ProxyInstance;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    let listener =
        std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("failed to allocate port")?;
    let port = listener
        .local_addr()
        .context("failed to read allocated port")?
        .port();
    drop(listener);
    Ok(port)
}
