use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::{debug_println, error_println};
use futures::StreamExt;
use sha2::Digest;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use yamux::{Config, Connection, Mode};

type ClientId = String;

/// Reverse proxy server
pub struct ReverseProxyServer {
    control_addr: SocketAddr,
    clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>,
    auth_token: Option<String>,
    allowed_tokens: Vec<String>,
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
            auth_token: std::env::var("REVERSE_PROXY_TOKEN").ok(),
            allowed_tokens: Vec::new(),
        }
    }

    /// Create new reverse proxy server with authentication
    pub fn new_with_auth(
        bind_ip: std::net::IpAddr,
        control_port: u16,
        auth_token: Option<String>,
        allowed_tokens: Vec<String>
    ) -> Self {
        let control_addr = SocketAddr::new(bind_ip, control_port);
        Self {
            control_addr,
            clients: Arc::new(Mutex::new(HashMap::new())),
            auth_token,
            allowed_tokens,
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
                    
                    let auth_token = self.auth_token.clone();
                    let allowed_tokens = self.allowed_tokens.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, addr, clients, auth_token, allowed_tokens).await {
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
                Err(_) if retries > 0 => {
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
        auth_token: Option<String>,
        allowed_tokens: Vec<String>,
    ) -> Result<()> {
        // Read ClientHello
        let msg = ControlMessage::read_from(&mut stream).await?;

        let (version, token, proxy_configs, config_hash) = match msg {
            ControlMessage::ClientHello { version, token, proxies, config_hash } => {
                (version, token, proxies, config_hash)
            },
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
                allowed_ports: Vec::new(),
                session_id: None,
            };
            response.write_to(&mut stream).await?;
            return Err(ReverseProxyError::UnsupportedVersion(version).into());
        }

        // Validate authentication token
        let token_valid = match &auth_token {
            Some(server_token) => {
                // Check if client token matches server token
                token == *server_token || allowed_tokens.contains(&token)
            },
            None => {
                // No authentication required on server
                true
            }
        };

        if !token_valid {
            let response = ControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                status: HandshakeStatus::InvalidToken,
                message: "Invalid or missing authentication token".to_string(),
                allowed_ports: Vec::new(),
                session_id: None,
            };
            response.write_to(&mut stream).await?;
            return Err(ReverseProxyError::HandshakeFailed("Authentication failed".to_string()).into());
        }

        // Verify configuration hash for tamper protection
        let expected_config_json = serde_json::to_string(&proxy_configs)
            .map_err(|e| ReverseProxyError::SerializationFailed(e.to_string()))?;
        let expected_hash = format!("{:x}", sha2::Sha256::digest(expected_config_json.as_bytes()));

        if expected_hash != config_hash {
            let response = ControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                status: HandshakeStatus::InvalidConfigHash,
                message: "Configuration hash mismatch - possible tampering detected".to_string(),
                allowed_ports: Vec::new(),
                session_id: None,
            };
            response.write_to(&mut stream).await?;
            return Err(ReverseProxyError::HandshakeFailed("Configuration integrity check failed".to_string()).into());
        }
        
        // Validate configurations
        if proxy_configs.is_empty() {
            let response = ControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                status: HandshakeStatus::ConfigError,
                message: "No proxy configurations provided".to_string(),
                allowed_ports: Vec::new(),
                session_id: None,
            };
            response.write_to(&mut stream).await?;
            return Err(ReverseProxyError::HandshakeFailed("No proxies".to_string()).into());
        }
        
        // Start listeners for each proxy configuration
        let mut listeners = Vec::new();
        let mut port_to_target = HashMap::new();
        
        for config in &proxy_configs {
            let server_port = config.server_port;
            // For now, use server_port for binding (could be enhanced to use dynamic port allocation)
            let actual_local_port = server_port;
            let bind_addr = SocketAddr::new(
                stream.local_addr()?.ip(),
                actual_local_port
            );

            debug_println!("Attempting to bind port {} (client requested: {})",
                actual_local_port, server_port);

            match TcpListener::bind(bind_addr).await {
                Ok(listener) => {
                    debug_println!("Opened port {} for reverse proxy (target: {}:{})",
                        actual_local_port, config.local_host.clone(), config.local_port);
                    port_to_target.insert(server_port, (config.local_host.clone(), config.local_port));
                    // Store the actual bound port for client reference
                    listeners.push((listener, actual_local_port));
                }
                Err(e) => {
                    let response = ControlMessage::ServerHello {
                        version: PROTOCOL_VERSION,
                        status: HandshakeStatus::PortAllocationFailed,
                        message: format!("Failed to bind port {}: {}", server_port, e),
                        allowed_ports: Vec::new(),
                        session_id: None,
                    };
                    response.write_to(&mut stream).await?;
                    return Err(ReverseProxyError::PortAllocationFailed(e.to_string()).into());
                }
            }
        }
        
        // Generate session ID and collect allowed ports
        let session_id = Some(format!("session_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            addr
        ));
        let allowed_ports: Vec<u16> = proxy_configs.iter().map(|c| c.server_port).collect();

        // Send success response
        let response = ControlMessage::ServerHello {
            version: PROTOCOL_VERSION,
            status: HandshakeStatus::Ok,
            message: format!("Connected, {} ports allocated", proxy_configs.len()),
            allowed_ports,
            session_id: session_id.clone(),
        };
        response.write_to(&mut stream).await?;
        
        println!("✅ Client {} connected with {} reverse proxies",
            addr, proxy_configs.len());
        
        // Upgrade to Yamux connection
        let compat_stream = stream.compat();
        let config = Config::default();
        let conn = Connection::new(compat_stream, config, Mode::Server);
        let yamux_control = conn.control();
        
        // Spawn task to drive the yamux connection and handle incoming streams
        tokio::spawn(async move {
            let stream = yamux::into_stream(conn);
            tokio::pin!(stream);

            while let Some(stream_result) = stream.next().await {
                match stream_result {
                    Ok(incoming_yamux_stream) => {
                        debug_println!("📥 Server received incoming yamux stream from client");

                        // Handle the incoming stream in a separate task
                        // In this reverse proxy setup, the server typically doesn't receive
                        // data streams from the client, but we handle them properly anyway
                        tokio::spawn(async move {
                            debug_println!("🔧 Processing incoming yamux stream on server side");
                            // For now, we just consume the stream to ensure proper connection cleanup
                            let mut yamux_tokio = incoming_yamux_stream.compat();
                            let mut buffer = [0u8; 1024];

                            loop {
                                match yamux_tokio.read(&mut buffer).await {
                                    Ok(0) => {
                                        debug_println!("📥 Incoming yamux stream closed by client");
                                        break;
                                    }
                                    Ok(n) => {
                                        debug_println!("📥 Server received {} bytes from client through yamux", n);
                                        // In this reverse proxy setup, we don't expect data from client,
                                        // but we handle it to ensure proper connection management
                                    }
                                    Err(e) => {
                                        debug_println!("📥 Error reading from incoming yamux stream: {}", e);
                                        break;
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        debug_println!("📥 Yamux stream error on server: {}", e);
                        break;
                    }
                }
            }

            debug_println!("📥 Server yamux connection closed");
        });
        
        // Spawn tasks for each listener
        let mut listener_handles = Vec::new();
        for (listener, server_port) in listeners {
            let mut control = yamux_control.clone();
            
            let handle = tokio::spawn(async move {
                loop {
                    debug_println!("Waiting for external connections on port {}", server_port);
                    match listener.accept().await {
                        Ok((mut user_stream, user_addr)) => {
                            println!("📥 New external connection from {} to port {}", user_addr, server_port);
                            
                            // Open a new yamux stream with retry
                            println!("🔍 Attempting to open yamux stream for external connection...");
                            let yamux_stream = match Self::open_yamux_stream_with_retry(&mut control, 3).await {
                                Ok(s) => {
                                    println!("✅ Yamux stream opened successfully");
                                    s
                                },
                                Err(e) => {
                                    error_println!("❌ Failed to open yamux stream after retries: {}", e);
                                    error_println!("⚠️  Client may not be ready to receive streams");
                                    // Don't break the loop - continue accepting connections
                                    continue;
                                }
                            };
                            
                            // Convert yamux stream back to tokio AsyncRead/Write
                            let mut yamux_tokio = yamux_stream.compat();
                            
                            // Send server_port as first 2 bytes
                            let port_header = server_port.to_be_bytes();
                            debug_println!("Sending port header: {:?} for external connection from {}", port_header, user_addr);
                            if let Err(e) = yamux_tokio.write_all(&port_header).await {
                                error_println!("Failed to write port header: {}", e);
                                continue;
                            }

                            // Ensure the port header is sent immediately
                            if let Err(e) = yamux_tokio.flush().await {
                                error_println!("Failed to flush port header: {}", e);
                                continue;
                            }
                            debug_println!("Port header sent successfully, starting bidirectional forwarding");
                            
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
