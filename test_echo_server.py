#!/usr/bin/env python3
import socket
import threading
import time

def handle_client(client_socket, addr):
    print(f"[ECHO] New connection from {addr}")
    try:
        while True:
            data = client_socket.recv(1024)
            if not data:
                break
            print(f"[ECHO] Received from {addr}: {data[:50]}...")
            # Echo the data back
            client_socket.send(data)
    except Exception as e:
        print(f"[ECHO] Error with {addr}: {e}")
    finally:
        client_socket.close()
        print(f"[ECHO] Connection closed: {addr}")

def start_echo_server():
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    # Bind to port 9001 (as configured in default.toml)
    server.bind(('127.0.0.1', 9001))
    server.listen(5)
    print("[ECHO] Echo server listening on 127.0.0.1:9001")

    try:
        while True:
            client_socket, addr = server.accept()
            client_thread = threading.Thread(target=handle_client, args=(client_socket, addr))
            client_thread.daemon = True
            client_thread.start()
    except KeyboardInterrupt:
        print("\n[ECHO] Server shutting down")
    finally:
        server.close()

if __name__ == "__main__":
    start_echo_server()