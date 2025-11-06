#!/usr/bin/env python3
import socket
import time
import sys

def test_proxy_connection():
    print("Testing proxy connection to 127.0.0.1:8080...")

    try:
        # Connect to proxy
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        print("Connecting to proxy...")
        sock.connect(('127.0.0.1', 8080))
        print("Connected to proxy!")

        # Send data
        message = b"Hello from client"
        print(f"Sending: {message}")
        sock.send(message)

        # Wait for response
        print("Waiting for response...")
        response = sock.recv(1024)
        print(f"Received: {response}")

        sock.close()
        print("Connection closed normally")

    except socket.timeout:
        print("ERROR: Connection timed out!")
        return False
    except Exception as e:
        print(f"ERROR: {e}")
        return False

    return True

def test_direct_connection():
    print("\nTesting direct connection to 127.0.0.1:9001...")

    try:
        # Connect directly to echo server
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        print("Connecting directly...")
        sock.connect(('127.0.0.1', 9001))
        print("Connected directly!")

        # Send data
        message = b"Hello from client"
        print(f"Sending: {message}")
        sock.send(message)

        # Wait for response
        print("Waiting for response...")
        response = sock.recv(1024)
        print(f"Received: {response}")

        sock.close()
        print("Direct connection closed normally")

    except socket.timeout:
        print("ERROR: Direct connection timed out!")
        return False
    except Exception as e:
        print(f"ERROR: {e}")
        return False

    return True

if __name__ == "__main__":
    print("🔍 Proxy Debug Test")
    print("==================")

    # Test direct connection first
    direct_ok = test_direct_connection()

    # Test proxy connection
    proxy_ok = test_proxy_connection()

    print(f"\nResults:")
    print(f"Direct connection: {'✅ OK' if direct_ok else '❌ FAILED'}")
    print(f"Proxy connection: {'✅ OK' if proxy_ok else '❌ FAILED'}")