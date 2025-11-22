# GSC-FQ High-Performance TCP Proxy with Reverse Proxy Support

[![Crates.io](https://img.shields.io/crates/v/gsc-fq.svg)](https://crates.io/crates/gsc-fq)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/putao520/gsc-fq#license)

GSC-FQ is a high-performance TCP proxy CLI tool built on Rust async runtime, featuring both **forward proxy** and **reverse proxy** capabilities with flexible TOML configuration.

## 🚀 Key Features

### 🔄 Forward Proxy (Traditional)
- Forward local ports to remote servers
- Source IP address spoofing support
- Multiple proxy rules configuration
- High-performance concurrent connections

### 🔀 Reverse Proxy (NEW!)
- Bidirectional TCP proxy with Yamux multiplexing
- Dynamic port allocation and mapping
- Client-server architecture with control connections
- Multiple reverse proxy configurations
- Connection pooling and quality-aware routing

## 📦 Installation

```bash
cargo install gsc-fq
```

Or build from source:

```bash
git clone https://github.com/putao520/gsc-fq
cd gsc-fq
cargo build --release
```

## 🎯 Quick Start

GSC-FQ supports two deployment modes. Choose the one that fits your needs:

### Mode 1: Forward Proxy (Traditional)

Create `default.toml`:

```toml
[server]
bind_ip = "127.0.0.1"
debug = false

[[proxies]]
local_port = 8080
remote_host = "example.com"
remote_port = 80
source_ip = "192.168.1.100"  # Optional: Source IP spoofing

[[proxies]]
local_port = 5432
remote_host = "db.example.com"
remote_port = 5432
```

### Mode 2: Reverse Proxy (NEW!)

Create `reverse_proxy.toml`:

```toml
[server]
bind_ip = "127.0.0.1"
debug = true
auth_token = "my-secret-token"     # Optional: Require TOKEN authentication
# allowed_tokens = ["token1", "token2"]  # Alternative: Multiple allowed tokens

# Simple symmetric port configuration
[[reverse_proxies]]
port = 8080                    # Server and client both use port 8080
local_host = "192.168.1.100"   # Forward to this host
source_ip = "10.0.0.1"         # Optional: Source IP for connections

# Advanced asymmetric port configuration
[[reverse_proxies]]
server_port = 8081             # Reverse proxy server listens on 8081
local_port = 3000              # Forward to local port 3000
local_host = "192.168.1.101"
source_ip = "10.0.0.2"
```

### Start GSC-FQ

```bash
# Auto-detects configuration (forward.toml, reverse_proxy.toml, or default.toml)
./gsc-fq

# Or specify configuration file
./gsc-fq --config reverse_proxy.toml
```

## ⚙️ Configuration

### Server Section

```toml
[server]
bind_ip = "127.0.0.1"  # IP address to bind reverse proxy client
debug = true           # Enable debug logging
auth_token = "secret"  # Optional: Require TOKEN authentication
allowed_tokens = ["token1", "token2"]  # Optional: Multiple allowed tokens
```

#### TOKEN Authentication (NEW!)

GSC-FQ supports TOKEN-based authentication for enhanced security:

**Server Configuration:**
```toml
[server]
# Option 1: Single token authentication
auth_token = "my-secret-token"

# Option 2: Multiple allowed tokens
allowed_tokens = ["token1", "token2", "token3"]

# Option 3: Environment variable (recommended for production)
# auth_token will be read from REVERSE_PROXY_TOKEN env var
```

**Client Configuration:**
```bash
# Option 1: Environment variable
export REVERSE_PROXY_TOKEN="my-secret-token"
./gsc-fq

# Option 2: Configuration file
[server]
auth_token = "my-secret-token"
```

**Security Features:**
- **SHA256 Configuration Hashing**: Prevents configuration tampering
- **Token Validation**: Both server and client validate tokens
- **Session Tracking**: Each connection gets a unique session ID
- **Multiple Token Support**: Allow different clients with different tokens

### Forward Proxy Rules

```toml
[[proxies]]
local_port = 8080           # Local port to listen on
remote_host = "target.com"  # Remote host to forward to
remote_port = 80            # Remote port to forward to
source_ip = "10.0.0.1"      # Optional: Source IP for outbound connections
```

### Reverse Proxy Rules

Two configuration approaches are supported:

#### Approach 1: Symmetric Ports (Simple)
```toml
[[reverse_proxies]]
port = 8080                    # Both server and client use port 8080
local_host = "localhost"       # Target host
source_ip = "192.168.1.100"   # Optional source IP
```

#### Approach 2: Asymmetric Ports (Advanced)
```toml
[[reverse_proxies]]
server_port = 8080             # Reverse proxy server port
local_port = 3000              # Local service port
local_host = "192.168.1.100"  # Target host
source_ip = "10.0.0.1"         # Optional source IP
```

**Configuration Rules:**
- `port` and `server_port/local_port` are mutually exclusive
- When using asymmetric ports, both `server_port` and `local_port` must be specified
- `server_port` values must be unique across all reverse proxy rules
- `local_host` defaults to "localhost" if not specified

## 🔀 Reverse Proxy Architecture

The reverse proxy uses a client-server architecture with Yamux multiplexing:

```
┌─────────────┐    Control Channel    ┌─────────────────┐
│   Client    │ ←───────────────────→ │   Server        │
│  (Connects) │    (Yamux Multiplex)   │ (Listens &      │
│             │                         │  Manages Ports) │
└─────────────┘                         └─────────────────┘
       │                                           │
       │ Data Connection (Port 8080)              │
       ├───────────────────────────────────────→ │
       │ ←─────────────────────────────────────── │
       ▼                                           ▼
┌─────────────┐                          ┌─────────────┐
│ Local Service│ ←───────────────────────→ │  Remote     │
│   (Port N)   │    Traffic Forwarded    │  Service    │
└─────────────┘                          └─────────────┘
```

### Key Components

- **Control Connection**: Yamux multiplexed connection for port negotiation
- **Port Mapping**: Dynamic allocation of server ports for reverse proxy
- **Connection Pooling**: Multiple parallel connections for high throughput
- **Quality-Aware Routing**: Intelligent connection selection based on performance

## 🌐 Deployment Scenarios

### Scenario 1: Service Exposure
```toml
# Expose internal service to external network
[[reverse_proxies]]
port = 443                      # External HTTPS port
local_host = "internal-service"  # Internal service
local_port = 8443               # Internal HTTPS port
```

### Scenario 2: Multi-Service Gateway
```toml
# Single gateway for multiple services
[[reverse_proxies]]
server_port = 8080
local_port = 3000   # Web service
local_host = "web-server"

[[reverse_proxies]]
server_port = 8081
local_port = 5432   # Database service
local_host = "db-server"

[[reverse_proxies]]
server_port = 8082
local_port = 6379   # Redis cache
local_host = "cache-server"
```

### Scenario 3: Development Proxy
```toml
# Development environment proxy
[[reverse_proxies]]
port = 3000
local_host = "localhost"
local_port = 3001  # Development server
source_ip = "192.168.1.100"  # Test with specific source IP
```

### Scenario 4: Secure Production Deployment
```toml
# Server configuration with security
[server]
bind_ip = "0.0.0.0"  # Listen on all interfaces
debug = false
auth_token = "prod-secure-token-2024"  # Enable authentication

# Production services with security
[[reverse_proxies]]
server_port = 443              # External HTTPS
local_port = 8443              # Internal HTTPS service
local_host = "web-internal"
source_ip = "10.0.1.100"       # Known load balancer IP

[[reverse_proxies]]
server_port = 80               # External HTTP
local_port = 8080              # Internal HTTP service
local_host = "api-internal"
source_ip = "10.0.1.101"       # Known load balancer IP
```

**Environment-based Authentication (Recommended):**
```bash
# Set secure token from environment
export REVERSE_PROXY_TOKEN="$(openssl rand -hex 32)"

# Or use secrets management
export REVERSE_PROXY_TOKEN="$VAULT_SECRET_TOKEN"

./gsc-fq --config reverse_proxy.toml
```

## 🛠️ Troubleshooting

### Common Issues

**Configuration File Not Found:**
```
Error: Configuration file 'reverse_proxy.toml' not found
```

**Port Conflicts:**
```
Error: Port 8080 is already in use
```

**Connection Failures:**
- Check firewall rules for both control and data ports
- Verify network connectivity between client and server
- Ensure target services are running

**Authentication Failures:**
```
Error: Handshake failed: Authentication failed
```
- Verify REVERSE_PROXY_TOKEN environment variable is set
- Check that client token matches server's auth_token or allowed_tokens
- Ensure tokens don't have trailing spaces or special characters

**Configuration Hash Mismatch:**
```
Error: Handshake failed: Configuration integrity check failed
```
- This indicates configuration tampering or version mismatch
- Restart both client and server with identical configuration files
- Check for configuration file corruption

### Debug Mode

Enable detailed logging:
```toml
[server]
debug = true  # Shows detailed connection and routing information
```

### Performance Tuning

Environment variables for connection tuning:
```bash
export YAMUX_POOL_SIZE=8              # Connection pool size (default: 32)
export BLACKHOLE_FAILURE_THRESHOLD=5 # Blackhole detection threshold
```

## 📊 Performance

- **Concurrent Connections**: 1000+ simultaneous connections
- **Throughput**: 1GB/s+ (with connection pooling)
- **Memory Usage**: 50MB+ (idle), scales with connections
- **Latency**: Sub-millisecond proxy overhead

## 🔒 Security Considerations

1. **Source IP Spoofing**: Ensure you have permission to use specified source IPs
2. **Network Access**: Configure firewall rules appropriately for control and data ports
3. **Configuration Security**: Protect configuration files from unauthorized access

## 📋 Requirements

- Rust 1.70+
- Supported OS: Linux, Windows, macOS
- Memory: 50MB minimum (scales with connections)
- Network: TCP/IP connectivity

## 📄 License

Dual-licensed under MIT or Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

## 🏗️ Development

```bash
# Build
cargo build --release

# Test
cargo test

# Run comprehensive tests
cargo test --test comprehensive_reverse_proxy_test

# Benchmarks
cargo bench
```

## 🎯 Use Cases

- **Service Exposure**: Expose internal services securely
- **API Gateway**: Single entry point for multiple microservices
- **Load Balancing**: Distribute traffic across multiple instances
- **Development Proxy**: Route traffic between development and production
- **Network Testing**: Simulate different network configurations
- **Service Migration**: Gradual traffic shifting between services