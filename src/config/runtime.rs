use crate::config::{ConfigFile, ConfigLoader};
use crate::error::{AppError, Result};
use crate::proxy::ProxyServerBuilder;
use crate::reverse_proxy::{ReverseProxyClient, ReverseProxyServer};
use std::net::IpAddr;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Runtime manager handles starting all configured services
pub struct RuntimeManager {
    config: ConfigFile,
    config_path: PathBuf,
}

impl RuntimeManager {
    /// Create new runtime manager by loading configuration from search paths
    pub fn new() -> Result<Self> {
        let (config, config_path) = ConfigLoader::load_with_search()?;

        eprintln!(
            "🚀 Loading configuration: {}",
            config_path.display()
        );

        Ok(Self {
            config,
            config_path,
        })
    }

    /// Get configuration reference
    pub fn config(&self) -> &ConfigFile {
        &self.config
    }

    /// Get config file path
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Run all configured services
    pub async fn run(&self) -> Result<()> {
        // Initialize debug system from config
        let debug_enabled = self
            .config
            .server
            .as_ref()
            .and_then(|s| s.debug)
            .unwrap_or(false);
        crate::utils::debug::init_debug(debug_enabled);

        // Parse bind IP from config
        let bind_ip: IpAddr = if let Some(server_section) = &self.config.server {
            if let Some(bind_ip_str) = &server_section.bind_ip {
                ConfigLoader::parse_ip_address(bind_ip_str)?
            } else {
                "0.0.0.0".parse().unwrap()
            }
        } else {
            "0.0.0.0".parse().unwrap()
        };

        let mut handles = Vec::new();

        // Start forward proxy if configured
        if !self.config.proxies.is_empty() {
            eprintln!("📡 Starting forward proxy with {} rules...", self.config.proxies.len());

            let mut forward_server = ProxyServerBuilder::new()
                .bind_ip(bind_ip)
                .add_proxies(self.config.proxies.clone())
                .build()?;

            handles.push(tokio::spawn(async move {
                if let Err(e) = forward_server.start().await {
                    eprintln!("❌ Forward proxy error: {}", e);
                }
            }));
        }

        // Start reverse proxy server if configured
        if let Some(server_config) = &self.config.reverse_proxy_server {
            let control_port = server_config.port;
            eprintln!("🏠 Starting reverse proxy server on port {}...", control_port);

            let mut reverse_server = ReverseProxyServer::new(bind_ip, control_port);
            handles.push(tokio::spawn(async move {
                if let Err(e) = reverse_server.start().await {
                    eprintln!("❌ Reverse proxy server error: {}", e);
                }
            }));
        }

        // Start reverse proxy client if configured
        if let Some(client_config) = &self.config.reverse_proxy_client {
            let server_address = client_config.server.clone();
            eprintln!("🌐 Starting reverse proxy client connecting to {}...", server_address);

            let server_addr: SocketAddr = server_address.parse().map_err(|_| AppError::Internal {
                message: format!("Invalid server address: {}", server_address),
            })?;

            let config = self.config.clone();
            handles.push(tokio::spawn(async move {
                let mut client = ReverseProxyClient::new(server_addr, config);
                if let Err(e) = client.start().await {
                    eprintln!("❌ Reverse proxy client error: {}", e);
                }
            }));
        }

        // Check if any service was started
        if handles.is_empty() {
            eprintln!("❌ No services configured! Please add one of the following to your config file:");
            eprintln!("   - [[proxies]] for forward proxy");
            eprintln!("   - [reverse_proxy_server] for reverse proxy server");
            eprintln!("   - [reverse_proxy_client] for reverse proxy client");
            return Err(AppError::Config(
                crate::error::ConfigError::InvalidConfigValue {
                    path: "config".to_string(),
                    reason: "No services configured".to_string(),
                },
            ));
        }

        eprintln!("✅ All services started successfully!");

        // Wait for all services (any error will terminate the program)
        match futures::future::select_all(handles).await.0 {
            Ok(_) => Ok(()),
            Err(e) => Err(AppError::Internal {
                message: format!("Service task failed: {}", e),
            }),
        }
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
