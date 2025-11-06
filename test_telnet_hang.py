#!/usr/bin/env python3
"""
重现 telnet 连接挂起问题
"""
import socket
import subprocess
import time
import threading
import sys

def create_silent_server(port):
    """创建一个接受连接但不主动发送数据的服务器"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', port))
    s.listen(5)
    print(f"[SILENT SERVER] Listening on port {port}")

    def handle_client():
        while True:
            conn, addr = s.accept()
            print(f"[SILENT SERVER] Accepted connection from {addr}")
            # 不发送任何数据，等待客户端先发送
            try:
                while True:
                    data = conn.recv(1024)
                    if not data:
                        break
                    print(f"[SILENT SERVER] Received: {data}")
                    # 简单回显
                    conn.send(data)
            except:
                pass
            finally:
                conn.close()
                print(f"[SILENT SERVER] Connection closed")

    threading.Thread(target=handle_client, daemon=True).start()
    return s

def test_telnet_behavior():
    """测试 telnet 连接行为"""
    print("=" * 60)
    print("Testing telnet connection behavior")
    print("=" * 60)

    # 创建静默服务器
    silent_server = create_silent_server(9001)
    time.sleep(0.5)

    # 创建配置
    with open('test_telnet.toml', 'w') as f:
        f.write("""[server]
bind_ip = "127.0.0.1"
debug = true

[[proxies]]
local_port = 8080
remote_host = "127.0.0.1"
remote_port = 9001
""")

    # 启动代理
    print("\n[PROXY] Starting proxy server...")
    proxy_process = subprocess.Popen(
        ['./target/release/gsc-fq'],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1
    )

    # 等待代理启动
    time.sleep(2)

    # 测试1: telnet 行为
    print("\n[TEST 1] Simulating telnet client behavior...")
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(5)
        s.connect(('127.0.0.1', 8080))
        print("[TEST 1] Connected to proxy")

        # telnet 客户端通常会等待服务器的欢迎消息
        print("[TEST 1] Waiting for server greeting (like telnet)...")
        try:
            data = s.recv(1024)
            if data:
                print(f"[TEST 1] Received: {data}")
            else:
                print("[TEST 1] No data received - connection appears to hang!")
        except socket.timeout:
            print("[TEST 1] Timeout waiting for server data")
        s.close()
    except Exception as e:
        print(f"[TEST 1] Error: {e}")

    # 测试2: 主动发送数据
    print("\n[TEST 2] Sending data first...")
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(5)
        s.connect(('127.0.0.1', 8080))
        print("[TEST 2] Connected to proxy")

        # 主动发送数据
        s.send(b"Hello server\n")
        print("[TEST 2] Sent data, waiting for response...")

        try:
            data = s.recv(1024)
            if data:
                print(f"[TEST 2] Received response: {data}")
            else:
                print("[TEST 2] No response")
        except socket.timeout:
            print("[TEST 2] Timeout waiting for response")
        s.close()
    except Exception as e:
        print(f"[TEST 2] Error: {e}")

    # 清理
    print("\n[CLEANUP] Stopping proxy...")
    proxy_process.terminate()
    proxy_process.wait(timeout=5)
    silent_server.close()

    print("\n" + "=" * 60)
    print("ANALYSIS:")
    print("- If TEST 1 hangs, it confirms the telnet waiting issue")
    print("- If TEST 2 works, it proves proxy works but needs initial data")
    print("- The issue is copy_bidirectional waits for data in both directions")
    print("=" * 60)

if __name__ == "__main__":
    test_telnet_behavior()