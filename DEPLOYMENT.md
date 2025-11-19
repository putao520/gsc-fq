# GSC-FQ 部署和运维指南

## 1. 部署前准备

### 1.1 环境要求

| 组件 | 最低版本 | 推荐版本 |
|-----|--------|--------|
| Rust | 1.70 | 1.80+ |
| Linux | 内核 4.15+ | 内核 5.10+ |
| 内存 | 10MB | 256MB+ |
| 磁盘 | 50MB | 100MB+ |

### 1.2 依赖检查

```bash
# 检查Rust版本
rustc --version

# 检查Cargo版本
cargo --version

# 检查内核版本（Linux）
uname -r

# 检查可用内存
free -h

# 检查磁盘空间
df -h
```

## 2. 构建和安装

### 2.1 开发环境安装

```bash
# 克隆仓库
git clone https://github.com/putao520/gsc-fq.git
cd gsc-fq

# 构建
cargo build

# 可执行文件位置
./target/debug/gsc-fq
```

### 2.2 生产环境安装

#### 2.2.1 从源码构建（推荐）

```bash
# 克隆仓库
git clone https://github.com/putao520/gsc-fq.git
cd gsc-fq

# 检出最新发布版本
git tag -l
git checkout v0.4.0  # 替换为最新版本

# 发布构建
cargo build --release

# 可执行文件位置
./target/release/gsc-fq

# 复制到系统路径
sudo cp ./target/release/gsc-fq /usr/local/bin/
```

#### 2.2.2 从Cargo安装

```bash
# 直接安装
cargo install gsc-fq

# 可执行文件位置
~/.cargo/bin/gsc-fq

# 确保$HOME/.cargo/bin在PATH中
export PATH="$PATH:$HOME/.cargo/bin"
```

### 2.3 Docker安装

```bash
# 构建Docker镜像
docker build -t gsc-fq:latest .

# 运行容器
docker run -d \
  --name gsc-fq \
  -v /etc/gsc-fq:/etc/gsc-fq \
  -p 8080:8080 \
  gsc-fq:latest
```

### 2.4 验证安装

```bash
# 检查版本
gsc-fq --help 2>&1 | head -3

# 运行帮助
gsc-fq --help

# 创建测试配置
cat > test_config.toml <<EOF
[server]
bind_ip = "127.0.0.1"
debug = false

[[proxies]]
local_port = 8080
remote_host = "example.com"
remote_port = 80
EOF

# 测试运行
timeout 2 gsc-fq || true
```

## 3. 配置管理

### 3.1 配置文件位置

| 部署方式 | 配置文件位置 |
|---------|-----------|
| 本地运行 | `./default.toml` |
| Systemd服务 | `/etc/gsc-fq/default.toml` |
| Docker | `/etc/gsc-fq/default.toml` |

### 3.2 配置文件权限

```bash
# 创建配置目录
sudo mkdir -p /etc/gsc-fq
sudo chown gsc-fq:gsc-fq /etc/gsc-fq
sudo chmod 750 /etc/gsc-fq

# 设置配置文件权限
sudo chmod 640 /etc/gsc-fq/default.toml
sudo chown gsc-fq:gsc-fq /etc/gsc-fq/default.toml
```

### 3.3 配置示例

```toml
# /etc/gsc-fq/default.toml
[server]
bind_ip = "0.0.0.0"
debug = false

# HTTP代理
[[proxies]]
local_port = 8080
remote_host = "backend1.example.com"
remote_port = 80

# 数据库代理
[[proxies]]
local_port = 5432
remote_host = "db.example.com"
remote_port = 5432
source_ip = "10.20.30.40"

# 反向代理规则（可选）
[[reverse_proxies]]
port = 8081
```

### 3.4 配置热更新

```bash
# 方式1: 创建新的代理实例
# 修改配置后，启动新的GSC-FQ实例在不同端口

# 方式2: 使用systemd重启
sudo systemctl restart gsc-fq

# 方式3: 手动重启
pkill gsc-fq
sleep 2
gsc-fq &
```

## 4. Systemd 服务配置

### 4.1 创建服务文件

```bash
sudo tee /etc/systemd/system/gsc-fq.service > /dev/null <<EOF
[Unit]
Description=GSC-FQ High-Performance TCP Proxy
After=network.target

[Service]
Type=simple
User=gsc-fq
Group=gsc-fq
WorkingDirectory=/etc/gsc-fq
ExecStart=/usr/local/bin/gsc-fq
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Resource limits
LimitNOFILE=65535
LimitNPROC=4096

# Security
PrivateTmp=yes
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
EOF
```

