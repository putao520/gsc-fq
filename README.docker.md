# GSC-FQ Docker 部署指南

## 🐳 Docker 镜像方案

我们提供两种优化的 Docker 镜像方案：

### 1. Alpine 版本（开发/测试推荐）
- **基础镜像**: `alpine:3.19`
- **大小**: ~15MB
- **特点**: 包含调试工具，便于开发调试

### 2. Distrolezz 版本（生产推荐）
- **基础镜像**: `gcr.io/distroless/cc-debian12`
- **大小**: ~8MB
- **特点**: 极简运行时，无 Shell，更高安全性

## 🚀 快速开始

### 本地构建
```bash
# 克隆项目
git clone https://github.com/putao520/gsc-fq.git
cd gsc-fq

# 构建镜像
docker build -t gsc-fq:latest .

# 或使用构建脚本
chmod +x scripts/docker-build.sh
./scripts/docker-build.sh latest development
```

### 运行容器

#### 基本运行（使用默认配置）
```bash
docker run -d \
  --name gsc-fq \
  -p 33100:33100 \
  -p 33200:33200 \
  -p 33300:33300 \
  gsc-fq:latest
```

#### 使用配置文件
```bash
docker run -d \
  --name gsc-fq \
  -p 33100:33100 \
  -p 33200:33200 \
  -p 33300:33300 \
  -v $(pwd)/config/docker.toml:/app/config/config.toml:ro \
  gsc-fq:latest --config /app/config/config.toml
```

### 使用 Docker Compose
```bash
# 开发环境
docker-compose up -d

# 生产环境
docker-compose --profile production up -d
```

## 📋 部署配置

### 环境变量
- `RUST_LOG`: 日志级别 (error/warn/info/debug/trace)
- `RUST_VERSION`: Rust 版本 (默认: 1.90)
- `PROXY_0_REMOTE_HOST`: 动态设置第一个代理的远程主机
- `PROXY_0_REMOTE_PORT`: 动态设置第一个代理的远程端口

### 端口映射
```yaml
ports:
  - "33100:33100"  # 第一个代理端口
  - "33200:33200"  # 第二个代理端口
  - "33300:33300"  # 第三个代理端口
```

### 数据卷
```yaml
volumes:
  - ./config:/app/config:ro  # 配置文件（只读）
  - ./logs:/app/logs         # 日志目录
```

## 🔧 生产环境最佳实践

### 1. 使用 distroless 镜像
```dockerfile
FROM gcr.io/distroless/cc-debian12
```

### 2. 非 root 用户运行
```dockerfile
USER 65534:65534
```

### 3. 健康检查
```dockerfile
HEALTHCHECK --interval=30s --timeout=10s --retries=3 \
    CMD pgrep gsc-fq > /dev/null || exit 1
```

### 4. 资源限制
```yaml
deploy:
  resources:
    limits:
      cpus: '0.5'
      memory: 128M
    reservations:
      cpus: '0.25'
      memory: 64M
```

## 🏗️ 镜像优化

### 多阶段构建
- **构建阶段**: 使用 `rust:1.90-alpine` 编译
- **运行阶段**: 使用最小基础镜像

### 优化特性
- 静态链接，减少运行时依赖
- 最小化层数，减少镜像大小
- 非 root 用户，提高安全性
- 健康检查，确保服务可用性

## 📊 镜像大小对比

| 版本 | 基础镜像 | Rust 版本 | 镜像大小 | 适用场景 |
|------|----------|-----------|----------|----------|
| Alpine | alpine:3.19 | 1.90 | ~15MB | 开发、测试 |
| Distrolezz | distroless/cc-debian12 | 1.90 | ~8MB | 生产环境 |

## 🚨 注意事项

1. **网络配置**: 确保容器可以访问目标服务器
2. **端口冲突**: 检查本地端口是否被占用
3. **配置文件**: 使用只读方式挂载配置文件
4. **日志管理**: 配置日志轮转，避免磁盘空间耗尽
5. **安全更新**: 定期更新基础镜像和依赖
6. **版本一致性**: 确保 Docker 中的 Rust 版本与项目一致

## 🔍 故障排除

### 查看容器日志
```bash
docker logs gsc-fq
docker logs -f gsc-fq  # 实时查看
```

### 进入容器调试（仅 Alpine 版本）
```bash
docker exec -it gsc-fq sh
```

### 检查容器状态
```bash
docker ps | grep gsc-fq
docker inspect gsc-fq
```

## 📚 更多文档

- [项目 README](./README.md)
- [配置管理](./SPEC/CONFIGURATION.md)
- [架构设计](./SPEC/ARCHITECTURE.md)