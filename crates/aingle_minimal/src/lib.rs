#![doc = include_str!("../README.md")]
#![allow(rustdoc::bare_urls)]
#![allow(rustdoc::invalid_html_tags)]
//! # AIngle Minimal - Ultra-Lightweight IoT Node
//!
//! Minimal AIngle node implementation optimized for resource-constrained IoT devices.
//!
//! ## Overview
//!
//! This crate provides a complete AIngle node that runs on devices with **less than 1MB RAM**,
//! making it suitable for ESP32, Arduino, Raspberry Pi Pico, and similar embedded systems.
//!
//! ## Features
//!
//! - **Ultra-light footprint**: Target <512KB RAM, <5MB storage
//! - **Sub-second confirmation**: Configurable publish intervals (default 5s)
//! - **Zero-fee transactions**: No staking or gas fees required
//! - **Mesh networking**: WiFi Direct, BLE, LoRa, Zigbee support
//! - **Battery-aware**: Adaptive power modes (deep sleep, light sleep, active)
//! - **CoAP Protocol**: Lightweight alternative to HTTP for IoT
//! - **DTLS Security**: Encrypted communications with PSK or certificates
//! - **OTA Updates**: Secure over-the-air firmware updates
//! - **Smart Agents**: Optional AI capabilities for edge intelligence
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use aingle_minimal::{MinimalNode, Config};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create node with IoT-optimized config
//!     let config = Config::iot_mode();
//!     let mut node = MinimalNode::new(config)?;
//!
//!     // Run the node
//!     smol::block_on(node.run())?;
//!     Ok(())
//! }
//! ```
//!
//! ## Advanced Examples
//!
//! ### Sensor Integration
//!
//! ```rust,no_run
//! use aingle_minimal::{MinimalNode, SensorManager, SensorType, Config};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut node = MinimalNode::new(Config::iot_mode())?;
//! let mut sensors = SensorManager::new();
//!
//! // Add temperature sensor
//! sensors.add_sensor(SensorType::Temperature, 0x48)?;
//!
//! // Read and publish sensor data
//! let reading = sensors.read(SensorType::Temperature)?;
//! node.publish_sensor_data(reading)?;
//! # Ok(())
//! # }
//! ```
//!
//! ### CoAP Server
//!
//! ```rust,no_run
//! use aingle_minimal::{CoapServer, CoapConfig};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let config = CoapConfig::default();
//! let server = CoapServer::new(config).await?;
//!
//! // Register resource handlers
//! server.register_resource("/temperature", |req| {
//!     // Handle GET /temperature
//!     Ok("23.5".into())
//! });
//!
//! server.run().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Power Management
//!
//! ```rust,no_run
//! use aingle_minimal::{PowerManager, PowerProfile, BatteryInfo};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut power = PowerManager::new();
//!
//! // Check battery and adjust power mode
//! let battery = power.battery_info()?;
//! if battery.percentage < 20.0 {
//!     power.set_profile(PowerProfile::PowerSave)?;
//! }
//!
//! // Sleep when idle
//! power.sleep(std::time::Duration::from_secs(10))?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Smart Agent (Edge AI)
//!
//! ```rust,ignore
//! use aingle_minimal::{SmartNode, SmartNodeConfig, SensorAdapter};
//!
//! let config = SmartNodeConfig::default();
//! let mut smart_node = SmartNode::new(config)?;
//!
//! // Agent learns from sensor patterns
//! let adapter = SensorAdapter::new();
//! smart_node.attach_sensor(adapter);
//!
//! // Run agent loop
//! loop {
//!     smart_node.step()?;
//!     smol::Timer::after(std::time::Duration::from_secs(1)).await;
//! }
//! ```
//!
//! ## Memory Budget
//!
//! | Component | Budget |
//! |-----------|--------|
//! | Runtime | 128KB |
//! | Crypto | 64KB |
//! | Network | 128KB |
//! | Storage | 128KB |
//! | App | 64KB |
//! | **Total** | **512KB** |
//!
//! ## Feature Flags
//!
//! | Feature | Description | Dependencies |
//! |---------|-------------|--------------|
//! | `coap` | CoAP server and client (default) | coap-lite |
//! | `sqlite` | SQLite storage backend | rusqlite |
//! | `rocksdb` | RocksDB storage backend (faster) | rocksdb |
//! | `webrtc` | WebRTC transport for browsers | webrtc, bytes |
//! | `ble` | Bluetooth LE for Desktop (macOS/Linux/Windows) | btleplug, uuid |
//! | `ble-esp32` | Bluetooth LE for ESP32 devices | esp32-nimble |
//! | `hw_wallet` | Hardware wallet support (Ledger/Trezor) | ledger-transport-hid |
//! | `ai_memory` | Titans memory system for agents | titans_memory |
//! | `smart_agents` | HOPE agents integration | hope_agents |
//! | `no_std` | Compile without standard library | - |
//!
//! ## Platform Support
//!
//! | Platform | Target | BLE Feature |
//! |----------|--------|-------------|
//! | macOS | x86_64-apple-darwin, aarch64-apple-darwin | `ble` |
//! | Linux | x86_64-unknown-linux-gnu | `ble` |
//! | Windows | x86_64-pc-windows-msvc | `ble` |
//! | ESP32 | xtensa-esp32-espidf | `ble-esp32` |
//! | ESP32-C3 | riscv32imc-esp-espidf | `ble-esp32` |
//! | ESP32-S3 | xtensa-esp32s3-espidf | `ble-esp32` |
//! | Raspberry Pi Pico | thumbv6m-none-eabi | - |
//!
//! ## ESP32 Setup (ble-esp32 feature)
//!
//! To compile for ESP32 with Bluetooth LE support:
//!
//! ### 1. Install ESP-IDF Toolchain
//! ```bash
//! # Install espup (ESP Rust toolchain installer)
//! cargo install espup
//! espup install
//!
//! # Source the environment
//! . $HOME/export-esp.sh
//! ```
//!
//! ### 2. Create sdkconfig.defaults
//! ```text
//! CONFIG_BT_ENABLED=y
//! CONFIG_BT_BLE_ENABLED=y
//! CONFIG_BT_BLUEDROID_ENABLED=n
//! CONFIG_BT_NIMBLE_ENABLED=y
//! CONFIG_BT_NIMBLE_NVS_PERSIST=y
//! ```
//!
//! ### 3. Build for ESP32
//! ```bash
//! cargo build --target xtensa-esp32-espidf --features ble-esp32
//! ```

