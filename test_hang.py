#!/usr/bin/env python3
import socket
import time
import sys

def test_telnet_like_connection():
    print("Testing telnet-like connection to 127.0.0.1:8080...")

    try:
        # Create socket
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)  # 5 second timeout

        print("Connecting...")
        sock.connect(('127.0.0.1', 8080))
        print("Connected!")

        # Check if socket is readable
        print("Checking if data is available...")
        ready = False
        for i in range(10):
            try:
                data = sock.recv(1, socket.MSG_PEEK | socket.MSG_DONTWAIT)
                if data:
                    print(f"Data available: {data}")
                    ready = True
                    break
                else:
                    print(f"No data yet, attempt {i+1}")
                    time.sleep(0.1)
            except socket.error as e:
                if e.errno == socket.EAGAIN or e.errno == socket.EWOULDBLOCK:
                    print(f"No data yet (would block), attempt {i+1}")
                    time.sleep(0.1)
                else:
                    print(f"Socket error: {e}")
                    break

        if not ready:
            print("No data received after waiting")
            # Try to send something
            print("Sending test data...")
            sock.send(b"test\n")

            # Wait for response
            print("Waiting for response...")
            for i in range(10):
                try:
                    data = sock.recv(1024)
                    if data:
                        print(f"Received: {data}")
                        break
                    else:
                        print(f"No response yet, attempt {i+1}")
                        time.sleep(0.1)
                except socket.timeout:
                    print("Timeout waiting for response")
                    break
                except socket.error as e:
                    print(f"Socket error while receiving: {e}")
                    break

        sock.close()
        print("Connection closed")

    except socket.timeout:
        print("TIMEOUT: Connection or operation timed out")
        return False
    except Exception as e:
        print(f"ERROR: {e}")
        return False

    return True

def test_persistent_connection():
    print("\nTesting persistent connection (simulating telnet session)...")

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)  # Longer timeout for persistent connection

        print("Connecting...")
        sock.connect(('127.0.0.1', 8080))
        print("Connected!")

        # Wait a bit to see if connection "hangs"
        print("Waiting 2 seconds to see if connection hangs...")
        time.sleep(2)

        # Try to send data
        print("Sending: hello")
        sock.send(b"hello\n")

        # Try to receive response
        print("Waiting for response...")
        try:
            data = sock.recv(1024)
            if data:
                print(f"Received: {data}")
            else:
                print("No data received")
        except socket.timeout:
            print("Timeout waiting for response")

        # Send another message
        print("Sending: world")
        sock.send(b"world\n")

        # Try to receive response again
        print("Waiting for second response...")
        try:
            data = sock.recv(1024)
            if data:
                print(f"Received: {data}")
            else:
                print("No data received for second message")
        except socket.timeout:
            print("Timeout waiting for second response")

        sock.close()
        print("Connection closed")
        return True

    except Exception as e:
        print(f"ERROR in persistent connection: {e}")
        return False

if __name__ == "__main__":
    print("Testing GSC-FQ proxy connection behavior...")
    print("=" * 50)

    success1 = test_telnet_like_connection()
    success2 = test_persistent_connection()

    print("\n" + "=" * 50)
    print("Test Results:")
    print(f"Telnet-like test: {'PASSED' if success1 else 'FAILED'}")
    print(f"Persistent connection test: {'PASSED' if success2 else 'FAILED'}")

    if not (success1 and success2):
        sys.exit(1)