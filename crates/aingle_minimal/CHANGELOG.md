# Changelog

All notable changes to `aingle_minimal` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2024-12-23

### Added

- **QUIC Transport**: Reliable encrypted UDP transport using `quinn` and `rustls`
  - Self-signed certificate generation with `rcgen`
  - Configurable via `TransportConfig::Quic`
  - Full async support with smol runtime

- **Peer Persistence**: Automatic saving and loading of known peers
  - `save_peers()` / `load_peers()` methods on `MinimalNode`
  - `PeerRecord` struct for serializable peer information
  - Auto-save every 5 minutes during node operation
  - Save on graceful shutdown
  - Quality-based filtering (skip peers with quality < 10)
  - Staleness filtering (skip peers not seen in 24 hours)

- **Complete CLI**: Full-featured command-line interface
  - `run` - Start the node with various options
  - `keygen` - Generate new Ed25519 keypairs
  - `info` - Display node and feature information
  - `config show|iot|low-power|validate` - Configuration management
  - `version` - Show version and build info
  - `bench` - Benchmark entry creation performance

- **Improved Error Handling**: Comprehensive error types per subsystem
  - `NetworkError` - Connection, transport, peer errors
  - `StorageError` - Database, persistence errors
  - `CryptoError` - Key, signature, encryption errors
  - `GossipError` - Protocol, rate-limit errors
  - `SyncError` - Synchronization errors
  - Helper methods: `is_recoverable()`, `requires_restart()`, `code()`

- **E2E Test Suite**: 29 multi-node communication tests
  - Node lifecycle tests
  - Peer management tests
  - Entry creation and retrieval
  - Gossip protocol simulation
  - Network topology tests (star, ring)

- **Bluetooth LE Structure**: Prepared for Desktop and ESP32
  - `btleplug` integration structure for macOS/Linux/Windows
  - `esp32-nimble` feature flag for ESP32 devices
  - `BleManager`, `BleConfig`, `BlePeer` types

### Changed

- Migrated from `xsalsa20poly1305` to `crypto_secretbox` (aead 0.5 API)
- Improved `matches!` macro usage in error handling
- Clarified operator precedence in sensor calculations

### Fixed

- Deprecation warnings in crypto module
- All clippy warnings resolved
- Doctest import for `NetworkError`

### Security

- QUIC transport provides TLS 1.3 encryption by default
- Peer quality scoring prevents low-quality peer accumulation

## [0.1.0] - 2024-12-01

### Added

- Initial release
- Core `MinimalNode` implementation
- CoAP transport with DTLS support
- SQLite and RocksDB storage backends
- Gossip protocol with Bloom filters
- Sync manager for peer synchronization
- Power management for IoT devices
- Sensor abstractions (DHT22, BMP280, MPU6050)
- OTA update framework
- mDNS peer discovery
- Configuration presets: `iot_mode()`, `low_power()`, `production()`

[0.2.0]: https://github.com/AIngleCode/aingle/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/AIngleCode/aingle/releases/tag/v0.1.0
