<p align="center">
  <img src="https://raw.githubusercontent.com/ApiliumCode/aingle/main/assets/aingle.svg" alt="AIngle" width="280"/>
</p>

<p align="center">
  <strong>The Semantic Infrastructure for Intelligent Applications</strong>
</p>

<p align="center">
  <em>Enabling enterprises to build secure, scalable, and intelligent distributed systems</em>
</p>

<p align="center">
  <a href="https://github.com/ApiliumCode/aingle/actions/workflows/ci.yml"><img src="https://github.com/ApiliumCode/aingle/actions/workflows/ci.yml/badge.svg" alt="Build Status"></a>
  <a href="../LICENSE"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.70%2B-orange.svg" alt="Rust"></a>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#platform-support">Platforms</a> •
  <a href="#esp32-setup">ESP32 Setup</a> •
  <a href="https://apilium.com">Website</a>
</p>

---

# AIngle Minimal

Ultra-lightweight AIngle node for IoT devices with **< 1MB RAM** footprint. Supports ESP32, Raspberry Pi, and desktop platforms.

## Features

| Feature | Description | Use Case |
|---------|-------------|----------|
| `coap` | CoAP protocol (default) | IoT communication |
| `sqlite` | SQLite storage | Edge devices |
| `rocksdb` | RocksDB storage | High-performance |
| `ble` | Bluetooth LE (Desktop) | macOS/Linux/Windows |
| `ble-esp32` | Bluetooth LE (ESP32) | Embedded IoT |
| `webrtc` | WebRTC transport | Browser nodes |
| `hw_wallet` | Ledger/Trezor support | Secure signing |
| `smart_agents` | HOPE AI agents | Edge intelligence |

## Quick Start

```toml
# Cargo.toml
[dependencies]
aingle_minimal = "0.1"
```

```rust
use aingle_minimal::{MinimalNode, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::iot_mode();
    let mut node = MinimalNode::new(config)?;
    smol::block_on(node.run())?;
    Ok(())
}
```

## Platform Support

| Platform | Target | BLE Feature |
|----------|--------|-------------|
| macOS | `aarch64-apple-darwin` | `ble` |
| Linux | `x86_64-unknown-linux-gnu` | `ble` |
| Windows | `x86_64-pc-windows-msvc` | `ble` |
| **ESP32** | `xtensa-esp32-espidf` | `ble-esp32` |
| **ESP32-C3** | `riscv32imc-esp-espidf` | `ble-esp32` |
| **ESP32-S3** | `xtensa-esp32s3-espidf` | `ble-esp32` |

## ESP32 Setup

To compile for ESP32 with Bluetooth LE:

### 1. Install ESP Toolchain

```bash
# Install espup (ESP Rust toolchain installer)
cargo install espup
espup install

# Source the environment (add to .bashrc/.zshrc)
. $HOME/export-esp.sh
```

### 2. Create `sdkconfig.defaults`

```ini
# Required for BLE
CONFIG_BT_ENABLED=y
CONFIG_BT_BLE_ENABLED=y
CONFIG_BT_BLUEDROID_ENABLED=n
CONFIG_BT_NIMBLE_ENABLED=y

# Optional: Persist bonding info
CONFIG_BT_NIMBLE_NVS_PERSIST=y
```

### 3. Build

```bash
# For ESP32
cargo build --target xtensa-esp32-espidf --features ble-esp32

# For ESP32-C3 (RISC-V)
cargo build --target riscv32imc-esp-espidf --features ble-esp32

# For ESP32-S3
cargo build --target xtensa-esp32s3-espidf --features ble-esp32
```

### 4. Flash

```bash
espflash flash target/xtensa-esp32-espidf/release/your-app
```

## Memory Budget

| Component | Budget |
|-----------|--------|
| Runtime | 128KB |
| Crypto | 64KB |
| Network | 128KB |
| Storage | 128KB |
| App | 64KB |
| **Total** | **512KB** |

## License

Apache 2.0 - See [LICENSE](../LICENSE)

## Links

- [Documentation](https://docs.rs/aingle_minimal)
- [Repository](https://github.com/ApiliumCode/aingle)
- [Apilium](https://apilium.com)
