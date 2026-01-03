use crate::config::loader::ConfigFile;
use crate::error::{Result, ReverseProxyError};
use crate::reverse_proxy::protocol::*;
use crate::{debug_println, error_println};
use futures::future::poll_fn;
use sha2::Digest;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream, UdpSocket};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use yamux::{Config, Connection, Mode};

/// Reverse proxy client
///
/// 简化的反向代理客户端，直接使用单个 Yamux 连接
pub struct ReverseProxyClient {
    server_addr: SocketAddr,
    config: ConfigFile,
    auth_token: Option<String>,
    totp_secret: Option<String>,
}

impl ReverseProxyClient {
    /// Create new reverse proxy client
    pub fn new(server_addr: SocketAddr, config: ConfigFile) -> Self {
        // 从环境变量读取auth_token
        let auth_token = std::env::var("REVERSE_PROXY_TOKEN").ok().or_else(|| {
            // 从配置文件读取token（如果指定）
            config.reverse_proxy_server.as_ref().and_then(|s| {
                if !s.allowed_tokens.is_empty() {
                    Some(s.allowed_tokens[0].clone())
                } else {
                    None
                }
            })
        });

        let totp_secret = config
            .reverse_proxy_client
            .as_ref()
            .and_then(|c| c.totp_secret.clone());

        Self {
            server_addr,
            config,
            auth_token,
            totp_secret,
        }
    }

    /// Create new reverse proxy client with custom auth token
    pub fn new_with_token(server_addr: SocketAddr, config: ConfigFile, auth_token: String) -> Self {
        Self {
            server_addr,
            config,
            auth_token: Some(auth_token),
            totp_secret: None,
        }
    }

