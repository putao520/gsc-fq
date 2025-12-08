# GSC-FQ 安装指南

## 快速安装

GSC-FQ 现已支持智能安装，自动检测设备架构并提供最适合的安装方式！

### 一键安装（推荐）

```bash
# 自动检测设备架构并安装
curl -sSL https://raw.githubusercontent.com/putao520/gsc-fq/main/scripts/install.sh | bash
```

### 使用安装脚本

```bash
# 显示帮助
./scripts/install.sh --help

# 自动检测并安装（优先从 GitHub 下载，失败则编译）
./scripts/install.sh

# 强制从源码编译
./scripts/install.sh --compile

# 指定安装目录
./scripts/install.sh -d ~/bin

# 仅下载模式（不编译）
./scripts/install.sh --mode download

# 编译指定架构
./scripts/install.sh --mode compile --arch x86_64
```

### 手动安装

#### 从 GitHub 下载

```bash
# 下载对应架构的二进制文件
# x86_64
wget https://github.com/putao520/gsc-fq/releases/latest/download/gsc-fq-linux-x86_64 -O gsc-fq

# ARM64
wget https://github.com/putao520/gsc-fq/releases/latest/download/gsc-fq-linux-arm64 -O gsc-fq

# ARMv7 (32位)
wget https://github.com/putao520/gsc-fq/releases/latest/download/gsc-fq-linux-armv7 -O gsc-fq

# 设置执行权限并安装
chmod +x gsc-fq
sudo mv gsc-fq /usr/local/bin/
```

#### 从源码编译

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆仓库
git clone https://github.com/putao520/gsc-fq.git
cd gsc-fq

# 编译（支持 x86_64、ARM64、ARMv7）
cargo build --release

# 安装
sudo cp target/release/gsc-fq /usr/local/bin/
```

## 支持的架构

| 架构 | 支持方式 | 适用设备 |
|------|----------|----------|
| x86_64 | ✅ 下载/编译 | Intel/AMD 64位服务器 |
| ARM64 | ✅ 下载/编译 | ARM 64位设备（Rockchip RK3588、树莓派4等） |
| ARMv7 | ✅ 编译 | ARM 32位设备（OpenWRT、树莓派2/3等） |
| i686 | ✅ 编译 | Intel 32位系统 |

## Docker 安装

```bash
# 拉取镜像
docker pull putao520/gsc-fq:latest

# 运行容器
docker run -it \
    -v $(pwd)/config.toml:/app/config.toml \
    putao520/gsc-fq:latest
```

多架构 Docker 镜像支持：
- `linux/amd64` - x86_64
- `linux/arm64` - ARM64
- `linux/arm/v7` - ARMv7

## 验证安装

```bash
# 检查版本
gsc-fq --version

# 显示帮助
gsc-fq --help
```

## 配置示例

创建配置文件 `~/.config/gsc-fq/config.toml`：

```toml
# 隧道代理配置
[[proxies]]
local_port = 33100
remote_host = "example.com"
remote_port = 443

# 反向代理服务端配置
[reverse_proxy_server]
listen_port = 33200
auth_token = "your-secret-token"

[[reverse_proxies]]
local_port = 8080
```

## 高级用法

### 环境变量

```bash
# 设置认证令牌
export GSC_FQ_AUTH_TOKEN="your-secret-token"

# 配置文件路径
export GSC_FQ_CONFIG="/path/to/config.toml"
```

### 系统服务

创建 systemd 服务文件 `/etc/systemd/system/gsc-fq.service`：

```ini
[Unit]
Description=GSC-FQ TCP Proxy
After=network.target

[Service]
Type=simple
User=gscfq
ExecStart=/usr/local/bin/gsc-fq
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

启动服务：
```bash
sudo systemctl enable gsc-fq
sudo systemctl start gsc-fq
```

## 故障排除

### 常见问题

1. **权限不足**
   ```bash
   # 确保脚本有执行权限
   chmod +x scripts/install.sh

   # 手动安装需要 sudo
   sudo mv gsc-fq /usr/local/bin/
   ```

2. **依赖缺失**
   ```bash
   # Ubuntu/Debian
   sudo apt-get install musl-tools gcc-arm-linux-gnueabihf

   # CentOS/RHEL
   sudo yum install musl-tools gcc-arm-linux-gnueabihf-gcc
   ```

3. **编译失败**
   ```bash
   # 更新 Rust 工具链
   rustup update

   # 添加目标架构
   rustup target add arm-unknown-linux-musleabihf
   ```

### 调试模式

```bash
# 启用调试日志
gsc-fq --debug

# 查看详细错误
RUST_LOG=debug gsc-fq
```

## 更新

```bash
# 使用安装脚本更新
./scripts/install.sh --force

# 或下载最新版本
wget https://github.com/putao520/gsc-fq/releases/latest/download/gsc-fq-linux-$(uname -m) -O gsc-fq
chmod +x gsc-fq
sudo mv gsc-fq /usr/local/bin/
```

## 贡献

欢迎提交 Issue 和 Pull Request！

- [GitHub Issues](https://github.com/putao520/gsc-fq/issues)
- [GitHub Discussions](https://github.com/putao520/gsc-fq/discussions)

## 许可证

MIT License

---

**最后更新**: 2025-12-08
**版本**: v0.8.5