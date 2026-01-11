use serde::Deserialize;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use crate::error::{AppError, ConfigError, Result};

/// TOML configuration file structure
#[derive(Debug, Deserialize, Clone)]
pub struct ConfigFile {
    pub server: Option<ServerSection>,
    #[serde(default)]
    pub proxies: Vec<ProxySection>,
    pub token: Option<String>,
    pub totp_secret: Option<String>,
    #[serde(default)]
    pub reverse_proxies: Vec<ReverseProxySection>,

    // Reverse proxy server configuration (optional)
    pub reverse_proxy_server: Option<ReverseProxyServerSection>,

    // Reverse proxy client configuration (optional)
    pub reverse_proxy_client: Option<ReverseProxyClientSection>,
}

/// Server configuration section
#[derive(Debug, Deserialize, Clone)]
pub struct ServerSection {
    pub bind_ip: Option<String>,
    pub debug: Option<bool>,
}

impl ServerSection {
    // Server section no longer handles authentication tokens
    // Authentication for reverse proxy is now handled in reverse_proxy_server section
}

/// Authentication mode for server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    None,     // No authentication required
    Single,   // Single token validation
    Multiple, // Multiple allowed tokens
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            bind_ip: Some("127.0.0.1".to_string()),
            debug: Some(false),
        }
    }
}

/// Proxy configuration section
#[derive(Debug, Deserialize, Clone)]
pub struct ProxySection {
    pub local: String,  // "8080" or "127.0.0.1:8080"
    pub remote: String, // "80" or "example.com:80" or "192.168.1.100:80"
    pub source_ip: Option<String>,
    pub allow_ips: Option<Vec<String>>,
    pub max_conns_per_ip: Option<usize>,
    pub cps_limit: Option<f64>, // connections per second
}

impl ProxySection {
    /// Get local port
    pub fn get_local_port(&self) -> Result<u16> {
        if self.local.contains(':') {
            // Format: "IP:PORT"
            let parts: Vec<&str> = self.local.split(':').collect();
            if parts.len() != 2 {
                return Err(AppError::Config(ConfigError::InvalidConfigValue {
                    path: "local".to_string(),
                    reason: format!(
                        "Invalid local format '{}', expected 'IP:PORT' or 'PORT'",
                        self.local
                    ),
                }));
            }
            parts[1].parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "local".to_string(),
                    reason: format!("Invalid port number in '{}'", self.local),
                })
            })
        } else {
            // Format: "PORT"
            self.local.parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "local".to_string(),
                    reason: format!("Invalid port number '{}'", self.local),
                })
            })
        }
    }

    /// Get local IP (None if using wildcard)
    pub fn get_local_ip(&self) -> Option<String> {
        if self.local.contains(':') {
            let parts: Vec<&str> = self.local.split(':').collect();
            if parts.len() == 2 {
                Some(parts[0].to_string())
            } else {
                None
            }
        } else {
            None // Use default bind IP
        }
    }

    /// Get remote host
    pub fn get_remote_host(&self) -> Result<String> {
        if self.remote.contains(':') {
            // Format: "HOST:PORT"
            let parts: Vec<&str> = self.remote.split(':').collect();
            if parts.len() < 2 {
                return Err(AppError::Config(ConfigError::InvalidConfigValue {
                    path: "remote".to_string(),
                    reason: format!(
                        "Invalid remote format '{}', expected 'HOST:PORT' or 'PORT'",
                        self.remote
                    ),
                }));
            }
            Ok(parts[0..parts.len() - 1].join(":")) // Handle IPv6 addresses
        } else {
            // Format: "PORT" - assume localhost
            Ok("localhost".to_string())
        }
    }

    /// Get remote port
    pub fn get_remote_port(&self) -> Result<u16> {
        if self.remote.contains(':') {
            // Format: "HOST:PORT"
            let parts: Vec<&str> = self.remote.rsplit(':').collect();
            if parts.len() < 2 {
                return Err(AppError::Config(ConfigError::InvalidConfigValue {
                    path: "remote".to_string(),
                    reason: format!("Invalid remote format '{}'", self.remote),
                }));
            }
            parts[0].parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "remote".to_string(),
                    reason: format!("Invalid port number in '{}'", self.remote),
                })
            })
        } else {
            // Format: "PORT"
            self.remote.parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "remote".to_string(),
                    reason: format!("Invalid port number '{}'", self.remote),
                })
            })
        }
    }
}

