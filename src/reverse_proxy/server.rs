use crate::error::{Result, ReverseProxyError};
use crate::reverse_proxy::protocol::*;
use crate::{debug_println, error_println};
use futures::future::poll_fn;
use sha2::Digest;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{broadcast, mpsc, Mutex};
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
    totp_secret: Option<String>,
    cleanup_task: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl ReverseProxyServer {
    /// Create new reverse proxy server
    pub fn new(bind_ip: std::net::IpAddr, control_port: u16) -> Self {
        let control_addr = SocketAddr::new(bind_ip, control_port);
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let (shutdown_tx, _) = broadcast::channel(1);

        // Start cleanup task
        let cleanup_task = Self::start_cleanup_task(clients.clone());

        Self {
            control_addr,
            clients,
            auth_token: std::env::var("REVERSE_PROXY_TOKEN").ok(),
            allowed_tokens: Vec::new(),
            totp_secret: None,
            cleanup_task: Some(cleanup_task),
            shutdown_tx,
        }
    }

    /// Get a shutdown sender for this server
    pub fn shutdown_token(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Set TOTP secret
    pub fn with_totp_secret(mut self, secret: Option<String>) -> Self {
        self.totp_secret = secret;
        self
    }

    /// Start background cleanup task for inactive clients
    fn start_cleanup_task(
        clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

            loop {
                interval.tick().await;

                let mut clients_guard = clients.lock().await;
                let now = std::time::Instant::now();

                // Remove clients that have been inactive for more than 5 minutes
                let to_remove: Vec<ClientId> = clients_guard
                    .iter()
                    .filter(|(_, session)| {
                        now.duration_since(session.last_activity)
                            > std::time::Duration::from_secs(300)
                    })
                    .map(|(id, _)| id.clone())
                    .collect();

                let removed_count = to_remove.len();
                for client_id in to_remove {
                    debug_println!("🧹 Cleaning up inactive client: {}", client_id);
                    let session = clients_guard.remove(&client_id);
                    if let Some(session) = session {
                        for handle in session.listeners {
                            handle.abort();
                        }
                    }
                }

                if removed_count > 0 {
                    debug_println!("🧹 Cleaned up {} inactive clients", removed_count);
                }
            }
        })
    }
}

impl Drop for ReverseProxyServer {
    fn drop(&mut self) {
        if let Some(task) = self.cleanup_task.take() {
            task.abort();
            debug_println!("🧹 Cleanup task aborted");
        }
    }
}

/// Client session information
#[allow(dead_code)]
struct ClientSession {
    proxies: Vec<ReverseProxyConfig>,
    listeners: Vec<JoinHandle<()>>,
    last_activity: std::time::Instant,
}

impl ClientSession {
    fn new(proxies: Vec<ReverseProxyConfig>, listeners: Vec<JoinHandle<()>>) -> Self {
        Self {
            proxies,
            listeners,
            last_activity: std::time::Instant::now(),
        }
    }

    #[allow(dead_code)]
    fn update_activity(&mut self) {
        self.last_activity = std::time::Instant::now();
    }
}

/// 通过 Channel 传递的隧道请求
struct TunnelRequest {
    tcp_stream: Option<TcpStream>,
    #[allow(dead_code)]
    udp_socket: Option<Arc<UdpSocket>>,
    target_port: u16,
    udp_data: Option<Vec<u8>>,
    udp_peer: Option<SocketAddr>,
}

impl ReverseProxyServer {
    /// Create new reverse proxy server with authentication
    pub fn new_with_auth(
        bind_ip: std::net::IpAddr,
        control_port: u16,
        auth_token: Option<String>,
        allowed_tokens: Vec<String>,
    ) -> Self {
        let control_addr = SocketAddr::new(bind_ip, control_port);
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let (shutdown_tx, _) = broadcast::channel(1);

        let cleanup_task = Self::start_cleanup_task(clients.clone());

        Self {
            control_addr,
            clients,
            auth_token,
            allowed_tokens,
            totp_secret: None, // Added totp_secret initialization
            cleanup_task: Some(cleanup_task),
            shutdown_tx,
        }
    }

