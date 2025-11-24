use std::net::SocketAddr;
use crate::config::{ConfigLoader, ConfigFile};
use crate::error::{AppError, Result};
use crate::proxy::ProxyServerBuilder;
use crate::reverse_proxy::{ReverseProxyServer, ReverseProxyClient};
use std::path::PathBuf;
use std::net::IpAddr;

/// Runtime mode enum
#[derive(Debug, Clone, PartialEq)]
pub enum RunMode {
    Forward,           // Forward proxy mode (default)
    ReverseServer,     // Reverse proxy server mode
    ReverseClient,     // Reverse proxy client mode
}

impl RunMode {
    /// Parse mode from string
    pub fn from_str(mode: &str) -> Self {
        match mode.to_lowercase().trim() {
            "forward" | "proxy" => RunMode::Forward,
            "reverse_server" | "server" | "reverse-server" => RunMode::ReverseServer,
            "reverse_client" | "client" | "reverse-client" => RunMode::ReverseClient,
            _ => {
                eprintln!("⚠️  Unknown runtime mode '{}', using 'forward' as fallback", mode);
                RunMode::Forward
            }
        }
    }

    /// Get mode as string
    pub fn as_str(&self) -> &'static str {
        match self {
            RunMode::Forward => "forward",
            RunMode::ReverseServer => "reverse_server",
            RunMode::ReverseClient => "reverse_client",
        }
    }
}

/// Runtime manager handles different execution modes
pub struct RuntimeManager {
    config: ConfigFile,
    config_path: PathBuf,
    mode: RunMode,
}

impl RuntimeManager {
    /// Create new runtime manager by loading configuration from search paths
    pub fn new() -> Result<Self> {
        let (config, config_path) = ConfigLoader::load_with_search()?;
        let mode_str = config.get_runtime_mode();
        let mode = RunMode::from_str(&mode_str);

        eprintln!("🚀 Runtime mode: {} (config: {})", mode.as_str(), config_path.display());

        Ok(Self {
            config,
            config_path,
            mode,
        })
    }

    /// Create runtime manager with specific mode (override config)
    pub fn new_with_mode(mode: RunMode) -> Result<Self> {
        let (config, config_path) = ConfigLoader::load_with_search()?;

        eprintln!("🚀 Runtime mode: {} (overridden, config: {})", mode.as_str(), config_path.display());

        Ok(Self {
            config,
            config_path,
            mode,
        })
    }

    /// Get current mode
    pub fn mode(&self) -> &RunMode {
        &self.mode
    }

    /// Get configuration reference
    pub fn config(&self) -> &ConfigFile {
        &self.config
    }

    /// Get config file path
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Run the appropriate mode
    pub async fn run(&self) -> Result<()> {
        // Initialize debug system from config
        let debug_enabled = self.config.server.as_ref()
            .and_then(|s| s.debug)
            .unwrap_or(false);
        crate::utils::debug::init_debug(debug_enabled);

        match self.mode {
            RunMode::Forward => self.run_forward_proxy().await,
            RunMode::ReverseServer => self.run_reverse_server().await,
            RunMode::ReverseClient => self.run_reverse_client().await,
        }
    }

    /// Run forward proxy mode
    async fn run_forward_proxy(&self) -> Result<()> {
        eprintln!("📡 Starting forward proxy mode...");

        if self.config.proxies.is_empty() {
            eprintln!("❌ No proxy configurations found in config file!");
            return Err(AppError::Config(
                crate::error::ConfigError::InvalidConfigValue {
                    path: "proxies".to_string(),
                    reason: "No proxy configurations found".to_string(),
                }
            ));
        }

        // Parse bind IP from config
        let bind_ip: IpAddr = if let Some(server_section) = &self.config.server {
            if let Some(bind_ip_str) = &server_section.bind_ip {
                ConfigLoader::parse_ip_address(bind_ip_str)?
            } else {
                "127.0.0.1".parse().unwrap()
            }
        } else {
            "127.0.0.1".parse().unwrap()
        };

        let mut server = ProxyServerBuilder::new()
            .bind_ip(bind_ip)
            .add_proxies(self.config.proxies.clone())
            .build()?;

        server.start().await?;
        Ok(())
    }

    /// Run reverse proxy server mode
    async fn run_reverse_server(&self) -> Result<()> {
        eprintln!("🏠 Starting reverse proxy server mode...");

        let control_port = if let Some(runtime) = &self.config.runtime {
            runtime.control_port
        } else {
            eprintln!("❌ No control_port specified in runtime configuration!");
            return Err(AppError::Config(
                crate::error::ConfigError::MissingRequiredField(
                    "runtime.control_port".to_string()
                )
            ));
        };

        // Get bind IP
        let bind_ip: IpAddr = self.config.server.as_ref()
            .and_then(|s| s.bind_ip.as_ref())
            .and_then(|ip| ip.parse().ok())
            .unwrap_or_else(|| "0.0.0.0".parse().unwrap());

        eprintln!("🔧 Control port: {}, Bind IP: {}", control_port, bind_ip);

        let mut server = ReverseProxyServer::new(bind_ip, control_port);
        server.start().await
    }

    /// Run reverse proxy client mode
    async fn run_reverse_client(&self) -> Result<()> {
        eprintln!("🌐 Starting reverse proxy client mode...");

        let server_address = if let Some(runtime) = &self.config.runtime {
            runtime.server_address.clone()
        } else {
            eprintln!("❌ No server_address specified in runtime configuration!");
            return Err(AppError::Config(
                crate::error::ConfigError::MissingRequiredField(
                    "runtime.server_address".to_string()
                )
            ));
        };

        let server_addr: SocketAddr = server_address.parse().map_err(|_| {
            AppError::Internal {
                message: format!("Invalid server address: {}", server_address)
            }
        })?;

        if self.config.reverse_proxies.is_empty() {
            eprintln!("❌ No reverse_proxies configured in config file!");
            return Err(AppError::Config(
                crate::error::ConfigError::InvalidConfigValue {
                    path: "reverse_proxies".to_string(),
                    reason: "No reverse proxy configurations found".to_string(),
                }
            ));
        }

        eprintln!("🔗 Connecting to server: {}", server_addr);
        eprintln!("📋 Reverse proxy rules: {}", self.config.reverse_proxies.len());

        let mut client = ReverseProxyClient::new(server_addr, self.config.clone());
        client.start().await
    }

    /// Print configuration search paths for debugging
    pub fn print_search_paths() {
        let paths = ConfigLoader::get_config_search_paths();
        eprintln!("🔍 Configuration file search paths (in priority order):");
        for (i, path) in paths.iter().enumerate() {
            let status = if path.exists() { "✅" } else { "❌" };
            eprintln!("  {}. {} {}", i + 1, status, path.display());
        }
    }
}