/// Reverse proxy configuration section
#[derive(Debug, Deserialize, Clone)]
pub struct ReverseProxySection {
    // Server side: can be "7000" or "0.0.0.0:7000"
    pub server: String,
    // Local side: can be "8080" or "127.0.0.1:8080"
    pub local: String,
    pub source_ip: Option<String>,
}

impl ReverseProxySection {
    /// Get server port
    pub fn get_server_port(&self) -> Result<u16> {
        if self.server.contains(':') {
            // Format: "IP:PORT"
            let parts: Vec<&str> = self.server.rsplit(':').collect();
            if parts.len() < 2 {
                return Err(AppError::Config(ConfigError::InvalidConfigValue {
                    path: "server".to_string(),
                    reason: format!("Invalid server format '{}'", self.server),
                }));
            }
            parts[0].parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "server".to_string(),
                    reason: format!("Invalid port number in '{}'", self.server),
                })
            })
        } else {
            // Format: "PORT"
            self.server.parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "server".to_string(),
                    reason: format!("Invalid port number '{}'", self.server),
                })
            })
        }
    }

    /// Get server IP (None if using wildcard)
    pub fn get_server_ip(&self) -> Option<String> {
        if self.server.contains(':') {
            let parts: Vec<&str> = self.server.split(':').collect();
            if parts.len() >= 2 {
                Some(parts[0..parts.len() - 1].join(":")) // Handle IPv6
            } else {
                None
            }
        } else {
            None // Use default bind IP
        }
    }

    /// Get local port
    pub fn get_local_port(&self) -> Result<u16> {
        if self.local.contains(':') {
            // Format: "IP:PORT"
            let parts: Vec<&str> = self.local.rsplit(':').collect();
            if parts.len() < 2 {
                return Err(AppError::Config(ConfigError::InvalidConfigValue {
                    path: "local".to_string(),
                    reason: format!("Invalid local format '{}'", self.local),
                }));
            }
            parts[0].parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "local".to_string(),
                    reason: format!("Invalid port number in '{}'", self.local),
                })
            })
        } else {
            // Format: "PORT"
            self.local.parse().map_err(|_| {
                AppError::Config(ConfigError::InvalidConfigValue {
                    path: "local".to_string(),
                    reason: format!("Invalid port number '{}'", self.local),
                })
            })
        }
    }

    /// Get local host (None if using localhost)
    pub fn get_local_host(&self) -> Option<String> {
        if self.local.contains(':') {
            let parts: Vec<&str> = self.local.split(':').collect();
            if parts.len() >= 2 {
                Some(parts[0..parts.len() - 1].join(":")) // Handle IPv6
            } else {
                Some("localhost".to_string())
            }
        } else {
            Some("localhost".to_string()) // Use localhost by default
        }
    }
}

/// Reverse proxy server configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ReverseProxyServerSection {
    /// Port for the reverse proxy server to listen on
    pub port: u16,
    #[serde(default)]
    pub allowed_tokens: Vec<String>, // Authentication tokens for reverse proxy clients
    pub totp_secret: Option<String>, // Base32 or Hex secret for TOTP
}

impl Default for ReverseProxyServerSection {
    fn default() -> Self {
        Self {
            port: 9001,
            allowed_tokens: Vec::new(),
            totp_secret: None,
        }
    }
}

/// Reverse proxy client configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ReverseProxyClientSection {
    /// Server address to connect to (e.g., "server.example.com:9001")
    pub server: String,
    pub token: Option<String>,
    pub totp_secret: Option<String>,
}

impl ConfigFile {
    /// Determine runtime mode from configuration
    pub fn get_runtime_mode(&self) -> String {
        if self.reverse_proxy_client.is_some() {
            "reverse_client".to_string()
        } else if self.reverse_proxy_server.is_some() {
            "reverse_server".to_string()
        } else {
            "forward".to_string()
        }
    }

