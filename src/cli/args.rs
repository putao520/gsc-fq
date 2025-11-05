use clap::Parser;
use std::net::IpAddr;

/// GSC-FQ high-performance TCP proxy forwarding CLI tool
#[derive(Parser, Debug)]
#[command(name = "gsc-fq")]
#[command(about = "A high-performance TCP proxy forwarding CLI tool")]
#[command(author = "Claude Code AI")]
#[command(version = "0.1.0")]
pub struct Args {
    /// Bind IP address (e.g., 0.0.0.0, 127.0.0.1, 192.168.1.100)
    #[arg(help = "Bind IP address for listening")]
    pub bind_ip: IpAddr,

    /// TOML configuration file path
    #[arg(short = 'c', long = "config", help = "Path to TOML configuration file")]
    pub config: Option<String>,

    /// Enable debug mode for detailed logging output
    #[arg(
        short = 'd',
        long = "debug",
        help = "Enable debug mode for detailed logging output"
    )]
    pub debug: bool,
}

impl Args {
    /// Validate arguments
    pub fn validate(&self) -> crate::error::Result<()> {
        // Validate config file
        if let Some(config_path) = &self.config {
            if !std::path::Path::new(config_path).exists() {
                return Err(
                    crate::error::ConfigError::ConfigFileNotFound(config_path.clone()).into(),
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_args_validation() {
        let args = Args {
            bind_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            config: None,
            debug: false,
        };

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_debug_mode() {
        let args = Args {
            bind_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            config: None,
            debug: true,
        };

        assert!(args.validate().is_ok());
        assert!(args.debug);
    }
}
