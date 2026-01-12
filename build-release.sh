#!/bin/bash
# Cross-platform build script for GSC-FQ
# Usage: ./build-release.sh

set -e

VERSION=${VERSION:-"$(cargo metadata --no-deps | grep '\"version\"' | head -1 | awk -F\" '{print $4}')"}
RELEASE_DIR="release"
TARGETS=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "x86_64-pc-windows-msvc"
)

echo "🚀 Building GSC-FQ v${VERSION}"
echo "📦 Release directory: ${RELEASE_DIR}"
echo ""

# Clean previous builds
rm -rf "${RELEASE_DIR}"
mkdir -p "${RELEASE_DIR}"

# Install cross-compilation tools for Linux aarch64
if command -v apt-get >/dev/null 2>&1; then
    echo "📦 Installing cross-compilation tools..."
    sudo apt-get update
    sudo apt-get install -y gcc-aarch64-linux-gnu
    cargo install cross --git https://github.com/cross-rs/cross
fi

# Build for each target
for target in "${TARGETS[@]}"; do
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "🔨 Building for ${target}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Determine artifact name
    case "${target}" in
        x86_64-unknown-linux-gnu)
            artifact_name="gsc-fq-linux-x86_64"
            use_cross=false
            ;;
        aarch64-unknown-linux-gnu)
            artifact_name="gsc-fq-linux-aarch64"
            use_cross=true
            ;;
        x86_64-apple-darwin)
            artifact_name="gsc-fq-macos-x86_64"
            use_cross=false
            ;;
        aarch64-apple-darwin)
            artifact_name="gsc-fq-macos-aarch64"
            use_cross=false
            ;;
        x86_64-pc-windows-msvc)
            artifact_name="gsc-fq-windows-x86_64"
            use_cross=false
            ;;
    esac

    # Build
    if [ "${use_cross}" = "true" ]; then
        cross build --release --target "${target}"
    else
        cargo build --release --target "${target}"
    fi

    # Prepare binary
    echo "📦 Preparing binary..."

    cd "target/${target}/release"

    # Determine binary extension
    if [[ "${target}" == *"windows"* ]]; then
        binary="gsc-fq.exe"
        archive="${artifact_name}.zip"
        mv "gsc-fq.exe" "${binary}" 2>/dev/null || true

        # Create zip archive
        if command -v 7z >/dev/null 2>&1; then
            7z a "../../${RELEASE_DIR}/${archive}" "${binary}"
        elif command -v zip >/dev/null 2>&1; then
            zip "../../${RELEASE_DIR}/${archive}" "${binary}"
        else
            echo "❌ Neither 7z nor zip found. Please install one of them."
            exit 1
        fi
    else
        binary="gsc-fq"
        archive="${artifact_name}.tar.gz"
        mv "gsc-fq" "${binary}" 2>/dev/null || true

        # Create tar.gz archive
        tar czf "../../${RELEASE_DIR}/${archive}" "${binary}"
    fi

    cd - > /dev/null

    echo "✅ Built ${artifact_name}"
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 Build Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Version: ${VERSION}"
echo "Release directory: ${RELEASE_DIR}"
echo ""

# Generate checksums
cd "${RELEASE_DIR}"
echo "🔐 Generating SHA256 checksums..."
sha256sum *.tar.gz *.zip 2>/dev/null | tee checksums.txt > /dev/null || true

echo ""
echo "✅ Build complete!"
echo ""
echo "📦 Artifacts:"
ls -lh
echo ""
echo "🔐 Checksums:"
cat checksums.txt
