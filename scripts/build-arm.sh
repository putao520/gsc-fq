#!/bin/bash

# GSC-FQ ARM 架构交叉编译脚本
# 支持编译 ARM64 和 ARMv7 架构的 Linux 二进制文件

set -e

# 默认参数
VERSION=${1:-$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "gsc-fq") | .version')}
TARGET=${2:-"all"}  # all, arm64, armv7
RUST_VERSION=${RUST_VERSION:-"1.90"}

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🔧 GSC-FQ ARM 交叉编译脚本${NC}"
echo -e "${BLUE}===============================${NC}"

# 检查 Rust 是否安装
if ! command -v rustup &> /dev/null; then
    echo -e "${RED}❌ Rust 未安装${NC}"
    exit 1
fi

# 颜色输出
echo -e "${YELLOW}📋 编译配置:${NC}"
echo -e "   版本: ${VERSION}"
echo -e "   目标: ${TARGET}"
echo -e "   Rust 版本: ${RUST_VERSION}"
echo ""

# 检查并安装 ARM 交叉编译目标
install_targets() {
    echo -e "${GREEN}📦 安装交叉编译目标...${NC}"

    # 检查并安装 ARM64 目标
    if ! rustup target list --installed | grep -q "aarch64-unknown-linux-musl"; then
        echo -e "${YELLOW}📦 安装 ARM64 目标...${NC}"
        rustup target add aarch64-unknown-linux-musl
    fi

    # 检查并安装 ARMv7 目标
    if ! rustup target list --installed | grep -q "arm-unknown-linux-musleabihf"; then
        echo -e "${YELLOW}📦 安装 ARMv7 目标...${NC}"
        rustup target add arm-unknown-linux-musleabihf
    fi

    # 检查并安装 x86_64 目标（如果还没有）
    if ! rustup target list --installed | grep -q "x86_64-unknown-linux-musl"; then
        echo -e "${YELLOW}📦 安装 x86_64 目标...${NC}"
        rustup target add x86_64-unknown-linux-musl
    fi

    echo -e "${GREEN}✅ 所有目标安装完成${NC}"
}

# 检查 musl-tools
check_musl_tools() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if ! command -v musl-gcc &> /dev/null; then
            echo -e "${YELLOW}⚠️  警告: musl-tools 未安装${NC}"
            echo -e "${YELLOW}💡 请手动安装: sudo apt-get install musl-tools 或 sudo yum install musl-tools${NC}"
            echo -e "${YELLOW}💡 或者继续编译（可能会失败）${NC}"
            read -p "是否继续编译? (y/N): " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                exit 1
            fi
        fi
    fi
}

# 编译函数
compile_target() {
    local target=$1
    local arch_name=$2
    local triple=$3

    echo -e "${GREEN}🔨 编译 ${arch_name} (${triple})...${NC}"

    # 设置环境变量
    export CC_${triple//-/_}=musl-gcc
    export CARGO_TARGET_${triple//-/_}_LINKER=musl-gcc

    # 创建输出目录
    mkdir -p "target/${triple}/release"

    # 编译
    cargo build --target "${triple}" --release

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ ${arch_name} 编译成功${NC}"
        # 重命名二进制文件
        cp "target/${triple}/release/gsc-fq" "target/${triple}/release/gsc-fq-linux-${arch_name}"
    else
        echo -e "${RED}❌ ${arch_name} 编译失败${NC}"
        exit 1
    fi
}

# 主编译流程
main() {
    # 安装依赖
    install_targets
    check_musl_tools

    # 根据目标进行编译
    if [ "$TARGET" = "all" ] || [ "$TARGET" = "x86_64" ]; then
        compile_target "x86_64" "x86_64" "x86_64-unknown-linux-musl"
    fi

    if [ "$TARGET" = "all" ] || [ "$TARGET" = "arm64" ]; then
        compile_target "arm64" "arm64" "aarch64-unknown-linux-musl"
    fi

    if [ "$TARGET" = "all" ] || [ "$TARGET" = "armv7" ]; then
        compile_target "armv7" "armv7" "arm-unknown-linux-musleabihf"
    fi
}

# 显示结果
show_results() {
    echo ""
    echo -e "${BLUE}📦 编译结果:${NC}"
    echo "编译的文件:"
    ls -la target/*/release/gsc-fq-linux-* 2>/dev/null || echo "没有找到编译的文件"

    echo ""
    echo -e "${YELLOW}🚀 使用方法:${NC}"
    echo "# x86_64"
    echo "./target/x86_64-unknown-linux-musl/release/gsc-fq-linux-x86_64"
    echo ""
    echo "# ARM64"
    echo "./target/aarch64-unknown-linux-musl/release/gsc-fq-linux-arm64"
    echo ""
    echo "# ARMv7 (32位)"
    echo "./target/arm-unknown-linux-musleabihf/release/gsc-fq-linux-armv7"

    echo ""
    echo -e "${GREEN}🎉 编译完成！${NC}"
}

# 执行主流程
main
show_results