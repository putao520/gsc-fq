# GSC-FQ

高性能 Rust 编写的代理与隐秘隧道工具。支持正向代理、反向代理、TCP/UDP 流量转发，以及基于 Token 和 TOTP 的双重身份验证。

## 🌟 核心功能
- **正向代理**：本地端口转发至远程服务（Jump Box 模式）。
- **反向代理**：通过隐秘隧道将内网服务暴露至公网。
- **混合模式**：单进程同时运行正向和反向代理。
- **UDP over TCP**：稳定转发 UDP 流量。
- **安全加固**：基于 Yamux 多路复用，支持 Token 及 TOTP (Google Authenticator) 动态验证。

## 🚀 快速开始

### 安装
```bash
cargo install gsc-fq
```

### 1. 正向代理 (Forward Proxy)
将本地 8080 端口转发到远程 API：
```toml
# config.toml
[[proxies]]
local = "8080"
remote = "api.example.com:443"
```

### 2. 反向代理 (Reverse Proxy)
**服务端 (公网机):**
```toml
[reverse_proxy_server]
port = 9001  # 控制连接端口
allowed_tokens = ["my-secret-token"]
```

**客户端 (内网机):**
```toml
[reverse_proxy_client]
server = "公网IP:9001"
token = "my-secret-token"

[[reverse_proxies]]
server = "443"           # 公网机暴露的端口
local = "127.0.0.1:3000" # 本地待暴露的服务
```

### 3. TOTP 动态验证
生成密钥：
```bash
gsc-fq -g
# 输出: Secret: JBSWY3DPEHPK3PXP...
```
配置服务端开启 TOTP：
```toml
[reverse_proxy_server]
port = 9001
totp_secret = "JBSWY3DPEHPK3PXP" # 填入生成的密钥
```

## 🛠️ 命令行参数
- `-c <PATH>`: 指定配置文件（默认搜寻 `config.toml`）。
- `-g`: 生成随机的 TOTP 密钥及展示二维码。

## ⚡ 性能表现
- **高并发**：支持 10,000+ 并发连接。
- **低延迟**：Yamux 多路复用优化，毫秒级响应。
- **低占用**：静态内存占用约 20MB。

## ⚖️ License
Dual-licensed under MIT or Apache-2.0.