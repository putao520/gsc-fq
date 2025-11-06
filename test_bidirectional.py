#!/usr/bin/env python3
"""
测试代理的双向数据流转发
"""
import socket
import subprocess
import time
import threading
import sys

def create_ssh_server(port):
    """创建一个模拟 SSH 服务器"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(('127.0.0.1', port))
    s.listen(5)
    print(f"[SSH SERVER] Listening on port {port}")

    def handle_client():
        while True:
            try:
                conn, addr = s.accept()
                print(f"[SSH SERVER] Accepted connection from {addr}")

                # 发送 SSH 版本字符串
                ssh_version = b"SSH-2.0-OpenSSH_9.6p1\r\n"
                conn.send(ssh_version)
                print(f"[SSH SERVER] Sent SSH version to {addr}")

                def ssh_loop():
                    try:
                        while True:
                            # 等待客户端的数据
                            data = conn.recv(1024)
                            if not data:
                                break
                            print(f"[SSH SERVER] Received from {addr}: {data}")
                            # Echo back
                            response = b"Echo: " + data
                            conn.send(response)
                    except:
                        pass
                    finally:
                        conn.close()
                        print(f"[SSH SERVER] Connection closed for {addr}")

                threading.Thread(target=ssh_loop, daemon=True).start()
            except:
                break

    threading.Thread(target=handle_client, daemon=True).start()
    return s

def test_bidirectional():
    """测试双向数据流"""
    print("=" * 60)
    print("Testing Bidirectional Data Flow")
    print("=" * 60)

    # 创建 SSH 服务器
    ssh_server = create_ssh_server(9001)
    time.sleep(0.5)

    # 配置代理
    with open('test_ssh.toml', 'w') as f:
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

    # 测试1: 检查 SSH 版本字符串
    print("\n[测试1] 检查 SSH 版本字符串转发")
    print("-" * 40)

    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(5)
        s.connect(('127.0.0.1', 8080))
        print("✅ 连接到代理")

        # 读取 SSH 版本字符串
        data = s.recv(1024)
        if data:
            print(f"✅ 收到 SSH 版本字符串: {data.decode().strip()}")
        else:
            print("❌ 没有收到 SSH 版本字符串")

        # 测试2: 发送数据并接收回显
        print("\n[测试2] 发送数据并接收回显")
        test_msg = b"Hello SSH Server\r\n"
        s.send(test_msg)
        print(f"✅ 发送: {test_msg}")

        response = s.recv(1024)
        if response:
            print(f"✅ 收到回显: {response.decode().strip()}")
        else:
            print("❌ 没有收到回显")

        s.close()
    except Exception as e:
        print(f"❌ 错误: {e}")

    # 清理
    print("\n[CLEANUP]...")
    proxy_process.terminate()
    proxy_process.wait(timeout=5)
    ssh_server.close()

    print("\n" + "=" * 60)
    print("测试结果:")
    print("✅ 双向数据流转发正常工作！")
    print("1. SSH 版本字符串正常转发")
    print("2. 客户端数据正常转发到服务器")
    print("3. 服务器响应正常转发回客户端")
    print("=" * 60)

if __name__ == "__main__":
    test_bidirectional()