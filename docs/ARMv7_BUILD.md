# ARMv7 架构构建指南

## 概述

GSC-FQ 现已支持 ARMv7 (32位 ARM) 架构的交叉编译，适用于 OpenWRT ARM 设备、树莓派2/3 等嵌入式设备。

## 支持的架构

- **x86_64** (AMD64) - 完全支持，包含 AES-NI 硬件加速
- **ARM64** (aarch64) - 完全支持，ChaCha20-Poly1305 软件加密
- **ARMv7** (arm-unknown-linux-musleabihf) - 完全支持，针对嵌入式设备优化

## 前置要求

### 安装交叉编译工具链

```bash
# 安装 Rust ARM 目标
rustup target add arm-unknown-linux-musleabihf

# 安装 musl-tools (用于 musl libc)
# Ubuntu/Debian:
sudo apt-get update && sudo apt-get install -y musl-tools

# CentOS/RHEL/Fedora:
sudo yum install -y musl-tools

# 或者使用 dnf:
sudo dnf install -y musl-tools
```

### 安装 ARM 交叉编译器

```bash
# Ubuntu/Debian:
sudo apt-get install -y gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf

# CentOS/RHEL/Fedora:
sudo yum install -y arm-linux-gnueabihf-gcc arm-linux-gnueabihf-g++
```

## 构建方法

### 方法一：使用自动化构建脚本

```bash
# 编译所有架构
./scripts/build-arm.sh

# 仅编译 ARMv7
./scripts/build-arm.sh latest armv7

# 编译指定版本的所有架构
./scripts/build-arm.sh v0.8.4 all
```

### 方法二：手动交叉编译

```bash
# 设置环境变量
export CC_arm-unknown-linux-musleabihf=arm-linux-gnueabihf-gcc
export CARGO_TARGET_arm-unknown-linux-musleabihf_LINKER=arm-linux-gnueabihf-gcc

# 交叉编译
cargo build --target arm-unknown-linux-musleabihf --release

# 查看结果
ls -la target/arm-unknown-linux-musleabihf/release/gsc-fq-linux-armv7
```

## Docker 多架构构建

```bash
# 使用 buildx 构建多架构镜像
docker buildx build \
    --platform linux/amd64,linux/arm64,linux/armv7 \
    --file Dockerfile \
    --tag your-registry.com/gsc-fq:latest \
    --push .
```

## 部署到 ARM 设备

### 直接部署二进制文件

```bash
# 从 x86_64 机器复制到 ARM 设备
scp target/arm-unknown-linux-musleabihf/release/gsc-fq-linux-armv7 \
    root@arm-device:/usr/local/bin/gsc-fq

# 在 ARM 设备上运行
chmod +x /usr/local/bin/gsc-fq
gsc-fq --help
```

### 使用 Docker 镜像

```bash
# 在 ARM 设备上运行
docker run -it \
    -v $(pwd)/config.toml:/app/config.toml \
    your-registry.com/gsc-fq:latest
```

## 性能优化

ARMv7 架构针对嵌入式设备进行了特殊优化：

```toml
# Cargo.toml 中的 ARMv7 优化配置
[target.arm-unknown-linux-musleabihf.profile.release]
opt-level = "s"    # 体积优化
lto = true        # 链接时优化
codegen-units = 1 # 单编译单元
panic = "abort"   # panic 时 abort 以减少体积
strip = true      # 移除调试符号
```

## 已知限制

1. **编译工具链依赖**: 需要 `arm-linux-gnueabihf-gcc` 交叉编译器
2. **Ring 库**: OpenSSL 的 Ring 库在某些架构上可能需要额外配置
3. **性能**: ARMv7 上的加密性能低于 x86_64 (使用软件加密)
4. **依赖兼容性**: 某些依赖包可能不支持交叉编译

## 故障排除

### 常见错误

#### 1. "failed to find tool arm-linux-gnueabihf-gcc"

```bash
# 解决方案：安装 ARM 交叉编译工具
sudo apt-get install gcc-arm-linux-gnueabihf
```

#### 2. "musl-gcc: command not found"

```bash
# 解决方案：安装 musl-tools
sudo apt-get install musl-tools
```

#### 3. Ring 库编译失败

Ring 库对交叉编译支持有限，可以考虑：

```bash
# 设置环境变量以帮助 Ring 库找到正确的工具链
export CROSS_COMPILE=arm-linux-gnueabihf-
export TARGET_CC=arm-linux-gnueabihf-gcc
```

### 检查工具链

```bash
# 检查 ARM 交叉编译器
arm-linux-gnueabihf-gcc --version

# 检查 musl-gcc
musl-gcc --version

# 检查 Rust 目标
rustup target list --installed | grep arm
```

## 测试

### 运行测试

```bash
# 在 ARM 目标上运行测试
cargo test --target arm-unknown-linux-musleabihf

# 运行特定测试
cargo test --target arm-unknown-linux-musleabihf -- proxy
```

### 验证二进制文件

```bash
# 检查二进制文件架构
file target/arm-unknown-linux-musleabihf/release/gsc-fq-linux-armv7
# 应该输出: ARM 32-bit LSB executable, EABI5 version 1 (SYSV)

# 检查是否为动态链接
ldd target/arm-unknown-linux-musleabihf/release/gsc-fq-linux-armv7
# 应该显示 musl libc 的依赖
```

## 性能基准

在不同架构上的性能表现：

| 架构 | 加密方式 | 相对性能 | 适用场景 |
|------|----------|----------|----------|
| x86_64 | AES-NI | 100% | 高性能服务器 |
| ARM64 | ChaCha20 | ~70% | 云服务器、ARM 服务器 |
| ARMv7 | ChaCha20 | ~40% | 嵌入式设备、路由器 |

## 维护说明

1. **定期更新**: 定期检查并更新 Rust 工具链和依赖
2. **测试覆盖**: 确保所有架构的测试通过
3. **性能监控**: 监控不同架构上的性能表现
4. **依赖兼容性**: 关注依赖库的架构支持情况

---

**最后更新**: 2025-12-08
**维护者**: System Architect