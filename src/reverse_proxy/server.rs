use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::{debug_println, error_println};
use futures::StreamExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use yamux::{Config, Connection, Mode};

type ClientId = String;

/// Reverse proxy server
pub struct ReverseProxyServer {
    control_addr: SocketAddr,
    clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>,
}

/// Client session information
struct ClientSession {
    proxies: Vec<ReverseProxyConfig>,
    listeners: Vec<JoinHandle<()>>,
}

impl ReverseProxyServer {
    /// Create new reverse proxy server
    pub fn new(bind_ip: std::net::IpAddr, control_port: u16) -> Self {
        let control_addr = SocketAddr::new(bind_ip, control_port);
        Self {
            control_addr,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Start the reverse proxy server
    pub async fn start(&mut self) -> Result<()> {
        let listener = TcpListener::bind(self.control_addr).await?;
        println!("🔄 Reverse Proxy Server listening on {}", self.control_addr);
        
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug_println!("New control connection from {}", addr);
                    let clients = self.clients.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, addr, clients).await {
                            error_println!("Client {} error: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error_println!("Accept error: {}", e);
                }
            }
        }
    }
    
    /// Open yamux stream with retry logic
    async fn open_yamux_stream_with_retry(
        control: &mut yamux::Control,
        max_retries: usize,
    ) -> std::result::Result<yamux::Stream, yamux::ConnectionError> {
        let mut retries = max_retries;
        loop {
            match control.open_stream().await {
                Ok(stream) => return Ok(stream),
                Err(e) if retries > 0 => {
                    retries -= 1;
                    debug_println!("Failed to open yamux stream, {} retries left", retries);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Handle client connection
    async fn handle_client(
        mut stream: TcpStream,
        addr: SocketAddr,
        clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>,
    ) -> Result<()> {
        // Read ClientHello
        let msg = ControlMessage::read_from(&mut stream).await?;
        
        let (version, proxy_configs) = match msg {
            ControlMessage::ClientHello { version, proxies } => (version, proxies),
            _ => {
                return Err(ReverseProxyError::ProtocolError(
                    "Expected ClientHello".to_string()
                ).into());
            }
        };
        
        // Check version
        if version != PROTOCOL_VERSION {
            let response = ControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                status: HandshakeStatus::VersionMismatch,
                message: format!("Unsupported version: {}", version),
            };
            response.write_to(&mut stream).await?;
            return Err(ReverseProxyError::UnsupportedVersion(version).into());
        }
        
        // Validate configurations
        if proxy_configs.is_empty() {
            let response = ControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                status: HandshakeStatus::ConfigError,
                message: "No proxy configurations provided".to_string(),
            };
            response.write_to(&mut stream).await?;
            return Err(ReverseProxyError::HandshakeFailed("No proxies".to_string()).into());
        }
        
        // Start listeners for each proxy configuration
        let mut listeners = Vec::new();
        let mut port_to_target = HashMap::new();
        
        for config in &proxy_configs {
            let server_port = config.server_port;
            let bind_addr = SocketAddr::new(
                stream.local_addr()?.ip(),
                server_port
            );
            
            match TcpListener::bind(bind_addr).await {
                Ok(listener) => {
                    debug_println!("Opened port {} for reverse proxy", server_port);
                    port_to_target.insert(server_port, (config.local_host.clone(), config.local_port));
                    listeners.push((listener, server_port));
                }
                Err(e) => {
                    let response = ControlMessage::ServerHello {
                        version: PROTOCOL_VERSION,
                        status: HandshakeStatus::PortAllocationFailed,
                        message: format!("Failed to bind port {}: {}", server_port, e),
                    };
                    response.write_to(&mut stream).await?;
                    return Err(ReverseProxyError::PortAllocationFailed(e.to_string()).into());
                }
            }
        }
        
        // Send success response
        let response = ControlMessage::ServerHello {
            version: PROTOCOL_VERSION,
            status: HandshakeStatus::Ok,
            message: format!("Connected, {} ports allocated", proxy_configs.len()),
        };
        response.write_to(&mut stream).await?;
        
        println!("✅ Client {} connected with {} reverse proxies",
            addr, proxy_configs.len());
        
        // Upgrade to Yamux connection
        let compat_stream = stream.compat();
        let config = Config::default();
        let conn = Connection::new(compat_stream, config, Mode::Server);
        let mut yamux_control = conn.control();
        
        // Spawn task to drive the yamux connection
        tokio::spawn(yamux::into_stream(conn).for_each(|_| async {}));
        
        // Spawn tasks for each listener
        let mut listener_handles = Vec::new();
        for (listener, server_port) in listeners {
            let mut control = yamux_control.clone();
            
            let handle = tokio::spawn(async move {
                loop {
                    match listener.accept().await {
                        Ok((mut user_stream, user_addr)) => {
                            debug_println!("New connection from {} to port {}", user_addr, server_port);
                            
                            // Open a new yamux stream with retry
                            let yamux_stream = match Self::open_yamux_stream_with_retry(&mut control, 3).await {
                                Ok(s) => s,
                                Err(e) => {
                                    error_println!("Failed to open yamux stream after retries: {}", e);
                                    // Don't break the loop - continue accepting connections
                                    continue;
                                }
                            };
                            
                            // Convert yamux stream back to tokio AsyncRead/Write
                            let mut yamux_tokio = yamux_stream.compat();
                            
                            // Send server_port as first 2 bytes
                            if let Err(e) = yamux_tokio.write_all(&server_port.to_be_bytes()).await {
                                error_println!("Failed to write port header: {}", e);
                                continue;
                            }
                            
                            // Spawn task to forward data bidirectionally
                            tokio::spawn(async move {
                                match copy_bidirectional(&mut user_stream, &mut yamux_tokio).await {
                                    Ok((to_yamux, from_yamux)) => {
                                        debug_println!("Connection closed: sent {} bytes, received {} bytes", 
                                            to_yamux, from_yamux);
                                    }
                                    Err(e) => {
                                        debug_println!("Connection error: {}", e);
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            error_println!("Accept error on port {}: {}", server_port, e);
                        }
                    }
                }
            });
            
            listener_handles.push(handle);
        }
        
        // Store client session
        let client_id = format!("{}", addr);
        {
            let mut clients_lock = clients.lock().await;
            clients_lock.insert(client_id.clone(), ClientSession {
                proxies: proxy_configs,
                listeners: listener_handles,
            });
        }
        
        // Wait for all listener tasks to complete (they run until error or abort)
        // Keep control alive so yamux connection stays open
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            
            // Check if all listeners are still running
            let mut all_dead = true;
            {
                let clients_lock = clients.lock().await;
                if let Some(session) = clients_lock.get(&client_id) {
                    for handle in &session.listeners {
                        if !handle.is_finished() {
                            all_dead = false;
                            break;
                        }
                    }
                }
            }
            
            if all_dead {
                break;
            }
        }
        
        // Cleanup
        {
            let mut clients_lock = clients.lock().await;
            if let Some(session) = clients_lock.remove(&client_id) {
                for handle in session.listeners {
                    handle.abort();
                }
            }
        }
        
        println!("❌ Client {} disconnected", addr);
        
        Ok(())
    }
}
