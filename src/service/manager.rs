//! Cross-platform service manager for GSC-FQ

use service_manager::{ServiceManager, ServiceManagerBuilder, ServiceType};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::error::{AppError, Result};

/// GSC-FQ Service Manager
pub struct GscFqServiceManager {
    service_name: String,
    service_manager: Box<dyn ServiceManager>,
}

impl GscFqServiceManager {
    /// Create new service manager instance
    pub fn new() -> Result<Self> {
        let service_name = "gsc-fq".to_string();

        // Use service-manager crate to detect platform and create appropriate manager
        let service_manager = ServiceManagerBuilder::default()
            .build()
            .map_err(|e| AppError::Internal {
                message: format!("Failed to create service manager: {}", e)
            })?;

        Ok(Self {
            service_name,
            service_manager: Box::new(service_manager),
        })
    }

    /// Install GSC-FQ as system service
    pub fn install<P: AsRef<Path>>(&self, executable_path: P) -> Result<()> {
        let exe_path = executable_path.as_ref();

        // Create service configuration
        let service_config = service_manager::ServiceConfig {
            name: self.service_name.clone(),
            display_name: "GSC-FQ High-Performance TCP Proxy".to_string(),
            description: Some("GSC-FQ TCP proxy service for high-performance network forwarding".to_string()),
            exec_path: exe_path.to_path_buf(),
            args: vec![
                OsString::from("service-mode")  // Run as service mode
            ],
            working_dir: exe_path.parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf(),
            user: None,
            group: None,
            environment: vec![
                ("RUST_LOG".to_string(), "info".to_string()),
            ],
            autostart: true,
            stdout: Some(self.get_log_path().join("gsc-fq.log")),
            stderr: Some(self.get_log_path().join("gsc-fq.err")),
        };

        // Install the service
        self.service_manager
            .install(&service_config)
            .map_err(|e| AppError::Internal {
                message: format!("Failed to install service: {}", e)
            })?;

        println!("✅ GSC-FQ service installed successfully!");
        println!("   Service name: {}", self.service_name);
        println!("   Executable: {}", exe_path.display());
        println!("   Log directory: {}", self.get_log_path().display());
        println!();
        println!("To start the service:");
        #[cfg(unix)]
        println!("   sudo systemctl start gsc-fq");
        #[cfg(windows)]
        println!("   sc start gsc-fq");

        Ok(())
    }

    /// Uninstall GSC-FQ service
    pub fn uninstall(&self) -> Result<()> {
        // Stop the service first if it's running
        if self.is_running()? {
            self.stop()?;
        }

        self.service_manager
            .remove(&self.service_name)
            .map_err(|e| AppError::Internal {
                message: format!("Failed to uninstall service: {}", e)
            })?;

        println!("✅ GSC-FQ service uninstalled successfully!");
        Ok(())
    }

    /// Start GSC-FQ service
    pub fn start(&self) -> Result<()> {
        self.service_manager
            .start(&self.service_name)
            .map_err(|e| AppError::Internal {
                message: format!("Failed to start service: {}", e)
            })?;

        println!("✅ GSC-FQ service started successfully!");
        Ok(())
    }

    /// Stop GSC-FQ service
    pub fn stop(&self) -> Result<()> {
        self.service_manager
            .stop(&self.service_name)
            .map_err(|e| AppError::Internal {
                message: format!("Failed to stop service: {}", e)
            })?;

        println!("✅ GSC-FQ service stopped successfully!");
        Ok(())
    }

    /// Restart GSC-FQ service
    pub fn restart(&self) -> Result<()> {
        self.stop()?;
        self.start()?;
        println!("✅ GSC-FQ service restarted successfully!");
        Ok(())
    }

    /// Check if service is installed
    pub fn is_installed(&self) -> Result<bool> {
        match self.service_manager.status(&self.service_name) {
            Ok(_) => Ok(true),
            Err(service_manager::ServiceError::ServiceNotFound) => Ok(false),
            Err(e) => Err(AppError::Internal {
                message: format!("Failed to check service status: {}", e)
            }),
        }
    }

    /// Check if service is running
    pub fn is_running(&self) -> Result<bool> {
        match self.service_manager.status(&self.service_name) {
            Ok(status) => Ok(status.is_active()),
            Err(service_manager::ServiceError::ServiceNotFound) => Ok(false),
            Err(e) => Err(AppError::Internal {
                message: format!("Failed to check service status: {}", e)
            }),
        }
    }

    /// Get service status
    pub fn status(&self) -> Result<ServiceStatus> {
        if !self.is_installed()? {
            return Ok(ServiceStatus::NotInstalled);
        }

        match self.service_manager.status(&self.service_name) {
            Ok(status) => {
                if status.is_active() {
                    Ok(ServiceStatus::Running)
                } else {
                    Ok(ServiceStatus::Stopped)
                }
            },
            Err(service_manager::ServiceError::ServiceNotFound) => Ok(ServiceStatus::NotInstalled),
            Err(e) => Err(AppError::Internal {
                message: format!("Failed to get service status: {}", e)
            }),
        }
    }

