#!/usr/bin/env python3
import socket
import threading
import time
import sys

def test_proxy_hanging():
    """Test connection that might hang when using the proxy"""
    print("Testing proxy connection behavior...")

    # Test 1: Connect via proxy and send data immediately
    print("\n1. Testing: Connect, send data immediately")
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect(('127.0.0.1', 8082))
        print("   Connected to proxy")

        # Send data immediately
        sock.send(b"Hello from client")
        print("   Sent data, waiting for response...")

        # This might hang
        response = sock.recv(1024)
        print(f"   Got response: {response}")

        sock.close()
        print("   Connection closed normally")

    except socket.timeout:
        print("   TIMEOUT: No response received within 5 seconds")
    except Exception as e:
        print(f"   ERROR: {e}")

    # Test 2: Connect and wait before sending data
    print("\n2. Testing: Connect, wait, then send data")
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10.0)
        sock.connect(('127.0.0.1', 8082))
        print("   Connected to proxy")

        # Wait before sending data
        time.sleep(2)
        print("   Waited 2 seconds, now sending data...")

        sock.send(b"Delayed hello")
        print("   Sent data, waiting for response...")

        # This might hang
        response = sock.recv(1024)
        print(f"   Got response: {response}")

        sock.close()
        print("   Connection closed normally")

    except socket.timeout:
        print("   TIMEOUT: No response received within 10 seconds")
    except Exception as e:
        print(f"   ERROR: {e}")

    # Test 3: Connect and never send data (just wait)
    print("\n3. Testing: Connect and wait without sending data")
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(8.0)
        sock.connect(('127.0.0.1', 8082))
        print("   Connected to proxy")

        # Just wait without sending anything
        print("   Waiting 8 seconds without sending anything...")
        time.sleep(8)

        # Try to send something after waiting
        sock.send(b"After waiting")
        print("   Sent data after waiting")

        response = sock.recv(1024)
        print(f"   Got response: {response}")

        sock.close()
        print("   Connection closed normally")

    except socket.timeout:
        print("   TIMEOUT: Connection timed out")
    except Exception as e:
        print(f"   ERROR: {e}")

    # Test 4: Connect and try to read immediately
    print("\n4. Testing: Connect and try to read immediately")
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect(('127.0.0.1', 8082))
        print("   Connected to proxy")

        # Try to read immediately without sending anything
        print("   Trying to read immediately without sending...")
        response = sock.recv(1024)
        print(f"   Got response: {response}")

        sock.close()
        print("   Connection closed normally")

    except socket.timeout:
        print("   TIMEOUT: No data available to read")
    except Exception as e:
        print(f"   ERROR: {e}")

if __name__ == "__main__":
    print("🔍 Testing Proxy Hanging Scenarios")
    print("=================================")
    test_proxy_hanging()