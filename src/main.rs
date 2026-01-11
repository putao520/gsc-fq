// 编译时完全禁用日志的最高性能版本
#![cfg_attr(not(debug_assertions), allow(dead_code))]
#![cfg_attr(not(debug_assertions), allow(unused_imports))]
#![cfg_attr(not(debug_assertions), allow(unused_variables))]

use gsc_fq::error::{AppError, ConfigError, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "-g" {
        let label = args.get(2).map(|s| s.as_str()).unwrap_or("user@gsc-fq");
        let (totp, secret_b32) = gsc_fq::utils::totp::Totp::generate_random();
        let uri = totp.generate_otpauth_uri(label, "GSC-FQ");
        let qr = totp.render_qr_code(&uri);

        println!("🔐 --- TOTP Secret Generation ---");
        println!("\n{}", qr);
        println!("Base32 Secret: {}", secret_b32);
        println!("OTPAuth URI:  {}", uri);
        println!("\nScan the QR code above or copy the Secret into Google Authenticator / Microsoft Authenticator.");
        println!(
            "Then add `totp_secret = \"{}\"` to your config file.",
            secret_b32
        );
        return Ok(());
    }

    // 使用 RuntimeManager 统一管理启动流程
    // 它会自动搜索配置文件 (default.toml, config.toml etc.)
    // 并根据配置决定运行模式 (Forward/Reverse Server/Reverse Client)
    match gsc_fq::config::runtime::RuntimeManager::new() {
        Ok(manager) => {
            if let Err(e) = manager.run().await {
                eprintln!("❌ Runtime error: {}", e);
                std::process::exit(1);
            }
        }
        Err(AppError::Config(ConfigError::ConfigFileNotFound(_))) => {
            eprintln!("❌ Configuration file not found!");
            eprintln!(
                "Please create a config file (e.g., default.toml) with your proxy configuration."
            );
            gsc_fq::config::runtime::RuntimeManager::print_search_paths();
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("❌ Startup failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