    /// Get log directory path
    fn get_log_path(&self) -> PathBuf {
        #[cfg(unix)]
        return PathBuf::from("/var/log/gsc-fq");

        #[cfg(windows)]
        return std::env::var("ProgramData")
            .map(|path| PathBuf::from(path).join("gsc-fq\\logs"))
            .unwrap_or_else(|_| PathBuf::from("C:\\ProgramData\\gsc-fq\\logs"));

        #[cfg(target_os = "macos")]
        return std::env::var("HOME")
            .map(|path| PathBuf::from(path).join("Library/Logs/gsc-fq"))
            .unwrap_or_else(|_| PathBuf::from("/var/log/gsc-fq"));
    }

    /// Create log directory if it doesn't exist
    pub fn ensure_log_directory(&self) -> Result<()> {
        let log_path = self.get_log_path();

        #[cfg(unix)]
        {
            std::fs::create_dir_all(&log_path)
                .map_err(|e| AppError::Internal {
                    message: format!("Failed to create log directory '{}': {}",
                                    log_path.display(), e)
                })?;

            // Set appropriate permissions on Unix systems
            Command::new("chmod")
                .args(&["755", log_path.to_str().unwrap()])
                .status()
                .map_err(|e| AppError::Internal {
                    message: format!("Failed to set log directory permissions: {}", e)
                })?;
        }

        #[cfg(not(unix))]
        {
            std::fs::create_dir_all(&log_path)
                .map_err(|e| AppError::Internal {
                    message: format!("Failed to create log directory '{}': {}",
                                    log_path.display(), e)
                })?;
        }

        Ok(())
    }

    /// Get configuration directory path
    pub fn get_config_path(&self) -> PathBuf {
        #[cfg(unix)]
        return PathBuf::from("/etc/gsc-fq");

        #[cfg(windows)]
        return std::env::var("ProgramData")
            .map(|path| PathBuf::from(path).join("gsc-fq\\config"))
            .unwrap_or_else(|_| PathBuf::from("C:\\ProgramData\\gsc-fq\\config"));

        #[cfg(target_os = "macos")]
        return std::env::var("HOME")
            .map(|path| PathBuf::from(path).join("Library/Application Support/gsc-fq"))
            .unwrap_or_else(|_| PathBuf::from("/etc/gsc-fq"));
    }

    /// Ensure config directory exists
    pub fn ensure_config_directory(&self) -> Result<()> {
        let config_path = self.get_config_path();

        std::fs::create_dir_all(&config_path)
            .map_err(|e| AppError::Internal {
                message: format!("Failed to create config directory '{}': {}",
                                config_path.display(), e)
            })?;

        Ok(())
    }
}

/// Service status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    NotInstalled,
    Stopped,
    Running,
}

impl ServiceStatus {
    /// Get status as string
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceStatus::NotInstalled => "not installed",
            ServiceStatus::Stopped => "stopped",
            ServiceStatus::Running => "running",
        }
    }

    /// Get status emoji
    pub fn emoji(&self) -> &'static str {
        match self {
            ServiceStatus::NotInstalled => "❌",
            ServiceStatus::Stopped => "⏸️",
            ServiceStatus::Running => "✅",
        }
    }
}

/// Run as daemon (Unix systems only)
#[cfg(unix)]
pub fn run_as_daemon() -> Result<()> {
    use daemonize::{Daemonize, DaemonizeError};

    let daemon = Daemonize::new()
        .pid_file("/tmp/gsc-fq.pid")
        .working_directory("/")
        .user(Some("gsc-fq"))
        .group(Some("gsc-fq"))
        .stdout(std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/var/log/gsc-fq/gsc-fq.log")?)
        .stderr(std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/var/log/gsc-fq/gsc-fq.err")?);

    daemon.start()
        .map_err(|e: DaemonizeError| AppError::Internal {
            message: format!("Failed to daemonize: {}", e)
        })?;

    println!("GSC-FQ daemon started successfully!");
    Ok(())
}

/// Stub for non-Unix systems
#[cfg(not(unix))]
pub fn run_as_daemon() -> Result<()> {
    // Daemon mode is not supported on non-Unix systems
    // On Windows, use the service-manager functionality instead
    Err(AppError::Internal {
        message: "Daemon mode is only supported on Unix-like systems. Use service management instead.".to_string()
    })
}

/// Service mode entry point
pub async fn run_service_mode() -> Result<()> {
    // In service mode, we use automatic config search
    let runtime = crate::config::RuntimeManager::new()?;

    // Ensure log directory exists
    let service_manager = GscFqServiceManager::new()?;
    service_manager.ensure_log_directory()?;

    // Run the configured runtime mode
    runtime.run().await
}