# GSC-FQ

[![Crates.io](https://img.shields.io/crates/v/gsc-fq.svg)](https://crates.io/crates/gsc-fq)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/putao520/gsc-fq#license)

A high-performance TCP tunnel proxy tool written in Rust. Perfect for port forwarding (tunnel proxy) and reverse tunnel proxying with declarative configuration.

## Quick Start

### Installation

```bash
cargo install gsc-fq
```

### Basic Usage

1. **Create `default.toml`:**

```toml
# Simple tunnel proxy - forward local ports to remote services
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

### Tunnel Proxy (Port Forwarding)

Create TCP tunnels that redirect local ports to remote services:

```toml
[[proxies]]
local = "8080"           # Listen on localhost:8080
remote = "api.com:443"    # Forward to api.com:443
source_ip = "10.0.0.1"   # Optional: Use specific source IP
```

### Reverse Tunnel Proxy (Service Exposure Through Tunnel)

Expose your local services through a tunnel with simple, intuitive configuration:

**Server Mode** - Wait for client connections:
```toml
[server]
bind_ip = "0.0.0.0"
debug = true

# Reverse proxy server configuration
[reverse_proxy_server]
port = 9001  # Control port for clients

[[reverse_proxies]]
server = "443"              # Port exposed on server
local = "localhost:3000"    # Your local service

[[reverse_proxies]]
server = "8080"             # Another exposed port
local = "localhost:8080"    # Another local service
```

**Client Mode** - Connect to server:
```toml
[server]
bind_ip = "127.0.0.1"
debug = true

# Reverse proxy client configuration
[reverse_proxy_client]
server = "server.example.com:9001"  # Server address

[[reverse_proxies]]
server = "443"              # Port exposed on server
local = "localhost:3000"    # Your local web server

[[reverse_proxies]]
server = "22"               # Expose SSH through server
local = "localhost:22"      # Your local SSH service
```

**Hybrid Mode** - Both server and client in single process:
```toml
[server]
bind_ip = "0.0.0.0"
debug = true

# Server component
[reverse_proxy_server]
port = 9001

# Client component (connects to local server)
[reverse_proxy_client]
server = "127.0.0.1:9001"

[[reverse_proxies]]
server = "8443"             # External access port
local = "localhost:3000"    # Local web service
```

**Architecture:**
```
[External User] → [Hybrid Process:8443] → [Internal Tunnel:9001] → [Local Service:3000]
```

**Configuration Logic:**
- **有 `[reverse_proxy_server]`** → 启动服务端
- **有 `[reverse_proxy_client]`** → 启动客户端
- **两者都有** → 自动混合模式
- **基于配置存在性，无需复杂模式设置**

### Combined Mode

Run both tunnel proxy and reverse tunnel proxy at the same time:

```toml
[server]
bind_ip = "127.0.0.1"
debug = true

# Tunnel proxy rules (port forwarding)
[[proxies]]
local = "5432"
remote = "database.company.com:5432"  # Database tunneling

[[proxies]]
local = "8080"
remote = "api.example.com:443"        # API tunneling

# Reverse proxy server configuration
[reverse_proxy_server]
port = 9001

[[reverse_proxies]]
server = "2222"                        # Expose SSH externally
local = "localhost:22"                 # Local SSH service
```

**Configuration Comparison:**

| Mode | Configuration | Use Case |
|------|-------------|----------|
| **Tunnel Proxy** | `[[proxies]]` only | Access remote databases, APIs |
| **Reverse Server** | `[reverse_proxy_server]` + `[[reverse_proxies]]` | Expose local services to internet |
| **Reverse Client** | `[reverse_proxy_client]` + `[[reverse_proxies]]` | Connect to remote reverse proxy |
| **Hybrid Mode** | Both server + client configurations | Self-contained reverse proxy |
| **Combined Mode** | `[[proxies]]` + reverse proxy configs | Maximum flexibility |

## Configuration Options

### Server Settings

```toml
[server]
bind_ip = "127.0.0.1"        # IP to bind to
debug = true                  # Enable debug logging
auth_token = "secret-token"   # Optional: Require authentication
allowed_tokens = ["token1", "token2"]  # Optional: Multiple valid tokens
```

### Tunnel Proxy Rules

```toml
[[proxies]]
local = "8080"                    # Local port to listen on
remote = "example.com:80"         # Remote host:port to forward to
source_ip = "192.168.1.100"      # Optional: Use custom source IP
```

### Reverse Proxy Rules

```toml
# Define what ports to expose through the tunnel
[[reverse_proxies]]
server = "8080"                   # Port on server side (what external users connect to)
local = "127.0.0.1:3000"          # Local service port (where your service runs)
source_ip = "10.0.0.1"             # Optional: Use custom source IP
```

### Reverse Proxy Configuration

**Server Configuration:**
```toml
[reverse_proxy_server]
port = 9001                    # Control port for client connections
```

**Client Configuration:**
```toml
[reverse_proxy_client]
server = "server.example.com:9001"  # Server address and control port
```

**Hybrid Mode** (both server and client):
```toml
[reverse_proxy_server]
port = 9001

[reverse_proxy_client]
server = "127.0.0.1:9001"  # Connect to local server
```

### Architecture Overview

The reverse proxy uses a **3-port architecture**:

1. **Server Tunnel Port** (`reverse_proxy_server.port`) - Clients connect to server
2. **Server Service Port** (`reverse_proxies[].server`) - External users access services
3. **Client Local Port** (`reverse_proxies[].local`) - Where your actual service runs

```
[External User] → [Server:8080] → [Tunnel:9001] → [Client:localhost:3000]
```

## Common Use Cases

### 1. Expose Local Web Server

**Server Configuration** (exposes port 443 to outside):
```toml
[server]
bind_ip = "0.0.0.0"
debug = true

[reverse_proxy_server]
port = 9001

[[reverse_proxies]]
server = "443"              # External port
local = "localhost:3000"    # Your local web server
```

**Client Configuration** (connects to server):
```toml
[server]
bind_ip = "127.0.0.1"
debug = true

[reverse_proxy_client]
server = "server.example.com:9001"

[[reverse_proxies]]
server = "443"              # Port to expose on server
local = "localhost:3000"    # Your local web server
```

### 2. Database Tunnel

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

### 5. Hybrid Self-Contained Proxy

Single process that both accepts connections and exposes local services:

```toml
[server]
bind_ip = "0.0.0.0"
debug = true

# Server component
[reverse_proxy_server]
port = 9001

# Client component (connects to local server)
[reverse_proxy_client]
server = "127.0.0.1:9001"

# Expose web dashboard on port 8080
[[reverse_proxies]]
server = "8080"
local = "localhost:3000"   # Web dashboard

# Expose API on port 8443
[[reverse_proxies]]
server = "8443"
local = "localhost:3001"   # API server
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

# Run unit tests
cargo test --lib

# Run integration tests
cargo test --test reverse_proxy_integration_test

# Run benchmarks
cargo bench
```

### Project Documentation

This project follows comprehensive SPEC-driven development:

- **[SPEC/01-REQUIREMENTS.md](SPEC/01-REQUIREMENTS.md)**: Functional requirements and acceptance criteria
- **[SPEC/02-ARCHITECTURE.md](SPEC/02-ARCHITECTURE.md)**: System architecture design and technical decisions
- **[SPEC/04-API-DESIGN.md](SPEC/04-API-DESIGN.md)**: API specifications and interface definitions
- **[SPEC/06-TESTING-STRATEGY.md](SPEC/06-TESTING-STRATEGY.md)**: Comprehensive testing strategy

## Authentication (Optional)

### Server Configuration

```toml
[server]
auth_token = "your-secret-token"
allowed_tokens = ["token1", "token2"]
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

## Performance

- **1000+ concurrent connections**
- **1GB/s+ throughput**
- **Sub-millisecond latency**
- **50MB memory usage (idle)**
- **AES-NI hardware acceleration**
- **Zero-copy data transfer**

## License

Dual-licensed under MIT or Apache-2.0.

---

**Questions?** Check the [examples](https://github.com/putao520/gsc-fq/tree/main/examples) or open an issue!