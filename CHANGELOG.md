# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.2] - 2026-01-12

### Performance
- **Reduced proxy latency by 50%**: From >500ms to ~250ms (public network)
- **Connection pool optimization**: Full 50-connection preheat (previously only 3)
- **Eliminated double handshake**: Fallback now creates only 1 connection (previously 2)

### Fixed
- **Connection pool synchronization**: Wait for preheat completion before accepting connections
- **Removed premature exit**: Connection pool now creates all 50 connections instead of stopping at 3
- **Fallback optimization**: Direct connection instead of test + reconnect

### Changed
- `connection_pool.rs`: Removed early exit logic in preheat_pool()
- `server.rs`: Synchronous connection pool start
- `stealth_handler.rs`: Optimized fallback to single connection attempt

---

## [0.9.1] - 2026-01-12

### Fixed
- **Hybrid mode support**: Now supports running forward proxy and reverse proxy server simultaneously
- Removed single-mode limitation - all configured services start based on config file
- Services that can run together:
  - `[[proxies]]` - Forward proxy
  - `[reverse_proxy_server]` - Reverse proxy server
  - `[reverse_proxy_client]` - Reverse proxy client

### Changed
- `runtime.rs`: Removed RunMode enum, simplified service startup logic
- `loader.rs`: Updated `get_runtime_mode()` to only return display mode (not used for service filtering)
- Services now start in parallel using `tokio::spawn`

---

## [0.9.0] - 2026-01-12

### Performance
- **macOS optimization**: 256KB buffer size achieving **4.02x** speedup (1MB scenario)
- **Windows optimization**: Fixed 512KB performance regression, using 256KB buffer
- **Linux splice() integration**: Zero-copy kernel data transfer for TCP streams
- **Adaptive buffer sizing**: Platform-specific optimal buffer selection
- **Memory optimization**: 84% reduction in memory usage (10MB → 1.63MB)

### Added
- `splice_optimizer.rs`: Linux splice() zero-copy optimizer module
- `adaptive_stream.rs`: TCP stream-specific adaptive copy with splice() support
- `adaptive_copy.rs`: Known-size data adaptive transfer
- Platform-specific buffer size optimization (macOS/Linux/Windows)

### Testing
- **E2E test coverage improved**: 48% → 91% (+43%)
- **Network resilience tests**: Connection interruption, service unreachable, timeout handling
- **High concurrency tests**: 200 concurrent connections (40x increase)
- **Edge case tests**: 100MB file transfer with SHA256 verification
- **Data validation tests**: Bidirectional integrity, ordering guarantees, fragmented transfer
- **15 new test cases**: All passing with SHA256 hash verification

### Changed
- `stealth_handler.rs`: Updated with platform-specific optimization comments
- `zero_copy.rs`: Platform-specific implementations for macOS, Windows, and Linux
- Buffer sizes: Optimized per platform based on benchmark results

### Fixed
- Resource leaks in ReverseProxyServer/Client connection handling
- TOTP RFC 6238 test vector compatibility
- Large file transfer now uses streaming processing

---

## [0.2.0] - 2025-11-06

### Fixed
- Fixed compilation issues on Linux environments
- Added `all` feature to `socket2` dependency for `set_reuse_port` support
- Added `zerocopy` feature to `nix` dependency for `splice` system call support
- Fixed missing imports in `src/utils/system.rs`
- Implemented missing `read_sysfs_value` function
- Fixed libc import conflicts by using `nix::libc`
- Fixed file descriptor handling in `check_splice_availability` function
- Resolved all compilation warnings

### Tested
- All 33 unit tests passing
- End-to-end integration tests passing
- Real-world proxy functionality verified

## [0.1.0] - Initial Release

### Added
- High-performance TCP proxy forwarding functionality
- TOML configuration file support
- Debug mode with detailed logging
- System requirements checking
- Connection pooling and statistics
- Graceful shutdown handling
- Docker support with Dockerfile and docker-compose

---

[Unreleased]: https://github.com/putao520/gsc-fq/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/putao520/gsc-fq/compare/v0.2.0...v0.9.0
[0.2.0]: https://github.com/putao520/gsc-fq/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/putao520/gsc-fq/releases/tag/v0.1.0