#![cfg_attr(feature = "no_std", no_std)]

#[cfg(feature = "no_std")]
extern crate alloc;

#[cfg(feature = "coap")]
pub mod coap;
pub mod config;
pub mod crypto;
pub mod discovery;
#[cfg(feature = "coap")]
pub mod dtls;
pub mod error;
pub mod gossip;
pub mod graph;
#[cfg(feature = "ai_memory")]
pub mod memory;
pub mod network;
pub mod node;
pub mod ota;
pub mod power;
pub mod sensors;
#[cfg(feature = "smart_agents")]
pub mod smart;
pub mod sync;
pub mod types;
#[cfg(feature = "webrtc")]
pub mod webrtc;
#[cfg(feature = "ble")]
pub mod bluetooth;
#[cfg(feature = "hw_wallet")]
pub mod wallet;

// Storage - trait is always available
pub mod storage_trait;

// Storage backends (feature-gated)
#[cfg(feature = "rocksdb")]
pub mod rocks_storage;
#[cfg(feature = "sqlite")]
pub mod storage;

// Storage factory for dynamic backend selection
#[cfg(any(feature = "sqlite", feature = "rocksdb"))]
pub mod storage_factory;

// Re-export storage types
pub use config::StorageBackendType;
#[cfg(any(feature = "sqlite", feature = "rocksdb"))]
pub use storage_factory::DynamicStorage;
pub use storage_trait::{StorageBackend, StorageStats};

// Re-exports
#[cfg(feature = "coap")]
pub use coap::{CoapConfig, CoapServer};
pub use config::Config;
pub use discovery::{DiscoveredPeer, Discovery};
#[cfg(feature = "coap")]
pub use dtls::{DtlsConfig, DtlsSession, SecureCoap, SecurityMode};
pub use error::{Error, Result};
pub use gossip::{BloomFilter, GossipManager, GossipStats, MessagePriority, TokenBucket};
pub use graph::{
    GraphStats as SemanticGraphStats, SemanticGraph, SemanticQuery, SemanticTriple, TripleObject,
};
#[cfg(feature = "ai_memory")]
pub use memory::IoTMemory;
pub use node::MinimalNode;
pub use ota::{OtaManager, UpdateChannel, UpdateInfo, UpdateState};
pub use power::{BatteryInfo, PowerManager, PowerProfile};
pub use sensors::{CalibrationParams, Sensor, SensorManager, SensorReading, SensorType};
#[cfg(feature = "smart_agents")]
pub use smart::{IoTPolicyBuilder, SensorAdapter, SmartNode, SmartNodeConfig, SmartNodeStats};
pub use sync::{PeerSyncState, SyncManager, SyncResult, SyncStats};
#[cfg(feature = "webrtc")]
pub use webrtc::{
    ConnectionState, PeerConnection, SignalingClient, SignalingConfig, SignalingMessage,
    SignalingServer, WebRtcConfig, WebRtcServer, WebRtcStats,
};
#[cfg(feature = "ble")]
pub use bluetooth::{BleConfig, BleManager, BlePeer, BleState, BleStats};
#[cfg(feature = "hw_wallet")]
pub use wallet::{
    ApduCommand, ApduResponse, DerivationPath, HwPublicKey, HwSignature, WalletConfig,
    WalletInfo, WalletManager, WalletState, WalletStats, WalletType,
};
pub use types::*;

/// Version information for the crate.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::VERSION;
/// println!("AIngle Minimal version: {}", VERSION);
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum supported Rust version.
///
/// This crate requires at least Rust 1.70 to compile.
pub const MSRV: &str = "1.70";

/// Target memory budget in bytes (512KB).
///
/// This is the design goal for the node's total memory footprint,
/// making it suitable for resource-constrained IoT devices.
///
/// See [`Config::memory_limit`](crate::Config::memory_limit) to configure
/// the actual memory limit for a node.
pub const MEMORY_BUDGET: usize = 512 * 1024; // 512KB

/// Environment variable for configuring publish interval.
///
/// Set this to a number of milliseconds to override the default publish interval.
///
/// # Examples
///
/// ```bash
/// # Set publish interval to 1 second
/// export AINGLE_PUBLISH_INTERVAL_MS=1000
/// ```
pub const ENV_PUBLISH_INTERVAL: &str = "AINGLE_PUBLISH_INTERVAL_MS";

/// Environment variable for enabling IoT mode.
///
/// When set (to any value), the node will use [`Config::iot_mode()`] as its base
/// configuration instead of the default.
///
/// # Examples
///
/// ```bash
/// # Enable IoT mode
/// export AINGLE_IOT_MODE=1
/// ```
pub const ENV_IOT_MODE: &str = "AINGLE_IOT_MODE";
