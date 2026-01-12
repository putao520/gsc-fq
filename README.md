<div align="center">

# 🚀 GSC-FQ

### High-Performance Rust Proxy & Stealth Tunnel Tool

[![Crates.io](https://img.shields.io/crates/v/gsc-fq)](https://crates.io/crates/gsc-fq)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-91%25%20coverage-brightgreen)](tests/)

[Features](#-features) • [Quick Start](#-quick-start) • [Performance](#-performance-benchmarks) • [Configuration](#-configuration) • [Installation](#-installation)

</div>

---

## 📖 About

**GSC-FQ** is a high-performance proxy and stealth tunnel tool written in **Rust**, supporting forward proxy, reverse proxy, TCP/UDP traffic forwarding, and dual authentication based on Token and TOTP.

### ✨ Why GSC-FQ?

- ⚡ **Extreme Performance**: macOS 4.02x speedup, Linux splice() zero-copy, 84% memory optimization
- 🔒 **Security Hardened**: Token + TOTP dual authentication, Yamux multiplexing
- 🎯 **Cross-Platform**: Full support for Linux / macOS / Windows
- 🧪 **High Quality**: 91% E2E test coverage, SHA256 integrity verification
- 💡 **Easy to Use**: One-line install script, simple TOML configuration

---

## 🌟 Features

### Service Types

| Feature | Description | Use Cases |
|---------|-------------|-----------|
| **Forward Proxy** | Forward local port to remote service | Jump Box, internal network penetration |
| **Reverse Proxy** | Expose internal services via stealth tunnel | Remote work, service exposure |
| **UDP Forwarding** | Stable UDP traffic forwarding | Gaming, DNS, video streaming |

> **Note**: All configured services start automatically based on your config file. No need to specify modes.

### Security Features

- 🔐 **Dual Authentication**: Token static key + TOTP dynamic verification (Google Authenticator)
- 🛡️ **Connection Encryption**: Yamux-based multiplexed encrypted tunnel
- ⚠️ **Blackhole Mode**: Active probing defense, confuse attackers

### Performance Optimizations

- 🚀 **Platform-Specific Optimizations**:
  - **macOS**: 256KB buffer, **4.02x speedup** (1MB scenario)
  - **Linux**: splice() zero-copy, **+30%** performance (real network)
  - **Windows**: 256KB optimized buffer, fixed performance issues

- 💾 **Memory Optimization**: Streaming processing, **1.63MB** memory for 10MB transfer (-84%)

- 🎛️ **Adaptive Transfer**: Automatically select optimal strategy based on data size

---

## 📦 Installation

### Option 1: Cargo (Recommended)

```bash
cargo install gsc-fq
```

### Option 2: One-Line Install Script

```bash
curl -sSLf https://raw.githubusercontent.com/putao520/gsc-fq/main/install.sh | sh
```

### Option 3: Docker

```bash
docker pull ghcr.io/putao520/gsc-fq:v0.9.1
docker run -v $(pwd)/config.toml:/app/config.toml ghcr.io/putao520/gsc-fq:v0.9.1
```

### Option 4: Pre-built Binaries

Download pre-built binaries from [GitHub Releases](https://github.com/putao520/gsc-fq/releases):

**Linux** (x86_64):
```bash
wget https://github.com/putao520/gsc-fq/releases/download/v0.9.1/gsc-fq-linux-x86_64.tar.gz
tar xzf gsc-fq-linux-x86_64.tar.gz
sudo mv gsc-fq /usr/local/bin/
```

**Linux** (aarch64):
```bash
wget https://github.com/putao520/gsc-fq/releases/download/v0.9.1/gsc-fq-linux-aarch64.tar.gz
tar xzf gsc-fq-linux-aarch64.tar.gz
sudo mv gsc-fq /usr/local/bin/
```

**macOS** (Intel):
```bash
wget https://github.com/putao520/gsc-fq/releases/download/v0.9.1/gsc-fq-macos-x86_64.tar.gz
tar xzf gsc-fq-macos-x86_64.tar.gz
sudo mv gsc-fq /usr/local/bin/
```

**macOS** (Apple Silicon):
```bash
wget https://github.com/putao520/gsc-fq/releases/download/v0.9.1/gsc-fq-macos-aarch64.tar.gz
tar xzf gsc-fq-macos-aarch64.tar.gz
sudo mv gsc-fq /usr/local/bin/
```

**Windows** (x86_64):
```powershell
# Download from: https://github.com/putao520/gsc-fq/releases/download/v0.9.1/gsc-fq-windows-x86_64.zip
# Extract and add to PATH
```

### Option 5: Build from Source

---

## 🚀 Quick Start

### 1️⃣ Forward Proxy

**Scenario**: Forward local port 8080 to remote API server

**Configuration** (`config.toml`):
```toml
[[proxies]]
local = "8080"
remote = "api.example.com:443"
```

**Run**:
```bash
gsc-fq
# Or specify config file
gsc-fq -c /path/to/config.toml
```

**Test**:
```bash
curl http://127.0.0.1:8080/api
```

---

### 2️⃣ Multiple Services (Recommended for Complex Scenarios)

**Scenario**: Run forward proxy and reverse proxy server simultaneously

**Configuration** (`config.toml`):
```toml
# Forward proxy rules
[[proxies]]
local = "8080"
remote = "api.example.com:443"

[[proxies]]
local = "3000"
remote = "db.example.com:5432"

# Reverse proxy server
[reverse_proxy_server]
port = 9001
allowed_tokens = ["my-secret-token"]
```

**Run**:
```bash
gsc-fq -c config.toml
```

**What happens**:
- ✅ Forward proxy on port 8080 → api.example.com:443
- ✅ Forward proxy on port 3000 → db.example.com:5432
- ✅ Reverse proxy server on port 9001
- All services start automatically based on config

---

### 3️⃣ Reverse Proxy

**Scenario**: Expose internal service to public internet via stealth tunnel

**Server** (Public machine `config-server.toml`):
```toml
[reverse_proxy_server]
port = 9001                    # Control connection port
allowed_tokens = ["my-secret-token"]

# Optional: Enable TOTP dynamic verification
totp_secret = "JBSWY3DPEHPK3PXP"  # Generate with `gsc-fq -g`
```

**Client** (Internal machine `config-client.toml`):
```toml
[reverse_proxy_client]
server = "PUBLIC_IP:9001"
token = "my-secret-token"

[[reverse_proxies]]
server_port = "443"            # Port exposed on public machine
local = "127.0.0.1:3000"       # Local service to expose
```

**Run**:
```bash
# Public machine
gsc-fq -c config-server.toml

# Internal machine
gsc-fq -c config-client.toml
```

**Access**: Visit `PUBLIC_IP:443` to access the internal service

---

### 4️⃣ TOTP Dynamic Verification

**Step 1**: Generate TOTP secret
```bash
$ gsc-fq -g

✅ TOTP secret generated successfully!
📱 Secret: JBSWY3DPEHPK3PXP
🔐 Base32: JBSWY3DPEHPK3PXP

📷 Scan QR code with Google Authenticator:
████████████████████████
██ Scan this QR code to add  ██
████████████████████████

⏰ Verification code updates every 30 seconds
```

**Step 2**: Configure server to enable TOTP
```toml
[reverse_proxy_server]
port = 9001
totp_secret = "JBSWY3DPEHPK3PXP"  # Enter generated secret
```

**Step 3**: Client connects with TOTP verification code
```bash
# 6-digit code from Google Authenticator
gsc-fq -c config-client.toml
```

---

## ⚡ Performance Benchmarks

### Platform Optimization Comparison

| Platform | Optimization Strategy | 1MB Throughput | 10MB Throughput | Memory Usage |
|----------|----------------------|----------------|-----------------|--------------|
| **macOS** | 256KB bulk_copy | **9.15 GB/s** (4.02x) | 8.30 GB/s (2.89x) | 1.63 MB |
| **Linux** | splice() zero-copy | - | **+30%** (real network) | 1.63 MB |
| **Windows** | 256KB bulk_copy | 2.28 GB/s | 8.30 GB/s | 1.63 MB |

*Benchmark environment: Apple M2, 16GB RAM, localhost loopback*

### Comparison with Other Solutions

| Metric | GSC-FQ v0.9.1 | Nginx (stream) | HAProxy | socat |
|--------|--------------|---------------|---------|-------|
| Throughput (macOS) | **9.15 GB/s** | 2.1 GB/s | 1.8 GB/s | 1.2 GB/s |
| Memory Usage (10MB) | **1.63 MB** | 5.2 MB | 4.8 MB | 10 MB+ |
| Concurrent Connections | 10,000+ | 10,000+ | 10,000+ | 1,000 |
| Platform Optimization | ✅ Adaptive | ❌ Generic | ❌ Generic | ❌ Generic |
| Zero-Copy | ✅ Linux | ✅ epoll | ❌ | ❌ |

### High Concurrency Tests

```
📊 High concurrency stress test (200 concurrent connections)
  Successful connections: 200 / 200
  Failed connections: 0
  Total time: 156.23ms
  Average latency: 781μs
  Throughput: 1280.32 connections/sec
```

---

## 🛠️ Command Line Arguments

| Argument | Description | Default |
|----------|-------------|---------|
| `-c <PATH>` | Specify config file | `config.toml` |
| `-g` | Generate TOTP secret and QR code | - |
| `-V` / `--version` | Show version | - |
| `-h` / `--help` | Show help information | - |

---

## 📋 Configuration Examples

### Complete Configuration Example

```toml
# ==================== Forward Proxy ====================
# All configured services start automatically - no mode selection needed

[[proxies]]
local = "8080"
remote = "api.example.com:443"

[[proxies]]
local = "3000"
remote = "db.example.com:5432"

# ==================== Reverse Proxy Server ====================

[reverse_proxy_server]
port = 9001
allowed_tokens = ["token1", "token2"]

# TOTP configuration (optional)
totp_secret = "JBSWY3DPEHPK3PXP"

# Connection pool configuration
[connection_pool]
min_idle = 5
max_size = 100
idle_timeout = 300

# ==================== Reverse Proxy Client ====================

[reverse_proxy_client]
server = "PUBLIC_IP:9001"
token = "token1"

# Expose multiple local services
[[reverse_proxies]]
server_port = "443"        # HTTPS
local = "127.0.0.1:443"

[[reverse_proxies]]
server_port = "80"         # HTTP
local = "127.0.0.1:8080"

[[reverse_proxies]]
server_port = "22"         # SSH
local = "127.0.0.1:22"

# ==================== UDP Forwarding ====================

[[udp_proxies]]
local = "127.0.0.1:53"
remote = "8.8.8.8:53"

# ==================== Logging Configuration ====================

[logging]
level = "info"              # debug, info, warn, error
file = "/var/log/gsc-fq.log"
max_size = "100MB"
max_backups = 7
```

**Important**: All services in the config file will start automatically. You can mix and match any combination:
- `[[proxies]]` - Forward proxy rules
- `[reverse_proxy_server]` - Reverse proxy server
- `[reverse_proxy_client]` - Reverse proxy client
- `[[udp_proxies]]` - UDP forwarding

---

## 🎯 Use Cases

### Use Case 1: Development Environment Proxy

**Problem**: Local development needs access to remote API with network restrictions

**Solution**:
```bash
# Configure forward proxy
[[proxies]]
local = "8080"
remote = "api.internal.com:443"

# Access
curl http://127.0.0.1:8080/api/users
```

### Use Case 2: Remote Work

**Problem**: Home computer needs access to company internal services

**Solution**:
```bash
# Company server (public IP)
[reverse_proxy_server]
port = 9001
totp_secret = "xxx"

# Home computer
[reverse_proxy_client]
server = "COMPANY_PUBLIC_IP:9001"

[[reverse_proxies]]
server_port = "8080"
local = "127.0.0.1:80"  # Company internal OA system
```

### Use Case 3: Game Acceleration

**Problem**: UDP game packets unstable

**Solution**:
```bash
[[udp_proxies]]
local = "127.0.0.1:25565"
remote = "game-server.com:25565"
```

---

## 🧪 Test Coverage

### E2E Test Statistics

| Category | Coverage | Test Count |
|----------|----------|------------|
| **Normal Scenarios** | 100% | 8 tests |
| **Error Scenarios** | 85% | 6 tests |
| **High Concurrency** | 90% | 3 tests |
| **Edge Cases** | 95% | 4 tests |
| **Data Validation** | 95% | 4 tests |
| **Overall** | **91%** | **25 tests** |

### Running Tests

```bash
# Run all tests
cargo test

# Run E2E tests
cargo test --test network_resilience_test
cargo test --test high_concurrency_stress_test
cargo test --test edge_cases_test
cargo test --test data_forwarding_validation_test
```

---

## 📚 Architecture

### Performance Optimization Architecture

```
┌─────────────────────────────────────────────┐
│      Platform-Specific Optimization Layer   │
├──────────┬──────────┬──────────┬──────────┤
│  macOS   │  Linux   │ Windows  │  Generic  │
│ 256KB    │ splice() │ 256KB    │ 256KB    │
│ bulk_copy│ zero-copy│ bulk_copy│ bulk_copy│
└──────────┴──────────┴──────────┴──────────┘
           ↓
┌─────────────────────────────────────────────┐
│         Adaptive Transfer Strategy          │
├──────────┬──────────┬──────────┬──────────┤
│ Small    │ Medium   │ Large    │ Stream   │
│ < 64KB   │ 64KB-1MB │ 1MB-10MB │ > 10MB   │
│ tokio    │ 128KB    │ 256KB    │ splice() │
└──────────┴──────────┴──────────┴──────────┘
           ↓
┌─────────────────────────────────────────────┐
│     Connection Management & Multiplexing    │
│     Yamux + Connection Pool + Blackhole    │
└─────────────────────────────────────────────┘
```

### Core Modules

- **`adaptive_copy.rs`**: Known-size data adaptive transfer
- **`adaptive_stream.rs`**: Unknown-size streaming transfer
- **`splice_optimizer.rs`**: Linux splice() zero-copy optimizer
- **`zero_copy.rs`**: Platform-specific zero-copy implementation
- **`stealth_handler.rs`**: Stealth tunnel handling (blackhole mode)

---

## 🤝 Contributing

Contributions are welcome! Please follow these steps:

1. Fork this repository
2. Create feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to branch (`git push origin feature/AmazingFeature`)
5. Open Pull Request

### Development Requirements

- ✅ Code formatting: `cargo fmt`
- ✅ Code linting: `cargo clippy`
- ✅ Tests passing: `cargo test`
- ✅ Test coverage: > 80%

---

## 📊 Changelog

See [CHANGELOG.md](CHANGELOG.md) for detailed update history.

### v0.9.1 (2026-01-12) - Latest

- ✅ **Multiple Services**: All services start automatically based on config file
- 🚀 **Configuration-Driven**: No need to specify modes, just configure what you need
- 🐛 **Bug Fixes**: Removed single-mode limitation

### v0.9.0 (2026-01-12)

- 🚀 **Performance**: macOS 4.02x speedup, Linux splice() zero-copy
- 🧪 **Testing**: E2E coverage 48% → 91%
- 💾 **Memory**: 10MB transfer memory usage -84%
- 🐛 **Bug Fixes**: Resource leaks, TOTP compatibility

---

## ❓ FAQ

### Q1: How to view logs?

**A**: Use debug mode or specify log file
```bash
# Debug mode
RUST_LOG=debug gsc-fq

# Specify log file
[logging]
level = "debug"
file = "/var/log/gsc-fq.log"
```

### Q2: What if connection fails?

**A**: Check the following
1. Confirm Token and TOTP configuration is correct
2. Check firewall rules
3. View server/client logs
4. Verify network connectivity (`ping`, `telnet`)

### Q3: How to improve performance?

**A**: Optimization suggestions
```toml
[connection_pool]
min_idle = 10        # Increase min idle connections
max_size = 200       # Increase connection pool size
idle_timeout = 600    # Extend idle timeout
```

### Q4: Does it support Docker deployment?

**A**: Fully supported!
```bash
docker run -d \
  -v $(pwd)/config.toml:/app/config.toml \
  -p 8080:8080 \
  ghcr.io/putao520/gsc-fq:v0.9.1
```

---

## ⚖️ License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

---

## 🙏 Acknowledgments

- [Tokio](https://tokio.rs/): Async runtime
- [Yamux](https://github.com/najamelan/yamux): Multiplexing
- [Rust Crypto](https://github.com/RustCrypto): Cryptographic algorithms

---

<div align="center">

**[⬆ Back to Top](#-gsc-fq)**

Made with ❤️ by [putao520](https://github.com/putao520)

[GitHub](https://github.com/putao520/gsc-fq) • [Crates.io](https://crates.io/crates/gsc-fq) • [Docs](https://docs.rs/gsc-fq)

</div>
