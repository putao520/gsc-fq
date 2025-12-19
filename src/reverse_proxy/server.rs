use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::{debug_println, error_println};
use futures::future::poll_fn;
use sha2::Digest;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_util::compat::TokioAsyncReadCompatExt;
use yamux::{Config, Connection, Mode};

type ClientId = String;

/// Reverse proxy server
pub struct ReverseProxyServer {
    control_addr: SocketAddr,
    clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>,
    auth_token: Option<String>,
    allowed_tokens: Vec<String>,
    cleanup_task: Option<tokio::task::JoinHandle<()>>,
}

impl ReverseProxyServer {
    /// Create new reverse proxy server
    pub fn new(bind_ip: std::net::IpAddr, control_port: u16) -> Self {
        let control_addr = SocketAddr::new(bind_ip, control_port);
        let clients = Arc::new(Mutex::new(HashMap::new()));

        // Start cleanup task
        let cleanup_task = Self::start_cleanup_task(clients.clone());

        Self {
            control_addr,
            clients,
            auth_token: std::env::var("REVERSE_PROXY_TOKEN").ok(),
            allowed_tokens: Vec::new(),
            cleanup_task: Some(cleanup_task),
        }
    }

    /// Start background cleanup task for inactive clients
    fn start_cleanup_task(clients: Arc<Mutex<HashMap<ClientId, ClientSession>>>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

            loop {
                interval.tick().await;

                let mut clients_guard = clients.lock().await;
                let now = std::time::Instant::now();

                // Remove clients that have been inactive for more than 5 minutes
                let to_remove: Vec<ClientId> = clients_guard.iter()
                    .filter(|(_, session)| now.duration_since(session.last_activity) > std::time::Duration::from_secs(300))
                    .map(|(id, _)| id.clone())
                    .collect();

                let removed_count = to_remove.len();
                for client_id in to_remove {
                    debug_println!("🧹 Cleaning up inactive client: {}", client_id);
                    let session = clients_guard.remove(&client_id);
                    if let Some(session) = session {
                        // Abort all listener handles
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

    fn update_activity(&mut self) {
        self.last_activity = std::time::Instant::now();
    }
}

impl ReverseProxyServer {
    /// Create new reverse proxy server with authentication
    pub fn new_with_auth(
        bind_ip: std::net::IpAddr,
        control_port: u16,
        auth_token: Option<String>,
        allowed_tokens: Vec<String>
    ) -> Self {
        let control_addr = SocketAddr::new(bind_ip, control_port);
        let clients = Arc::new(Mutex::new(HashMap::new()));

        // Start cleanup task
        let cleanup_task = Self::start_cleanup_task(clients.clone());

        Self {
            control_addr,
            clients,
            auth_token,
            allowed_tokens,
            cleanup_task: Some(cleanup_task),
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

    /// Handle a single yamux stream with activity update to prevent cleanup
    async fn handle_server_stream_with_activity_update(
        yamux_stream: yamux::Stream,
        client_id: &ClientId,
        clients: &Arc<Mutex<HashMap<ClientId, ClientSession>>>,
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

        // Handle the stream data forwarding with periodic activity updates
        Self::handle_stream_forwarding_with_activity_update(compat_stream, target, client_id, clients).await
    }

    /// Handle the stream data forwarding with periodic activity updates
    async fn handle_stream_forwarding_with_activity_update(
        mut yamux_stream: tokio_util::compat::Compat<yamux::Stream>,
        target: ReverseProxyConfig,
        client_id: &ClientId,
        clients: &Arc<Mutex<HashMap<ClientId, ClientSession>>>,
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

        // Create activity update interval (update every 30 seconds during long transfers)
        let mut activity_interval = interval(std::time::Duration::from_secs(30));
        let mut data_transfer_started = false;

        // Read data with periodic activity updates
        let mut read_buffer = [0u8; 1024];
        let mut write_buffer = [0u8; 1024];
        loop {
            tokio::select! {
                // Update activity periodically
                _ = activity_interval.tick() => {
                    debug_println!("🔄 Updating activity for client {} during data transfer", client_id);
                    let mut clients_lock = clients.lock().await;
                    if let Some(session) = clients_lock.get_mut(client_id) {
                        session.update_activity();
                    }
                }

                // Read data from Yamux
                result = yamux_stream.read(&mut read_buffer) => {
                    match result {
                        Ok(0) => {
                            // EOF - connection closed
                            debug_println!("📥 Connection closed for client {}", client_id);
                            break;
                        }
                        Ok(n) => {
                            if !data_transfer_started {
                                debug_println!("📥 Starting data transfer for client {} ({} bytes)", client_id, n);
                                data_transfer_started = true;
                                // Update activity when data transfer starts
                                let mut clients_lock = clients.lock().await;
                                if let Some(session) = clients_lock.get_mut(client_id) {
                                    session.update_activity();
                                }
                            }
                            // Forward data to local stream
                            if let Err(e) = local_stream.write_all(&read_buffer[..n]).await {
                                debug_println!("Write error to local stream: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            debug_println!("Read error: {}", e);
                            break;
                        }
                    }
                }

                // Write data from local to Yamux
                result = local_stream.read(&mut write_buffer) => {
                    match result {
                        Ok(0) => {
                            // EOF - connection closed
                            debug_println!("📥 Local connection closed for client {}", client_id);
                            break;
                        }
                        Ok(n) => {
                            // Forward data to Yamux stream
                            if let Err(e) = yamux_stream.write_all(&write_buffer[..n]).await {
                                debug_println!("Write error to Yamux stream: {}", e);
                                break;
                            }
                            if let Err(e) = yamux_stream.flush().await {
                                debug_println!("Flush error: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            debug_println!("Local read error: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        // Final activity update when stream ends
        debug_println!("🔚 Data transfer completed for client {}", client_id);
        let mut clients_lock = clients.lock().await;
        if let Some(session) = clients_lock.get_mut(client_id) {
            session.update_activity();
        }

        Ok(())
    }

    /// Handle a direct TCP connection from proxy port
    async fn handle_direct_tcp_connection(
        tcp_stream: TcpStream,
        proxy_configs: Vec<ReverseProxyConfig>,
    ) -> Result<()> {
        // Get the local port this connection came in on
        let local_port = tcp_stream.local_addr()?.port();
        debug_println!("Received direct TCP connection on local port {}", local_port);

        // Find the corresponding local target based on the listening port
        let local_target = proxy_configs.iter()
            .find(|c| c.server_port == local_port)
            .cloned();

        let Some(target) = local_target else {
            error_println!("No proxy configuration found for local port {}", local_port);
            return Err(crate::error::ReverseProxyError::ConnectionFailed(
                format!("No proxy configuration found for local port {}", local_port)
            ).into());
        };

        debug_println!("Forwarding connection from port {} to {}:{}",
            local_port, target.local_host, target.local_port);

        // Use a more robust forwarding approach to prevent data corruption
        Self::handle_stream_with_no_corruption(tcp_stream, target).await
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

        // Use bidirectional copy for reliable data transfer
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

    /// Robust data forwarding without data corruption
    async fn handle_stream_with_no_corruption(
        mut stream: TcpStream,
        target: ReverseProxyConfig,
    ) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Connect to local service
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let mut local_stream = TcpStream::connect(&local_addr).await.map_err(|e| {
            ReverseProxyError::ConnectionFailed(format!(
                "Failed to connect to local service {}: {}",
                local_addr, e
            ))
        })?;

        debug_println!("Connected to local service: {}", local_addr);

        // Use separate buffers to avoid borrowing issues
        let mut client_buf = [0u8; 4096];
        let mut server_buf = [0u8; 4096];
        let mut client_closed = false;
        let mut server_closed = false;

        while !client_closed && !server_closed {
            tokio::select! {
                // Read from client and write to server
                result = stream.read(&mut client_buf) => {
                    match result {
                        Ok(0) => {
                            client_closed = true;
                            debug_println!("📥 Client closed connection");
                            break;
                        }
                        Ok(n) => {
                            // Forward exact bytes to local service
                            if let Err(e) = local_stream.write_all(&client_buf[..n]).await {
                                debug_println!("Error writing to local service: {}", e);
                                break;
                            }
                            if let Err(e) = local_stream.flush().await {
                                debug_println!("Error flushing to local service: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            debug_println!("Client read error: {}", e);
                            break;
                        }
                    }
                }

                // Read from server and write to client
                result = local_stream.read(&mut server_buf) => {
                    match result {
                        Ok(0) => {
                            server_closed = true;
                            debug_println!("📥 Server (local service) closed connection");
                            break;
                        }
                        Ok(n) => {
                            // Forward exact bytes back to client
                            if let Err(e) = stream.write_all(&server_buf[..n]).await {
                                debug_println!("Error writing to client: {}", e);
                                break;
                            }
                            if let Err(e) = stream.flush().await {
                                debug_println!("Error flushing to client: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            debug_println!("Server read error: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        debug_println!("✅ Robust stream forwarding completed");
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
                        let mut consecutive_accept_errors = 0;
                        const MAX_ACCEPT_ERRORS: u32 = 10;

                        // Accept connections on this port
                        loop {
                            match listener.accept().await {
                                Ok((stream, peer_addr)) => {
                                    debug_println!("🔧 New proxy connection from {} on port {}", peer_addr, port);
                                    consecutive_accept_errors = 0; // Reset error counter on success

                                    // Handle the proxy connection
                                    if let Err(e) = Self::handle_direct_tcp_connection(stream, configs_for_listener.clone()).await {
                                        error_println!("❌ Failed to handle TCP connection: {}", e);
                                    }
                                }
                                Err(e) => {
                                    consecutive_accept_errors += 1;
                                    error_println!("❌ Accept error {} on port {}: {}", consecutive_accept_errors, port, e);

                                    // Only exit if we get many consecutive errors
                                    if consecutive_accept_errors >= MAX_ACCEPT_ERRORS {
                                        error_println!("❌ Too many consecutive accept errors ({}), stopping listener for port {}",
                                            consecutive_accept_errors, port);
                                        break;
                                    }

                                    // Brief delay before retrying
                                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                }
                            }
                        }
                        debug_println!("🔧 Proxy port {} listener stopped after {} consecutive errors",
                            port, consecutive_accept_errors);
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
            clients_lock.insert(client_id.clone(), ClientSession::new(
                proxy_configs_clone,
                listener_handles,
            ));
        }

        debug_println!("🔧 Server ready - {} proxy ports bound and listening", proxy_configs.len());

        // Upgrade to Yamux connection for the control channel
        let compat_stream = stream.compat();
        let config = Config::default();
        debug_println!("🔧 Yamux config: using default settings");
        let mut conn = Connection::new(compat_stream, config, Mode::Server);

        debug_println!("🔧 Server control connection established for client {}", client_id);

        // Keep the control connection alive and handle incoming Yamux streams
        let mut last_stream_time = std::time::Instant::now();
        let mut idle_check_interval = tokio::time::interval(std::time::Duration::from_secs(10));

        debug_println!("🔧 Starting control connection monitoring for client {}", client_id);

        loop {
            tokio::select! {
                // Handle incoming Yamux streams
                result = poll_fn(|cx| conn.poll_next_inbound(cx)) => {
                    match result {
                        Some(Ok(stream)) => {
                            debug_println!("🔧 Server received new Yamux stream from client {}", client_id);
                            last_stream_time = std::time::Instant::now();

                            // Update client activity
                            {
                                let mut clients_lock = clients.lock().await;
                                if let Some(session) = clients_lock.get_mut(&client_id) {
                                    session.update_activity();
                                }
                            }
                            // Handle the stream with the stored proxy configs
                            match Self::handle_server_stream_with_activity_update(stream, &client_id, &clients, proxy_configs.clone()).await {
                                Ok(()) => {
                                    debug_println!("✅ Stream handled successfully for client {}", client_id);
                                }
                                Err(e) => {
                                    error_println!("❌ Failed to handle server stream: {}", e);
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error_println!("❌ Failed to accept stream from client {}: {}", client_id, e);
                            break;
                        }
                        None => {
                            debug_println!("🔧 Server control connection closed for client {} (poll returned None)", client_id);
                            // Update activity one last time before disconnecting
                            {
                                let mut clients_lock = clients.lock().await;
                                if let Some(session) = clients_lock.get_mut(&client_id) {
                                    session.update_activity();
                                }
                            }
                            break;
                        }
                    }
                }

                // Periodic idle check to prevent the connection from blocking indefinitely
                _ = idle_check_interval.tick() => {
                    let idle_time = last_stream_time.elapsed();
                    debug_println!("🔧 Control connection idle for {} seconds for client {}", idle_time.as_secs(), client_id);

                    // Keep updating activity to prevent cleanup
                    {
                        let mut clients_lock = clients.lock().await;
                        if let Some(session) = clients_lock.get_mut(&client_id) {
                            session.update_activity();
                        }
                    }
                }
            }
        }

        // Don't remove the client session here - it will be cleaned up by the background task
        debug_println!("🔧 Client {} disconnected, proxy listeners will remain active until timeout", addr);

        Ok(())
    }
}