    /// Start the reverse proxy client with automatic reconnection
    pub async fn start(&mut self) -> Result<()> {
        let mut retry_count = 0u64;
        let mut backoff_seconds = 1u64;
        const MAX_BACKOFF: u64 = 60;
        const MAX_RETRIES: u64 = 10;

        loop {
            if retry_count >= MAX_RETRIES {
                error_println!(
                    "Maximum retry attempts ({}) exceeded, giving up",
                    MAX_RETRIES
                );
                return Err(ReverseProxyError::ConnectionFailed(format!(
                    "Failed after {} retry attempts",
                    MAX_RETRIES
                ))
                .into());
            }

            match self.run_connection().await {
                Ok(_) => {
                    println!("✅ Connection completed successfully");
                    return Ok(());
                }
                Err(e) => {
                    retry_count += 1;
                    error_println!("Connection failed (attempt {}): {}", retry_count, e);

                    if retry_count < MAX_RETRIES {
                        println!(
                            "🔄 Reconnecting in {} seconds... (attempt {}/{})",
                            backoff_seconds, retry_count, MAX_RETRIES
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)).await;
                        backoff_seconds = (backoff_seconds * 2).min(MAX_BACKOFF);
                    }
                }
            }
        }
    }

    /// Run a single connection session
    async fn run_connection(&mut self) -> Result<()> {
        // 1. 解析代理配置
        let proxy_configs = self.parse_proxy_configs()?;

        if proxy_configs.is_empty() {
            return Err(ReverseProxyError::HandshakeFailed(
                "No valid reverse proxy configurations".to_string(),
            )
            .into());
        }

        println!(
            "🔄 Connecting to reverse proxy server: {}",
            self.server_addr
        );

        // 2. 创建优化的 TCP 连接
        let mut stream = self.create_optimized_tcp().await?;

        // 3. 执行握手协议
        self.do_handshake(&mut stream, &proxy_configs).await?;

        // 4. 显示活跃的反向代理
        println!("\n📡 Active Reverse Proxies:");
        for config in &proxy_configs {
            println!(
                "   Server:{} → Local:{}:{}",
                config.server_port, config.local_host, config.local_port
            );
        }
        println!();

        // 5. 升级到 Yamux 连接
        let compat_stream = stream.compat();
        let yamux_config = Self::create_optimized_yamux_config();
        let mut conn = Connection::new(compat_stream, yamux_config, Mode::Client);

        println!("✅ Yamux connection established, waiting for tunnel requests...");

        // 6. 主循环：处理来自服务端的流
        loop {
            match poll_fn(|cx| conn.poll_next_inbound(cx)).await {
                Some(Ok(yamux_stream)) => {
                    debug_println!("🔧 Received new tunnel stream from server");

                    let configs = proxy_configs.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_tunnel_stream(yamux_stream, configs).await {
                            error_println!("❌ Tunnel stream error: {}", e);
                        }
                    });
                }
                Some(Err(e)) => {
                    error_println!("❌ Yamux connection error: {}", e);
                    return Err(ReverseProxyError::ConnectionFailed(e.to_string()).into());
                }
                None => {
                    debug_println!("🔧 Yamux connection closed by server");
                    return Err(ReverseProxyError::ConnectionFailed(
                        "Connection closed by server".to_string(),
                    )
                    .into());
                }
            }
        }
    }

    /// Parse reverse proxy configurations from config file
    fn parse_proxy_configs(&self) -> Result<Vec<ReverseProxyConfig>> {
        let mut proxy_configs = Vec::new();

        for rproxy in &self.config.reverse_proxies {
            let server_port = rproxy.get_server_port().map_err(|e| {
                ReverseProxyError::HandshakeFailed(format!("Invalid server config: {}", e))
            })?;
            let server_host = rproxy.get_server_ip();
            let local_port = rproxy.get_local_port().map_err(|e| {
                ReverseProxyError::HandshakeFailed(format!("Invalid local config: {}", e))
            })?;
            let local_host = rproxy
                .get_local_host()
                .unwrap_or_else(|| "localhost".to_string());

            debug_println!(
                "🔧 Proxy config: server_port={}, local={}:{}",
                server_port,
                local_host,
                local_port
            );

            proxy_configs.push(ReverseProxyConfig {
                server_port,
                server_host,
                local_host,
                local_port,
            });
        }

        Ok(proxy_configs)
    }

    /// Create optimized TCP connection with performance tuning
    async fn create_optimized_tcp(&self) -> Result<TcpStream> {
        let socket = TcpSocket::new_v4()?;

        // TCP 优化参数
        socket.set_nodelay(true)?; // 禁用 Nagle 算法
        socket.set_recv_buffer_size(4 * 1024 * 1024)?; // 4MB 接收缓冲
        socket.set_send_buffer_size(4 * 1024 * 1024)?; // 4MB 发送缓冲
        socket.set_keepalive(true)?; // 启用 TCP keepalive

        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            socket.connect(self.server_addr),
        )
        .await
        .map_err(|_| ReverseProxyError::ConnectionFailed("Connection timeout".to_string()))??;

        Ok(stream)
    }

    /// Perform handshake with server
    async fn do_handshake(
        &self,
        stream: &mut TcpStream,
        config: &[ReverseProxyConfig],
    ) -> Result<()> {
        debug_println!("🔧 Starting handshake with {} configs", config.len());

        // 计算配置哈希
        let config_json = serde_json::to_string(config)
            .map_err(|e| ReverseProxyError::SerializationFailed(e.to_string()))?;
        let config_hash = format!("{:x}", sha2::Sha256::digest(config_json.as_bytes()));

        // 获取认证令牌
        let token = self.auth_token.clone().unwrap_or_default();

        // 生成 TOTP 验证码（如果配置了密钥）
        let totp_code = self.totp_secret.as_ref().map(|secret| {
            let totp = crate::utils::totp::Totp::from_base32(secret)
                .unwrap_or_else(|_| crate::utils::totp::Totp::new(secret.as_bytes().to_vec()));
            totp.generate_current()
        });

        // 发送 ClientHello
        let hello = ControlMessage::ClientHello {
            version: PROTOCOL_VERSION,
            token,
            totp_code,
            proxies: config.to_vec(),
            config_hash,
        };

        debug_println!("🔧 Sending ClientHello");
        hello.write_to(stream).await?;

        // 接收 ServerHello
        debug_println!("🔧 Waiting for ServerHello...");
        let response = ControlMessage::read_from(stream).await?;

        match response {
            ControlMessage::ServerHello {
                status, message, ..
            } => {
                debug_println!(
                    "🔧 Received ServerHello: status={:?}, message={}",
                    status,
                    message
                );
                match status {
                    HandshakeStatus::Ok => {
                        println!("✅ Handshake successful: {}", message);
                        Ok(())
                    }
                    _ => Err(ReverseProxyError::HandshakeFailed(message).into()),
                }
            }
            _ => Err(
                ReverseProxyError::HandshakeFailed("Invalid server response".to_string()).into(),
            ),
        }
    }

    /// Create optimized Yamux configuration
    fn create_optimized_yamux_config() -> Config {
        let mut config = Config::default();

        // 高性能配置
        config.set_max_connection_receive_window(None);
        config.set_max_num_streams(2048);

        debug_println!("🔧 Yamux config: max_window=4MB, max_streams=2048");

        config
    }

    /// Handle a single tunnel stream from server
    async fn handle_tunnel_stream(
        yamux_stream: yamux::Stream,
        proxy_configs: Vec<ReverseProxyConfig>,
    ) -> Result<()> {
        let mut stream = yamux_stream.compat();

        // 1. 读取 4 字节协议头: [Type(1)] [Reserved(1)] [Port(2)]
        let mut header = [0u8; 4];
        stream.read_exact(&mut header).await.map_err(|e| {
            ReverseProxyError::ConnectionFailed(format!("Failed to read header: {}", e))
        })?;

        let msg_type = header[0];
        let _reserved = header[1];
        let target_port = u16::from_be_bytes([header[2], header[3]]);

        debug_println!(
            "🔧 Tunnel request: Type=0x{:02x}, Port={}",
            msg_type,
            target_port
        );
        println!(
            "DEBUG: Client Received Tunnel Request Type=0x{:02x} for Port={}",
            msg_type, target_port
        );

        // 2. 根据类型处理
        match msg_type {
            0x01 => {
                // TCP 直接转发
            }
            0x02 => {
                // UDP forwarding
                // 3. 查找目标本地服务配置
                let target = proxy_configs
                    .iter()
                    .find(|c| c.server_port == target_port)
                    .cloned()
                    .ok_or_else(|| {
                        ReverseProxyError::ConnectionFailed(format!(
                            "No local target for port {}",
                            target_port
                        ))
                    })?;

                // Read UDP payload length
                let mut len_bytes = [0u8; 2];
                stream.read_exact(&mut len_bytes).await.map_err(|e| {
                    ReverseProxyError::ConnectionFailed(format!(
                        "Failed to read UDP payload length: {}",
                        e
                    ))
                })?;
                let len = u16::from_be_bytes(len_bytes) as usize;

                // Read payload
                let mut payload = vec![0u8; len];
                stream.read_exact(&mut payload).await.map_err(|e| {
                    ReverseProxyError::ConnectionFailed(format!(
                        "Failed to read UDP payload: {}",
                        e
                    ))
                })?;

                debug_println!(
                    "🔧 UDP Forwarding {} bytes to local service {}:{}",
                    payload.len(),
                    target.local_host,
                    target.local_port
                );

                // Create UDP socket to send to local target
                let local_addr = format!("{}:{}", target.local_host, target.local_port);
                let socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| {
                    ReverseProxyError::ConnectionFailed(format!("Failed to bind UDP socket: {}", e))
                })?;

                // Send data
                socket.send_to(&payload, &local_addr).await.map_err(|e| {
                    ReverseProxyError::ConnectionFailed(format!("Failed to send UDP data: {}", e))
                })?;

                // Try to read response (with timeout)
                let mut buf = [0u8; 65535];
                match tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    socket.recv_from(&mut buf),
                )
                .await
                {
                    Ok(Ok((n, from))) => {
                        println!("DEBUG: Client Received UDP response {} bytes from {}, sending back to server", n, from);
                        // Write response back to stream
                        if let Err(e) = stream.write_all(&buf[..n]).await {
                            println!("FAILED to write UDP response to tunnel: {}", e);
                        }
                    }
                    Ok(Err(e)) => {
                        println!("UDP recv error or closed: {}", e);
                    }
                    Err(_) => {
                        // Timeout - no response, just close
                        println!(
                            "UDP wait for response timed out on socket {:?}",
                            socket.local_addr()
                        );
                    }
                }

                // Close stream
                drop(stream);
                // the server needs to keep the stream open.
                //
                // Given the current server implementation:
                // "let _ = compat_stream.get_mut().shutdown().await;"
                // The stream is closed. So we just handle the incoming packet.

                return Ok(());
            }
            0x03 => {
                return Err(ReverseProxyError::ConnectionFailed(
                    "PROXY protocol not implemented".to_string(),
                )
                .into());
            }
            _ => {
                return Err(ReverseProxyError::ConnectionFailed(format!(
                    "Unknown protocol type 0x{:02x}",
                    msg_type
                ))
                .into());
            }
        }

        // 3. 查找目标本地服务配置
        let target = proxy_configs
            .iter()
            .find(|c| c.server_port == target_port)
            .cloned()
            .ok_or_else(|| {
                ReverseProxyError::ConnectionFailed(format!(
                    "No local target for port {}",
                    target_port
                ))
            })?;

        debug_println!(
            "🔧 Forwarding to local service {}:{}",
            target.local_host,
            target.local_port
        );

        // 4. 连接本地服务并双向转发
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        let mut local_stream = TcpStream::connect(&local_addr).await.map_err(|e| {
            ReverseProxyError::ConnectionFailed(format!(
                "Failed to connect to local service {}: {}",
                local_addr, e
            ))
        })?;

        match tokio::io::copy_bidirectional(&mut stream, &mut local_stream).await {
            Ok((from_server, to_server)) => {
                debug_println!(
                    "🔧 Tunnel closed: {} bytes from server, {} bytes to server",
                    from_server,
                    to_server
                );
            }
            Err(e) => {
                debug_println!("🔧 Tunnel copy error: {}", e);
            }
        }

        Ok(())
    }
}
