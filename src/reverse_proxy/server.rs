use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::{debug_println, error_println};
use futures::future::poll_fn;
use sha2::Digest;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::compat::TokioAsyncReadCompatExt;
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
#[allow(dead_code)]
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

    
    /// Handle a single yamux stream (server side)
    async fn handle_server_stream(
        yamux_stream: yamux::Stream,
        proxy_configs: Vec<ReverseProxyConfig>,
    ) -> Result<()> {
        use tokio_util::compat::FuturesAsyncReadCompatExt;

        let mut compat_stream = yamux_stream.compat();

        // Read port header (first 2 bytes)
        let mut port_bytes = [0u8; 2];
        debug_println!("Reading port header from incoming stream...");

        if let Err(e) = compat_stream.read_exact(&mut port_bytes).await {
            error_println!("Failed to read port header: {}", e);
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                format!("Failed to read port header: {}", e)
            ).into());
        }

        let server_port = u16::from_be_bytes(port_bytes);
        debug_println!("Received incoming stream for server port {}", server_port);

        // Find the corresponding local target
        let local_target = proxy_configs.iter()
            .find(|c| c.server_port == server_port)
            .cloned();

        let Some(target) = local_target else {
            error_println!("Unknown server port: {}", server_port);
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                format!("Unknown server port: {}", server_port)
            ).into());
        };

        // Handle the stream data forwarding
        Self::handle_stream_forwarding(compat_stream, target).await
    }

    /// Handle the stream data forwarding
    async fn handle_stream_forwarding(
        mut yamux_stream: tokio_util::compat::Compat<yamux::Stream>,
        target: ReverseProxyConfig,
    ) -> Result<()> {
        // Connect to local service
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let mut local_stream = TcpStream::connect(&local_addr).await.map_err(|e| {
            crate::error::ReverseProxyError::ConnectionFailed(format!(
                "Failed to connect to local service {}: {}",
                local_addr, e
            ))
        })?;

        debug_println!("Connected to local service: {}", local_addr);

        // Bidirectional copy
        match tokio::io::copy_bidirectional(&mut yamux_stream, &mut local_stream).await {
            Ok((from_yamux, to_yamux)) => {
                debug_println!(
                    "Stream closed. Transferred: {} bytes from server, {} bytes to server",
                    from_yamux, to_yamux
                );
            }
            Err(e) => {
                debug_println!("Copy error: {}", e);
            }
        }

        Ok(())
    }

    /// Handle a direct TCP connection from proxy port
    async fn handle_direct_tcp_connection(
        mut tcp_stream: TcpStream,
        proxy_configs: Vec<ReverseProxyConfig>,
    ) -> Result<()> {
        // Read port header from the proxy connection
        let mut port_bytes = [0u8; 2];
        if let Err(e) = tcp_stream.read_exact(&mut port_bytes).await {
            error_println!("Failed to read port header: {}", e);
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                format!("Failed to read port header: {}", e)
            ).into());
        }

        let server_port = u16::from_be_bytes(port_bytes);
        debug_println!("Received direct TCP connection for server port {}", server_port);

        // Find the corresponding local target
        let local_target = proxy_configs.iter()
            .find(|c| c.server_port == server_port)
            .cloned();

        let Some(target) = local_target else {
            error_println!("Unknown server port: {}", server_port);
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                format!("Unknown server port: {}", server_port)
            ).into());
        };

        // Handle the TCP stream data forwarding
        Self::handle_stream(tcp_stream, target).await
    }

    /// Handle a single yamux stream (legacy)
    async fn handle_stream(
        mut yamux_stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        target: ReverseProxyConfig,
    ) -> Result<()> {
        use tokio::io::copy_bidirectional;

        // Connect to local service
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let mut local_stream = TcpStream::connect(&local_addr).await.map_err(|e| {
            ReverseProxyError::ConnectionFailed(format!(
                "Failed to connect to local service {}: {}",
                local_addr, e
            ))
        })?;

        debug_println!("Connected to local service: {}", local_addr);

        // Bidirectional copy
        match copy_bidirectional(&mut yamux_stream, &mut local_stream).await {
            Ok((from_yamux, to_yamux)) => {
                debug_println!(
                    "Stream closed. Transferred: {} bytes from server, {} bytes to server",
                    from_yamux, to_yamux
                );
            }
            Err(e) => {
                debug_println!("Copy error: {}", e);
            }
        }

        Ok(())
    }

        
    /// Handle client connection
    async fn handle_client(
        mut stream: TcpStream,
        addr: SocketAddr,
        clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>,
        auth_token: Option<String>,
        allowed_tokens: Vec<String>,
    ) -> Result<()> {
        // Bind to all interfaces since this is a tunnel to remote clients
        let bind_ip = std::net::Ipv4Addr::UNSPECIFIED; // 0.0.0.0
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

        // Start listening for proxy connections on bound ports
        let client_id = format!("{}", addr);
        let proxy_configs_clone = proxy_configs.clone(); // Clone for use in spawned tasks

        // Spawn listener tasks for each proxy port
        let mut listener_handles = Vec::new();
        for config in &proxy_configs_clone {
            let bind_addr = format!("{}:{}", bind_ip, config.server_port);
            debug_println!("🔧 Binding proxy port: {}", bind_addr);

            match TcpListener::bind(bind_addr).await {
                Ok(listener) => {
                    let port = config.server_port;
                    let configs_for_listener = proxy_configs_clone.clone();

                    let handle = tokio::spawn(async move {
                        debug_println!("✅ Proxy port {} bound and listening", port);
                        // Accept connections on this port
                        loop {
                            match listener.accept().await {
                                Ok((stream, peer_addr)) => {
                                    debug_println!("🔧 New proxy connection from {} on port {}", peer_addr, port);
                                    // Handle the proxy connection
                                    if let Err(e) = Self::handle_direct_tcp_connection(stream, configs_for_listener.clone()).await {
                                        error_println!("❌ Failed to handle direct TCP connection: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error_println!("❌ Failed to accept connection on port {}: {}", port, e);
                                    break;
                                }
                            }
                        }
                    });
                    listener_handles.push(handle);
                    debug_println!("✅ Proxy port {} bound successfully", port);
                }
                Err(e) => {
                    error_println!("❌ Failed to bind port {}: {}", config.server_port, e);
                    return Err(ReverseProxyError::PortAllocationFailed(
                        format!("Failed to bind port {}: {}", config.server_port, e)
                    ).into());
                }
            }
        }

        // Store client session with listener handles
        {
            let mut clients_lock = clients.lock().await;
            clients_lock.insert(client_id.clone(), ClientSession {
                proxies: proxy_configs_clone,
                listeners: listener_handles,
            });
        }

        debug_println!("🔧 Server ready - {} proxy ports bound and listening", proxy_configs.len());

        // Upgrade to Yamux connection for the control channel
        let compat_stream = stream.compat();
        let config = Config::default();
        let mut conn = Connection::new(compat_stream, config, Mode::Server);

        debug_println!("🔧 Server control connection established for client {}", client_id);

        // Keep the control connection alive and handle incoming Yamux streams
        loop {
            match poll_fn(|cx| conn.poll_next_inbound(cx)).await {
                Some(Ok(stream)) => {
                    debug_println!("🔧 Server received new Yamux stream from client {}", client_id);
                    // Handle the stream with the stored proxy configs
                    if let Err(e) = Self::handle_server_stream(stream, proxy_configs.clone()).await {
                        error_println!("❌ Failed to handle server stream: {}", e);
                    }
                }
                Some(Err(e)) => {
                    error_println!("❌ Failed to accept stream from client {}: {}", client_id, e);
                    break;
                }
                None => {
                    debug_println!("🔧 Server control connection closed for client {}", client_id);
                    break;
                }
            }
        }

        // Cleanup
        {
            let mut clients_lock = clients.lock().await;
            clients_lock.remove(&client_id);
        }
        
        println!("❌ Client {} disconnected", addr);
        
        Ok(())
    }
}
