# GSC-FQ Tests

This directory contains integration and end-to-end tests for the GSC-FQ proxy system.

## Test Files

### proxy_functionality_test.rs
Tests for forward proxy functionality, including:
- Basic data forwarding
- Multiple message handling

### blackhole_functionality_test.rs
Tests for blackhole server detection functionality.

### reverse_proxy_e2e_test.rs
End-to-end tests for reverse proxy mode, including:
- **test_reverse_proxy_server_client_ping_pong**: Basic reverse proxy functionality with a simple HTTP PING/PONG server
- **test_reverse_proxy_multiple_ports**: Tests multiple reverse proxy ports simultaneously
- **test_reverse_proxy_multiple_connections**: Tests handling multiple concurrent connections
- **test_reverse_proxy_with_port_shorthand**: Tests the port shorthand configuration (using `port` instead of `server_port` and `local_port`)

## Test Infrastructure

The `support/` module provides helper utilities:

### PingPongServer
A simple HTTP server that responds to `/ping` requests with "PONG". Used for testing reverse proxy functionality.

### ReverseProxyServerHandle
Manages the lifecycle of a ReverseProxyServer instance for testing.

### ReverseProxyClientHandle
Manages the lifecycle of a ReverseProxyClient instance for testing.

### ProxyHandle
Manages forward proxy instances for testing.

### TestServer
A simple echo server for testing forward proxy functionality.

## Running Tests

Run all tests:
```bash
cargo test
```

Run only reverse proxy E2E tests:
```bash
cargo test --test reverse_proxy_e2e_test
```

Run with output:
```bash
cargo test --test reverse_proxy_e2e_test -- --nocapture
```

Run a specific test:
```bash
cargo test test_reverse_proxy_server_client_ping_pong -- --nocapture
```

## Test Architecture

The reverse proxy E2E tests follow this pattern:

1. **Setup Phase**
   - Start a local HTTP server (PingPongServer) on a random port
   - Start the ReverseProxyServer on a control port
   - Start the ReverseProxyClient that connects to the server

2. **Execution Phase**
   - Connect to the exposed server port
   - Send HTTP requests
   - Verify responses

3. **Cleanup Phase**
   - Gracefully shutdown all components

## Notes

- All tests use random available ports to avoid conflicts
- Tests use multi-threaded tokio runtime with 4 worker threads
- The PingPongServer is a minimal HTTP/1.1 server for testing purposes only
- Tests include proper error handling and resource cleanup
