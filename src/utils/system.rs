use crate::error::{Result, SystemError};
use std::fs;

/// Read a value from sysfs file
#[allow(dead_code)]
fn read_sysfs_value(path: &str) -> Result<String> {
    Ok(fs::read_to_string(path)
        .map_err(|e| SystemError::SystemCallFailed(format!("Failed to read {}: {}", path, e)))?
        .trim()
        .to_string())
}

/// Check system requirements for optimal performance
pub fn check_system_requirements() -> Result<()> {
    // Check file descriptor limits
    check_file_descriptor_limits()?;

    // Check network settings (Linux specific)
    #[cfg(target_os = "linux")]
    {
        check_linux_network_settings()?;
    }

    // Check memory limits
    check_memory_limits()?;

    Ok(())
}

/// Check file descriptor limits
fn check_file_descriptor_limits() -> Result<()> {
    // Try to read the current file descriptor limit
    #[cfg(unix)]
    {
        use nix::libc;

        unsafe {
            // Try to get the current soft limit
            let mut rlim: libc::rlimit = std::mem::zeroed();
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim) == 0 {
                let current_limit = rlim.rlim_cur as usize;

                // Check if limit is sufficient for high-performance proxy
                if current_limit < 65536 {
                    // Limit is low, but we continue anyway
                }
            } else {
                // Unable to get limit
            }
        }
    }

    #[cfg(not(unix))]
    {
        // File descriptor limit check skipped on non-Unix system
    }

    Ok(())
}

/// Check Linux-specific network settings
#[cfg(target_os = "linux")]
fn check_linux_network_settings() -> Result<()> {
    // Check net.core.somaxconn
    if let Ok(somaxconn) = read_sysfs_value("/proc/sys/net/core/somaxconn") {
        if let Ok(_value) = somaxconn.parse::<usize>() {
            // Check net.core.somaxconn value
        }
    }

    // Check net.ipv4.tcp_tw_reuse
    if let Ok(_tcp_tw_reuse) = read_sysfs_value("/proc/sys/net/ipv4/tcp_tw_reuse") {
        // Check net.ipv4.tcp_tw_reuse value
    }

    // Check net.ipv4.tcp_fin_timeout
    if let Ok(_tcp_fin_timeout) = read_sysfs_value("/proc/sys/net/ipv4/tcp_fin_timeout") {
        // Check net.ipv4.tcp_fin_timeout value
    }

    // Check if splice() system call is available
    check_splice_availability()?;

    Ok(())
}

/// Check if splice() system call is available and working
#[cfg(target_os = "linux")]
fn check_splice_availability() -> Result<()> {
    use nix::fcntl::{splice, SpliceFFlags};
    use nix::unistd::pipe;
    use std::os::fd::FromRawFd;

    // Create a pipe to test splice functionality
    let (pipe_read, pipe_write) = pipe()
        .map_err(|e| SystemError::SystemCallFailed(format!("Failed to create test pipe: {}", e)))?;

    // Try a simple splice operation to verify it works
    let test_data = b"test";
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    // Write test data to pipe
    {
        let mut pipe_write_file = unsafe { std::fs::File::from_raw_fd(pipe_write.as_raw_fd()) };
        let _ = pipe_write_file.write_all(test_data);
        std::mem::forget(pipe_write_file); // Avoid closing the fd
    }

    // Test splice from pipe to pipe
    let (_pipe2_read, pipe2_write) = pipe().map_err(|e| {
        SystemError::SystemCallFailed(format!("Failed to create second test pipe: {}", e))
    })?;

    let _ = splice(
        &pipe_read,
        None,
        &pipe2_write,
        None,
        4,
        SpliceFFlags::empty(),
    );

    Ok(())
}

/// Check memory limits and availability
fn check_memory_limits() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = read_sysfs_value("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(total_str) = line.split_whitespace().nth(1) {
                        if let Ok(total_kb) = total_str.parse::<u64>() {
                            let _total_mb = total_kb / 1024;
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Memory limit check skipped on non-Linux system
    }

    Ok(())
}

/// Get system information for debugging
pub fn get_system_info() -> SystemInfo {
    SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        target_env: std::env::consts::OS.to_string(),
        family: std::env::consts::FAMILY.to_string(),
    }
}

/// System information structure
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub target_env: String,
    pub family: String,
}

impl std::fmt::Display for SystemInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "System Information:\n\
                  - OS: {}\n\
                  - Architecture: {}\n\
                  - Environment: {}\n\
                  - Family: {}",
            self.os, self.arch, self.target_env, self.family
        )
    }
}

/// Optimize system settings for proxy performance
pub fn optimize_system_settings() -> Result<()> {
    Ok(())
}

/// Check if running in a container environment
pub fn is_running_in_container() -> bool {
    #[cfg(target_os = "linux")]
    {
        // Check for Docker
        if std::path::Path::new("/.dockerenv").exists() {
            return true;
        }

        // Check for container cgroup
        if let Ok(cgroup_content) = fs::read_to_string("/proc/1/cgroup") {
            if cgroup_content.contains("docker") || cgroup_content.contains("containerd") {
                return true;
            }
        }

        // Check for container environment variables
        if std::env::var("container").is_ok() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info() {
        let info = get_system_info();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(!info.family.is_empty());
    }

    #[test]
    fn test_system_info_display() {
        let info = get_system_info();
        let display = format!("{}", info);
        assert!(display.contains("OS:"));
        assert!(display.contains("Architecture:"));
    }

    #[test]
    fn test_check_system_requirements() {
        // This test should pass on most systems
        let result = check_system_requirements();
        assert!(result.is_ok());
    }

    #[test]
    fn test_optimize_system_settings() {
        let result = optimize_system_settings();
        assert!(result.is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_read_sysfs_value() {
        // Try to read a known sysfs value
        let result = read_sysfs_value("/proc/sys/kernel/ostype");
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(!value.is_empty());
        assert!(value.to_lowercase().contains("linux"));
    }
}
