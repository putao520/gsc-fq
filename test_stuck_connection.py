#!/usr/bin/env python3
import socket
import threading
import time

def create_silent_server():
    """Create a server that accepts connections but doesn't send any data"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', 9003))
    s.listen(5)
    print("Silent server listening on port 9003")

    while True:
        conn, addr = s.accept()
        print(f"Silent server accepted connection from {addr}")
        # Just keep the connection open but don't send anything
        try:
            # Just read data but never respond
            while True:
                data = conn.recv(1024)
                if not data:
                    break
                print(f"Silent server received: {data}")
                # Don't send any response
        except:
            pass
        finally:
            conn.close()

def test_connection_hang():
    print("Testing connection that might hang...")

    try:
        # Connect to the silent server via proxy
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10.0)  # 10 second timeout
        print("Connecting to silent server via proxy...")
        sock.connect(('127.0.0.1', 8080))
        print("Connected to proxy!")

        # Try to send some data
        message = b"Hello to silent server"
        print(f"Sending: {message}")
        sock.send(message)

        # Try to read response (this might hang)
        print("Waiting for response (this might hang)...")
        response = sock.recv(1024)
        print(f"Unexpected response: {response}")

        sock.close()

    except socket.timeout:
        print("Connection timed out as expected")
        return True
    except Exception as e:
        print(f"Error: {e}")
        return False

    return False

if __name__ == "__main__":
    print("🔍 Testing Connection Hang Scenario")
    print("==================================")

    # Start silent server in background
    import subprocess
    import sys

    # First, let's test our proxy with a port that has no server
    print("\n1. Testing connection to non-existent server (port 9999)...")
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect(('127.0.0.1', 8080))
        print("Connected to proxy")
        sock.send(b"test")
        print("Sent data, waiting for response...")
        response = sock.recv(1024)
        print(f"Got response: {response}")
    except Exception as e:
        print(f"Error as expected: {e}")

    # Test with a different proxy configuration