#!/bin/bash
# Simple build script for current platform
# Usage: ./build.sh

set -e

VERSION=${VERSION:-"$(grep '^version = ' Cargo.toml | head -1 | awk -F\" '{print $2}')"}
RELEASE_DIR="release"

echo "🚀 Building GSC-FQ v${VERSION} for current platform"
echo ""

# Clean previous builds
rm -rf "${RELEASE_DIR}"
mkdir -p "${RELEASE_DIR}"

# Build release binary
echo "🔨 Building release binary..."
cargo build --release

# Determine platform info
OS=$(uname -s)
ARCH=$(uname -m)

case "${OS}" in
    Linux*)     PLATFORM="linux";;
    Darwin*)    PLATFORM="macos";;
    *)          PLATFORM="unknown";;
esac

case "${ARCH}" in
    x86_64)     ARCH_NAME="x86_64";;
    aarch64)    ARCH_NAME="aarch64";;
    arm64)      ARCH_NAME="aarch64";;
    *)          ARCH_NAME="unknown";;
esac

ARTIFACT_NAME="gsc-fq-${PLATFORM}-${ARCH_NAME}"

# Copy binary to release directory
echo "📦 Preparing binary..."
if [ "${OS}" = "Linux" ] || [ "${OS}" = "Darwin" ]; then
    cp "target/release/gsc-fq" "${RELEASE_DIR}/gsc-fq"
    cd "${RELEASE_DIR}"
    tar czf "${ARTIFACT_NAME}.tar.gz" gsc-fq
    echo "✅ Created ${ARTIFACT_NAME}.tar.gz"
elif [ "${OS}" = "Windows" ]; then
    cp "target/release/gsc-fq.exe" "${RELEASE_DIR}/gsc-fq.exe"
    cd "${RELEASE_DIR}"
    if command -v 7z >/dev/null 2>&1; then
        7z a "${ARTIFACT_NAME}.zip" gsc-fq.exe
    elif command -v zip >/dev/null 2>&1; then
        zip "${ARTIFACT_NAME}.zip" gsc-fq.exe
    else
        echo "❌ Neither 7z nor zip found. Please install one of them."
        exit 1
    fi
    echo "✅ Created ${ARTIFACT_NAME}.zip"
fi

cd - > /dev/null

echo ""
echo "✅ Build complete!"
echo "📦 Artifact: ${RELEASE_DIR}/${ARTIFACT_NAME}"
echo ""
ls -lh "${RELEASE_DIR}"