### 4.2 创建用户和目录

```bash
# 创建系统用户
sudo useradd -r -s /bin/false gsc-fq

# 创建配置目录
sudo mkdir -p /etc/gsc-fq
sudo chown gsc-fq:gsc-fq /etc/gsc-fq
sudo chmod 750 /etc/gsc-fq

# 创建日志目录
sudo mkdir -p /var/log/gsc-fq
sudo chown gsc-fq:gsc-fq /var/log/gsc-fq
```

### 4.3 启用和启动服务

```bash
# 重载systemd配置
sudo systemctl daemon-reload

# 启用服务开机自启
sudo systemctl enable gsc-fq

# 启动服务
sudo systemctl start gsc-fq

# 检查服务状态
sudo systemctl status gsc-fq

# 查看服务日志
sudo journalctl -u gsc-fq -f

# 重启服务
sudo systemctl restart gsc-fq

# 停止服务
sudo systemctl stop gsc-fq
```

## 5. Docker 部署

### 5.1 Dockerfile

```dockerfile
FROM rust:latest as builder

WORKDIR /app
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/gsc-fq /usr/local/bin/

RUN mkdir -p /etc/gsc-fq && \
    chmod +x /usr/local/bin/gsc-fq

WORKDIR /etc/gsc-fq

ENTRYPOINT ["gsc-fq"]
```

### 5.2 Docker 运行

```bash
# 构建镜像
docker build -t gsc-fq:0.4.0 .

# 运行容器
docker run -d \
  --name gsc-fq \
  --restart always \
  -v /etc/gsc-fq/default.toml:/etc/gsc-fq/default.toml:ro \
  -p 8080:8080 \
  -p 5432:5432 \
  gsc-fq:0.4.0

# 查看日志
docker logs -f gsc-fq

# 停止容器
docker stop gsc-fq

# 删除容器
docker rm gsc-fq
```

### 5.3 Docker Compose

```yaml
version: '3.8'

services:
  gsc-fq:
    image: gsc-fq:0.4.0
    build:
      context: .
      dockerfile: Dockerfile
    container_name: gsc-fq
    restart: always
    volumes:
      - /etc/gsc-fq/default.toml:/etc/gsc-fq/default.toml:ro
    ports:
      - "8080:8080"
      - "5432:5432"
    environment:
      - RUST_LOG=info
    healthcheck:
      test: ["CMD", "nc", "-zv", "127.0.0.1", "8080"]
      interval: 30s
      timeout: 10s
      retries: 3
```

## 6. 监控和日志

### 6.1 启用调试模式

```toml
# default.toml
[server]
debug = true
```

### 6.2 日志查看

```bash
# Systemd日志
sudo journalctl -u gsc-fq -f

# 特定时间范围的日志
sudo journalctl -u gsc-fq --since "2024-01-01 00:00:00" --until "2024-01-01 23:59:59"

# 最后N行日志
sudo journalctl -u gsc-fq -n 100

# 按优先级过滤
sudo journalctl -u gsc-fq -p err
```

### 6.3 监控系统资源

```bash
# 监控进程资源使用
watch -n 1 'ps aux | grep gsc-fq'

# 监控网络连接
sudo ss -tulpn | grep gsc-fq
sudo netstat -tulpn | grep gsc-fq

# 监控文件描述符
lsof -p $(pgrep gsc-fq)

# 监控内存
pmap $(pgrep gsc-fq)
```

### 6.4 性能监控工具

```bash
# 使用top/htop
top -p $(pgrep gsc-fq)
htop -p $(pgrep gsc-fq)

# 使用perf（Linux）
sudo perf record -p $(pgrep gsc-fq) sleep 10
sudo perf report

# 使用flamegraph
sudo flamegraph -p $(pgrep gsc-fq) -- sleep 10
```

## 7. 备份和恢复

### 7.1 配置备份

```bash
# 备份配置文件
sudo cp /etc/gsc-fq/default.toml /backup/gsc-fq-default.toml.bak

# 定期备份（cron）
0 2 * * * sudo cp /etc/gsc-fq/default.toml /backup/gsc-fq-default-$(date +\%Y\%m\%d).toml
```

### 7.2 恢复

```bash
# 从备份恢复
sudo cp /backup/gsc-fq-default.toml.bak /etc/gsc-fq/default.toml

# 重启服务
sudo systemctl restart gsc-fq
```

## 8. 升级和更新

### 8.1 升级流程

