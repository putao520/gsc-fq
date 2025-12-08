#!/bin/bash

# GSC-FQ Docker 构建脚本
# 使用方法: ./scripts/docker-build.sh [version] [environment]

set -e

# 默认参数
VERSION=${1:-"latest"}
ENVIRONMENT=${2:-"development"}
REGISTRY=${REGISTRY:-"your-registry.com"}
RUST_VERSION=${RUST_VERSION:-"1.90"}  # 添加 Rust 版本参数

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🐳 GSC-FQ Docker 构建脚本${NC}"
echo -e "${BLUE}=============================${NC}"

# 检查 Docker 是否安装
if ! command -v docker &> /dev/null; then
    echo -e "${RED}❌ Docker 未安装或不在 PATH 中${NC}"
    exit 1
fi

# 检查 Dockerfile 是否存在
if [ ! -f "Dockerfile" ]; then
    echo -e "${RED}❌ Dockerfile 不存在${NC}"
    exit 1
fi

echo -e "${YELLOW}📋 构建配置:${NC}"
echo -e "   版本: ${VERSION}"
echo -e "   环境: ${ENVIRONMENT}"
echo -e "   Rust 版本: ${RUST_VERSION}"
echo -e "   注册表: ${REGISTRY}"
echo ""

# 构建函数
build_image() {
    local dockerfile=$1
    local tag=$2
    local description=$3

    echo -e "${GREEN}🔨 构建 ${description}...${NC}"

    docker build \
        --file "${dockerfile}" \
        --tag "${tag}" \
        --build-arg BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
        --build-arg VCS_REF="$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')" \
        --build-arg VERSION="${VERSION}" \
        --build-arg RUST_VERSION="${RUST_VERSION}" \
        .

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ ${description} 构建成功: ${tag}${NC}"
    else
        echo -e "${RED}❌ ${description} 构建失败${NC}"
        exit 1
    fi
}

# 构建开发版本
if [ "$ENVIRONMENT" = "development" ] || [ "$ENVIRONMENT" = "all" ]; then
    build_image "Dockerfile" "gsc-fq:${VERSION}" "开发版本 (Alpine)"
fi

# 构建生产版本
if [ "$ENVIRONMENT" = "production" ] || [ "$ENVIRONMENT" = "all" ]; then
    build_image "Dockerfile.distrolezz" "gsc-fq:${VERSION}-distroless" "生产版本 (Distrolezz)"
fi

# 构建多架构镜像（如果支持）
if command -v docker buildx &> /dev/null; then
    echo -e "${GREEN}🏗️  构建多架构镜像...${NC}"
    docker buildx build \
        --platform linux/amd64,linux/arm64,linux/armv7 \
        --file "Dockerfile" \
        --tag "${REGISTRY}/gsc-fq:${VERSION}" \
        --build-arg RUST_VERSION="${RUST_VERSION}" \
        --push \
        .
    echo -e "${GREEN}✅ 多架构镜像推送完成${NC}"
else
    echo -e "${YELLOW}⚠️  Docker buildx 不可用，跳过多架构构建${NC}"
fi

# 显示镜像信息
echo ""
echo -e "${BLUE}📦 构建的镜像:${NC}"
docker images | grep "gsc-fq" | head -10

# 提供推送命令
echo ""
echo -e "${YELLOW}🚀 推送到注册表:${NC}"
echo "docker push ${REGISTRY}/gsc-fq:${VERSION}"
echo "docker push ${REGISTRY}/gsc-fq:${VERSION}-distroless"

echo -e "${GREEN}🎉 构建完成！${NC}"