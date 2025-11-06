# 多阶段构建 Dockerfile for GSC-FQ

# Stage 1: 构建阶段 - 使用 Rust 官方镜像进行编译
FROM rust:1.90-alpine as builder

# 设置工作目录
WORKDIR /app

# 安装构建依赖（alpine需要额外的包）
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl3 \
    openssl3-dev

# 复制 Cargo 文件
COPY Cargo.toml Cargo.lock ./

# 创建空的 src 目录以缓存依赖
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# 复制源代码
COPY src ./src

# 构建应用（优化编译）
RUN cargo build --release

# Stage 2: 运行阶段 - 使用最小的基础镜像
FROM alpine:3.19

# 安装运行时依赖
RUN apk add --no-cache \
    ca-certificates \
    openssl3

# 创建非 root 用户
RUN addgroup -g 1000 gscfq && \
    adduser -D -s /bin/sh -u 1000 -G gscfq gscfq

# 设置工作目录
WORKDIR /app

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/gsc-fq /usr/local/bin/gsc-fq

# 创建配置目录
RUN mkdir -p /app/config && \
    chown -R gscfq:gscfq /app

# 切换到非 root 用户
USER gscfq

# 暴露默认端口
EXPOSE 33100 33200 33300

# 设置健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD pgrep gsc-fq > /dev/null || exit 1

# 默认命令
CMD ["gsc-fq", "--help"]