```bash
# 1. 备份当前配置
sudo cp /etc/gsc-fq/default.toml /backup/default.toml.bak

# 2. 停止服务
sudo systemctl stop gsc-fq

# 3. 下载新版本
cd /tmp
wget https://github.com/putao520/gsc-fq/releases/download/v0.4.1/gsc-fq-0.4.1-x86_64-unknown-linux-gnu.tar.gz
tar xzf gsc-fq-0.4.1-*.tar.gz

# 4. 安装新版本
sudo cp gsc-fq /usr/local/bin/gsc-fq
sudo chown root:root /usr/local/bin/gsc-fq
sudo chmod 755 /usr/local/bin/gsc-fq

# 5. 启动服务
sudo systemctl start gsc-fq

# 6. 验证
sudo systemctl status gsc-fq
```

### 8.2 版本检查

```bash
# 检查当前版本
gsc-fq --version 2>&1 | grep -i version

# 检查远程版本
curl -s https://api.github.com/repos/putao520/gsc-fq/releases/latest | grep tag_name
```

## 9. 故障排查

### 9.1 常见问题

#### 服务无法启动

```bash
# 检查配置语法
cargo run -- < /dev/null  # 测试加载配置

# 查看错误日志
sudo journalctl -u gsc-fq -n 50

# 手动运行调试
cd /etc/gsc-fq
/usr/local/bin/gsc-fq
```

#### 端口被占用

```bash
# 查找占用端口的进程
sudo lsof -i :8080
sudo netstat -tulpn | grep :8080

# 杀死占用端口的进程
sudo kill -9 <PID>

# 更改配置使用其他端口
sudo nano /etc/gsc-fq/default.toml
```

#### 内存使用过高

```bash
# 检查内存使用
pmap $(pgrep gsc-fq)
ps aux | grep gsc-fq

# 减少并发连接
# 修改default.toml中的配置

# 重启服务
sudo systemctl restart gsc-fq
```

### 9.2 调试技巧

```bash
# 启用详细日志
RUST_LOG=debug /usr/local/bin/gsc-fq

# 使用strace追踪系统调用
sudo strace -f -e trace=network,openat,read,write -p $(pgrep gsc-fq)

# 使用tcpdump监听网络
sudo tcpdump -i any port 8080 or port 5432
```

## 10. 安全加固

### 10.1 文件权限

```bash
# 设置适当的文件权限
sudo chmod 700 /etc/gsc-fq
sudo chmod 600 /etc/gsc-fq/default.toml

# 限制访问
sudo chown gsc-fq:gsc-fq /etc/gsc-fq
sudo chown gsc-fq:gsc-fq /etc/gsc-fq/default.toml
```

### 10.2 防火墙配置

```bash
# UFW（Ubuntu/Debian）
sudo ufw allow 8080/tcp
sudo ufw allow 5432/tcp

# iptables
sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 5432 -j ACCEPT

# firewalld
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --permanent --add-port=5432/tcp
sudo firewall-cmd --reload
```

### 10.3 SELinux/AppArmor

```bash
# SELinux
sudo semanage fcontext -a -t bin_t "/usr/local/bin/gsc-fq"
sudo restorecon /usr/local/bin/gsc-fq

# AppArmor
sudo nano /etc/apparmor.d/usr.local.bin.gsc-fq
# 配置应用配置文件
```

## 11. 性能调优

### 11.1 系统参数优化

```bash
# 增加打开文件描述符限制
echo "fs.file-max = 2097152" | sudo tee -a /etc/sysctl.conf

# 增加TCP连接限制
echo "net.ipv4.ip_local_port_range = 1024 65535" | sudo tee -a /etc/sysctl.conf

# 应用更改
sudo sysctl -p
```

### 11.2 应用配置优化

```toml
# 针对高负载的配置
[server]
bind_ip = "0.0.0.0"
debug = false  # 禁用调试以提高性能

[[proxies]]
local_port = 8080
remote_host = "backend.example.com"
remote_port = 80
# 源IP可能会增加CPU负载，仅在必要时使用
```

## 12. 故障恢复

### 12.1 自动重启

```bash
# Systemd已配置Restart=always

# 验证自动重启是否工作
sudo systemctl is-enabled gsc-fq

# 查看重启历史
sudo systemctl status gsc-fq | grep Restart
```

### 12.2 灾难恢复计划

1. **定期备份配置文件**
2. **保存服务启动脚本**
3. **文档记录所有自定义配置**
4. **测试恢复流程**

---

**部署指南版本**: 1.0  
**最后更新**: 2024年11月  
