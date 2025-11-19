# Reverse Proxy E2E Test Documentation

## Overview

This document describes the newly added end-to-end (E2E) tests for the reverse proxy functionality in GSC-FQ.

## Test File

**Location**: `tests/reverse_proxy_e2e_test.rs`

## Test Scenarios

### 1. test_reverse_proxy_server_client_ping_pong

**Purpose**: Validates basic reverse proxy functionality with a simple HTTP PING/PONG server.

**Test Flow**:
1. Start a local PingPongServer on a random port
2. Start ReverseProxyServer on a control port
3. Start ReverseProxyClient that connects to the server and registers the local service
4. Connect to the exposed server port
5. Send HTTP GET /ping request
6. Verify response is "200 OK" with body "PONG"
7. Clean up all resources

**What it tests**:
- Reverse proxy server startup and control port binding
- Client connection and handshake with server
- Port allocation and forwarding setup
- Yamux multiplexing over the control connection
- Bidirectional data transfer through the tunnel
- HTTP protocol compatibility

### 2. test_reverse_proxy_multiple_ports

**Purpose**: Tests the ability to expose multiple local services through different ports simultaneously.

**Test Flow**:
1. Start two local PingPongServers on different ports
2. Start ReverseProxyServer
3. Configure client to expose both services with different server ports
4. Connect and verify both services respond correctly

**What it tests**:
- Multiple port handling
- Concurrent port listeners
- Independent data streams per port
- Configuration with multiple [[reverse_proxies]] entries

### 3. test_reverse_proxy_multiple_connections

**Purpose**: Tests handling of multiple concurrent connections to the same exposed port.

**Test Flow**:
1. Start a local PingPongServer
2. Start ReverseProxyServer and ReverseProxyClient
3. Spawn 5 concurrent connections to the exposed port
4. Verify all connections succeed and get correct responses

**What it tests**:
- Concurrent connection handling
- Yamux stream multiplexing
- Resource management under load
- Connection isolation

### 4. test_reverse_proxy_with_port_shorthand

**Purpose**: Tests the simplified port configuration syntax.

**Test Flow**:
1. Use `port = X` configuration (instead of separate server_port and local_port)
2. Verify the port is correctly used for both server and local endpoints

**What it tests**:
- Configuration parsing for the `port` shorthand
- Backward compatibility with simple configurations

## Test Infrastructure

### PingPongServer

A minimal HTTP/1.1 server for testing:

```rust
pub struct PingPongServer {
    addr: SocketAddr,
    handle: Option<JoinHandle<()>>,
}
```

**Features**:
- Binds to a random available port
- Responds to `GET /ping` with "HTTP/1.1 200 OK\r\n...\r\nPONG"
- Returns 404 for other paths
- Proper HTTP header parsing
- Graceful shutdown support

### ReverseProxyServerHandle

Manages the lifecycle of a ReverseProxyServer instance:

```rust
pub struct ReverseProxyServerHandle {
    control_port: u16,
    handle: Option<JoinHandle<()>>,
}
```

**Features**:
- Spawns server in a tokio task
- Waits for control port to be ready
- Provides graceful shutdown
- Automatic cleanup on drop

### ReverseProxyClientHandle

Manages the lifecycle of a ReverseProxyClient instance:

```rust
pub struct ReverseProxyClientHandle {
    handle: Option<JoinHandle<()>>,
}
```

**Features**:
- Accepts reverse proxy configuration
- Creates ConfigFile with proper structure
- Spawns client in a tokio task
- Waits for connection establishment
- Provides graceful shutdown
- Automatic cleanup on drop

## Running the Tests

Run all reverse proxy E2E tests:
```bash
cargo test --test reverse_proxy_e2e_test
```

Run with detailed output:
```bash
cargo test --test reverse_proxy_e2e_test -- --nocapture
```

Run a specific test:
```bash
cargo test test_reverse_proxy_server_client_ping_pong -- --nocapture
```

## Test Configuration

All tests use:
- Multi-threaded tokio runtime with 4 worker threads
- Random available ports to avoid conflicts
- 5-second timeout for port readiness checks
- Proper error handling with anyhow::Result
- Automatic resource cleanup

## Expected Output

Successful test output example:
```
Local HTTP server started on port 54321
Control port: 54320, Server port: 54322
Reverse proxy server started
Reverse proxy client started and connected
Server port 54322 is ready
Connected to server port 54322
Sent HTTP request
Response status: HTTP/1.1 200 OK
Response body: PONG
Test completed successfully
```

## Architecture

The tests validate this architecture:

```
[External Client] 
       ↓
[ReverseProxyServer:server_port] 
       ↓ (yamux over control connection)
[ReverseProxyClient]
       ↓
[Local Service (PingPongServer):local_port]
```

## Integration Points Tested

1. **Protocol Handshake**: ClientHello/ServerHello exchange
2. **Port Allocation**: Server allocates and binds requested ports
3. **Yamux Multiplexing**: Multiple streams over single control connection
4. **Data Forwarding**: Bidirectional data transfer through the tunnel
5. **Error Handling**: Proper cleanup on errors
6. **Configuration**: Both shorthand and full port specifications

## Notes

- Tests are isolated and can run in parallel
- Each test uses unique ports to avoid conflicts
- All resources are properly cleaned up after each test
- Tests include extensive logging for debugging
- The PingPongServer is a minimal HTTP implementation for testing only

## Future Enhancements

Potential additions to the test suite:
- Large data transfer tests
- Connection timeout tests
- Server/client reconnection tests
- Multiple clients connecting to same server
- Performance benchmarks
- Error injection tests
