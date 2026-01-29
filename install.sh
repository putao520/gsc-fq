#!/bin/bash
# gsc-fq 安装脚本
# 用法: curl -sSLf https://raw.githubusercontent.com/putao520/gsc-fq/main/install.sh | sh

set -e

VERSION="${VERSION:-v0.9.3}"
REPO="putao520/gsc-fq"
BINARY_NAME="gsc-fq"

echo "🚀 开始安装 ${BINARY_NAME} ${VERSION}"

# 检测系统
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux*)     OS=linux;;
    Darwin*)    OS=macos;;
    *)          echo "❌ 不支持的操作系统: ${OS}"; exit 1;;
esac

case "${ARCH}" in
    x86_64)     ARCH=x86_64;;
    aarch64)    ARCH=aarch64;;
    arm64)      ARCH=aarch64;;
    *)          echo "❌ 不支持的架构: ${ARCH}"; exit 1;;
esac

echo "📦 检测到系统: ${OS} ${ARCH}"

# 确定安装目录
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "${INSTALL_DIR}"

echo "📁 安装目录: ${INSTALL_DIR}"

# 下载 URL
BINARY_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}-${OS}-${ARCH}"

echo "⬇️  下载二进制文件: ${BINARY_URL}"

# 下载二进制文件
if command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "${INSTALL_DIR}/${BINARY_NAME}" "${BINARY_URL}"
elif command -v curl >/dev/null 2>&1; then
    curl -sSLf -o "${INSTALL_DIR}/${BINARY_NAME}" "${BINARY_URL}"
else
    echo "❌ 需要 wget 或 curl 来下载文件"
    exit 1
fi

# 设置执行权限
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

echo "✅ 二进制文件已安装到: ${INSTALL_DIR}/${BINARY_NAME}"

# 检查 PATH
if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    echo ""
    echo "⚠️  注意: ${INSTALL_DIR} 不在 PATH 中"
    echo ""
    echo "请将以下行添加到你的 shell 配置文件 (~/.bashrc 或 ~/.zshrc):"
    echo ""
    echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
    echo "然后运行: source ~/.bashrc (或 source ~/.zshrc)"
    echo ""
fi

# 验证安装
echo ""
echo "🔍 验证安装..."
if "${INSTALL_DIR}/${BINARY_NAME}" --version >/dev/null 2>&1; then
    VERSION_OUTPUT=$("${INSTALL_DIR}/${BINARY_NAME}" --version 2>/dev/null || echo "${VERSION}")
    echo "✅ ${BINARY_NAME} 安装成功!"
    echo "📌 版本: ${VERSION_OUTPUT}"
    echo ""
    echo "🎉 安装完成! 运行 '${BINARY_NAME} --help' 查看使用说明"
else
    echo "⚠️  安装可能有问题，请手动验证"
fi
