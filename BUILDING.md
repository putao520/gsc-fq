# Building GSC-FQ

This guide explains how to build GSC-FQ for different platforms.

## Prerequisites

- Rust toolchain (1.70+)
- For cross-compilation: `cross` tool and appropriate compilers

## Quick Start

### Build for Current Platform

```bash
./build.sh
```

This will build a binary for your current platform and create a compressed archive in the `release/` directory.

### Build for All Platforms

```bash
./build-release.sh
```

This will build binaries for all supported platforms:
- Linux x86_64
- Linux aarch64
- macOS x86_64
- macOS aarch64 (Apple Silicon)
- Windows x86_64

## Platform-Specific Instructions

### Linux

#### Native Build (x86_64)

```bash
cargo build --release
# Binary: target/release/gsc-fq
```

#### Cross-Compile (aarch64)

```bash
# Install cross-compilation tools
sudo apt-get install gcc-aarch64-linux-gnu
cargo install cross --git https://github.com/cross-rs/cross

# Build
cross build --release --target aarch64-unknown-linux-gnu
# Binary: target/aarch64-unknown-linux-gnu/release/gsc-fq
```

### macOS

#### Native Build (x86_64 or aarch64)

```bash
cargo build --release
# Binary: target/release/gsc-fq
```

No cross-compilation needed - just build on the target architecture.

### Windows

#### Native Build (x86_64)

```powershell
cargo build --release
# Binary: target\release\gsc-fq.exe
```

## GitHub Actions

The project uses GitHub Actions to automatically build binaries for all platforms when a new tag is pushed.

### Trigger Automatic Build

```bash
git tag v0.9.0
git push origin v0.9.0
```

This will automatically:
1. Build binaries for all platforms
2. Generate SHA256 checksums
3. Create a GitHub Release with all binaries

### Manual Trigger

Go to: https://github.com/putao520/gsc-fq/actions/workflows/release.yml
Click "Run workflow" → Select branch → Click "Run workflow"

## Build Artifacts

After building, you'll find the following files in `release/`:

```
release/
├── gsc-fq-linux-x86_64.tar.gz
├── gsc-fq-linux-aarch64.tar.gz
├── gsc-fq-macos-x86_64.tar.gz
├── gsc-fq-macos-aarch64.tar.gz
├── gsc-fq-windows-x86_64.zip
└── checksums.txt
```

## Verifying Checksums

Always verify the downloaded binary using SHA256 checksums:

```bash
# Download checksums
wget https://github.com/putao520/gsc-fq/releases/download/v0.9.0/checksums.txt

# Download binary
wget https://github.com/putao520/gsc-fq/releases/download/v0.9.0/gsc-fq-linux-x86_64.tar.gz

# Verify
sha256sum -c checksums.txt
```

## Troubleshooting

### Cross-Compilation Errors

If you encounter errors while cross-compiling:

1. Ensure you have the correct GCC cross-compiler installed:
   ```bash
   sudo apt-get install gcc-aarch64-linux-gnu
   ```

2. Install the `cross` tool:
   ```bash
   cargo install cross --git https://github.com/cross-rs/cross
   ```

3. Use `cross` instead of `cargo` for cross-compilation:
   ```bash
   cross build --release --target aarch64-unknown-linux-gnu
   ```

### macOS Code Signing

To code-sign the macOS binary:

```bash
codesign --force --deep --sign "Developer ID Application: Your Name" target/release/gsc-fq
```

### Windows Build Errors

If you encounter linking errors on Windows:

1. Install Rust with the MSVC toolchain
2. Install Visual Studio Build Tools
3. Use `x86_64-pc-windows-msvc` target

## Advanced: Custom Build Flags

### Enable Debug Symbols

```bash
cargo build --release
# With debug symbols
RUSTFLAGS=-g cargo build --release
```

### Strip Binary

```bash
strip target/release/gsc-fq
```

### Optimize for Size

```bash
cargo build --release --opt-level=z
```

## Continuous Integration

See `.github/workflows/release.yml` for the complete CI/CD configuration.

## Building Packages

### Debian Package

```bash
cargo install cargo-deb
cargo deb
# Output: target/debian/*.deb
```

### RPM Package

```bash
cargo install cargo-rpm
cargo rpm build
# Output: target/release/generate-rpm/
```

### Arch Linux Package

Use the provided PKGBUILD in `packaging/arch/`:

```bash
makepkg -si
```

## Support

For build issues, please open a GitHub issue:
https://github.com/putao520/gsc-fq/issues
