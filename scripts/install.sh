#!/bin/bash

# GSC-FQ 智能安装脚本
# 自动检测目标设备架构并下载/编译对应的二进制文件

set -e

# 版本配置
VERSION="0.8.5"
GITHUB_REPO="putao520/gsc-fq"
CRATE_NAME="gsc-fq"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 全局变量
DETECTED_ARCH=""
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="gsc-fq"
COMPILE_MODE="auto"  # auto, download, compile
FORCE_COMPILE=false
SKIP_DOWNLOAD=false

# 显示帮助信息
show_help() {
    cat << EOF
GSC-FQ 智能安装脚本 v${VERSION}

用法: $0 [选项]

选项:
    -h, --help              显示此帮助信息
    -v, --version           显示版本信息
    -d, --dir DIR           指定安装目录 (默认: ${INSTALL_DIR})
    -c, --compile           强制从源码编译
    -f, --force             强制覆盖已存在的文件
    --skip-download         跳过下载，直接编译
    -n, --dry-run           模拟运行，不实际执行
    --mode MODE             设置安装模式 (auto/download/compile)
    --arch ARCHS           指定构建的架构 (仅 compile 模式有效)

示例:
    $0                          # 自动检测并安装
    $0 -d ~/bin                 # 安装到 ~/bin
    $0 -c                       # 强制从源码编译
    $0 --mode download          # 仅从 GitHub 下载
    $0 --mode compile           # 仅从源码编译

支持的架构:
    x86_64, x86_64-linux, x64
    aarch64, aarch64-linux, arm64
    armv7l, arm-unknown-linux-musleabihf, armhf
    i686, i386, x86

EOF
}

# 显示版本信息
show_version() {
    echo "GSC-FQ 智能安装脚本 v${VERSION}"
    echo "支持自动检测设备架构并安装对应的二进制文件"
    echo "GitHub: ${GITHUB_REPO}"
}

# 检测目标架构
detect_arch() {
    echo -e "${BLUE}🔍 检测目标架构...${NC}"

    # 如果手动指定了架构，直接使用
    if [ -n "$ARCH_LIST" ]; then
        echo -e "${CYAN}📋 使用指定的架构: ${DETECTED_ARCH}${NC}"
        return 0
    fi

    # 获取当前架构
    local arch=$(uname -m)

    # 映射架构名称
    case $arch in
        x86_64)
            DETECTED_ARCH="x86_64"
            ;;
        aarch64|arm64)
            DETECTED_ARCH="aarch64"
            ;;
        armv7l|arm)
            # 进一步检测是否为 ARMv7
            if [ -f /proc/cpuinfo ]; then
                local cpu_arch=$(grep -o 'CPU architecture:\s*[0-9]' /proc/cpuinfo | head -1 | awk '{print $3}')
                if [ "$cpu_arch" = "7" ]; then
                    DETECTED_ARCH="armv7"
                else
                    DETECTED_ARCH="armv7"  # 默认为 ARMv7
                fi
            else
                DETECTED_ARCH="armv7"  # 默认为 ARMv7
            fi
            ;;
        i386|i686)
            DETECTED_ARCH="i686"
            ;;
        *)
            echo -e "${RED}❌ 不支持的架构: $arch${NC}"
            echo -e "${YELLOW}💡 请使用 --mode compile 选项手动编译${NC}"
            exit 1
            ;;
    esac

    echo -e "${GREEN}✅ 检测到架构: ${DETECTED_ARCH}${NC}"
}

