# AIngle ESP32 BLE Node

Complete example of an AIngle IoT node running on ESP32 with Bluetooth Low Energy (BLE) connectivity.

## Features

- **BLE Mesh Networking**: Discover and connect to other AIngle nodes
- **Sensor Integration**: Read temperature, humidity, and other sensors
- **Power Management**: Adaptive power profiles based on battery level
- **Over-the-Air Updates**: Secure firmware updates (planned)

## Supported Devices

| Device | Target | Status |
|--------|--------|--------|
| ESP32 | `xtensa-esp32-espidf` | ✅ Tested |
| ESP32-C3 | `riscv32imc-esp-espidf` | ✅ Supported |
| ESP32-S3 | `xtensa-esp32s3-espidf` | ✅ Supported |

## Prerequisites

### 1. Install ESP Toolchain

```bash
# Install espup (ESP Rust toolchain manager)
cargo install espup

# Install the ESP toolchain (Xtensa + RISC-V)
espup install

# Source the environment (add to ~/.bashrc or ~/.zshrc)
. $HOME/export-esp.sh
```

### 2. Install Flash Tool

```bash
cargo install espflash
```

### 3. Verify Installation

```bash
# Check Xtensa toolchain
rustup show

# Should show:
# xtensa-esp32-espidf (installed)
# riscv32imc-esp-espidf (installed)
```

## Building

### For ESP32 (Xtensa)

```bash
cd examples/esp32_ble_node
cargo build --release --target xtensa-esp32-espidf
```

### For ESP32-C3 (RISC-V)

```bash
cargo build --release --target riscv32imc-esp-espidf
```

### For ESP32-S3 (Xtensa)

```bash
cargo build --release --target xtensa-esp32s3-espidf
```

## Flashing

Connect your ESP32 via USB, then:

```bash
# For ESP32
espflash flash target/xtensa-esp32-espidf/release/esp32-ble-node

# For ESP32-C3
espflash flash target/riscv32imc-esp-espidf/release/esp32-ble-node

# For ESP32-S3
espflash flash target/xtensa-esp32s3-espidf/release/esp32-ble-node
```

### Monitor Serial Output

```bash
espflash monitor
```

## Configuration

The `sdkconfig.defaults` file contains the ESP-IDF configuration:

| Setting | Value | Description |
|---------|-------|-------------|
| `CONFIG_BT_NIMBLE_ENABLED` | `y` | Use NimBLE BLE stack |
| `CONFIG_BT_NIMBLE_MAX_CONNECTIONS` | `4` | Max simultaneous connections |
| `CONFIG_PM_ENABLE` | `y` | Enable power management |
| `CONFIG_ESP_WIFI_ENABLED` | `n` | Disable WiFi (BLE only) |

## Hardware Connections

### Optional Sensors

| Sensor | I2C Address | Pins |
|--------|-------------|------|
| Temperature (TMP102/LM75) | 0x48 | SDA: GPIO21, SCL: GPIO22 |
| Humidity (SHT31) | 0x44 | SDA: GPIO21, SCL: GPIO22 |
| BME280 (Temp+Humidity+Pressure) | 0x76 | SDA: GPIO21, SCL: GPIO22 |

### Power

- USB: 5V via USB-C/Micro-USB
- Battery: 3.7V LiPo (with voltage divider on ADC pin)

## Architecture

```
┌─────────────────────────────────────────────┐
│              ESP32 BLE Node                 │
├─────────────────────────────────────────────┤
│  ┌─────────────┐  ┌──────────────────────┐  │
│  │   Sensors   │  │    Power Manager     │  │
│  │ Temperature │  │  Battery Monitor     │  │
│  │  Humidity   │  │  Sleep Management    │  │
│  └──────┬──────┘  └──────────┬───────────┘  │
│         │                     │             │
│  ┌──────▼─────────────────────▼──────────┐  │
│  │           AIngle MinimalNode          │  │
│  │     Semantic Graph + Gossip Protocol  │  │
│  └──────────────────┬────────────────────┘  │
│                     │                       │
│  ┌──────────────────▼────────────────────┐  │
│  │           BLE Manager                 │  │
│  │   Advertising │ Scanning │ GATT       │  │
│  └──────────────────┬────────────────────┘  │
│                     │                       │
└─────────────────────┼───────────────────────┘
                      │
              ┌───────▼───────┐
              │  BLE Antenna  │
              │    2.4 GHz    │
              └───────────────┘
```

## BLE Protocol

### Service UUID
```
AIngle Service: 6E400001-B5A3-F393-E0A9-E50E24DCCA9E
```

### Characteristics

| Characteristic | UUID | Properties |
|----------------|------|------------|
| TX (Node → Peer) | `6E400002-B5A3-F393-E0A9-E50E24DCCA9E` | Write |
| RX (Peer → Node) | `6E400003-B5A3-F393-E0A9-E50E24DCCA9E` | Notify |

### Message Format

Sensor data is transmitted as JSON:
```json
{
  "type": "temperature",
  "value": 23.5,
  "unit": "°C",
  "timestamp": 1703260800
}
```

## Memory Usage

| Component | RAM | Flash |
|-----------|-----|-------|
| Runtime | 128KB | - |
| NimBLE Stack | 40KB | 180KB |
| AIngle Minimal | 64KB | 120KB |
| Application | 32KB | 50KB |
| **Total** | **~264KB** | **~350KB** |

ESP32 has 520KB SRAM and 4MB Flash, leaving plenty of headroom.

## Power Consumption

| Mode | Current | Description |
|------|---------|-------------|
| Active (BLE TX) | ~130mA | Transmitting data |
| Active (Idle) | ~80mA | BLE advertising only |
| Light Sleep | ~2mA | Periodic wakeup |
| Deep Sleep | ~10µA | RTC timer wakeup |

With a 2000mAh battery and 10-second sensor intervals:
- Active operation: ~20 hours
- With deep sleep between readings: ~weeks

## Troubleshooting

### Build Errors

**"error: linker `xtensa-esp32-elf-gcc` not found"**
```bash
# Re-run espup and source environment
espup install
. $HOME/export-esp.sh
```

**"CONFIG_BT_ENABLED not found"**
- Ensure `sdkconfig.defaults` is in the project root
- Run `cargo clean` and rebuild

### Runtime Errors

**"Failed to take peripherals"**
- Check that no other code is using ESP-IDF peripherals
- Ensure single-threaded initialization

**"BLE init failed"**
- Verify NimBLE is enabled in sdkconfig
- Check Bluetooth antenna connection

## License

Apache-2.0 - See [LICENSE](../../LICENSE)

## Links

- [AIngle Repository](https://github.com/ApiliumCode/aingle)
- [ESP-RS Book](https://esp-rs.github.io/book/)
- [NimBLE Documentation](https://mynewt.apache.org/latest/network/docs/index.html)