    /// Validate configuration integrity and return non-fatal warnings
    pub fn validate(&mut self) -> std::result::Result<Vec<String>, ConfigError> {
        use std::collections::HashSet;

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // 验证反向代理配置的一致性
        if !self.reverse_proxies.is_empty() {
            let has_server = self.reverse_proxy_server.is_some();
            let has_client = self.reverse_proxy_client.is_some();

            if !has_server && !has_client {
                errors.push("Reverse proxies configured but neither reverse_proxy_server nor reverse_proxy_client specified".to_string());
            }
        }

        if let Some(server) = self.server.as_mut() {
            if let Some(current) = server.bind_ip.clone() {
                let trimmed = current.trim();
                if trimmed.is_empty() {
                    warnings.push(
                        "server.bind_ip is empty, falling back to default 127.0.0.1".to_string(),
                    );
                    server.bind_ip = Some("127.0.0.1".to_string());
                } else if trimmed.parse::<IpAddr>().is_err() {
                    errors.push(format!(
                        "server.bind_ip '{}' is not a valid IP address",
                        trimmed
                    ));
                } else if trimmed != current {
                    server.bind_ip = Some(trimmed.to_string());
                }
            }
        }

        let mut local_ports = HashSet::new();

        // Validate proxies (forward proxy)
        for (index, proxy) in self.proxies.iter_mut().enumerate() {
            let prefix = format!("proxies[{}]", index);

            // Validate local format
            let local_port = match proxy.get_local_port() {
                Ok(port) => port,
                Err(e) => {
                    errors.push(format!("{}.local: {}", prefix, e));
                    continue;
                }
            };

            if local_port == 0 {
                errors.push(format!("{}.local port must be between 1 and 65535", prefix));
            }

            // Validate remote format
            let _remote_host = match proxy.get_remote_host() {
                Ok(host) => host,
                Err(e) => {
                    errors.push(format!("{}.remote: {}", prefix, e));
                    continue;
                }
            };

            let remote_port = match proxy.get_remote_port() {
                Ok(port) => port,
                Err(e) => {
                    errors.push(format!("{}.remote: {}", prefix, e));
                    continue;
                }
            };

            if remote_port == 0 {
                errors.push(format!(
                    "{}.remote port must be between 1 and 65535",
                    prefix
                ));
            }

            // Check for duplicate local ports
            if !local_ports.insert(local_port) {
                errors.push(format!(
                    "Duplicate local port {} detected in {}",
                    local_port, prefix
                ));
            }

            // Validate and sanitize source_ip if present
            if let Some(ref source_ip) = proxy.source_ip {
                let trimmed = source_ip.trim();

                if trimmed.is_empty() {
                    warnings.push(format!("{}.source_ip is empty and will be ignored", prefix));
                    proxy.source_ip = None;
                } else if trimmed.eq_ignore_ascii_case("null") {
                    warnings.push(format!(
                        "{}.source_ip contains 'null' value; will be ignored",
                        prefix
                    ));
                    proxy.source_ip = None;
                } else if trimmed.parse::<IpAddr>().is_err() {
                    errors.push(format!(
                        "{}.source_ip '{}' is not a valid IP address",
                        prefix, trimmed
                    ));
                }
            }
        }

        // Validate reverse_proxies
        let mut server_ports = HashSet::new();

        for (index, rproxy) in self.reverse_proxies.iter().enumerate() {
            let prefix = format!("reverse_proxies[{}]", index);

            // Validate server format
            let server_port = match rproxy.get_server_port() {
                Ok(port) => port,
                Err(e) => {
                    errors.push(format!("{}.server: {}", prefix, e));
                    continue;
                }
            };

            if server_port == 0 {
                errors.push(format!(
                    "{}.server port must be between 1 and 65535",
                    prefix
                ));
            }

            // Check for duplicate server ports
            if !server_ports.insert(server_port) {
                errors.push(format!(
                    "Duplicate server port {} detected in {}",
                    server_port, prefix
                ));
            }

            // Validate local format
            let local_port = match rproxy.get_local_port() {
                Ok(port) => port,
                Err(e) => {
                    errors.push(format!("{}.local: {}", prefix, e));
                    continue;
                }
            };

            if local_port == 0 {
                errors.push(format!("{}.local port must be between 1 and 65535", prefix));
            }

            // Validate source_ip if present
            if let Some(ref source_ip) = rproxy.source_ip {
                let trimmed = source_ip.trim();

                if trimmed.is_empty() {
                    warnings.push(format!("{}.source_ip is empty and will be ignored", prefix));
                } else if trimmed.eq_ignore_ascii_case("null") {
                    warnings.push(format!(
                        "{}.source_ip contains 'null' value; will be ignored",
                        prefix
                    ));
                } else if trimmed.parse::<IpAddr>().is_err() {
                    errors.push(format!(
                        "{}.source_ip '{}' is not a valid IP address",
                        prefix, trimmed
                    ));
                }
            }
        }

        if !errors.is_empty() {
            return Err(ConfigError::InvalidConfigValue {
                path: "config".to_string(),
                reason: errors.join("; "),
            });
        }

        Ok(warnings)
    }
}

