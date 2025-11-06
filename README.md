# GSC-FQ High-Performance TCP Proxy CLI Tool

[![Crates.io](https://img.shields.io/crates/v/gsc-fq.svg)](https://crates.io/crates/gsc-fq)
[![Documentation](https://docs.rs/gsc-fq/badge.svg)](https://docs.rs/gsc-fq)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/putao520/gsc-fq#license)

GSC-FQ is a high-performance TCP data stream proxy forwarding CLI tool built on Rust async runtime, supporting flexible TOML configuration and intelligent debugging system.

## 🚀 Features

- **High Performance**: Built on Tokio async runtime, supports thousands of concurrent connections
- **Simple Configuration**: Only requires a `default.toml` file in the current directory
- **Smart Debugging**: Zero-overhead debugging system, controlled via configuration file
- **Source IP Control**: Optional source IP address configuration for each proxy rule
- **High-Performance Forwarding**: Zero-copy optimization using Tokio ecosystem libraries
- **Graceful Shutdown**: Signal handling and resource cleanup
- **Zero Dependency**: No command-line arguments, completely configuration-driven

## 📦 Installation

### Install via Cargo

```bash
cargo install gsc-fq
```

### Install as System Service

After installing via cargo, you can set up gsc-fq as a systemd service:

```bash
# Create service file
sudo tee /etc/systemd/system/gsc-fq.service > /dev/null <<EOF
[Unit]
Description=GSC-FQ TCP Proxy
After=network.target

[Service]
Type=simple
User=gsc-fq
WorkingDirectory=/etc/gsc-fq
ExecStart=$(which gsc-fq)
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

# Create user and config directory
sudo useradd -r -s /bin/false gsc-fq
sudo mkdir -p /etc/gsc-fq
sudo chown gsc-fq:gsc-fq /etc/gsc-fq

# Create your configuration in /etc/gsc-fq/default.toml
sudo nano /etc/gsc-fq/default.toml

# Enable and start the service
sudo systemctl daemon-reload
sudo systemctl enable gsc-fq
sudo systemctl start gsc-fq

# Check status
sudo systemctl status gsc-fq
```

### Build from Source

```bash
git clone https://github.com/putao520/gsc-fq
cd gsc-fq
cargo build --release
```

## 🎯 Quick Start

### 1. Create Configuration File

Create a `default.toml` file in the same directory as the executable:

```toml
[server]
bind_ip = "127.0.0.1"
debug = false

[[proxies]]
local_port = 8080
remote_host = "example.com"
remote_port = 80

[[proxies]]
local_port = 5432
remote_host = "db.example.com"
remote_port = 5432
source_ip = "192.168.1.100"  # Optional: Specify source IP
```

### 2. Run the Proxy

```bash
# Simply run the program - it will automatically read default.toml
./gsc-fq
```

## ⚙️ Configuration Format

The `default.toml` file supports the following sections:

### Server Section

```toml
[server]
bind_ip = "127.0.0.1"  # IP address to bind to
debug = false          # Enable debug logging
```

### Proxy Rules

```toml
[[proxies]]
local_port = 8080           # Local port to listen on
remote_host = "target.com"  # Remote host to forward to
remote_port = 80            # Remote port to forward to
source_ip = "10.0.0.1"      # Optional: Source IP for outbound connections
```

You can define multiple proxy rules by adding more `[[proxies]]` sections.


## 🛠️ Troubleshooting

### Configuration File Not Found

If the specified configuration file doesn't exist, the program will exit with an error:

```
Error: Configuration file 'config.toml' not found
```

### Port Conflicts

If the local port in the configuration is already in use, the program will report an error:

```
Error: Port 8080 is already in use
```

### Configuration Validation

The program now performs comprehensive validation before starting:
- Verifies IP address format (server and proxy `source_ip`)
- Ensures port numbers are within 1-65535 and detects duplicates
- Checks that required fields are present and non-empty
- Trims accidental whitespace and normalizes optional values
- Downgrades `source_ip = null` to a warning and ignores the value

### Invalid TOML Syntax

If the configuration file contains invalid TOML, you'll get a detailed error with line/column information and hints:

```
Error: Invalid TOML format: TOML parse error at line 12, column 17: expected string
Tip: Check for syntax errors like 'null' values (should be omitted), missing quotes, or invalid data types
```


## 🔒 Security Considerations

1. **Source IP Spoofing**: Ensure you have permission to use the specified source IP address
2. **Firewall Configuration**: Ensure firewall rules are properly configured for local and remote ports
3. **Configuration File Permissions**: Protect configuration files from unauthorized access

## 🏗️ Development

### Build

```bash
# Development build
cargo build

# Release build
cargo build --release
```

### Test

```bash
# Run all tests
cargo test

# Run specific test modules
cargo test --test proxy_functionality_test --test blackhole_functionality_test

# Run library tests only
cargo test --lib

# Benchmark tests
cargo bench
```

### Testing

- Run tests: `cargo test`
- Proxy functionality tests
- Blackhole mode tests
- Unit tests for all modules

### Lint

```bash
cargo clippy
cargo fmt --check
```

## 📋 System Requirements

- Rust 1.70+
- Supported OS: Linux, Windows, macOS
- Memory: Minimum 10MB free memory
- Network: TCP/IP network support

## 📄 License

This project is dual-licensed under MIT or Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

## 🤝 Contributing

Issues and Pull Requests are welcome! Please ensure:

1. Code passes `cargo test` and `cargo clippy`
2. Add appropriate test cases
3. Update relevant documentation



## 🎯 Use Cases

- Network testing
- Service migration
- Load balancing
- Protocol analysis
- Security testing

## 🏗️ Architecture

```
Client → GSC-FQ → Target Server
```

Built on Tokio async runtime for high-performance concurrent connections.

