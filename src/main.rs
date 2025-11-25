// 编译时完全禁用日志的最高性能版本
#![cfg_attr(not(debug_assertions), allow(dead_code))]
#![cfg_attr(not(debug_assertions), allow(unused_imports))]
#![cfg_attr(not(debug_assertions), allow(unused_variables))]

use gsc_fq::config::ConfigLoader;
use gsc_fq::error::{AppError, ConfigError, Result};
use gsc_fq::proxy::ProxyServerBuilder;
use gsc_fq::reverse_proxy::{ReverseProxyServer, ReverseProxyClient};
use gsc_fq::utils::system::check_system_requirements;
use std::net::{IpAddr, SocketAddr};

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration from default.toml
    let config = match ConfigLoader::load_from_file("default.toml") {
        Ok(config) => config,
        Err(AppError::Config(ConfigError::ConfigFileNotFound(_))) => {
            eprintln!("❌ Configuration file 'default.toml' not found!");
            eprintln!("Please create a default.toml file with your proxy configuration.");
            std::process::exit(1);
        }
        Err(e) => return Err(e),
    };

    
    // Initialize debug system from config
    let debug_enabled = if let Some(server_section) = &config.server {
        server_section.debug.unwrap_or(false)
    } else {
        false
    };
    gsc_fq::utils::debug::init_debug(debug_enabled);

    // Check system requirements
    #[cfg(debug_assertions)]
    check_system_requirements()?;

    // Create a vector to hold all running tasks
    let mut tasks = Vec::new();

    // 1. Start forward proxy if proxies are configured
    if !config.proxies.is_empty() {
        println!("🚀 Starting forward proxy with {} rules...", config.proxies.len());

        // Parse bind IP from config
        let bind_ip: IpAddr = if let Some(server_section) = &config.server {
            if let Some(bind_ip_str) = &server_section.bind_ip {
                bind_ip_str.parse().map_err(|e| {
                    AppError::Config(ConfigError::InvalidIpAddress(format!(
                        "Invalid bind IP '{}': {}",
                        bind_ip_str, e
                    )))
                })?
            } else {
                "127.0.0.1".parse().unwrap()
            }
        } else {
            "127.0.0.1".parse().unwrap()
        };

        let mut forward_proxy = ProxyServerBuilder::new()
            .bind_ip(bind_ip)
            .add_proxies(config.proxies.clone())
            .build()?;

        // Start forward proxy in a separate task
        tasks.push(tokio::spawn(async move {
            if let Err(e) = forward_proxy.start().await {
                eprintln!("❌ Forward proxy failed: {}", e);
            }
        }));
    }

    // 2. Start reverse proxy server if configured
    if let Some(server_config) = &config.reverse_proxy_server {
        println!("🚀 Starting reverse proxy server on port {}...", server_config.port);

        // Get bind IP
        let bind_ip: IpAddr = config.server.as_ref()
            .and_then(|s| s.bind_ip.as_ref())
            .and_then(|ip| ip.parse().ok())
            .unwrap_or_else(|| "0.0.0.0".parse().unwrap());

        let mut reverse_server = ReverseProxyServer::new(bind_ip, server_config.port);
        tasks.push(tokio::spawn(async move {
            if let Err(e) = reverse_server.start().await {
                eprintln!("❌ Reverse proxy server failed: {}", e);
            }
        }));
    }

    // 3. Start reverse proxy client if configured
    if let Some(client_config) = &config.reverse_proxy_client {
        let server_addr: SocketAddr = client_config.server.parse()
            .map_err(|_| AppError::Internal {
                message: format!("Invalid reverse_proxy_client server address: {}", client_config.server)
            })?;

        if config.reverse_proxies.is_empty() {
            eprintln!("❌ No [[reverse_proxies]] configured for reverse proxy client");
            std::process::exit(1);
        }

        println!("🚀 Starting reverse proxy client connecting to {}...", server_addr);

        let mut reverse_client = ReverseProxyClient::new(server_addr, config.clone());
        tasks.push(tokio::spawn(async move {
            if let Err(e) = reverse_client.start().await {
                eprintln!("❌ Reverse proxy client failed: {}", e);
            }
        }));
    }

    if tasks.is_empty() {
        eprintln!("❌ No proxy configurations found in default.toml!");
        eprintln!("Add [[proxies]] for forward proxy or [[reverse_proxies]] + reverse_proxy_server/client for reverse proxy.");
        std::process::exit(1);
    }

    // Wait for all tasks to complete
    futures::future::join_all(tasks).await;

    Ok(())
}

