#!/usr/bin/env python3
"""
Simple timeout test for GSC-FQ proxy
"""
import socket
import time
import threading
import sys
import os

def slow_echo_server(port, delay_ms=100):
    """Echo server with configurable delay"""
    try:
        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(('127.0.0.1', port))
        server.listen(5)
        print(f"Slow echo server started on port {port} with {delay_ms}ms delay")

        while True:
            conn, addr = server.accept()
            print(f"Slow echo server received connection from {addr}")

            def handle_connection(conn, addr):
                try:
                    while True:
                        data = conn.recv(1024)
                        if not data:
                            break
                        print(f"Slow echo server received {len(data)} bytes from {addr}, delaying {delay_ms}ms")
                        time.sleep(delay_ms / 1000.0)  # Add delay
                        conn.send(data)
                except Exception as e:
                    print(f"Slow echo server error for {addr}: {e}")
                finally:
                    conn.close()
                    print(f"Slow echo server closed connection from {addr}")

            thread = threading.Thread(target=handle_connection, args=(conn, addr))
            thread.daemon = True
            thread.start()

    except Exception as e:
        print(f"Slow echo server error on port {port}: {e}")

def test_timeout_scenarios():
    """Test various timeout scenarios"""
    proxy_port = 9080
    echo_port = 9001

    print("GSC-FQ Timeout Analysis Test")
    print("============================")

    # Start slow echo server
    echo_thread = threading.Thread(target=slow_echo_server, args=(echo_port, 200))  # 200ms delay
    echo_thread.daemon = True
    echo_thread.start()

    time.sleep(1)  # Let server start

    # Test 1: Normal connection timing
    print("\n1. Testing normal connection timing...")
    try:
        start_time = time.time()
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect(('127.0.0.1', proxy_port))
        connect_time = time.time() - start_time
        print(f"   Proxy connect time: {connect_time:.3f}s")

        # Test data round-trip
        start_time = time.time()
        sock.send(b"Hello")
        response = sock.recv(1024)
        roundtrip_time = time.time() - start_time
        print(f"   Data round-trip time: {roundtrip_time:.3f}s")
        print(f"   Response: {response}")
        sock.close()

    except Exception as e:
        print(f"   Error: {e}")

    # Test 2: Multiple concurrent connections
    print("\n2. Testing concurrent connections...")
    def concurrent_test(i):
        try:
            start_time = time.time()
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(3.0)
            sock.connect(('127.0.0.1', proxy_port))
            connect_time = time.time() - start_time

            message = f"Concurrent test {i}".encode()
            sock.send(message)
            response = sock.recv(1024)
            total_time = time.time() - start_time

            sock.close()
            return i, connect_time, total_time, response.decode()
        except Exception as e:
            return i, None, None, str(e)

    threads = []
    results = []
    for i in range(5):
        t = threading.Thread(target=lambda i=i: results.append(concurrent_test(i)))
        t.start()
        threads.append(t)

    for t in threads:
        t.join()

    for i, connect_time, total_time, result in results:
        if connect_time is not None:
            print(f"   Connection {i}: connect={connect_time:.3f}s, total={total_time:.3f}s")
        else:
            print(f"   Connection {i}: FAILED - {result}")

    # Test 3: Connection that sends but doesn't read
    print("\n3. Testing write-only connection behavior...")
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(2.0)
        sock.connect(('127.0.0.1', proxy_port))
        sock.send(b"Write only test")
        sock.flush()
        print("   Sent data, not reading response...")

        # Wait to see what happens
        time.sleep(1)

        # Try to read something
        try:
            response = sock.recv(1024)
            print(f"   Received: {response}")
        except socket.timeout:
            print("   Timeout when trying to read - this is expected behavior")

        sock.close()

    except Exception as e:
        print(f"   Write-only test error: {e}")

    # Test 4: Large data transfer
    print("\n4. Testing large data transfer...")
    try:
        large_data = b'A' * (64 * 1024)  # 64KB
        start_time = time.time()

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10.0)
        sock.connect(('127.0.0.1', proxy_port))

        sock.send(large_data)
        response = sock.recv(len(large_data))

        total_time = time.time() - start_time
        print(f"   64KB transfer in {total_time:.3f}s ({len(large_data)/total_time/1024:.1f} KB/s)")
        sock.close()

    except Exception as e:
        print(f"   Large data transfer error: {e}")

if __name__ == "__main__":
    # Start proxy server
    print("Starting proxy server...")
    proxy_process = os.popen("timeout 30s cargo run -- --config timeout_test.toml 2>&1", "r")
    time.sleep(3)  # Let proxy start

    try:
        test_timeout_scenarios()
    finally:
        proxy_process.close()
        print("\nTest completed")