/// Configuration loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<ConfigFile> {
        let path_ref = path.as_ref();
        let content = fs::read_to_string(path_ref).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::Config(ConfigError::ConfigFileNotFound(
                    path_ref.to_string_lossy().to_string(),
                ))
            } else {
                AppError::Config(ConfigError::ReadFailed(format!(
                    "Failed to read config file '{}': {}",
                    path_ref.display(),
                    e
                )))
            }
        })?;

        let (config, warnings) = Self::load_from_str_with_warnings(&content)?;
        Self::emit_warnings(&warnings);
        Ok(config)
    }

    /// Load configuration from string
    pub fn load_from_str(content: &str) -> Result<ConfigFile> {
        let (config, warnings) =
            Self::load_from_str_with_warnings(content).map_err(AppError::from)?;
        Self::emit_warnings(&warnings);
        Ok(config)
    }

    /// Load configuration from string and capture validation warnings
    fn load_from_str_with_warnings(
        content: &str,
    ) -> std::result::Result<(ConfigFile, Vec<String>), ConfigError> {
        let (sanitized, mut warnings) = Self::sanitize_special_values(content);

        let mut config = toml::from_str::<ConfigFile>(&sanitized)
            .map_err(|err| Self::map_toml_error(&sanitized, err))?;
        let mut validation_warnings = config.validate()?;
        warnings.append(&mut validation_warnings);

        Ok((config, warnings))
    }

    fn emit_warnings(warnings: &[String]) {
        for warning in warnings {
            eprintln!("⚠️  Configuration Warning: {}", warning);
        }
    }

    /// Parse IP address
    pub fn parse_ip_address(ip_str: &str) -> Result<IpAddr> {
        let trimmed = ip_str.trim();
        Ok(trimmed
            .parse::<IpAddr>()
            .map_err(|_| ConfigError::InvalidIpAddress(trimmed.to_string()))?)
    }

    /// Create socket address - ONLY accepts IP addresses, no DNS resolution
    /// Tunnel proxy should only use IP addresses directly
    pub fn create_socket_addr(host: &str, port: u16) -> Result<SocketAddr> {
        // Only parse as IP address - no DNS resolution allowed
        host.parse::<IpAddr>()
            .map(|ip| SocketAddr::new(ip, port))
            .map_err(|_| ConfigError::InvalidIpAddress(format!(
                "Invalid IP address '{}'. Tunnel proxy requires IP addresses, not hostnames. Use nslookup or dig to resolve hostnames manually.",
                host
            )))
            .map_err(AppError::Config)
    }

    /// Check if configuration file exists
    pub fn config_file_exists<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().exists()
    }

    fn sanitize_special_values(content: &str) -> (String, Vec<String>) {
        let mut sanitized = String::with_capacity(content.len());
        let warnings = Vec::new();

        for raw_line in content.split_inclusive('\n') {
            let (line_body, newline) = match raw_line.strip_suffix('\n') {
                Some(body) => (body, "\n"),
                None => (raw_line, ""),
            };

            let mut replaced_line = line_body.to_string();

            if let Some(eq_index) = line_body.find('=') {
                let key_part = &line_body[..eq_index];
                if key_part.trim().eq_ignore_ascii_case("source_ip") {
                    let prefix = &line_body[..=eq_index];
                    let rest = &line_body[eq_index + 1..];
                    let trimmed_rest = rest.trim_start();

                    if trimmed_rest.len() >= 4 && trimmed_rest[..4].eq_ignore_ascii_case("null") {
                        let remainder = &trimmed_rest[4..];
                        if remainder.is_empty()
                            || remainder.starts_with(|c: char| c.is_whitespace() || c == '#')
                        {
                            let whitespace_len = rest.len() - trimmed_rest.len();
                            let whitespace = &rest[..whitespace_len];
                            let suffix = &rest[whitespace_len + 4..];
                            replaced_line = format!("{}{}\"null\"{}", prefix, whitespace, suffix);
                        }
                    }
                }
            }

            sanitized.push_str(&replaced_line);
            sanitized.push_str(newline);
        }

        if !content.ends_with('\n') && sanitized.ends_with('\n') {
            sanitized.pop();
        }

        (sanitized, warnings)
    }

    fn map_toml_error(content: &str, error: toml::de::Error) -> ConfigError {
        let message = error.message().to_string();

        if let Some(field) = message.strip_prefix("missing field `") {
            if let Some(end) = field.find('`') {
                let field_name = &field[..end];
                return ConfigError::MissingRequiredField(field_name.to_string());
            }
        }

        let (line, column) = error
            .span()
            .map(|span| Self::offset_to_line_col(content, span.start))
            .unwrap_or((0, 0));
        ConfigError::InvalidTomlFormat(format!(
            "TOML parse error at line {}, column {}: {}\n\
             Tip: Check for syntax errors like 'null' values (should be omitted), \
             missing quotes, or invalid data types",
            line, column, message
        ))
    }

    fn offset_to_line_col(content: &str, offset: usize) -> (usize, usize) {
        let mut line = 1usize;
        let mut column = 1usize;
        let upto = offset.min(content.len());

        for byte in &content.as_bytes()[..upto] {
            match byte {
                b'\n' => {
                    line += 1;
                    column = 1;
                }
                b'\r' => column = 1,
                _ => column += 1,
            }
        }

        (line, column)
    }

    /// Load configuration by searching CLI arguments or common paths
    pub fn load_with_search() -> Result<(ConfigFile, std::path::PathBuf)> {
        // 1. Check CLI args first
        if let Some(path) = Self::get_cli_config_path() {
            if path.exists() {
                match Self::load_from_file(&path) {
                    Ok(config) => return Ok((config, path)),
                    Err(e) => {
                        eprintln!(
                            "❌ Failed to load config specified by CLI '{:?}': {}",
                            path, e
                        );
                        // If user explicitly provided a path and it fails, we should probably fail hard?
                        // But existing logic might prefer fallback. For now, let's return error to be explicit.
                        return Err(e);
                    }
                }
            } else {
                return Err(AppError::Config(ConfigError::ConfigFileNotFound(format!(
                    "CLI specified config file not found: {:?}",
                    path
                ))));
            }
        }

        // 2. Fallback to default search paths
        let paths = Self::get_config_search_paths();

        for path in &paths {
            if path.exists() {
                match Self::load_from_file(path) {
                    Ok(config) => return Ok((config, path.clone())),
                    Err(e) => eprintln!("⚠️  Failed to load config from {}: {}", path.display(), e),
                }
            }
        }

        Err(AppError::Config(ConfigError::ConfigFileNotFound(format!(
            "No configuration file found. Searched: {:?}",
            paths
        ))))
    }

    /// Check for `-c` or `--config` argument
    fn get_cli_config_path() -> Option<std::path::PathBuf> {
        let args: Vec<String> = std::env::args().collect();
        for i in 1..args.len() {
            if (args[i] == "-c" || args[i] == "--config") && i + 1 < args.len() {
                return Some(std::path::PathBuf::from(&args[i + 1]));
            }
        }
        None
    }

    /// Get list of configuration search paths (defaults)
    pub fn get_config_search_paths() -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        paths.push(std::path::PathBuf::from("default.toml"));
        paths.push(std::path::PathBuf::from("config.toml"));
        paths.push(std::path::PathBuf::from("gsc-fq.toml"));
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use tempfile::NamedTempFile;

    #[test]
    fn test_validate_detects_invalid_source_ip() {
        let mut config = ConfigFile {
            server: None,
            proxies: vec![ProxySection {
                local: "8080".to_string(),
                remote: "example.com:80".to_string(),
                source_ip: Some("invalid-ip".to_string()),
                allow_ips: None,
                max_conns_per_ip: None,
                cps_limit: None,
            }],
            token: Some("default".to_string()),
            totp_secret: None,
            reverse_proxies: vec![],
            reverse_proxy_server: None,
            reverse_proxy_client: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        if let Err(ConfigError::InvalidConfigValue { reason, .. }) = result {
            assert!(reason.contains("not a valid IP address"));
        } else {
            panic!("Expected InvalidConfigValue error");
        }
    }

    #[test]
    fn test_validate_handles_null_source_ip() {
        let content = r#"
[[proxies]]
local = "9000"
remote = "example.com:443"
source_ip = "null"
"#;

        let (config, warnings) =
            ConfigLoader::load_from_str_with_warnings(content).expect("Should load config");

        assert!(warnings
            .iter()
            .any(|w| w.contains("source_ip") && w.contains("null")));
        assert!(config.proxies[0].source_ip.is_none());
    }

    #[test]
    fn test_sanitize_unquoted_null_source_ip() {
        let content = r#"
[[proxies]]
local = "8000"
remote = "example.org:80"
source_ip = null
"#;

        let (config, warnings) =
            ConfigLoader::load_from_str_with_warnings(content).expect("Should load config");

        assert!(warnings
            .iter()
            .any(|w| w.contains("source_ip") && w.contains("null")));
        assert!(config.proxies[0].source_ip.is_none());
    }

    #[test]
    fn test_missing_required_field_error() {
        let content = r#"
[[proxies]]
local = "7000"
# Missing remote field on purpose
"#;

        let err = ConfigLoader::load_from_str_with_warnings(content).unwrap_err();
        match err {
            ConfigError::MissingRequiredField(field) => {
                assert_eq!(field, "remote");
            }
            other => panic!("Expected MissingRequiredField, got {:?}", other),
        }
    }

    #[test]
    fn test_duplicate_local_port_detection() {
        let mut config = ConfigFile {
            server: None,
            proxies: vec![
                ProxySection {
                    local: "8080".to_string(),
                    remote: "example.com:80".to_string(),
                    source_ip: None,
                    allow_ips: None,
                    max_conns_per_ip: None,
                    cps_limit: None,
                },
                ProxySection {
                    local: "8080".to_string(),
                    remote: "example.net:8080".to_string(),
                    source_ip: None,
                    allow_ips: None,
                    max_conns_per_ip: None,
                    cps_limit: None,
                },
            ],
            token: Some("default".to_string()),
            totp_secret: None,
            reverse_proxies: vec![],
            reverse_proxy_server: None,
            reverse_proxy_client: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        if let Err(ConfigError::InvalidConfigValue { reason, .. }) = result {
            assert!(reason.contains("Duplicate local port"));
        } else {
            panic!("Expected InvalidConfigValue error for duplicate port");
        }
    }

    #[test]
    fn test_config_file_not_found() {
        let err = ConfigLoader::load_from_file("does_not_exist.toml").unwrap_err();
        match err {
            AppError::Config(ConfigError::ConfigFileNotFound(path)) => {
                assert!(path.contains("does_not_exist.toml"));
            }
            other => panic!("Expected ConfigFileNotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_read_and_validate_from_file() {
        let file = NamedTempFile::new().expect("create temp file");
        let content = r#"
[[proxies]]
local = "8100"
remote = "example.com:8101"
source_ip = null
"#;
        fs::write(file.path(), content).expect("write config");

        let config = ConfigLoader::load_from_file(file.path()).expect("load config");
        assert!(config.proxies[0].source_ip.is_none());
    }

    #[test]
    fn test_server_bind_ip_empty_defaults() {
        let content = r#"[server]
bind_ip = ""
allowed_tokens = []

[[proxies]]
local = "8200"
remote = "example.com:8201"
"#;

        let (config, warnings) =
            ConfigLoader::load_from_str_with_warnings(content).expect("load config");

        assert!(warnings.iter().any(|w| w.contains("bind_ip")));
        let server = config.server.as_ref().expect("server section");
        assert_eq!(server.bind_ip.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn test_ip_parsing() {
        let ipv4 = ConfigLoader::parse_ip_address("192.168.1.1").unwrap();
        assert_eq!(ipv4, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));

        let ipv6 = ConfigLoader::parse_ip_address("::1").unwrap();
        assert_eq!(ipv6, IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));

        let trimmed = ConfigLoader::parse_ip_address(" 127.0.0.1 ").unwrap();
        assert_eq!(trimmed, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));

        let invalid = ConfigLoader::parse_ip_address("invalid");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_load_from_str() {
        let toml_content = r#"[server]
bind_ip = " 127.0.0.1 "
allowed_tokens = []

[[proxies]]
local = "8080"
remote = "example.com:80"
"#;

        let (config, warnings) =
            ConfigLoader::load_from_str_with_warnings(toml_content).expect("load config");
        assert!(warnings.is_empty());

        let server = config.server.as_ref().expect("Server section should exist");
        assert_eq!(server.bind_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(config.proxies.len(), 1);
        assert_eq!(config.proxies[0].local, "8080");
        assert_eq!(config.proxies[0].remote, "example.com:80");
    }
}
