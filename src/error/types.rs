use std::io;
use thiserror::Error;

/// Application error type
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    #[error("Proxy error: {0}")]
    Proxy(#[from] ProxyError),

    #[error("Performance error: {0}")]
    Performance(#[from] PerformanceError),

    #[error("System error: {0}")]
    System(#[from] SystemError),

    #[error("Reverse proxy error: {0}")]
    ReverseProxy(#[from] ReverseProxyError),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] toml::de::Error),

    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Configuration related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid IP address: {0}")]
    InvalidIpAddress(String),

    #[error("Port {0} is already in use")]
    PortInUse(u16),

    #[error("Cannot bind to port {0}: {1}")]
    PortBindError(u16, String),

    #[error("Insufficient privileges to bind to port {0}")]
    InsufficientPrivileges(u16),

    #[error("Invalid port: {0}")]
    InvalidPort(u16),

    #[error("Invalid TOML format: {0}")]
    InvalidTomlFormat(String),

    #[error("Invalid configuration value at {path}: {reason}")]
    InvalidConfigValue { path: String, reason: String },

    #[error("Missing required configuration field: {0}")]
    MissingRequiredField(String),

    #[error("Configuration file not found: {0}")]
    ConfigFileNotFound(String),

    #[error("Configuration file read failed: {0}")]
    ReadFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Network related errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Listen failed: {0}")]
    ListenFailed(String),

    #[error("Socket creation failed: {0}")]
    SocketCreationFailed(String),

    #[error("Address resolution failed: {0}")]
    AddressResolutionFailed(String),

    #[error("Connection timeout")]
    ConnectionTimeout,

    #[error("Connection closed by remote")]
    ConnectionClosed,

    #[error("Connection reset")]
    ConnectionReset,

    #[error("Network unreachable")]
    NetworkUnreachable,

    #[error("Invalid buffer size: {0}")]
    InvalidBufferSize(usize),

    #[error("Invalid socket option: {0}")]
    InvalidSocketOption(String),
}

/// Proxy related errors
#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Proxy startup failed: {0}")]
    StartupFailed(String),

    #[error("Proxy stop failed: {0}")]
    StopFailed(String),

    #[error("Data forwarding failed: {0}")]
    ForwardingFailed(String),

    #[error("Connection pool error: {0}")]
    ConnectionPoolError(String),

    #[error("Buffer pool error: {0}")]
    BufferPoolError(String),

    #[error("Invalid proxy configuration: {0}")]
    InvalidProxyConfig(String),

    #[error("Proxy instance not found: {0}")]
    InstanceNotFound(String),

    #[error("Forwarding strategy error: {0}")]
    ForwardingStrategyError(String),
}

/// Performance optimization related errors
#[derive(Error, Debug)]
pub enum PerformanceError {
    #[error("TCP optimization failed: {0}")]
    TcpOptimizationFailed(String),

    #[error("Buffer pool initialization failed: {0}")]
    BufferPoolInitFailed(String),

    #[error("Connection pool initialization failed: {0}")]
    ConnectionPoolInitFailed(String),

    #[error("Insufficient system resources: {0}")]
    InsufficientResources(String),

    #[error("Invalid performance parameter: {0}")]
    InvalidPerformanceParameter(String),

    #[error("Zero-Copy operation failed: {0}")]
    ZeroCopyFailed(String),
}

/// System related errors
#[derive(Error, Debug)]
pub enum SystemError {
    #[error("Signal handling failed: {0}")]
    SignalHandlingFailed(String),

    #[error("Insufficient file descriptors")]
    InsufficientFileDescriptors,

    #[error("System call failed: {0}")]
    SystemCallFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("System resource exhausted: {0}")]
    ResourceExhausted(String),
}

/// Reverse proxy related errors
#[derive(Error, Debug)]
pub enum ReverseProxyError {
    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("Client disconnected")]
    ClientDisconnected,

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Message serialization failed: {0}")]
    SerializationFailed(String),

    #[error("Message deserialization failed: {0}")]
    DeserializationFailed(String),

    #[error("Control channel error: {0}")]
    ControlChannelError(String),

    #[error("Port allocation failed: {0}")]
    PortAllocationFailed(String),

    #[error("Connection multiplexing error: {0}")]
    MultiplexingError(String),
    
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(u8),

    #[error("Cryptography error: {0}")]
    CryptoError(String),
}

impl AppError {
    /// Create internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            AppError::Network(NetworkError::ConnectionTimeout)
            | AppError::Network(NetworkError::ConnectionClosed)
            | AppError::Network(NetworkError::ConnectionReset) => true,

            AppError::System(SystemError::InsufficientFileDescriptors) => false,
            AppError::Config(ConfigError::InsufficientPrivileges(_)) => false,

            _ => true,
        }
    }

    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            AppError::Config(_) => ErrorSeverity::Fatal,
            AppError::System(SystemError::InsufficientFileDescriptors) => ErrorSeverity::Fatal,
            AppError::Network(NetworkError::ConnectionClosed) => ErrorSeverity::Warning,
            AppError::Network(NetworkError::ConnectionTimeout) => ErrorSeverity::Error,
            _ => ErrorSeverity::Error,
        }
    }
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Fatal,   // Fatal error, program must exit
    Error,   // Error, needs handling but can continue
    Warning, // Warning, just log
    Info,    // Info, just for logging
}

/// Result type alias
pub type Result<T> = std::result::Result<T, AppError>;

/// Error handling extension trait
pub trait ErrorExt<T> {
    /// Add context information
    fn with_context(self, context: impl Into<String>) -> Result<T>;

    /// Convert to internal error
    fn internal_error(self) -> Result<T>;
}

impl<T, E> ErrorExt<T> for std::result::Result<T, E>
where
    E: Into<AppError>,
{
    fn with_context(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|e| AppError::Internal {
            message: format!("{}: {}", context.into(), e.into()),
        })
    }

    fn internal_error(self) -> Result<T> {
        self.map_err(|e| AppError::Internal {
            message: format!("Internal error: {}", e.into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_severity() {
        let fatal_error = AppError::Config(ConfigError::InvalidIpAddress("test".to_string()));
        assert_eq!(fatal_error.severity(), ErrorSeverity::Fatal);

        let warning_error = AppError::Network(NetworkError::ConnectionClosed);
        assert_eq!(warning_error.severity(), ErrorSeverity::Warning);
    }

    #[test]
    fn test_recoverable_errors() {
        let recoverable = AppError::Network(NetworkError::ConnectionTimeout);
        assert!(recoverable.is_recoverable());

        let non_recoverable = AppError::System(SystemError::InsufficientFileDescriptors);
        assert!(!non_recoverable.is_recoverable());
    }
}
