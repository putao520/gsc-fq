#!/usr/bin/env python3
import socket
import threading
import time

def create_hanging_server():
    """Create a server that accepts connections but doesn't send or receive data"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', 9004))
    s.listen(5)
    print("Hanging server listening on port 9004")

    while True:
        conn, addr = s.accept()
        print(f"Hanging server accepted connection from {addr}, doing nothing...")
        # Just keep the connection open but don't do anything
        try:
            # Don't read or write, just keep the connection alive
            while True:
                time.sleep(1)
        except:
            pass
        finally:
            conn.close()

def create_slow_server():
    """Create a server that accepts connections and responds very slowly"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', 9005))
    s.listen(5)
    print("Slow server listening on port 9005")

    while True:
        conn, addr = s.accept()
        print(f"Slow server accepted connection from {addr}")
        try:
            # Wait 30 seconds before reading or responding
            time.sleep(30)
            data = conn.recv(1024)
            if data:
                time.sleep(5)  # Additional delay
                conn.send(b"Slow response")
        except:
            pass
        finally:
            conn.close()

if __name__ == "__main__":
    import sys
    if len(sys.argv) > 1 and sys.argv[1] == "slow":
        print("Starting slow server...")
        create_slow_server()
    else:
        print("Starting hanging server...")
        create_hanging_server()