# 检查系统依赖
check_dependencies() {
    echo -e "${BLUE}🔍 检查系统依赖...${NC}"

    local missing_deps=()

    # 检查基础工具
    for cmd in curl wget tar unzip; do
        if ! command -v $cmd &> /dev/null; then
            missing_deps+=($cmd)
        fi
    done

    # 如果选择编译模式，检查 Rust 和编译工具
    if [ "$COMPILE_MODE" = "compile" ] || [ "$COMPILE_MODE" = "auto" ] && [ "$SKIP_DOWNLOAD" = false ]; then
        if ! command -v rustup &> /dev/null; then
            missing_deps+=("rustup")
        fi
        if ! command -v cargo &> /dev/null; then
            missing_deps+=("cargo")
        fi
    fi

    if [ ${#missing_deps[@]} -ne 0 ]; then
        echo -e "${YELLOW}⚠️  缺少依赖: ${missing_deps[*]}${NC}"
        echo -e "${YELLOW}💡 请安装缺少的依赖后再试${NC}"
        echo ""
        echo "安装建议:"
        echo "  Ubuntu/Debian: sudo apt-get install curl wget tar unzip"
        echo "  CentOS/RHEL:   sudo yum install curl wget tar unzip"
        echo "  安装 Rust:    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi

    echo -e "${GREEN}✅ 所有依赖检查通过${NC}"
}

# 从 GitHub 下载二进制文件
download_from_github() {
    if [ "$SKIP_DOWNLOAD" = true ]; then
        echo -e "${YELLOW}⚠️  跳过下载模式${NC}"
        return 0
    fi

    echo -e "${BLUE}📦 从 GitHub 下载二进制文件...${NC}"

    # 构建 GitHub 下载 URL
    local DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/v${VERSION}/gsc-fq-linux-${DETECTED_ARCH}"
    local DOWNLOAD_DIR=$(mktemp -d)

    echo -e "${CYAN}🔗 下载 URL: ${DOWNLOAD_URL}${NC}"

    # 尝试下载
    if [ "$DRY_RUN" = true ]; then
        echo -e "${CYAN}🔧 模拟下载: ${DOWNLOAD_URL}${NC}"
        return 0
    fi

    if command -v curl &> /dev/null; then
        if ! curl -L --fail -o "${DOWNLOAD_DIR}/${BINARY_NAME}" "${DOWNLOAD_URL}"; then
            echo -e "${RED}❌ 下载失败: 无法从 GitHub 下载 ${DETECTED_ARCH} 版本${NC}"
            rm -rf "${DOWNLOAD_DIR}"
            return 1
        fi
    elif command -v wget &> /dev/null; then
        if ! wget -q -O "${DOWNLOAD_DIR}/${BINARY_NAME}" "${DOWNLOAD_URL}"; then
            echo -e "${RED}❌ 下载失败: 无法从 GitHub 下载 ${DETECTED_ARCH} 版本${NC}"
            rm -rf "${DOWNLOAD_DIR}"
            return 1
        fi
    else
        echo -e "${RED}❌ 下载失败: 需要 curl 或 wget${NC}"
        return 1
    fi

    # 设置执行权限
    chmod +x "${DOWNLOAD_DIR}/${BINARY_NAME}"

    # 安装二进制文件
    install_binary "${DOWNLOAD_DIR}/${BINARY_NAME}"

    # 清理临时文件
    rm -rf "${DOWNLOAD_DIR}"

    echo -e "${GREEN}✅ 从 GitHub 下载并安装成功${NC}"
}

# 从源码编译
compile_from_source() {
    echo -e "${BLUE}🔧 从源码编译...${NC}"

    # 检查 Rust 工具链
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}❌ Rust/Cargo 未安装${NC}"
        echo -e "${YELLOW}💡 请先安装 Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
        exit 1
    fi

    # 检查并安装交叉编译目标
    echo -e "${CYAN}📦 检查交叉编译目标...${NC}"

    local TARGET_TRIPLE=""
    case $DETECTED_ARCH in
        x86_64)
            TARGET_TRIPLE="x86_64-unknown-linux-musl"
            ;;
        aarch64)
            TARGET_TRIPLE="aarch64-unknown-linux-musl"
            ;;
        armv7)
            TARGET_TRIPLE="arm-unknown-linux-musleabihf"
            ;;
        i686)
            TARGET_TRIPLE="i686-unknown-linux-musl"
            ;;
        *)
            echo -e "${RED}❌ 不支持的编译目标: ${DETECTED_ARCH}${NC}"
            exit 1
            ;;
    esac

    # 安装目标
    if ! rustup target list --installed | grep -q "$TARGET_TRIPLE"; then
        echo -e "${YELLOW}📦 安装 Rust 目标: ${TARGET_TRIPLE}${NC}"
        rustup target add "$TARGET_TRIPLE"
    fi

    # 检查 musl-tools
    if ! command -v musl-gcc &> /dev/null; then
        echo -e "${YELLOW}⚠️  musl-tools 未安装${NC}"
        echo -e "${YELLOW}💡 编译可能失败，建议安装: sudo apt-get install musl-tools${NC}"
        read -p "是否继续编译? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi

    # 设置编译环境
    export CC_${TARGET_TRIPLE//-/_}=musl-gcc
    export CARGO_TARGET_${TARGET_TRIPLE//-/_}_LINKER=musl-gcc

    # 创建临时构建目录
    local BUILD_DIR=$(mktemp -d)

    # 下载源码
    echo -e "${CYAN}📥 下载源码...${NC}"
    if command -v curl &> /dev/null; then
        curl -L "https://github.com/${GITHUB_REPO}/archive/refs/tags/v${VERSION}.tar.gz" | tar -xz -C "${BUILD_DIR}" --strip-components=1
    elif command -v wget &> /dev/null; then
        wget -q -O - "https://github.com/${GITHUB_REPO}/archive/refs/tags/v${VERSION}.tar.gz" | tar -xz -C "${BUILD_DIR}" --strip-components=1
    fi

    # 编译
    cd "${BUILD_DIR}"
    echo -e "${CYAN}🔨 开始编译...${NC}"

    if [ "$DRY_RUN" = true ]; then
        echo -e "${CYAN}🔧 模拟编译: cargo build --target ${TARGET_TRIPLE} --release${NC}"
    else
        if ! cargo build --target "$TARGET_TRIPLE" --release; then
            echo -e "${RED}❌ 编译失败${NC}"
            rm -rf "${BUILD_DIR}"
            exit 1
        fi
    fi

    # 获取编译后的二进制文件
    local COMPILED_BINARY="target/${TARGET_TRIPLE}/release/gsc-fq"

    if [ ! -f "$COMPILED_BINARY" ]; then
        echo -e "${RED}❌ 编译后的二进制文件不存在${NC}"
        rm -rf "${BUILD_DIR}"
        exit 1
    fi

    # 安装二进制文件
    install_binary "$COMPILED_BINARY"

    # 清理临时文件
    cd - > /dev/null
    rm -rf "${BUILD_DIR}"

    echo -e "${GREEN}✅ 编译并安装成功${NC}"
}

# 安装二进制文件
install_binary() {
    local binary_path=$1

    if [ ! -f "$binary_path" ]; then
        echo -e "${RED}❌ 二进制文件不存在: $binary_path${NC}"
        exit 1
    fi

    # 检查是否需要 root 权限
    if [ "$INSTALL_DIR" = "/usr/local/bin" ] && [ "$(id -u)" -ne 0 ]; then
        echo -e "${YELLOW}⚠️  需要管理员权限安装到 /usr/local/bin${NC}"
        echo -e "${YELLOW}💡 请运行: sudo $0 $*${NC}"
        exit 1
    fi

    # 创建安装目录
    mkdir -p "$INSTALL_DIR"

    # 检查文件是否已存在
    local install_path="${INSTALL_DIR}/${BINARY_NAME}"
    if [ -f "$install_path" ] && [ "$FORCE_COMPILE" = false ]; then
        echo -e "${YELLOW}⚠️  ${install_path} 已存在${NC}"
        read -p "是否覆盖? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo -e "${BLUE}👋 安装已取消${NC}"
            exit 0
        fi
    fi

    # 复制二进制文件
    cp "$binary_path" "$install_path"

    # 设置权限
    chmod +x "$install_path"

    # 验证安装
    if command -v "$BINARY_NAME" &> /dev/null; then
        echo -e "${GREEN}✅ 安装成功！${NC}"
        echo -e "${CYAN}📍 安装位置: ${install_path}${NC}"
        echo -e "${CYAN}🔍 版本信息:${NC}"
        "$BINARY_NAME" --version 2>/dev/null || echo "  版本: v${VERSION}"
    else
        echo -e "${RED}❌ 安装失败，请检查 PATH 环境变量${NC}"
        exit 1
    fi
}

# 主函数
main() {
    # 解析命令行参数
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_help
                exit 0
                ;;
            -v|--version)
                show_version
                exit 0
                ;;
            -d|--dir)
                INSTALL_DIR="$2"
                shift 2
                ;;
            -c|--compile)
                COMPILE_MODE="compile"
                shift
                ;;
            -f|--force)
                FORCE_COMPILE=true
                shift
                ;;
            --skip-download)
                SKIP_DOWNLOAD=true
                shift
                ;;
            -n|--dry-run)
                DRY_RUN=true
                shift
                ;;
            --mode)
                COMPILE_MODE="$2"
                shift 2
                ;;
            --arch)
                # 解析逗号分隔的架构列表
                ARCH_LIST="$2"
                # 重新设置 DETECTED_ARCH
                DETECTED_ARCH="${ARCH_LIST//,/ }"
                shift 2
                ;;
            *)
                echo -e "${RED}❌ 未知选项: $1${NC}"
                show_help
                exit 1
                ;;
        esac
    done

    # 显示欢迎信息
    echo -e "${BLUE}🚀 GSC-FQ 智能安装脚本 v${VERSION}${NC}"
    echo -e "${BLUE}==================================${NC}"
    echo ""

    # 检测架构
    detect_arch

    # 检查依赖
    check_dependencies

    # 根据模式执行安装
    case $COMPILE_MODE in
        auto)
            if [ "$SKIP_DOWNLOAD" = false ]; then
                # 先尝试从 GitHub 下载
                if download_from_github; then
                    # 下载成功，退出
                    exit 0
                else
                    # 下载失败，尝试编译
                    echo -e "${YELLOW}⚠️  GitHub 下载失败，尝试从源码编译${NC}"
                    compile_from_source
                fi
            else
                compile_from_source
            fi
            ;;
        download)
            download_from_github
            ;;
        compile)
            compile_from_source
            ;;
        *)
            echo -e "${RED}❌ 未知模式: $COMPILE_MODE${NC}"
            exit 1
            ;;
    esac

    echo ""
    echo -e "${GREEN}🎉 安装完成！${NC}"
    echo -e "${CYAN}📖 更多信息: https://github.com/${GITHUB_REPO}${NC}"
}

# 运行主函数
main "$@"