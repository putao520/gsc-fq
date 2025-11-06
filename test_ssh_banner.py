#!/usr/bin/env python3
"""
测试 SSH 服务器版本字符串转发
"""
import socket
import subprocess
import time
import threading

def test_ssh_banner():
    """测试 SSH 服务器版本字符串是否正确转发"""
    print("=" * 60)
    print("Testing SSH Banner Forwarding")
    print("=" * 60)

    # 测试1: 直接连接 SSH 服务器
    print("\n[测试1] 直接连接 SSH 服务器 (端口 22)")
    print("-" * 40)

    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(3)
        s.connect(('127.0.0.1', 22))

        # 读取 SSH 版本字符串
        data = s.recv(1024)
        if data:
            print(f"✅ SSH 服务器发送: {data.decode().strip()}")
        else:
            print("❌ SSH 服务器没有发送数据")
        s.close()
    except Exception as e:
        print(f"❌ 连接失败: {e}")

    # 测试2: 通过代理连接 SSH 服务器
    print("\n[测试2] 通过代理连接 SSH 服务器")
    print("-" * 40)

    # 配置代理指向 SSH
    with open('test_ssh.toml', 'w') as f:
        f.write("""[server]
bind_ip = "127.0.0.1"
debug = true

[[proxies]]
local_port = 8080
remote_host = "127.0.0.1"
remote_port = 22
""")

    # 启动代理
    proxy_process = subprocess.Popen(
        ['./target/release/gsc-fq'],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1
    )

    # 等待代理启动
    time.sleep(3)

    # 测试通过代理连接
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(5)
        s.connect(('127.0.0.1', 8080))
        print("✅ 通过代理连接成功")

        # 尝试读取 SSH 版本字符串
        print("等待 SSH 版本字符串...")
        data = s.recv(1024)
        if data:
            print(f"✅ 收到: {data.decode().strip()}")
        else:
            print("❌ 没有收到任何数据 - 这就是问题所在！")

        s.close()
    except Exception as e:
        print(f"❌ 错误: {e}")

    # 清理
    print("\n清理...")
    proxy_process.terminate()
    proxy_process.wait(timeout=5)

    print("\n" + "=" * 60)
    print("结论:")
    print("如果测试1收到数据但测试2没收到，说明代理没有正确转发")
    print("SSH 服务器的初始版本字符串没有被转发给客户端")
    print("=" * 60)

if __name__ == "__main__":
    test_ssh_banner()