#!/bin/bash

# GSC-FQ 安装脚本测试

set -e

echo "🧪 GSC-FQ 安装脚本测试"
echo "======================="

# 测试架构检测
echo "🔍 测试 1: 架构检测"
echo "-------------------"

# 模拟不同架构
echo "模拟 x86_64:"
ARCH_LIST="x86_64" ARCH_LIST_SET=true ./scripts/install.sh --mode compile --dry-run --arch x86_64 | grep -E "(检测到架构|使用指定的架构)"

echo "模拟 ARM64:"
ARCH_LIST="aarch64" ARCH_LIST_SET=true ./scripts/install.sh --mode compile --dry-run --arch aarch64 | grep -E "(检测到架构|使用指定的架构)"

echo "模拟 ARMv7:"
ARCH_LIST="armv7" ARCH_LIST_SET=true ./scripts/install.sh --mode compile --dry-run --arch armv7 | grep -E "(检测到架构|使用指定的架构)"

echo ""

# 测试下载模式
echo "📦 测试 2: 下载模式"
echo "------------------"
echo "模拟下载模式（GitHub 不存在文件，会失败）:"
./scripts/install.sh --mode download --dry-run --arch x86_64 || echo "预期失败：GitHub 上还没有预编译文件"

echo ""

# 测试编译模式
echo "🔨 测试 3: 编译模式（无实际编译）"
echo "------------------------------"
echo "模拟编译模式（显示命令，不实际编译）:"
./scripts/install.sh --mode compile --dry-run --arch x86_64

echo ""

# 测试帮助信息
echo "❓ 测试 4: 帮助信息"
echo "------------------"
./scripts/install.sh --help | head -15

echo ""

# 测试发布脚本
echo "🚀 测试 5: 发布脚本"
echo "------------------"
./scripts/publish.sh --help | head -15

echo ""

echo "✅ 所有测试完成！"
echo ""
echo "📖 使用说明:"
echo "  1. 在有网络的环境中运行: ./scripts/install.sh"
echo "  2. 编译特定架构: ./scripts/install.sh --mode compile --arch x86_64"
echo "  3. 安装到指定目录: ./scripts/install.sh -d ~/bin"
echo "  4. 发布新版本: ./scripts/publish.sh v0.8.5"
echo ""
echo "🎯 目标:"
echo "  - 自动检测设备架构"
echo "  - 优先从 GitHub 下载预编译文件"
echo "  - 下载失败时自动编译"
echo "  - 支持多架构发布到 crates.io 和 GitHub"