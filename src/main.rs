// 编译时完全禁用日志的最高性能版本
#![cfg_attr(not(debug_assertions), allow(dead_code))]
#![cfg_attr(not(debug_assertions), allow(unused_imports))]
#![cfg_attr(not(debug_assertions), allow(unused_variables))]

use gsc_fq::config::ConfigLoader;
use gsc_fq::error::{AppError, ConfigError, Result};
use gsc_fq::proxy::ProxyServerBuilder;
use gsc_fq::utils::system::check_system_requirements;
use std::env;
use std::net::IpAddr;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    // Handle help flag and reject unknown arguments
    if args.len() > 1 {
        if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
            eprintln!("error: unexpected argument '--help' found");
            eprintln!("This program does not support command line arguments.");
            eprintln!("Please configure the proxy using the default.toml file.");
            std::process::exit(1);
        } else {
            eprintln!("error: unexpected arguments provided:");
            for arg in &args[1..] {
                eprintln!("  {}", arg);
            }
            eprintln!("This program does not support command line arguments.");
            eprintln!("Please configure the proxy using the default.toml file.");
            std::process::exit(1);
        }
    }

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
            // No bind_ip in config, use 127.0.0.1
            "127.0.0.1".parse().unwrap()
        }
    } else {
        // No server section, use 127.0.0.1
        "127.0.0.1".parse().unwrap()
    };

    let mut server = ProxyServerBuilder::new()
        .bind_ip(bind_ip)
        .add_proxies(config.proxies)
        .build()?;
    // Start the proxy server (this will block until shutdown)
    server.start().await?;

    Ok(())
}
