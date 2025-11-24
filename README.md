# GSC-FQ

[![Crates.io](https://img.shields.io/crates/v/gsc-fq.svg)](https://crates.io/crates/gsc-fq)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/putao520/gsc-fq#license)

A high-performance TCP proxy tool written in Rust. Perfect for port forwarding and reverse proxying.

## Quick Start

### Installation

```bash
cargo install gsc-fq
```

### Basic Usage

1. **Create `default.toml`:**

```toml
[server]
bind_ip = "127.0.0.1"
debug = false

# Forward local port 8080 to remote service
[[proxies]]
local = "8080"
remote = "example.com:80"

# Forward local port 5432 to database
[[proxies]]
local = "5432"
remote = "db.example.com:5432"
```

2. **Run the proxy:**

```bash
./gsc-fq
```

That's it! Your local ports `8080` and `5432` will now forward to the specified remote services.

## What It Does

### Forward Proxy (Port Forwarding)

Redirects local ports to remote services:

```toml
[[proxies]]
local = "8080"           # Listen on localhost:8080
remote = "api.com:443"    # Forward to api.com:443
source_ip = "10.0.0.1"   # Optional: Use specific source IP
```

### Reverse Proxy (Service Exposure)

Expose your local services to the outside world:

```toml
[server]
bind_ip = "0.0.0.0"  # Listen on all interfaces

[[reverse_proxies]]
server = "443"              # Outside world connects to port 443
local = "localhost:3000"     # Forward to your local service
```

### Combined Mode

Run both forward and reverse proxy at the same time:

```toml
[server]
bind_ip = "127.0.0.1"

# Forward proxy rules
[[proxies]]
local = "8080"
remote = "api.example.com:443"

# Reverse proxy rules
[[reverse_proxies]]
server = "8443"
local = "localhost:3000"

# Enable reverse proxy
reverse_mode = "server"
reverse_target = "59000"
```

## Configuration Options

### Server Settings

```toml
[server]
bind_ip = "127.0.0.1"        # IP to bind to
debug = true                  # Enable debug logging
auth_token = "secret-token"   # Optional: Require authentication
allowed_tokens = ["token1", "token2"]  # Optional: Multiple valid tokens
```

### Forward Proxy Rules

```toml
[[proxies]]
local = "8080"                    # Local port to listen on
remote = "example.com:80"         # Remote host:port to forward to
source_ip = "192.168.1.100"      # Optional: Use custom source IP
```

### Reverse Proxy Rules

```toml
[[reverse_proxies]]
server = "8080"                   # External port to listen on
local = "192.168.1.100:3000"      # Local service to forward to
source_ip = "10.0.0.1"             # Optional: Use custom source IP
```

### Reverse Proxy Mode

```toml
# For reverse proxy server
reverse_mode = "server"
reverse_target = "59000"          # Control port

# For reverse proxy client
reverse_mode = "client"
reverse_target = "server.com:59000"  # Server address
```

## Common Use Cases

### 1. Expose Local Web Server

```toml
[[reverse_proxies]]
server = "443"
local = "localhost:3000"
```

### 2. Database Proxy

```toml
[[proxies]]
local = "5432"
remote = "production-db.company.com:5432"
```

### 3. Development Environment

```toml
[[proxies]]
local = "8080"
remote = "staging-api.company.com:443"

[[proxies]]
local = "3001"
remote = "staging-db.company.com:5432"
```

### 4. API Gateway

```toml
[server]
bind_ip = "0.0.0.0"

[[reverse_proxies]]
server = "80"
local = "user-service:3000"

[[reverse_proxies]]
server = "81"
local = "order-service:3001"
```

## Performance

- **1000+ concurrent connections**
- **1GB/s+ throughput**
- **Sub-millisecond latency**
- **50MB memory usage (idle)**

## Development

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run comprehensive tests
cargo test --test comprehensive_reverse_proxy_test

# Run benchmarks
cargo bench
```

## Authentication (Optional)

### Server Configuration

```toml
[server]
auth_token = "your-secret-token"
```

### Client Usage

```bash
# Method 1: Environment variable
export REVERSE_PROXY_TOKEN="your-secret-token"
./gsc-fq

# Method 2: Config file
[server]
auth_token = "your-secret-token"
```

## Requirements

- Rust 1.70+
- Supported OS: Linux, Windows, macOS
- Memory: 50MB minimum
- Network: TCP/IP connectivity

## License

Dual-licensed under MIT or Apache-2.0.

---

**Questions?** Check the [examples](https://github.com/putao520/gsc-fq/tree/main/examples) or open an issue!