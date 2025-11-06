#!/usr/bin/env python3
"""
Simple test script to analyze GSC-FQ proxy server connection issues
"""
import socket
import time
import threading
import sys

def echo_server(port):
    """Simple echo server for testing"""
    try:
        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(('127.0.0.1', port))
        server.listen(5)
        print(f"Echo server started on port {port}")

        while True:
            conn, addr = server.accept()
            print(f"Echo server received connection from {addr}")

            def handle_connection(conn, addr):
                try:
                    while True:
                        data = conn.recv(1024)
                        if not data:
                            break
                        print(f"Echo server received {len(data)} bytes from {addr}: {data}")
                        conn.send(data)
                except Exception as e:
                    print(f"Echo server error for {addr}: {e}")
                finally:
                    conn.close()
                    print(f"Echo server closed connection from {addr}")

            thread = threading.Thread(target=handle_connection, args=(conn, addr))
            thread.daemon = True
            thread.start()

    except Exception as e:
        print(f"Echo server error on port {port}: {e}")

def test_proxy_connection(proxy_port, test_message="Hello Proxy"):
    """Test connection through proxy"""
    try:
        print(f"Testing connection to proxy port {proxy_port}")

        # Connect to proxy
        proxy_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        proxy_socket.settimeout(10.0)  # 10 second timeout

        print(f"Attempting to connect to 127.0.0.1:{proxy_port}")
        start_time = time.time()
        proxy_socket.connect(('127.0.0.1', proxy_port))
        connect_time = time.time() - start_time
        print(f"Connected to proxy in {connect_time:.3f} seconds")

        # Send test message
        print(f"Sending message: {test_message}")
        start_time = time.time()
        proxy_socket.send(test_message.encode())

        # Receive response
        response = proxy_socket.recv(1024)
        receive_time = time.time() - start_time
        print(f"Received response in {receive_time:.3f} seconds: {response}")

        proxy_socket.close()
        print("Test completed successfully")
        return True

    except socket.timeout as e:
        print(f"TIMEOUT: {e}")
        return False
    except ConnectionRefusedError as e:
        print(f"CONNECTION REFUSED: {e}")
        return False
    except Exception as e:
        print(f"ERROR: {e}")
        return False

def main():
    if len(sys.argv) < 3:
        print("Usage: python test_connection.py <proxy_port> <echo_port>")
        print("Example: python test_connection.py 8080 9001")
        sys.exit(1)

    proxy_port = int(sys.argv[1])
    echo_port = int(sys.argv[2])

    print(f"Testing GSC-FQ Proxy Server")
    print(f"Proxy: 127.0.0.1:{proxy_port}")
    print(f"Echo: 127.0.0.1:{echo_port}")
    print("=" * 50)

    # Start echo server in background
    echo_thread = threading.Thread(target=echo_server, args=(echo_port,))
    echo_thread.daemon = True
    echo_thread.start()

    # Give echo server time to start
    time.sleep(1)

    # Test proxy connection
    success = test_proxy_connection(proxy_port)

    if success:
        print("\n✅ Proxy test PASSED")
    else:
        print("\n❌ Proxy test FAILED")

        # Additional diagnostics
        print("\nRunning diagnostics...")

        # Test direct connection to echo server
        print("1. Testing direct connection to echo server...")
        try:
            direct_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            direct_socket.settimeout(5.0)
            direct_socket.connect(('127.0.0.1', echo_port))
            direct_socket.send(b"Direct test")
            response = direct_socket.recv(1024)
            print(f"   Direct connection successful: {response}")
            direct_socket.close()
        except Exception as e:
            print(f"   Direct connection failed: {e}")

        # Test proxy port availability
        print("2. Testing proxy port availability...")
        try:
            test_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            test_socket.settimeout(5.0)
            test_socket.connect(('127.0.0.1', proxy_port))
            test_socket.close()
            print("   Proxy port is accessible")
        except ConnectionRefusedError:
            print("   Proxy port not accessible - Connection refused")
        except Exception as e:
            print(f"   Proxy port test failed: {e}")

if __name__ == "__main__":
    main()