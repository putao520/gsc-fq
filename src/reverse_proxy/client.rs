use crate::config::loader::{ConfigFile, ReverseProxySection};
use crate::error::{ReverseProxyError, Result};
use crate::reverse_proxy::protocol::*;
use crate::{debug_println, error_println};
use futures::StreamExt;
use std::net::{IpAddr, SocketAddr};
use tokio::io::{copy_bidirectional, AsyncReadExt};
use tokio::net::TcpStream;
use tokio_util::compat::{FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use yamux::{Config, Connection, Mode};

/// Reverse proxy client
pub struct ReverseProxyClient {
    server_addr: SocketAddr,
    config: ConfigFile,
}

impl ReverseProxyClient {
    /// Create new reverse proxy client
    pub fn new(server_addr: SocketAddr, config: ConfigFile) -> Self {
        Self {
            server_addr,
            config,
        }
    }
    
    /// Start the reverse proxy client
    pub async fn start(&mut self) -> Result<()> {
        println!("🔄 Connecting to reverse proxy server at {}", self.server_addr);
        
        // Connect to server
        let mut stream = TcpStream::connect(self.server_addr).await?;
        println!("✅ Connected to server");
        
        // Convert ReverseProxySection to ReverseProxyConfig
        let proxy_configs: Vec<ReverseProxyConfig> = self.config.reverse_proxies
            .iter()
            .filter_map(|rproxy| {
                let server_port = rproxy.get_server_port()?;
                let local_port = rproxy.get_local_port()?;
                let local_host = rproxy.get_local_host();
                
                Some(ReverseProxyConfig {
                    server_port,
                    local_host,
                    local_port,
                })
            })
            .collect();
        
        if proxy_configs.is_empty() {
            return Err(ReverseProxyError::HandshakeFailed(
                "No valid reverse proxy configurations".to_string()
            ).into());
        }
        
        // Send ClientHello
        let hello = ControlMessage::ClientHello {
            version: PROTOCOL_VERSION,
            proxies: proxy_configs.clone(),
        };
        hello.write_to(&mut stream).await?;
        
        // Receive ServerHello
        let response = ControlMessage::read_from(&mut stream).await?;
        
        match response {
            ControlMessage::ServerHello { version: _, status, message } => {
                match status {
                    HandshakeStatus::Ok => {
                        println!("✅ {}", message);
                    }
                    HandshakeStatus::VersionMismatch => {
                        return Err(ReverseProxyError::HandshakeFailed(
                            format!("Version mismatch: {}", message)
                        ).into());
                    }
                    HandshakeStatus::ConfigError => {
                        return Err(ReverseProxyError::HandshakeFailed(
                            format!("Config error: {}", message)
                        ).into());
                    }
                    HandshakeStatus::PortAllocationFailed => {
                        return Err(ReverseProxyError::PortAllocationFailed(message).into());
                    }
                }
            }
            _ => {
                return Err(ReverseProxyError::ProtocolError(
                    "Expected ServerHello".to_string()
                ).into());
            }
        }
        
        // Display active reverse proxies
        println!("\n📡 Active Reverse Proxies:");
        for config in &proxy_configs {
            println!("   Server:{} → Local:{}:{}",
                config.server_port,
                config.local_host,
                config.local_port
            );
        }
        println!();
        
        // Upgrade to Yamux connection
        let compat_stream = stream.compat();
        let yamux_config = Config::default();
        let conn = Connection::new(compat_stream, yamux_config, Mode::Client);
        
        // Convert yamux connection to stream
        let incoming = yamux::into_stream(conn);
        tokio::pin!(incoming);
        
        // Main loop: accept incoming yamux streams
        while let Some(stream_result) = incoming.next().await {
            match stream_result {
                Ok(yamux_stream) => {
                    let mut yamux_tokio = yamux_stream.compat();
                    
                    // Read port header (first 2 bytes)
                    let mut port_bytes = [0u8; 2];
                    if let Err(e) = yamux_tokio.read_exact(&mut port_bytes).await {
                        error_println!("Failed to read port header: {}", e);
                        continue;
                    }
                    
                    let server_port = u16::from_be_bytes(port_bytes);
                    debug_println!("New stream for port {}", server_port);
                    
                    // Find the corresponding local target
                    let local_target = proxy_configs.iter()
                        .find(|c| c.server_port == server_port)
                        .cloned();
                    
                    if let Some(target) = local_target {
                        let source_ip = self.config.reverse_proxies.iter()
                            .find(|rp| rp.get_server_port() == Some(server_port))
                            .and_then(|rp| rp.source_ip.clone());
                        
                        // Spawn task to handle this stream
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_stream(
                                yamux_tokio,
                                target,
                                source_ip,
                            ).await {
                                error_println!("Stream error: {}", e);
                            }
                        });
                    } else {
                        error_println!("No local target for server port {}", server_port);
                    }
                }
                Err(e) => {
                    error_println!("Yamux stream error: {}", e);
                    break;
                }
            }
        }
        
        println!("❌ Disconnected from server");
        Ok(())
    }
    
    /// Handle a single yamux stream
    async fn handle_stream(
        mut yamux_tokio: tokio_util::compat::Compat<yamux::Stream>,
        target: ReverseProxyConfig,
        source_ip: Option<String>,
    ) -> Result<()> {
        debug_println!("Handling stream to {}:{}",
            target.local_host, target.local_port);
        
        // Connect to local target
        let local_addr = format!("{}:{}", target.local_host, target.local_port);
        
        let mut local_stream = if let Some(src_ip) = source_ip {
            // Bind to specific source IP
            let src_addr: IpAddr = src_ip.parse()
                .map_err(|e| ReverseProxyError::ProtocolError(format!("Invalid source IP: {}", e)))?;
            
            let socket = if src_addr.is_ipv4() {
                tokio::net::TcpSocket::new_v4()?
            } else {
                tokio::net::TcpSocket::new_v6()?
            };
            
            socket.bind(SocketAddr::new(src_addr, 0))?;
            let target_addr: SocketAddr = local_addr.parse()
                .map_err(|e| ReverseProxyError::ProtocolError(format!("Invalid target address: {}", e)))?;
            socket.connect(target_addr).await?
        } else {
            TcpStream::connect(&local_addr).await?
        };
        
        debug_println!("Stream connected to local target");
        
        // Forward data bidirectionally
        match copy_bidirectional(&mut local_stream, &mut yamux_tokio).await {
            Ok((to_yamux, from_yamux)) => {
                debug_println!("Stream closed: sent {} bytes, received {} bytes", 
                    to_yamux, from_yamux);
            }
            Err(e) => {
                debug_println!("Stream error: {}", e);
            }
        }
        
        Ok(())
    }
}
