# GSC-FQ High-Performance TCP Proxy CLI Tool

[![Crates.io](https://img.shields.io/crates/v/gsc-fq.svg)](https://crates.io/crates/gsc-fq)
[![Documentation](https://docs.rs/gsc-fq/badge.svg)](https://docs.rs/gsc-fq)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/putao520/gsc-fq#license)

GSC-FQ is a high-performance TCP data stream proxy forwarding CLI tool built on Rust async runtime, supporting flexible TOML configuration and intelligent debugging system.

## 🚀 Features

- **High Performance**: Built on Tokio async runtime, supports thousands of concurrent connections
- **Flexible Configuration**: Supports TOML configuration files with multiple forwarding rules
- **Smart Debugging**: Zero-overhead debugging system in production, enable detailed logs on demand
- **Source IP Spoofing**: Optional source IP address configuration
- **Zero-Copy Forwarding**: Efficient data transfer using `tokio::io::copy_bidirectional`
- **Graceful Shutdown**: Signal handling and resource cleanup
- **Simple to Use**: Clean CLI interface with only two startup parameters

## 📦 Installation

### Install via Cargo

```bash
cargo install gsc-fq
```

### Build from Source

```bash
git clone https://github.com/putao520/gsc-fq
cd gsc-fq
cargo build --release
```

### Basic Usage

```bash
# Start with default forwarding rules
gsc-fq 127.0.0.1

# Start with configuration file
gsc-fq --config config.toml 127.0.0.1

# Enable debug mode
gsc-fq --debug 127.0.0.1
```

## ⚙️ Configuration

### Configuration File

Create a TOML configuration file:

```toml
[[proxy]]
local_port = 8080
remote_host = "127.0.0.1"
remote_port = 12345
source_ip = "192.168.1.100"  # Optional: Set source IP address

[[proxy]]
local_port = 8081
remote_host = "example.com"
remote_port = 80
```

### Default Forwarding Rules

When no configuration file is specified, GSC-FQ includes built-in forwarding rules for common use cases. You can create your own configuration file to customize the forwarding behavior:

```toml
[[proxy]]
local_port = 8080
remote_host = "your-server.com"
remote_port = 80

[[proxy]]
local_port = 8443
remote_host = "your-server.com"
remote_port = 443
```

## 📋 Command Line Options

```
Usage: gsc-fq [OPTIONS] <BIND_IP>

Arguments:
  <BIND_IP>  Bind IP address for listening

Options:
  -c, --config <FILE>  TOML configuration file path
  -d, --debug          Enable debug mode for detailed logging output
  -h, --help           Display help information
  -V, --version        Display version information
```

## 🎯 Smart Debugging

GSC-FQ provides an intelligent debugging system:

- **Production Mode** (without `--debug`): Silent operation with zero performance overhead
- **Debug Mode** (with `--debug`): Shows detailed connection handling information

```bash
# Production mode - High performance silent operation
gsc-fq 127.0.0.1

# Debug mode - Detailed connection logs
gsc-fq --debug 127.0.0.1
```

## ⚡ Performance

- **Zero-Copy Data Forwarding** - Efficient data transfer using `tokio::io::copy_bidirectional`
- **Unlimited Concurrency** - Based on `tokio::spawn` async task scheduling
- **Memory Safe** - Rust memory safety guarantees, avoiding buffer overflows
- **Production Optimized** - LTO and code generation optimizations enabled in Release mode

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

### Invalid Field Values

When configuration values fail validation, the loader reports the exact field path to help you fix the issue:

```
Error: Invalid configuration value at config: proxies[0].source_ip 'not-an-ip' is not a valid IP address
```

### Tolerating Optional Fields

Certain recoverable issues are logged as warnings so the application can still start safely:

```
⚠️  Configuration Warning: proxies[0].source_ip contains invalid 'null' value; the field will be ignored
⚠️  Configuration Warning: server.bind_ip is empty, falling back to default 127.0.0.1
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

# Run integration tests
cargo test --test integration

# Benchmark tests
cargo bench
```

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

## 📝 Changelog

### v0.1.0 (2025-11-05)
- 🎉 Initial release
- ✅ Basic TCP proxy forwarding functionality
- ✅ TOML configuration file support
- ✅ Smart debugging system
- ✅ Graceful shutdown mechanism
- ✅ Cross-platform support
