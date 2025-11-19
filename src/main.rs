// 编译时完全禁用日志的最高性能版本
#![cfg_attr(not(debug_assertions), allow(dead_code))]
#![cfg_attr(not(debug_assertions), allow(unused_imports))]
#![cfg_attr(not(debug_assertions), allow(unused_variables))]

use gsc_fq::config::ConfigLoader;
use gsc_fq::error::{AppError, ConfigError, Result};
use gsc_fq::proxy::ProxyServerBuilder;
use gsc_fq::reverse_proxy::{ReverseProxyServer, ReverseProxyClient};
use gsc_fq::utils::system::check_system_requirements;
use std::env;
use std::net::{IpAddr, SocketAddr};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    match args.len() {
        // No arguments: forward proxy mode
        1 => run_forward_proxy().await,
        
        // Single argument commands  
        3 => {
            match args[1].as_str() {
                "s" => {
                    // Reverse proxy server mode: gsc-fq s 7000
                    let port: u16 = args[2].parse()
                        .map_err(|_| AppError::Internal {
                            message: format!("Invalid port: {}", args[2])
                        })?;
                    run_reverse_server(port).await
                }
                "c" => {
                    // Reverse proxy client mode: gsc-fq c 1.2.3.4:7000
                    let server_addr: SocketAddr = args[2].parse()
                        .map_err(|_| AppError::Internal {
                            message: format!("Invalid server address: {}", args[2])
                        })?;
                    run_reverse_client(server_addr).await
                }
                _ => {
                    print_usage();
                    std::process::exit(1);
                }
            }
        }
        
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("GSC-FQ - High-Performance TCP Proxy");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  gsc-fq              Run in forward proxy mode (default)");
    eprintln!("  gsc-fq s <PORT>     Run reverse proxy server on PORT");
    eprintln!("  gsc-fq c <ADDR>     Run reverse proxy client, connect to server ADDR");
    eprintln!();
    eprintln!("EXAMPLES:");
    eprintln!("  gsc-fq                    # Forward proxy using default.toml");
    eprintln!("  gsc-fq s 7000             # Reverse proxy server on port 7000");
    eprintln!("  gsc-fq c 1.2.3.4:7000     # Connect to reverse proxy server");
}

/// Run forward proxy mode (original functionality)
async fn run_forward_proxy() -> Result<()> {
    // Load configuration from default.toml in current directory
    let config_path = "default.toml";
    let config = match ConfigLoader::load_from_file(config_path) {
        Ok(config) => config,
        Err(AppError::Config(ConfigError::ConfigFileNotFound(_))) => {
            eprintln!("❌ Configuration file 'default.toml' not found in current directory!");
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

    let mut server = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxies(config.proxies)
        .build()?;
        
    server.start().await?;
    Ok(())
}

/// Run reverse proxy server mode
async fn run_reverse_server(control_port: u16) -> Result<()> {
    // Load configuration for bind_ip and debug settings
    let config_path = "default.toml";
    let config = ConfigLoader::load_from_file(config_path).unwrap_or_else(|_| {
        use gsc_fq::config::loader::{ConfigFile, ServerSection};
        ConfigFile {
            server: Some(ServerSection::default()),
            proxies: Vec::new(),
            reverse_proxies: Vec::new(),
        }
    });
    
    // Initialize debug system
    let debug_enabled = config.server.as_ref()
        .and_then(|s| s.debug)
        .unwrap_or(false);
    gsc_fq::utils::debug::init_debug(debug_enabled);
    
    // Get bind IP
    let bind_ip: IpAddr = config.server.as_ref()
        .and_then(|s| s.bind_ip.as_ref())
        .and_then(|ip| ip.parse().ok())
        .unwrap_or_else(|| "0.0.0.0".parse().unwrap());
    
    let mut server = ReverseProxyServer::new(bind_ip, control_port);
    server.start().await
}

/// Run reverse proxy client mode
async fn run_reverse_client(server_addr: SocketAddr) -> Result<()> {
    // Load configuration
    let config_path = "config_test.toml";
    let config = match ConfigLoader::load_from_file(config_path) {
        Ok(config) => config,
        Err(AppError::Config(ConfigError::ConfigFileNotFound(_))) => {
            eprintln!("❌ Configuration file 'default.toml' not found!");
            eprintln!("Please create a default.toml with [[reverse_proxies]] section.");
            std::process::exit(1);
        }
        Err(e) => return Err(e),
    };
    
    // Check if reverse_proxies is configured
    if config.reverse_proxies.is_empty() {
        eprintln!("❌ No [[reverse_proxies]] configured in default.toml!");
        eprintln!("Example configuration:");
        eprintln!("  [[reverse_proxies]]");
        eprintln!("  port = 8080");
        std::process::exit(1);
    }
    
    // Initialize debug system
    let debug_enabled = config.server.as_ref()
        .and_then(|s| s.debug)
        .unwrap_or(false);
    gsc_fq::utils::debug::init_debug(debug_enabled);
    
    let mut client = ReverseProxyClient::new(server_addr, config);
    client.start().await
}
