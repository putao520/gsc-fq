//! Service management CLI commands

use crate::service::manager::{GscFqServiceManager, ServiceStatus};
use crate::error::Result;
use std::env;

/// Handle service management commands
pub async fn handle_service_command(command: &str) -> Result<()> {
    let service_manager = GscFqServiceManager::new()?;

    match command {
        "install" => {
            let current_exe = env::current_exe()
                .map_err(|e| crate::error::AppError::Internal {
                    message: format!("Failed to get current executable path: {}", e)
                })?;

            // Ensure required directories exist
            service_manager.ensure_log_directory()?;
            service_manager.ensure_config_directory()?;

            service_manager.install(&current_exe)?;
        }

        "uninstall" => {
            service_manager.uninstall()?;
        }

        "start" => {
            service_manager.start()?;
        }

        "stop" => {
            service_manager.stop()?;
        }

        "restart" => {
            service_manager.restart()?;
        }

        "status" => {
            let status = service_manager.status()?;
            match status {
                ServiceStatus::NotInstalled => {
                    println!("{} GSC-FQ service is not installed", status.emoji());
                    println!();
                    println!("To install the service:");
                    #[cfg(unix)]
                    println!("  sudo gsc-fq service install");
                    #[cfg(windows)]
                    println!("  gsc-fq service install  (run as Administrator)");
                },
                ServiceStatus::Stopped => {
                    println!("{} GSC-FQ service is installed but stopped", status.emoji());
                    println!();
                    println!("To start the service:");
                    #[cfg(unix)]
                    println!("  sudo systemctl start gsc-fq");
                    #[cfg(windows)]
                    println!("  gsc-fq service start  (run as Administrator)");
                },
                ServiceStatus::Running => {
                    println!("{} GSC-FQ service is running", status.emoji());
                    println!();
                    println!("Config path: {}", service_manager.get_config_path().display());
                    println!("Log path: {}", service_manager.get_log_path().display());
                }
            }
        }

        _ => {
            eprintln!("❌ Unknown service command: {}", command);
            eprintln!();
            print_service_help();
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Print service command help
pub fn print_service_help() {
    eprintln!("GSC-FQ Service Management Commands");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  gsc-fq service <command>");
    eprintln!();
    eprintln!("COMMANDS:");
    eprintln!("  install    Install GSC-FQ as system service");
    eprintln!("  uninstall  Remove GSC-FQ service");
    eprintln!("  start      Start GSC-FQ service");
    eprintln!("  stop       Stop GSC-FQ service");
    eprintln!("  restart    Restart GSC-FQ service");
    eprintln!("  status     Show service status");
    eprintln!();
    eprintln!("PLATFORM SPECIFIC:");
    #[cfg(unix)]
    eprintln!("  Linux/macOS: Run with 'sudo' for install/uninstall/start/stop commands");
    #[cfg(windows)]
    eprintln!("  Windows: Run as Administrator for install/uninstall/start/stop commands");
    eprintln!();
    eprintln!("EXAMPLES:");
    #[cfg(unix)]
    eprintln!("  sudo gsc-fq service install");
    #[cfg(windows)]
    eprintln!("  gsc-fq service install  # (run as Administrator)");
    eprintln!("  gsc-fq service status");
    eprintln!("  gsc-fq service restart");
    eprintln!();
    eprintln!("CONFIGURATION:");
    println!("  System config: {}", GscFqServiceManager::new()
        .map(|m| m.get_config_path().display().to_string())
        .unwrap_or_else(|_| "N/A".to_string()));
    println!("  Log directory: {}", GscFqServiceManager::new()
        .map(|m| m.get_log_path().display().to_string())
        .unwrap_or_else(|_| "N/A".to_string()));
}