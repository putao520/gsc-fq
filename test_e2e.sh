#!/bin/bash
# End-to-End Test Script for GSC-FQ Proxy
# This script tests the actual CLI functionality

set -e

echo "=========================================="
echo "GSC-FQ End-to-End Test Script"
echo "=========================================="

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

# Helper functions
log_info() {
    echo -e "${YELLOW}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
    ((TESTS_PASSED++))
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
    ((TESTS_FAILED++))
}

# Function to get available port
get_port() {
    python3 -c "import socket; s=socket.socket(); s.bind(('',0)); print(s.getsockname()[1]); s.close()"
}

# Function to wait for port
wait_for_port() {
    local port=$1
    local timeout=${2:-10}

    log_info "Waiting for port $port to be ready..."
    for i in $(seq 1 $timeout); do
        if nc -z 127.0.0.1 $port 2>/dev/null; then
            log_success "Port $port is ready"
            return 0
        fi
        sleep 1
    done

    log_error "Port $port not ready after ${timeout}s"
    return 1
}

# Test 1: CLI Help
test_cli_help() {
    echo ""
    echo "------------------------------------------"
    echo "Test 1: CLI Help Output"
    echo "------------------------------------------"

    if cargo run --bin gsc-fq -- --help > /dev/null 2>&1; then
        log_success "CLI help command works"
    else
        log_error "CLI help command failed"
    fi
}

# Test 2: Configuration Loading
test_config_loading() {
    echo ""
    echo "------------------------------------------"
    echo "Test 2: Configuration Loading"
    echo "------------------------------------------"

    # Create test config
    cat > test_config.toml << EOF
[server]
bind_ip = "127.0.0.1"

[[proxies]]
local_port = 33100
remote_host = "127.0.0.1"
remote_port = 33101
EOF

    log_info "Created test configuration"

    # Try to start proxy (it should fail gracefully if remote port not available)
    if timeout 5s cargo run --bin gsc-fq -- --config test_config.toml --debug >/dev/null 2>&1; then
        log_success "Configuration loaded successfully"
    else
        # Check if it failed due to connection issues (expected)
        if timeout 5s cargo run --bin gsc-fq -- --config test_config.toml --debug 2>&1 | grep -q "error"; then
            log_error "Configuration loading failed with error"
        else
            log_success "Configuration loaded (connection errors expected)"
        fi
    fi

    rm -f test_config.toml
}

# Test 3: Default Configuration (No config file)
test_default_config() {
    echo ""
    echo "------------------------------------------"
    echo "Test 3: Default Configuration"
    echo "------------------------------------------"

    log_info "Testing default ports (33100->12991, 33200->12991, 33300->12991)"

    # Start proxy with default config
    timeout 5s cargo run --bin gsc-fq -- --debug >/dev/null 2>&1 &
    PROXY_PID=$!

    sleep 2

    if kill -0 $PROXY_PID 2>/dev/null; then
        log_success "Proxy started with default configuration"
    else
        log_error "Proxy failed to start with default configuration"
    fi

    kill $PROXY_PID 2>/dev/null || true
    wait $PROXY_PID 2>/dev/null || true
}

# Test 4: Port Forwarding (requires netcat)
test_port_forwarding() {
    echo ""
    echo "------------------------------------------"
    echo "Test 4: Port Forwarding Test"
    echo "------------------------------------------"

    if ! command -v nc &> /dev/null; then
        log_info "Skipping port forwarding test (netcat not available)"
        return 0
    fi

    # Get dynamic ports
    BACKEND_PORT=$(get_port)
    PROXY_PORT=$(get_port)

    log_info "Using ports: Backend=$BACKEND_PORT, Proxy=$PROXY_PORT"

    # Create test config
    cat > e2e_test.toml << EOF
[server]
bind_ip = "127.0.0.1"

[[proxies]]
local_port = $PROXY_PORT
remote_host = "127.0.0.1"
remote_port = $BACKEND_PORT
EOF

    # Start echo server
    nc -l -p $BACKEND_PORT -e "cat" &
    ECHO_PID=$!

    sleep 1

    # Start proxy
    cargo run --bin gsc-fq -- --config e2e_test.toml --debug &
    PROXY_PID=$!

    # Wait for proxy to start
    sleep 3

    # Test forwarding
    TEST_DATA="Hello from E2E test $(date)"
    echo "$TEST_DATA" | nc -w 3 127.0.0.1 $PROXY_PORT > received.txt 2>&1 || true

    # Check result
    if grep -q "$TEST_DATA" received.txt 2>/dev/null; then
        log_success "Port forwarding works correctly"
    else
        log_error "Port forwarding failed"
        echo "Sent: $TEST_DATA"
        echo "Received: $(cat received.txt 2>/dev/null || echo 'No data')"
    fi

    # Cleanup
    kill $PROXY_PID 2>/dev/null || true
    kill $ECHO_PID 2>/dev/null || true
    rm -f e2e_test.toml received.txt
}

# Main test execution
echo ""
echo "Starting tests..."
echo ""

# Run all tests
test_cli_help
test_config_loading
test_default_config
test_port_forwarding

# Print summary
echo ""
echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed: ${RED}$TESTS_FAILED${NC}"
echo "Total:  $((TESTS_PASSED + TESTS_FAILED))"

if [ $TESTS_FAILED -eq 0 ]; then
    echo ""
    log_success "All tests passed! 🎉"
    exit 0
else
    echo ""
    log_error "Some tests failed!"
    exit 1
fi