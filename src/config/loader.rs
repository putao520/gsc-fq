use serde::Deserialize;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use crate::error::{AppError, ConfigError, Result};

/// TOML configuration file structure
#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub server: Option<ServerSection>,
    pub proxies: Vec<ProxySection>,
}

/// Server configuration section
#[derive(Debug, Deserialize)]
pub struct ServerSection {
    pub bind_ip: Option<String>,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            bind_ip: Some("127.0.0.1".to_string()),
        }
    }
}

/// Proxy configuration section
#[derive(Debug, Deserialize, Clone)]
pub struct ProxySection {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub source_ip: Option<String>,
}

impl ConfigFile {
    /// Validate configuration integrity and return non-fatal warnings
    pub fn validate(&mut self) -> std::result::Result<Vec<String>, ConfigError> {
        use std::collections::HashSet;

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        if self.proxies.is_empty() {
            return Err(ConfigError::MissingRequiredField("proxies".to_string()));
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

        let mut ports = HashSet::new();

        for (index, proxy) in self.proxies.iter_mut().enumerate() {
            let prefix = format!("proxies[{}]", index);

            if proxy.local_port == 0 {
                errors.push(format!("{}.local_port must be between 1 and 65535", prefix));
            }

            if proxy.remote_port == 0 {
                errors.push(format!(
                    "{}.remote_port must be between 1 and 65535",
                    prefix
                ));
            }

            let trimmed_host = proxy.remote_host.trim();
            if trimmed_host.is_empty() {
                errors.push(format!("{}.remote_host cannot be empty", prefix));
            } else if trimmed_host != proxy.remote_host {
                proxy.remote_host = trimmed_host.to_string();
            }

            if !ports.insert(proxy.local_port) {
                errors.push(format!(
                    "Duplicate local_port {} detected in {}",
                    proxy.local_port, prefix
                ));
            }

            if let Some(source_ip) = proxy.source_ip.clone() {
                let trimmed = source_ip.trim();

                if trimmed.is_empty() {
                    warnings.push(format!("{}.source_ip is empty and will be ignored", prefix));
                    proxy.source_ip = None;
                    continue;
                }

                if trimmed.eq_ignore_ascii_case("null") {
                    warnings.push(format!(
                        "{}.source_ip contains invalid 'null' value; the field will be ignored",
                        prefix
                    ));
                    proxy.source_ip = None;
                    continue;
                }

                if trimmed.parse::<IpAddr>().is_err() {
                    errors.push(format!(
                        "{}.source_ip '{}' is not a valid IP address",
                        prefix, trimmed
                    ));
                } else if trimmed != source_ip {
                    proxy.source_ip = Some(trimmed.to_string());
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
        let config = Self::load_with_fallback(path).map_err(AppError::from)?;
        Ok(config)
    }

    /// Load configuration from string
    pub fn load_from_str(content: &str) -> Result<ConfigFile> {
        let (config, warnings) =
            Self::load_from_str_with_warnings(content).map_err(AppError::from)?;
        Self::emit_warnings(&warnings);
        Ok(config)
    }

    /// Load configuration from file with detailed fallback handling
    pub fn load_with_fallback<P: AsRef<Path>>(
        path: P,
    ) -> std::result::Result<ConfigFile, ConfigError> {
        let path_ref = path.as_ref();
        let content = match fs::read_to_string(path_ref) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ConfigError::ConfigFileNotFound(
                    path_ref.to_string_lossy().to_string(),
                ));
            }
            Err(e) => {
                return Err(ConfigError::ReadFailed(format!(
                    "Failed to read config file '{}': {}",
                    path_ref.display(),
                    e
                )));
            }
        };

        let (config, warnings) = Self::load_from_str_with_warnings(&content)?;
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

    /// Get default configuration
    /// 获取默认配置
    pub fn get_default_config() -> ConfigFile {
        // 直接返回内置的默认配置；若加载失败则认为程序构建有误
        Self::load_default_config_file()
            .expect("Built-in default configuration should always be valid")
    }

    /// Load default configuration
    /// 加载默认配置
    fn load_default_config_file() -> std::result::Result<ConfigFile, ConfigError> {
        const DEFAULT_CONFIG: &str = include_str!("../../default.toml");
        let (config, warnings) = Self::load_from_str_with_warnings(DEFAULT_CONFIG)?;
        Self::emit_warnings(&warnings);
        Ok(config)
    }

    /// Parse IP address
    pub fn parse_ip_address(ip_str: &str) -> Result<IpAddr> {
        let trimmed = ip_str.trim();
        Ok(trimmed
            .parse::<IpAddr>()
            .map_err(|_| ConfigError::InvalidIpAddress(trimmed.to_string()))?)
    }

    /// Create socket address
    pub fn create_socket_addr(ip: &str, port: u16) -> Result<SocketAddr> {
        let ip = Self::parse_ip_address(ip)?;
        Ok(SocketAddr::new(ip, port))
    }

    /// Check if configuration file exists
    pub fn config_file_exists<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().exists()
    }

    /// Create example configuration file
    pub fn create_example_config<P: AsRef<Path>>(path: P) -> Result<()> {
        let example_config = r#"[server]
bind_ip = "0.0.0.0"

[[proxies]]
local_port = 8080
remote_host = "203.0.113.10"
remote_port = 80
source_ip = "198.51.100.10"

[[proxies]]
local_port = 5432
remote_host = "db.example.test"
remote_port = 5432

[[proxies]]
local_port = 9090
remote_host = "api.example.test"
remote_port = 443
"#;

        fs::write(path, example_config).map_err(|e| {
            ConfigError::ReadFailed(format!("Failed to write example config: {}", e))
        })?;

        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = ConfigLoader::get_default_config();
        let server = config
            .server
            .as_ref()
            .expect("Default config should contain a server section");
        assert_eq!(server.bind_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(config.proxies.len(), 3);

        let first = &config.proxies[0];
        assert_eq!(first.local_port, 33100);
        assert_eq!(first.remote_host, "198.51.100.10");
        assert_eq!(first.remote_port, 8080);

        let last = &config.proxies[2];
        assert_eq!(last.local_port, 33300);
        assert_eq!(last.remote_host, "198.51.100.30");
        assert_eq!(last.remote_port, 8080);
    }

    #[test]
    fn test_validate_detects_invalid_source_ip() {
        let mut config = ConfigFile {
            server: None,
            proxies: vec![ProxySection {
                local_port: 8080,
                remote_host: "example.com".to_string(),
                remote_port: 80,
                source_ip: Some("invalid-ip".to_string()),
            }],
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
local_port = 9000
remote_host = "example.com"
remote_port = 443
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
local_port = 8000
remote_host = "example.org"
remote_port = 80
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
local_port = 7000
remote_host = "example.com"
# Missing remote_port on purpose
"#;

        let err = ConfigLoader::load_from_str_with_warnings(content).unwrap_err();
        match err {
            ConfigError::MissingRequiredField(field) => {
                assert_eq!(field, "remote_port");
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
                    local_port: 8080,
                    remote_host: "example.com".to_string(),
                    remote_port: 80,
                    source_ip: None,
                },
                ProxySection {
                    local_port: 8080,
                    remote_host: "example.net".to_string(),
                    remote_port: 8080,
                    source_ip: None,
                },
            ],
        };

        let result = config.validate();
        assert!(result.is_err());
        if let Err(ConfigError::InvalidConfigValue { reason, .. }) = result {
            assert!(reason.contains("Duplicate local_port"));
        } else {
            panic!("Expected InvalidConfigValue error for duplicate port");
        }
    }

    #[test]
    fn test_config_file_not_found() {
        let err = ConfigLoader::load_with_fallback("does_not_exist.toml").unwrap_err();
        match err {
            ConfigError::ConfigFileNotFound(path) => {
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
local_port = 8100
remote_host = "example.com"
remote_port = 8101
source_ip = null
"#;
        fs::write(file.path(), content).expect("write config");

        let config = ConfigLoader::load_with_fallback(file.path()).expect("load config");
        assert!(config.proxies[0].source_ip.is_none());
    }

    #[test]
    fn test_server_bind_ip_empty_defaults() {
        let content = r#"[server]
bind_ip = ""

[[proxies]]
local_port = 8200
remote_host = "example.com"
remote_port = 8201
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

[[proxies]]
local_port = 8080
remote_host = "example.com "
remote_port = 80
"#;

        let (config, warnings) =
            ConfigLoader::load_from_str_with_warnings(toml_content).expect("load config");
        assert!(warnings.is_empty());

        let server = config.server.as_ref().expect("Server section should exist");
        assert_eq!(server.bind_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(config.proxies.len(), 1);
        assert_eq!(config.proxies[0].local_port, 8080);
        assert_eq!(config.proxies[0].remote_host, "example.com");
    }
}
