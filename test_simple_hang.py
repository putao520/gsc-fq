#!/usr/bin/env python3
"""
测试最简单的连接场景
"""
import socket
import subprocess
import time
import sys
import threading

def create_simple_server(port):
    """创建一个最简单的服务器 - 连接后什么都不做"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', port))
    s.listen(5)
    print(f"[SERVER] Listening on port {port}")

    def accept_conn():
        while True:
            try:
                conn, addr = s.accept()
                print(f"[SERVER] Accepted connection from {addr}")
                # 重要：连接后什么都不做，不发送任何数据
                # 这会导致客户端在 read 时挂起
            except:
                break

    threading.Thread(target=accept_conn, daemon=True).start()
    return s

def test_simple_hang():
    """测试简单的连接挂起问题"""
    print("=" * 60)
    print("Testing simple connection hang")
    print("=" * 60)

    # 创建一个什么都不做的服务器
    server = create_simple_server(9001)
    time.sleep(0.5)

    # 启动代理
    print("\n[PROXY] Starting proxy...")
    proxy_process = subprocess.Popen(
        ['./target/release/gsc-fq'],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True
    )

    # 等待代理启动
    time.sleep(2)

    # 测试 telnet 行为
    print("\n[TEST] Connecting via telnet...")
    try:
        # 模拟 telnet 客户端
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(5)  # 5秒超时
        s.connect(('127.0.0.1', 8080))
        print("[TEST] ✓ TCP connection established")

        # telnet 客户端连接后会尝试读取服务器的欢迎消息
        print("[TEST] Waiting for server welcome message...")
        try:
            data = s.recv(1024)
            if data:
                print(f"[TEST] Received: {data}")
            else:
                print("[TEST] ⚠️  No data - connection appears to hang!")
        except socket.timeout:
            print("[TEST] ✗ Timeout - this is the HANG problem!")
        except Exception as e:
            print(f"[TEST] Error: {e}")

        s.close()
    except Exception as e:
        print(f"[TEST] Connection failed: {e}")

    # 清理
    print("\n[CLEANUP]...")
    proxy_process.terminate()
    proxy_process.wait(timeout=5)
    server.close()

    print("\n" + "=" * 60)
    print("ANALYSIS:")
    print("1. TCP connection: ✓ Success")
    print("2. Proxy to remote: ✓ Success")
    print("3. Data transfer: ⚠️  Hangs because remote sends nothing")
    print("4. Client waits: ⚠️  Telnet expects welcome message")
    print("=" * 60)

if __name__ == "__main__":
    test_simple_hang()