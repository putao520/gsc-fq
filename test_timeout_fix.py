#!/usr/bin/env python3
"""
测试代理服务器超时修复效果
"""
import subprocess
import time
import socket
import threading
import sys

def create_hanging_server(port):
    """创建一个接受连接但不发送数据的服务器"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', port))
    s.listen(5)
    print(f"[HANGING] Server listening on port {port}")

    def accept_conn():
        while True:
            conn, addr = s.accept()
            print(f"[HANGING] Accepted connection from {addr}, but will not send data")
            # 不发送任何数据，让客户端挂起

    threading.Thread(target=accept_conn, daemon=True).start()
    return s

def test_proxy_timeout():
    """测试代理超时是否正常工作"""
    print("=" * 60)
    print("Testing proxy timeout fix")
    print("=" * 60)

    # 创建挂起的服务器
    hanging_server = create_hanging_server(9001)
    time.sleep(0.5)

    # 创建测试配置
    with open('test_timeout.toml', 'w') as f:
        f.write("""[server]
bind_ip = "127.0.0.1"
debug = true

[[proxies]]
local_port = 8080
remote_host = "127.0.0.1"
remote_port = 9001
""")

    # 启动代理服务器
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

    # 测试连接
    print("\n[TEST] Connecting to proxy...")
    start_time = time.time()

    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(70)  # 设置客户端超时比代理稍长
        s.connect(('127.0.0.1', 8080))
        print(f"[TEST] Connected to proxy in {time.time() - start_time:.2f} seconds")

        # 发送数据
        s.send(b"Hello proxy")
        print("[TEST] Sent data to proxy")

        # 尝试接收数据（应该会超时）
        try:
            data = s.recv(1024)
            print(f"[TEST] Received: {data}")
        except socket.timeout:
            print(f"[TEST] Client socket timeout after {time.time() - start_time:.2f} seconds")

        s.close()

    except Exception as e:
        print(f"[TEST] Error: {e}")

    elapsed = time.time() - start_time
    print(f"\n[RESULT] Total test time: {elapsed:.2f} seconds")

    # 清理
    print("\n[CLEANUP] Stopping proxy server...")
    proxy_process.terminate()
    proxy_process.wait(timeout=5)
    hanging_server.close()

    # 验证结果
    if 60 <= elapsed <= 65:
        print(f"\n✅ SUCCESS: Proxy timed out correctly after ~60 seconds")
        return True
    elif elapsed < 60:
        print(f"\n❌ FAILED: Proxy timed out too early ({elapsed:.2f} seconds)")
        return False
    else:
        print(f"\n❌ FAILED: Proxy did not timeout within expected time ({elapsed:.2f} seconds)")
        return False

if __name__ == "__main__":
    success = test_proxy_timeout()
    sys.exit(0 if success else 1)