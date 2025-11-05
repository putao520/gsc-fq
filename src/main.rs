// 编译时完全禁用日志的最高性能版本
#![cfg_attr(not(debug_assertions), allow(dead_code))]
#![cfg_attr(not(debug_assertions), allow(unused_imports))]
#![cfg_attr(not(debug_assertions), allow(unused_variables))]

use clap::Parser;
use gsc_fq::cli::Args;
use gsc_fq::config::{ConfigFile, ConfigLoader};
use gsc_fq::error::{AppError, ConfigError, Result};
use gsc_fq::proxy::ProxyServerBuilder;
use gsc_fq::utils::system::check_system_requirements;
use std::net::IpAddr;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Validate CLI arguments
    args.validate()?;

    // Initialize debug system
    gsc_fq::utils::debug::init_debug(args.debug);

    // Check system requirements
    #[cfg(debug_assertions)]
    check_system_requirements()?;

    // Load configuration
    let config = if let Some(config_path) = &args.config {
        // 加载指定配置文件，如果不存在会自动使用空配置
        match ConfigLoader::load_from_file(config_path) {
            Ok(config) => config,
            Err(AppError::Config(ConfigError::ConfigFileNotFound(path))) => {
                eprintln!("⚠️  Configuration file '{}' not found, starting with empty configuration", path);
                ConfigFile {
                    server: None,
                    proxies: Vec::new(),
                }
            }
            Err(e) => return Err(e),
        }
    } else {
        // 没有指定配置文件，使用空配置
        eprintln!("ℹ️  No configuration file specified, starting with empty configuration (use -c for config file)");
        ConfigFile {
            server: None,
            proxies: Vec::new(),
        }
    };

    // Parse bind IP - use command line arg first, then config, then default
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
