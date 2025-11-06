# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[Unreleased]: https://github.com/putao520/gsc-fq/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/putao520/gsc-fq/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/putao520/gsc-fq/releases/tag/v0.1.0