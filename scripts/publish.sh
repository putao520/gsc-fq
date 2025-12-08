#!/bin/bash

# GSC-FQ 发布脚本
# 发布到 Cargo.io 和 GitHub，同时构建多架构二进制文件

set -e

# 配置
VERSION=${1:-$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "gsc-fq") | .version')}
GITHUB_REPO="putao520/gsc-fq"
CRATE_NAME="gsc-fq"
REGISTRY="crates.io"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 全局变量
SKIP_BUILD=false
SKIP_PUBLISH=false
DRY_RUN=false
BUILD_ARCHS=()

# 显示帮助信息
show_help() {
    cat << EOF
GSC-FQ 发布脚本

用法: $0 [版本] [选项]

参数:
    版本      指定发布版本号 (默认: 从 Cargo.toml 读取)

选项:
    -h, --help              显示此帮助信息
    -b, --build             仅构建多架构二进制文件
    -p, --publish           仅发布到 crates.io
    -n, --dry-run           模拟运行，不实际执行
    --arch ARCHS           指定构建的架构 (逗号分隔)
                           支持的架构: x86_64, aarch64, armv7, i686
                           默认: x86_64,aarch64,armv7

示例:
    $0                      # 发布当前版本
    $0 v0.8.5               # 发布 v0.8.5 版本
    $0 --build              # 仅构建多架构二进制文件
    $0 --arch x86_64,aarch64 # 仅构建指定架构
    $0 -n                   # 模拟运行

EOF
}

