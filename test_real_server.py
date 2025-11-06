#!/usr/bin/env python3
"""
创建一个真实的模拟服务器来测试代理
"""
import socket
import threading
import time

def create_echo_server(port):
    """创建一个标准的 echo 服务器"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', port))
    s.listen(5)
    print(f"[ECHO SERVER] Listening on port {port}")

    def handle_client():
        while True:
            conn, addr = s.accept()
            print(f"[ECHO SERVER] Accepted connection from {addr}")

            # 发送欢迎消息（模拟 telnet 服务器）
            welcome_msg = b"Welcome to Echo Server\r\n"
            conn.send(welcome_msg)
            print(f"[ECHO SERVER] Sent welcome message to {addr}")

            def echo_loop():
                try:
                    while True:
                        data = conn.recv(1024)
                        if not data:
                            break
                        print(f"[ECHO SERVER] Received from {addr}: {data}")
                        # Echo back with prefix
                        response = b"Echo: " + data
                        conn.send(response)
                except:
                    pass
                finally:
                    conn.close()
                    print(f"[ECHO SERVER] Connection closed for {addr}")

            threading.Thread(target=echo_loop, daemon=True).start()

    threading.Thread(target=handle_client, daemon=True).start()
    return s

def test_with_real_server():
    """测试与真实服务器的代理连接"""
    print("=" * 60)
    print("Testing proxy with real echo server")
    print("=" * 60)

    # 创建 echo 服务器
    echo_server = create_echo_server(9001)
    time.sleep(0.5)

    # 创建配置
    with open('test_real.toml', 'w') as f:
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
    import subprocess
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
    print("\n[TEST] Connecting to proxy via telnet-like client...")
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(10)
        s.connect(('127.0.0.1', 8080))
        print("[TEST] Connected to proxy")

        # 尝试接收欢迎消息
        try:
            data = s.recv(1024)
            if data:
                print(f"[TEST] Welcome message: {data}")
        except socket.timeout:
            print("[TEST] No welcome message received")

        # 发送测试数据
        test_msg = b"Hello World\r\n"
        s.send(test_msg)
        print(f"[TEST] Sent: {test_msg}")

        # 接收回显
        try:
            response = s.recv(1024)
            if response:
                print(f"[TEST] Response: {response}")
        except socket.timeout:
            print("[TEST] No response received")

        s.close()
    except Exception as e:
        print(f"[TEST] Error: {e}")

    # 清理
    print("\n[CLEANUP] Stopping proxy and server...")
    proxy_process.terminate()
    proxy_process.wait(timeout=5)
    echo_server.close()

    print("\n✅ Test completed!")

if __name__ == "__main__":
    test_with_real_server()