    /// Start the reverse proxy server
    pub async fn start(&mut self) -> Result<()> {
        let listener = TcpListener::bind(self.control_addr).await?;
        println!("🔄 Reverse Proxy Server listening on {}", self.control_addr);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                // Accept new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            debug_println!("New control connection from {}", addr);
                            let clients = self.clients.clone();

                            let auth_token = self.auth_token.clone();
                            let allowed_tokens = self.allowed_tokens.clone();
                            let totp_secret = self.totp_secret.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_client(
                                    stream,
                                    addr,
                                    clients,
                                    auth_token,
                                    allowed_tokens,
                                    totp_secret,
                                )
                                .await
                                {
                                    error_println!("Client {} error: {}", addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            error_println!("Accept error: {}", e);
                        }
                    }
                }

                // Check for shutdown signal
                _ = shutdown_rx.recv() => {
                    println!("🛑 Reverse Proxy Server shutting down...");
                    break;
                }
            }
        }

        // Cleanup: close all client connections
        let clients = self.clients.lock().await;
        debug_println!("Closing {} client sessions", clients.len());
        drop(clients);

        Ok(())
    }

    /// Handle client connection
    async fn handle_client(
        mut stream: TcpStream,
        addr: SocketAddr,
        clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>,
        auth_token: Option<String>,
        allowed_tokens: Vec<String>,
        totp_secret: Option<String>,
    ) -> Result<()> {
        let bind_ip = std::net::Ipv4Addr::UNSPECIFIED;

        // Read ClientHello
        let msg = ControlMessage::read_from(&mut stream).await?;

        let (version, token, totp_code, proxy_configs, config_hash) = match msg {
            ControlMessage::ClientHello {
                version,
                token,
                totp_code,
                proxies,
                config_hash,
            } => (version, token, totp_code, proxies, config_hash),
            _ => {
                return Err(
                    ReverseProxyError::ProtocolError("Expected ClientHello".to_string()).into(),
                );
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
            Some(server_token) => token == *server_token || allowed_tokens.contains(&token),
            None => true,
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
            return Err(ReverseProxyError::HandshakeFailed(
                "Authentication failed (Token)".to_string(),
            )
            .into());
        }

        // Validate TOTP if configured
        if let Some(secret) = &totp_secret {
            let totp = crate::utils::totp::Totp::from_base32(secret)
                .unwrap_or_else(|_| crate::utils::totp::Totp::new(secret.as_bytes().to_vec()));
            let totp_valid = match totp_code {
                Some(code) => totp.verify(code),
                None => false,
            };

            if !totp_valid {
                let response = ControlMessage::ServerHello {
                    version: PROTOCOL_VERSION,
                    status: HandshakeStatus::InvalidToken, // Reuse InvalidToken for TOTP failure
                    message: "Invalid or missing TOTP code".to_string(),
                    allowed_ports: Vec::new(),
                    session_id: None,
                };
                response.write_to(&mut stream).await?;
                return Err(ReverseProxyError::HandshakeFailed(
                    "Authentication failed (TOTP)".to_string(),
                )
                .into());
            }
        }

        // Verify configuration hash
        let expected_config_json = serde_json::to_string(&proxy_configs)
            .map_err(|e| ReverseProxyError::SerializationFailed(e.to_string()))?;
        let expected_hash = format!(
            "{:x}",
            sha2::Sha256::digest(expected_config_json.as_bytes())
        );

        if expected_hash != config_hash {
            let response = ControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                status: HandshakeStatus::InvalidConfigHash,
                message: "Configuration hash mismatch".to_string(),
                allowed_ports: Vec::new(),
                session_id: None,
            };
            response.write_to(&mut stream).await?;
            return Err(ReverseProxyError::HandshakeFailed(
                "Configuration integrity check failed".to_string(),
            )
            .into());
        }

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

        let session_id = Some(format!(
            "session_{}_{}",
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

        println!(
            "✅ Client {} connected with {} reverse proxies",
            addr,
            proxy_configs.len()
        );

        // Create channel for tunnel requests
        let (tx, mut rx) = mpsc::channel::<TunnelRequest>(256);

        let client_id = format!("{}", addr);
        let proxy_configs_clone = proxy_configs.clone();

        // Spawn listener tasks for each proxy port
        let mut listener_handles = Vec::new();
        for config in &proxy_configs_clone {
            let bind_addr = format!("{}:{}", bind_ip, config.server_port);
            debug_println!("🔧 Binding proxy port: {}", bind_addr);

            match TcpListener::bind(&bind_addr).await {
                Ok(listener) => {
                    let port = config.server_port;
                    let tx_clone = tx.clone();

                    let handle = tokio::spawn(async move {
                        debug_println!("✅ Proxy port {} listening", port);

                        loop {
                            match listener.accept().await {
                                Ok((tcp_stream, peer_addr)) => {
                                    debug_println!(
                                        "🔧 New connection from {} on port {}",
                                        peer_addr,
                                        port
                                    );

                                    // Send to main yamux loop via channel
                                    if tx_clone
                                        .send(TunnelRequest {
                                            tcp_stream: Some(tcp_stream),
                                            udp_socket: None,
                                            target_port: port,
                                            udp_data: None,
                                            udp_peer: None,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        debug_println!(
                                            "Channel closed, stopping listener for port {}",
                                            port
                                        );
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error_println!("Accept error on port {}: {}", port, e);
                                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                }
                            }
                        }
                    });
                    listener_handles.push(handle);

                    // Also bind UDP if possible
                    match UdpSocket::bind(&bind_addr).await {
                        Ok(udp_socket) => {
                            let udp_socket = Arc::new(udp_socket);
                            let tx_clone = tx.clone();
                            let port = config.server_port;

                            let handle = tokio::spawn(async move {
                                debug_println!("✅ Proxy port {} listening (UDP)", port);
                                let mut buf = [0u8; 65535];

                                loop {
                                    match udp_socket.recv_from(&mut buf).await {
                                        Ok((n, peer_addr)) => {
                                            debug_println!(
                                                "🔧 UDP data from {} on port {}",
                                                peer_addr,
                                                port
                                            );

                                            if tx_clone
                                                .send(TunnelRequest {
                                                    tcp_stream: None,
                                                    udp_socket: Some(udp_socket.clone()),
                                                    target_port: port,
                                                    udp_data: Some(buf[..n].to_vec()),
                                                    udp_peer: Some(peer_addr),
                                                })
                                                .await
                                                .is_err()
                                            {
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            error_println!(
                                                "UDP recv error on port {}: {}",
                                                port,
                                                e
                                            );
                                            tokio::time::sleep(std::time::Duration::from_millis(
                                                100,
                                            ))
                                            .await;
                                        }
                                    }
                                }
                            });
                            listener_handles.push(handle);
                        }
                        Err(e) => {
                            // UDP bind failure is non-fatal for now unless strict requirements
                            error_println!(
                                "⚠️ Failed to bind UDP port {}: {}",
                                config.server_port,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    error_println!("❌ Failed to bind port {}: {}", config.server_port, e);
                }
            }
        }

        // Store client session
        {
            let mut clients_lock = clients.lock().await;
            clients_lock.insert(
                client_id.clone(),
                ClientSession::new(proxy_configs_clone.clone(), listener_handles),
            );
        }

        debug_println!("🔧 Server ready - tunneling via Yamux");

        // Upgrade to Yamux connection
        let compat_stream = stream.compat();

        let mut config = Config::default();
        config.set_max_connection_receive_window(None);
        config.set_max_num_streams(2048);

        debug_println!("🔧 High-performance Yamux tunnel enabled");
        let mut conn = Connection::new(compat_stream, config, Mode::Server);

        // Main loop: handle both inbound streams and tunnel requests
        loop {
            tokio::select! {
                // Handle tunnel requests from listener tasks
                Some(request) = rx.recv() => {
                    debug_println!("🔧 Processing tunnel request for port {}", request.target_port);

                    // Open outbound stream to client
                    match poll_fn(|cx| conn.poll_new_outbound(cx)).await {
                        Ok(yamux_stream) => {
                            let mut compat_stream = yamux_stream.compat();
                            let target_port = request.target_port;

                            // Spawn task to handle this tunnel
                            tokio::spawn(async move {
                                if let Some(mut tcp_stream) = request.tcp_stream {
                                    // TCP Handling
                                    // Write 4-byte header: [Type(1)][Reserved(1)][Port(2)]
                                    let header = [0x01u8, 0x00, (target_port >> 8) as u8, target_port as u8];
                                    if let Err(e) = compat_stream.write_all(&header).await {
                                        error_println!("Failed to write tunnel header: {}", e);
                                        return;
                                    }

                                    // Bidirectional copy
                                    match tokio::io::copy_bidirectional(&mut tcp_stream, &mut compat_stream).await {
                                        Ok((from_client, to_client)) => {
                                            debug_println!("Tunnel closed: {} bytes sent, {} bytes received", from_client, to_client);
                                        }
                                        Err(e) => {
                                            debug_println!("Tunnel error: {}", e);
                                        }
                                    }
                                } else if let (Some(udp_data), Some(_)) = (request.udp_data, request.udp_peer) {
                                    // UDP Handling
                                    // Write 4-byte header: [Type(2)][Reserved(1)][Port(2)]
                                    let header = [0x02u8, 0x00, (target_port >> 8) as u8, target_port as u8];
                                    if let Err(e) = compat_stream.write_all(&header).await {
                                        error_println!("Failed to write packet header: {}", e);
                                        return;
                                    }

                                    // Write UDP payload length (u16)
                                    let len = udp_data.len() as u16;
                                    if let Err(e) = compat_stream.write_all(&len.to_be_bytes()).await {
                                        error_println!("Failed to write UDP payload length: {}", e);
                                        return;
                                    }

                                    // Write UDP payload
                                    if let Err(e) = compat_stream.write_all(&udp_data).await {
                                        error_println!("Failed to write UDP payload: {}", e);
                                        return;
                                    }

                                    // Flush to ensure client receives it
                                    let _ = compat_stream.flush().await;

                                    // Attempt to read response from client (Framed: Len(u16) + Payload)
                                    // Use a loop to support multiple response packets if needed, or just one.
                                    // Given client behavior, it sends response(s) then drops stream.
                                    loop {
                                        let mut len_bytes = [0u8; 2];
                                        if let Err(_) = compat_stream.read_exact(&mut len_bytes).await {
                                            // EOF or error
                                            break;
                                        }
                                        let len = u16::from_be_bytes(len_bytes) as usize;

                                        let mut response_buf = vec![0u8; len];
                                        if let Err(e) = compat_stream.read_exact(&mut response_buf).await {
                                            debug_println!("UDP response payload read error: {}", e);
                                            break;
                                        }

                                        debug_println!("🔧 Received UDP response {} bytes from client", len);

                                        // Send back to UDP peer
                                        if let Some(peer) = request.udp_peer {
                                           if let Some(socket) = &request.udp_socket {
                                                // Handle 0-byte packet by sending empty slice
                                                if let Err(e) = socket.send_to(&response_buf, peer).await {
                                                     error_println!("Failed to forward UDP response to peer: {}", e);
                                                }
                                           }
                                        }
                                    }

                                    // Close stream
                                    drop(compat_stream);
                                }
                            });
                        }
                        Err(e) => {
                            error_println!("Failed to open Yamux stream: {}", e);
                        }
                    }
                }

                // Handle inbound streams (unexpected in this mode, but keep connection alive)
                result = poll_fn(|cx| conn.poll_next_inbound(cx)) => {
                    match result {
                        Some(Ok(_stream)) => {
                            debug_println!("🔧 Received unexpected inbound stream from client");
                        }
                        Some(Err(e)) => {
                            error_println!("❌ Yamux error: {}", e);
                            break;
                        }
                        None => {
                            debug_println!("🔧 Yamux connection closed");
                            break;
                        }
                    }
                }
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

        debug_println!("🔧 Client {} disconnected", addr);
        Ok(())
    }
}