# 解析命令行参数
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_help
                exit 0
                ;;
            -b|--build)
                SKIP_PUBLISH=true
                shift
                ;;
            -p|--publish)
                SKIP_BUILD=true
                shift
                ;;
            -n|--dry-run)
                DRY_RUN=true
                shift
                ;;
            --arch)
                BUILD_ARCHS=(${2//,/ })
                shift 2
                ;;
            *)
                if [[ $1 =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                    VERSION=$1
                    # 移除 v 前缀（如果存在）
                    VERSION=${VERSION#v}
                else
                    echo -e "${RED}❌ 无效的版本号: $1${NC}"
                    exit 1
                fi
                shift
                ;;
        esac
    done

    # 设置默认架构
    if [ ${#BUILD_ARCHS[@]} -eq 0 ]; then
        BUILD_ARCHS=("x86_64" "aarch64" "armv7")
    fi
}

# 预检查
pre_checks() {
    echo -e "${BLUE}🔍 执行预检查...${NC}"

    # 检查 Git 仓库状态
    if [ ! -d ".git" ]; then
        echo -e "${RED}❌ 不在 Git 仓库中${NC}"
        exit 1
    fi

    # 检查是否有未提交的更改
    if [ -n "$(git status --porcelain)" ]; then
        echo -e "${YELLOW}⚠️  有未提交的更改:${NC}"
        git status --porcelain
        echo -e "${YELLOW}💡 请先提交更改再发布${NC}"
        read -p "是否继续? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi

    # 检查 Cargo.toml 中的版本
    local cargo_version=$(grep '^version = ' Cargo.toml | head -1 | awk -F'"' '{print $2}')
    if [ "$VERSION" != "$cargo_version" ]; then
        echo -e "${YELLOW}⚠️  版本不匹配${NC}"
        echo -e "   命令行参数: v${VERSION}"
        echo -e "   Cargo.toml:  v${cargo_version}"
        if [ "$DRY_RUN" = false ]; then
            read -p "是否更新 Cargo.toml 到 v${VERSION}? (y/N): " -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                sed -i "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
                echo -e "${GREEN}✅ Cargo.toml 已更新${NC}"
            else
                echo -e "${YELLOW}💡 请手动同步版本号${NC}"
                exit 1
            fi
        fi
    fi

    # 检查 API token
    if [ "$SKIP_PUBLISH" = false ] && [ -z "$CARGO_REGISTRY_TOKEN" ]; then
        echo -e "${YELLOW}⚠️  CARGO_REGISTRY_TOKEN 未设置${NC}"
        echo -e "${YELLOW}💡 设置方法: export CARGO_REGISTRY_TOKEN=your_token${NC}"
        if [ "$DRY_RUN" = false ]; then
            read -p "是否继续? (y/N): " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                exit 1
            fi
        fi
    fi

    # 检查 GitHub token
    if [ -z "$GITHUB_TOKEN" ]; then
        echo -e "${YELLOW}⚠️  GITHUB_TOKEN 未设置${NC}"
        echo -e "${YELLOW}💡 设置方法: export GITHUB_TOKEN=your_token${NC}"
        if [ "$DRY_RUN" = false ]; then
            read -p "是否继续? (y/N): " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                exit 1
            fi
        fi
    fi

    echo -e "${GREEN}✅ 预检查通过${NC}"
}

# 构建多架构二进制文件
build_binaries() {
    echo -e "${BLUE}🏗️  构建多架构二进制文件...${NC}"
    echo -e "${CYAN}📋 架构列表: ${BUILD_ARCHS[*]}${NC}"

    # 检查 Rust 工具链
    if ! command -v rustup &> /dev/null; then
        echo -e "${RED}❌ Rust 未安装${NC}"
        exit 1
    fi

    # 创建发布目录
    local release_dir="target/release-artifacts"
    rm -rf "$release_dir"
    mkdir -p "$release_dir"

    # 构建每个架构
    for arch in "${BUILD_ARCHS[@]}"; do
        echo -e "${BLUE}🔨 构建 ${arch}...${NC}"

        local target=""
        case $arch in
            x86_64)
                target="x86_64-unknown-linux-musl"
                ;;
            aarch64)
                target="aarch64-unknown-linux-musl"
                ;;
            armv7)
                target="arm-unknown-linux-musleabihf"
                ;;
            i686)
                target="i686-unknown-linux-musl"
                ;;
            *)
                echo -e "${RED}❌ 不支持的架构: $arch${NC}"
                continue
                ;;
        esac

        # 检查目标是否已安装
        if ! rustup target list --installed | grep -q "$target"; then
            echo -e "${YELLOW}📦 安装 Rust 目标: $target${NC}"
            if [ "$DRY_RUN" = false ]; then
                rustup target add "$target"
            fi
        fi

        # 构建二进制文件
        if [ "$DRY_RUN" = false ]; then
            # 设置环境变量
            export CC_${target//-/_}=musl-gcc
            export CARGO_TARGET_${target//-/_}_LINKER=musl-gcc

            # 构建命令
            local build_cmd="cargo build --target $target --release"

            if [ "$arch" = "armv7" ]; then
                build_cmd="cargo build --target $target --release"
            fi

            echo -e "${CYAN}🔧 执行: $build_cmd${NC}"

            if ! $build_cmd; then
                echo -e "${RED}❌ ${arch} 构建失败${NC}"
                continue
            fi

            # 复制并重命名二进制文件
            local src_binary="target/$target/release/gsc-fq"
            local dst_binary="$release_dir/gsc-fq-linux-$arch"

            if [ -f "$src_binary" ]; then
                cp "$src_binary" "$dst_binary"
                chmod +x "$dst_binary"
                echo -e "${GREEN}✅ ${arch} 构建成功: $dst_binary${NC}"
            else
                echo -e "${RED}❌ ${arch} 二进制文件不存在${NC}"
            fi
        else
            echo -e "${CYAN}🔧 模拟构建: cargo build --target $target --release${NC}"
        fi
    done

    # 创建校验和文件
    if [ "$DRY_RUN" = false ] && [ -d "$release_dir" ]; then
        echo -e "${BLUE}🔐 创建校验和文件...${NC}"
        cd "$release_dir"
        sha256sum gsc-fq-* > SHA256SUMS
        cd - > /dev/null
    fi

    echo -e "${GREEN}✅ 构建完成${NC}"
}

# 发布到 crates.io
publish_to_crates() {
    echo -e "${BLUE}📦 发布到 crates.io...${NC}"

    # 检查是否有更改
    local changes=$(git diff --name-only HEAD~1 HEAD | grep -E '\.(rs|toml|md)$' | wc -l)
    if [ "$changes" -eq 0 ] && [ "$DRY_RUN" = false ]; then
        echo -e "${YELLOW}⚠️  没有代码更改，跳过发布${NC}"
        return 0
    fi

    # 运行 cargo publish
    local publish_cmd="cargo publish --dry-run"
    if [ "$DRY_RUN" = false ]; then
        publish_cmd="cargo publish"
    fi

    echo -e "${CYAN}🚀 执行: $publish_cmd${NC}"

    if $publish_cmd; then
        echo -e "${GREEN}✅ 发布到 crates.io 成功${NC}"
    else
        echo -e "${RED}❌ 发布到 crates.io 失败${NC}"
        if [ "$DRY_RUN" = false ]; then
            exit 1
        fi
    fi
}

# 创建 GitHub Release
create_github_release() {
    echo -e "${BLUE}🚀 创建 GitHub Release...${NC}"

    # 检查 gh 命令
    if ! command -v gh &> /dev/null; then
        echo -e "${YELLOW}⚠️  gh 命令未安装，跳过 GitHub Release 创建${NC}"
        echo -e "${YELLOW}💡 安装方法: https://github.com/cli/cli#installation${NC}"
        return 0
    fi

    # 创建 Release 说明
    local release_notes=$(mktemp)
    cat << EOF > "$release_notes"
## v${VERSION}

### 新特性
- 🚀 支持 ARMv7 (32位 ARM) 架构
- 🔧 智能安装脚本：自动检测设备架构并下载对应二进制文件
- 📦 多架构 Docker 支持：支持 linux/amd64, linux/arm64, linux/armv7
- 🐛 修复了 ARM64 设备的编译问题

### 技术改进
- 优化了交叉编译配置
- 改进了依赖管理
- 增强了构建脚本

### 安装方法
\`\`\`bash
# 自动检测架构并安装
curl -sSL https://raw.githubusercontent.com/${GITHUB_REPO}/main/scripts/install.sh | bash

# 或手动安装
./scripts/install.sh
\`\`\`

### 架构支持
- x86_64 (AMD64)
- aarch64 (ARM64)
- armv7 (ARM32)

EOF

    # 创建 GitHub Release
    local gh_cmd="gh release create v${VERSION} --title \"v${VERSION}\" --notes-file \"$release_notes\""

    if [ "$DRY_RUN" = false ]; then
        # 上传二进制文件
        for arch in "${BUILD_ARCHS[@]}"; do
            local binary_file="target/release-artifacts/gsc-fq-linux-$arch"
            if [ -f "$binary_file" ]; then
                gh_cmd="$gh_cmd \"$binary_file\""
            fi
        done

        # 上传校验和文件
        if [ -f "target/release-artifacts/SHA256SUMS" ]; then
            gh_cmd="$gh_cmd \"target/release-artifacts/SHA256SUMS\""
        fi
    else
        echo -e "${CYAN}🔧 模拟创建 GitHub Release${NC}"
        echo -e "${CYAN}📝 Release 说明:${NC}"
        cat "$release_notes"
    fi

    echo -e "${CYAN}🚀 执行: $gh_cmd${NC}"

    if [ "$DRY_RUN" = false ]; then
        if eval $gh_cmd; then
            echo -e "${GREEN}✅ GitHub Release 创建成功${NC}"
        else
            echo -e "${RED}❌ GitHub Release 创建失败${NC}"
            exit 1
        fi
    else
        echo -e "${CYAN}🔧 模拟执行完成${NC}"
    fi

    # 清理临时文件
    rm -f "$release_notes"
}

# 清理
cleanup() {
    # 清理构建文件
    if [ "$DRY_RUN" = false ]; then
        cargo clean
    fi

    echo -e "${GREEN}🎉 发布流程完成！${NC}"
    echo -e "${CYAN}🔗 查看 Release: https://github.com/${GITHUB_REPO}/releases${NC}"
    echo -e "${CYAN}📦 查看 Cargo.io: https://crates.io/crates/gsc-fq${NC}"
}

# 主函数
main() {
    # 显示欢迎信息
    echo -e "${BLUE}🚀 GSC-FQ 发布脚本${NC}"
    echo -e "${BLUE}==================${NC}"
    echo -e "${CYAN}📦 版本: v${VERSION}${NC}"
    echo ""

    # 解析参数
    parse_args "$@"

    # 预检查
    if [ "$SKIP_BUILD" = false ] || [ "$SKIP_PUBLISH" = false ]; then
        pre_checks
    fi

    # 构建二进制文件
    if [ "$SKIP_BUILD" = false ]; then
        build_binaries
    fi

    # 发布到 crates.io
    if [ "$SKIP_PUBLISH" = false ]; then
        publish_to_crates
    fi

    # 创建 GitHub Release
    if [ "$SKIP_PUBLISH" = false ]; then
        create_github_release
    fi

    # 清理
    cleanup
}

# 运行主函数
main "$@"