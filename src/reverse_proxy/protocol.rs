use serde::{Deserialize, Serialize};
use crate::error::{ReverseProxyError, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum message size (16MB)
const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// Control channel message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Client → Server: Handshake + configuration + TOKEN
    ClientHello {
        version: u8,
        token: String,
        proxies: Vec<ReverseProxyConfig>,
        config_hash: String, // SHA256 hash of client config for verification
    },

    /// Server → Client: Handshake response
    ServerHello {
        version: u8,
        status: HandshakeStatus,
        message: String,
        allowed_ports: Vec<u16>, // Allowed ports for this token
        session_id: Option<String>, // Session identifier
    },

    /// Server → Client: Heartbeat/ping
    Ping,

    /// Client → Server: Heartbeat/pong
    Pong,
}

/// Reverse proxy configuration sent by client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseProxyConfig {
    pub server_port: u16,
    pub server_host: Option<String>,  // 新增：服务器绑定IP
    pub local_host: String,
    pub local_port: u16,
}

/// Handshake status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandshakeStatus {
    Ok,
    VersionMismatch,
    ConfigError,
    PortAllocationFailed,
    InvalidToken,
    TokenExpired,
    AccessDenied,
    InvalidConfigHash,
}

impl ControlMessage {
    /// Serialize message to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| ReverseProxyError::SerializationFailed(e.to_string()).into())
    }
    
    /// Deserialize message from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        bincode::deserialize(bytes)
            .map_err(|e| ReverseProxyError::DeserializationFailed(e.to_string()).into())
    }
    
    /// Write message to async stream
    pub async fn write_to<W: AsyncWriteExt + Unpin>(&self, writer: &mut W) -> Result<()> {
        let data = self.to_bytes()?;
        let len = data.len() as u32;
        
        if len > MAX_MESSAGE_SIZE {
            return Err(ReverseProxyError::InvalidMessage(
                format!("Message too large: {} bytes", len)
            ).into());
        }
        
        // Write length prefix (4 bytes, big-endian)
        writer.write_all(&len.to_be_bytes()).await?;
        
        // Write message data
        writer.write_all(&data).await?;
        writer.flush().await?;
        
        Ok(())
    }
    
    /// Read message from async stream
    pub async fn read_from<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Self> {
        // Read length prefix
        let mut len_bytes = [0u8; 4];
        reader.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes);
        
        if len > MAX_MESSAGE_SIZE {
            return Err(ReverseProxyError::InvalidMessage(
                format!("Message too large: {} bytes", len)
            ).into());
        }
        
        // Read message data
        let mut data = vec![0u8; len as usize];
        reader.read_exact(&mut data).await?;
        
        Self::from_bytes(&data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_serialization() {
        let msg = ControlMessage::ClientHello {
            version: PROTOCOL_VERSION,
            token: "test-token".to_string(),
            proxies: vec![ReverseProxyConfig {
                server_port: 8080,
                server_host: None,
                local_host: "localhost".to_string(),
                local_port: 80,
            }],
            config_hash: "abc123".to_string(),
        };

        let bytes = msg.to_bytes().unwrap();
        let decoded = ControlMessage::from_bytes(&bytes).unwrap();

        match decoded {
            ControlMessage::ClientHello { version, proxies, token, config_hash } => {
                assert_eq!(version, PROTOCOL_VERSION);
                assert_eq!(proxies.len(), 1);
                assert_eq!(proxies[0].server_port, 8080);
                assert_eq!(token, "test-token");
                assert_eq!(config_hash, "abc123");
            }
            _ => panic!("Unexpected message type"),
        }
    }
    
    #[tokio::test]
    async fn test_message_read_write() {
        let msg = ControlMessage::Ping;
        
        let mut buffer = Vec::new();
        msg.write_to(&mut buffer).await.unwrap();
        
        let mut cursor = &buffer[..];
        let decoded = ControlMessage::read_from(&mut cursor).await.unwrap();
        
        matches!(decoded, ControlMessage::Ping);
    }
}
