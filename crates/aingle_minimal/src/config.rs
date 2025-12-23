//! Configuration for the minimal AIngle node.
//!
//! This module provides configuration types and presets for different deployment
//! scenarios, from resource-constrained IoT devices to production servers.
//!
//! # Configuration Presets
//!
//! The [`Config`] type provides several preset configurations optimized for
//! different use cases:
//!
//! - [`Config::iot_mode()`] - Sub-second publishing, minimal memory footprint
//! - [`Config::low_power()`] - Battery-optimized for long runtime
//! - [`Config::production()`] - High-performance server configuration
//!
//! # Examples
//!
//! ```
//! # use aingle_minimal::Config;
//! // Use IoT preset for embedded devices
//! let config = Config::iot_mode();
//!
//! // Or create a custom configuration
//! let mut config = Config::default();
//! config.memory_limit = 256 * 1024; // 256KB
//! config.enable_mdns = true;
//! ```

use crate::{ENV_IOT_MODE, ENV_PUBLISH_INTERVAL};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Defines the power consumption profile for the node.
///
/// Power modes allow the node to adapt its behavior based on available energy,
/// which is crucial for battery-operated IoT devices.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{Config, PowerMode};
/// let mut config = Config::test_mode();
/// config.power_mode = PowerMode::Low;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerMode {
    /// Full performance with no power-saving measures.
    ///
    /// Use this mode when the device is connected to a stable power source
    /// or when maximum performance is required.
    Full,
    /// A balance between performance and power consumption.
    ///
    /// This is the default mode, suitable for most use cases.
    Balanced,
    /// Prioritizes low power usage by reducing activity.
    ///
    /// Gossip intervals are longer and network activity is minimized.
    Low,
    /// Critical mode for minimal activity when battery is very low.
    ///
    /// The node operates at minimal capacity to preserve remaining battery.
    Critical,
}

impl Default for PowerMode {
    fn default() -> Self {
        Self::Balanced
    }
}

/// Defines the network transport to be used by the node.
///
/// Different transports are optimized for different environments. CoAP is ideal
/// for IoT devices, QUIC for high-performance servers, and Mesh for device-to-device
/// communication without infrastructure.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::TransportConfig;
/// // Use CoAP on default port
/// let coap = TransportConfig::Coap {
///     bind_addr: "0.0.0.0".to_string(),
///     port: 5683,
/// };
///
/// // Use QUIC for production
/// let quic = TransportConfig::Quic {
///     bind_addr: "0.0.0.0".to_string(),
///     port: 8443,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportConfig {
    /// An in-memory transport for testing purposes only.
    ///
    /// This transport doesn't actually send data over the network and is useful
    /// for unit tests and local development.
    Memory,
    /// A UDP/QUIC-based transport for high-performance networking.
    ///
    /// QUIC provides reliable, encrypted communication with multiplexing and
    /// is suitable for production servers with good network connectivity.
    Quic {
        /// The IP address to bind to (e.g., "0.0.0.0" for all interfaces).
        bind_addr: String,
        /// The UDP port to listen on.
        port: u16,
    },
    /// A CoAP-based transport, optimized for IoT devices.
    ///
    /// CoAP (Constrained Application Protocol) is lightweight and designed for
    /// resource-constrained devices. It's the recommended transport for IoT deployments.
    Coap {
        /// The IP address to bind to (e.g., "0.0.0.0" for all interfaces).
        bind_addr: String,
        /// The UDP port to listen on (default: 5683).
        port: u16,
    },
    /// Configuration for mesh networking between devices.
    ///
    /// Mesh networking enables peer-to-peer communication without requiring
    /// network infrastructure like WiFi routers.
    Mesh {
        /// The mesh networking technology to use.
        mode: MeshMode,
    },
    /// WebRTC transport for browser-based clients.
    ///
    /// Enables AIngle nodes to run in web browsers and communicate with
    /// native nodes through WebRTC data channels.
    #[cfg(feature = "webrtc")]
    WebRtc {
        /// STUN server URL for NAT traversal (e.g., "stun:stun.l.google.com:19302")
        stun_server: String,
        /// Optional TURN server URL for relay when direct connection fails
        turn_server: Option<String>,
        /// Port for signaling server (WebSocket)
        signaling_port: u16,
    },
    /// Bluetooth Low Energy transport for IoT mesh networking.
    ///
    /// Enables low-power, short-range communication ideal for
    /// battery-powered IoT devices.
    #[cfg(feature = "ble")]
    Ble {
        /// Device name to advertise
        device_name: String,
        /// Enable mesh relay (forward messages from other nodes)
        mesh_relay: bool,
        /// Transmission power in dBm (-40 to +4)
        tx_power: i8,
    },
}

impl Default for TransportConfig {
    /// Defaults to CoAP, as this crate is optimized for IoT.
    fn default() -> Self {
        Self::Coap {
            bind_addr: "0.0.0.0".to_string(),
            port: 5683,
        }
    }
}

/// The type of mesh networking to use for device-to-device communication.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{TransportConfig, MeshMode};
/// let config = TransportConfig::Mesh {
///     mode: MeshMode::BluetoothLE,
/// };
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MeshMode {
    /// WiFi Direct for peer-to-peer WiFi communication.
    ///
    /// Provides high bandwidth but higher power consumption than BLE or LoRa.
    WiFiDirect,
    /// Bluetooth Low Energy for short-range, low-power networking.
    ///
    /// Ideal for battery-operated devices that need to communicate within
    /// a few meters of each other.
    BluetoothLE,
    /// LoRa for long-range, low-power, low-bandwidth communication.
    ///
    /// Can communicate over several kilometers with minimal power, but at
    /// much lower data rates than WiFi or BLE.
    LoRa,
    /// Automatically select the best available mesh mode.
    ///
    /// The node will detect available mesh technologies and choose the
    /// most appropriate one based on device capabilities.
    Auto,
}

/// Configuration for the gossip protocol that synchronizes data across the network.
///
/// Gossip is the mechanism by which nodes exchange information about what data they
/// have, discover new records, and maintain consistency across the network.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::GossipConfig;
/// // Create an IoT-optimized gossip config
/// let config = GossipConfig::iot_mode();
/// assert_eq!(config.max_peers, 4);
///
/// // Or create a custom configuration
/// let mut config = GossipConfig::default();
/// config.max_peers = 16;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    /// The delay between iterations of the main gossip loop.
    ///
    /// Shorter delays mean faster data propagation but higher CPU and network usage.
    pub loop_delay: Duration,
    /// The delay to wait after a successful gossip exchange.
    ///
    /// After successfully syncing with peers, the node can wait longer before
    /// gossiping again since it's likely up to date.
    pub success_delay: Duration,
    /// The delay to wait after a failed gossip exchange.
    ///
    /// Failed exchanges trigger longer delays to avoid wasting resources on
    /// unreachable or problematic peers.
    pub error_delay: Duration,
    /// The target output bandwidth in Mbps to avoid saturating the network.
    ///
    /// The gossip system will rate-limit itself to stay within this bandwidth target.
    pub output_target_mbps: f64,
    /// The maximum number of peers to attempt to gossip with in a single cycle.
    ///
    /// Higher values increase redundancy and data propagation speed but use more
    /// network bandwidth and CPU.
    pub max_peers: usize,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            loop_delay: Duration::from_millis(1000),
            success_delay: Duration::from_secs(60),
            error_delay: Duration::from_secs(300),
            output_target_mbps: 0.5,
            max_peers: 8,
        }
    }
}

impl GossipConfig {
    /// Returns a configuration optimized for IoT devices with more aggressive timing.
    ///
    /// This preset features faster gossip cycles for sub-second data propagation,
    /// which is essential for time-sensitive IoT applications like sensor networks.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::GossipConfig;
    /// let config = GossipConfig::iot_mode();
    /// // Fast loop delay for quick data propagation
    /// assert_eq!(config.loop_delay.as_millis(), 100);
    /// ```
    pub fn iot_mode() -> Self {
        Self {
            loop_delay: Duration::from_millis(100),
            success_delay: Duration::from_secs(5),
            error_delay: Duration::from_secs(30),
            output_target_mbps: 5.0,
            max_peers: 4,
        }
    }

    /// Returns a configuration optimized for low-power devices with longer delays.
    ///
    /// This preset minimizes network activity and CPU usage to extend battery life.
    /// Data propagation is slower but power consumption is significantly reduced.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::GossipConfig;
    /// let config = GossipConfig::low_power();
    /// // Longer delays to save power
    /// assert!(config.loop_delay.as_millis() > 1000);
    /// ```
    pub fn low_power() -> Self {
        Self {
            loop_delay: Duration::from_millis(5000),
            success_delay: Duration::from_secs(300),
            error_delay: Duration::from_secs(600),
            output_target_mbps: 0.1,
            max_peers: 2,
        }
    }
}

/// The type of storage backend to use for the node's database.
///
/// Different backends offer different tradeoffs between performance, resource usage,
/// and platform compatibility.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{StorageConfig, StorageBackendType};
/// // Use SQLite for IoT devices
/// let config = StorageConfig {
///     backend: StorageBackendType::Sqlite,
///     db_path: "./data.db".to_string(),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackendType {
    /// Use SQLite for storage.
    ///
    /// SQLite is a lightweight, single-file database that's ideal for IoT devices
    /// and embedded systems. It has minimal dependencies and works well on
    /// resource-constrained platforms.
    Sqlite,
    /// Use RocksDB for storage.
    ///
    /// RocksDB is a high-performance, embedded key-value store suitable for
    /// production servers handling high throughput. It requires more resources
    /// than SQLite but offers better performance under load.
    Rocksdb,
    /// Use in-memory storage.
    ///
    /// This backend stores data only in RAM and is intended for testing purposes.
    /// Data is lost when the node stops.
    Memory,
}

impl Default for StorageBackendType {
    fn default() -> Self {
        // SQLite is the default for IoT compatibility.
        Self::Sqlite
    }
}

impl std::fmt::Display for StorageBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite => write!(f, "sqlite"),
            Self::Rocksdb => write!(f, "rocksdb"),
            Self::Memory => write!(f, "memory"),
        }
    }
}

/// Configuration for the storage system.
///
/// Storage configuration controls how and where the node persists its data,
/// including size limits and pruning behavior.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::StorageConfig;
/// // Create an IoT-optimized storage config
/// let config = StorageConfig::sqlite("./iot_data.db");
///
/// // Or use RocksDB for production
/// let config = StorageConfig::rocksdb("./data");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// The [`StorageBackendType`] to use.
    pub backend: StorageBackendType,
    /// The path to the database file or directory.
    ///
    /// For SQLite, this is the path to the .db file. For RocksDB, this is the
    /// directory path. This field is ignored for the `Memory` backend.
    pub db_path: String,
    /// The maximum allowed size of the database in bytes.
    ///
    /// When the database approaches this limit, pruning will be triggered
    /// if `aggressive_pruning` is enabled.
    pub max_size: usize,
    /// If `true`, the storage will aggressively prune old data to stay within `max_size`.
    ///
    /// Pruning removes the oldest entries while preserving recent data as
    /// specified by `keep_recent`.
    pub aggressive_pruning: bool,
    /// The number of recent entries to always keep, even when pruning.
    ///
    /// This ensures that important recent data is never lost during cleanup.
    pub keep_recent: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackendType::default(),
            db_path: "./aingle_data.db".to_string(),
            max_size: 5 * 1024 * 1024, // 5MB
            aggressive_pruning: true,
            keep_recent: 1000,
        }
    }
}

impl StorageConfig {
    /// Creates a new SQLite storage configuration.
    ///
    /// SQLite is recommended for IoT devices and embedded systems due to its
    /// lightweight footprint and minimal dependencies.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::StorageConfig;
    /// let config = StorageConfig::sqlite("./sensor_data.db");
    /// assert_eq!(config.db_path, "./sensor_data.db");
    /// ```
    pub fn sqlite(path: &str) -> Self {
        Self {
            backend: StorageBackendType::Sqlite,
            db_path: path.to_string(),
            ..Default::default()
        }
    }

    /// Creates a new RocksDB storage configuration, optimized for production loads.
    ///
    /// RocksDB offers better performance than SQLite for high-throughput scenarios
    /// and is recommended for production servers.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::StorageConfig;
    /// let config = StorageConfig::rocksdb("./rocksdb_data");
    /// assert_eq!(config.max_size, 100 * 1024 * 1024); // 100MB
    /// ```
    pub fn rocksdb(path: &str) -> Self {
        Self {
            backend: StorageBackendType::Rocksdb,
            db_path: path.to_string(),
            max_size: 100 * 1024 * 1024, // 100MB for production
            aggressive_pruning: false,
            keep_recent: 100_000,
        }
    }

    /// Creates a new in-memory storage configuration for testing.
    ///
    /// This configuration stores all data in RAM and is useful for unit tests
    /// and development. Data is not persisted when the node stops.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::StorageConfig;
    /// let config = StorageConfig::memory();
    /// // Data only exists in memory
    /// ```
    pub fn memory() -> Self {
        Self {
            backend: StorageBackendType::Memory,
            db_path: ":memory:".to_string(),
            max_size: 10 * 1024 * 1024,
            aggressive_pruning: false,
            keep_recent: 10_000,
        }
    }
}

/// The main configuration for a [`MinimalNode`](crate::MinimalNode).
///
/// This struct contains all settings needed to configure and run an AIngle node,
/// from network transport to storage backend to power management.
///
/// # Configuration Presets
///
/// Use one of the preset methods for common scenarios:
/// - [`Config::iot_mode()`] - Optimized for IoT devices with sub-second publishing
/// - [`Config::low_power()`] - Battery-optimized for extended runtime
/// - [`Config::production()`] - High-performance server configuration
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{Config, MinimalNode};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Use an IoT preset
/// let config = Config::iot_mode();
/// let node = MinimalNode::new(config)?;
///
/// // Or customize the default configuration
/// let mut config = Config::default();
/// config.memory_limit = 256 * 1024; // 256KB
/// config.enable_mdns = true;
/// config.validate()?; // Always validate before use
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// A unique identifier for the node.
    ///
    /// If `None`, a random identifier will be generated automatically at startup.
    pub node_id: Option<String>,

    /// The interval at which the node publishes its data to the network.
    ///
    /// A value of `Duration::ZERO` means immediate (sub-second) publishing,
    /// which is ideal for real-time IoT applications but uses more bandwidth.
    pub publish_interval: Duration,

    /// The power consumption profile for the node.
    ///
    /// See [`PowerMode`] for available options.
    pub power_mode: PowerMode,

    /// The network transport configuration.
    ///
    /// See [`TransportConfig`] for available transport options.
    pub transport: TransportConfig,

    /// The gossip protocol configuration.
    ///
    /// See [`GossipConfig`] for tuning options.
    pub gossip: GossipConfig,

    /// The storage system configuration.
    ///
    /// See [`StorageConfig`] for backend options.
    pub storage: StorageConfig,

    /// The total memory limit for the node in bytes.
    ///
    /// The node will attempt to stay within this limit, though enforcement
    /// depends on platform capabilities. Minimum is 64KB.
    pub memory_limit: usize,

    /// If `true`, the node will collect and expose performance metrics.
    ///
    /// Metrics add slight overhead but are useful for monitoring and debugging.
    pub enable_metrics: bool,

    /// If `true`, the node will use mDNS for automatic peer discovery.
    ///
    /// mDNS allows nodes to discover each other on the local network without
    /// requiring manual peer configuration.
    pub enable_mdns: bool,

    /// The logging level.
    ///
    /// Valid values: "trace", "debug", "info", "warn", "error".
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node_id: None,
            publish_interval: Duration::from_secs(5),
            power_mode: PowerMode::Balanced,
            transport: TransportConfig::default(),
            gossip: GossipConfig::default(),
            storage: StorageConfig::default(),
            memory_limit: 512 * 1024, // 512KB
            enable_metrics: false,
            enable_mdns: true, // Enable by default for auto-discovery
            log_level: "info".to_string(),
        }
    }
}

impl Config {
    /// Returns a configuration optimized for IoT devices.
    ///
    /// This configuration features:
    /// - Sub-second publish interval (immediate publishing)
    /// - Fast gossip cycles for quick data propagation
    /// - Minimal memory footprint (256KB limit)
    /// - CoAP transport (lightweight for constrained devices)
    /// - SQLite storage (1MB max with aggressive pruning)
    /// - mDNS enabled for automatic peer discovery
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{Config, MinimalNode};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create an IoT node
    /// let config = Config::iot_mode();
    /// let mut node = MinimalNode::new(config)?;
    ///
    /// // Publish sensor data with sub-second confirmation
    /// let hash = node.create_entry("temperature: 23.5C")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn iot_mode() -> Self {
        Self {
            node_id: None,
            publish_interval: Duration::ZERO, // Immediate publish
            power_mode: PowerMode::Full,
            transport: TransportConfig::Coap {
                bind_addr: "0.0.0.0".to_string(),
                port: 5683,
            },
            gossip: GossipConfig::iot_mode(),
            storage: StorageConfig {
                backend: StorageBackendType::Sqlite,
                db_path: "./aingle_iot.db".to_string(),
                max_size: 1024 * 1024, // 1MB
                aggressive_pruning: true,
                keep_recent: 100,
            },
            memory_limit: 256 * 1024, // 256KB
            enable_metrics: false,
            enable_mdns: true, // Auto-discovery for IoT networks
            log_level: "warn".to_string(),
        }
    }

    /// Returns a configuration optimized for low-power, battery-operated devices.
    ///
    /// This configuration features:
    /// - Longer publish interval (30 seconds)
    /// - Slow gossip cycles to minimize network activity
    /// - Minimal memory footprint (128KB limit)
    /// - CoAP transport
    /// - SQLite storage (512KB max with aggressive pruning)
    /// - mDNS disabled to save power
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{Config, MinimalNode, PowerMode};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create a battery-optimized node
    /// let config = Config::low_power();
    /// assert_eq!(config.power_mode, PowerMode::Low);
    /// assert_eq!(config.memory_limit, 128 * 1024);
    ///
    /// let mut node = MinimalNode::new(config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn low_power() -> Self {
        Self {
            node_id: None,
            publish_interval: Duration::from_secs(30),
            power_mode: PowerMode::Low,
            transport: TransportConfig::Coap {
                bind_addr: "0.0.0.0".to_string(),
                port: 5683,
            },
            gossip: GossipConfig::low_power(),
            storage: StorageConfig {
                backend: StorageBackendType::Sqlite,
                db_path: "./aingle_lowpower.db".to_string(),
                max_size: 512 * 1024, // 512KB
                aggressive_pruning: true,
                keep_recent: 50,
            },
            memory_limit: 128 * 1024, // 128KB
            enable_metrics: false,
            enable_mdns: false, // Disabled to save power
            log_level: "error".to_string(),
        }
    }

    /// Returns a configuration suitable for production servers.
    ///
    /// This configuration features:
    /// - Fast publish interval (100ms)
    /// - QUIC transport for high-performance networking
    /// - RocksDB storage (100MB max)
    /// - Large memory limit (512MB)
    /// - Metrics enabled for monitoring
    /// - mDNS enabled for auto-discovery
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use aingle_minimal::{Config, MinimalNode};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Create a production server node
    ///     let config = Config::production("./production_data");
    ///     let mut node = MinimalNode::new(config)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn production(db_path: &str) -> Self {
        Self {
            node_id: None,
            publish_interval: Duration::from_millis(100),
            power_mode: PowerMode::Full,
            transport: TransportConfig::Quic {
                bind_addr: "0.0.0.0".to_string(),
                port: 8443,
            },
            gossip: GossipConfig::default(),
            storage: StorageConfig::rocksdb(db_path),
            memory_limit: 512 * 1024 * 1024, // 512MB
            enable_metrics: true,
            enable_mdns: true, // Auto-discovery in production
            log_level: "info".to_string(),
        }
    }

    /// Creates a `Config` from environment variables.
    ///
    /// This method reads configuration from the following environment variables:
    /// - `AINGLE_IOT_MODE` - If set, use IoT mode as base config
    /// - `AINGLE_PUBLISH_INTERVAL_MS` - Override publish interval in milliseconds
    /// - `AINGLE_GOSSIP_LOOP_ITERATION_DELAY_MS` - Override gossip loop delay in milliseconds
    /// - `AINGLE_MEMORY_LIMIT_KB` - Override memory limit in kilobytes
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Config;
    /// // Set environment variables (in practice, these would be set externally)
    /// std::env::set_var("AINGLE_IOT_MODE", "1");
    /// std::env::set_var("AINGLE_MEMORY_LIMIT_KB", "512");
    ///
    /// let config = Config::from_env();
    /// assert_eq!(config.memory_limit, 512 * 1024);
    /// # std::env::remove_var("AINGLE_IOT_MODE");
    /// # std::env::remove_var("AINGLE_MEMORY_LIMIT_KB");
    /// ```
    pub fn from_env() -> Self {
        let mut config = if std::env::var(ENV_IOT_MODE).is_ok() {
            Self::iot_mode()
        } else {
            Self::default()
        };

        // Override publish interval from env
        if let Ok(interval_str) = std::env::var(ENV_PUBLISH_INTERVAL) {
            if let Ok(interval_ms) = interval_str.parse::<u64>() {
                config.publish_interval = Duration::from_millis(interval_ms);
            }
        }

        // Override gossip loop delay
        if let Ok(delay_str) = std::env::var("AINGLE_GOSSIP_LOOP_ITERATION_DELAY_MS") {
            if let Ok(delay_ms) = delay_str.parse::<u64>() {
                config.gossip.loop_delay = Duration::from_millis(delay_ms);
            }
        }

        // Override memory limit
        if let Ok(limit_str) = std::env::var("AINGLE_MEMORY_LIMIT_KB") {
            if let Ok(limit_kb) = limit_str.parse::<usize>() {
                config.memory_limit = limit_kb * 1024;
            }
        }

        config
    }

    /// Returns a configuration optimized for testing.
    ///
    /// This configuration features:
    /// - In-memory storage (no disk I/O)
    /// - Memory transport (no network)
    /// - Fast publish interval (10ms)
    /// - Small memory footprint (64KB)
    /// - mDNS disabled
    /// - Metrics disabled
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{Config, MinimalNode};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = Config::test_mode();
    /// let mut node = MinimalNode::new(config)?;
    /// // Node uses in-memory storage, perfect for tests
    /// # Ok(())
    /// # }
    /// ```
    pub fn test_mode() -> Self {
        Self {
            node_id: None,
            publish_interval: Duration::from_millis(10),
            power_mode: PowerMode::Full,
            transport: TransportConfig::Memory,
            gossip: GossipConfig {
                loop_delay: Duration::from_millis(10),
                success_delay: Duration::from_millis(50),
                error_delay: Duration::from_millis(100),
                output_target_mbps: 10.0,
                max_peers: 5,
            },
            storage: StorageConfig::memory(),
            memory_limit: 64 * 1024, // 64KB
            enable_metrics: false,
            enable_mdns: false,
            log_level: "debug".to_string(),
        }
    }

    /// Validates the configuration to ensure settings are reasonable.
    ///
    /// This method checks that:
    /// - Memory limit is at least 64KB
    /// - Storage max size is at least 256KB
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MemoryTooLow`] if memory limit is below 64KB.
    /// Returns [`ConfigError::StorageTooLow`] if storage max size is below 256KB.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Config;
    /// let config = Config::default();
    /// assert!(config.validate().is_ok());
    ///
    /// let mut bad_config = Config::default();
    /// bad_config.memory_limit = 1024; // Only 1KB - too small
    /// assert!(bad_config.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.memory_limit < 64 * 1024 {
            return Err(ConfigError::MemoryTooLow(self.memory_limit));
        }

        if self.storage.max_size < 256 * 1024 {
            return Err(ConfigError::StorageTooLow(self.storage.max_size));
        }

        Ok(())
    }
}

/// Defines errors that can occur during configuration validation.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::Config;
/// let mut config = Config::default();
/// config.memory_limit = 1024; // Too low
///
/// match config.validate() {
///     Err(e) => println!("Config error: {}", e),
///     Ok(_) => println!("Config is valid"),
/// }
/// ```
#[derive(Debug)]
pub enum ConfigError {
    /// The specified memory limit is below the minimum requirement of 64KB.
    MemoryTooLow(usize),
    /// The specified storage limit is below the minimum requirement of 256KB.
    StorageTooLow(usize),
    /// The configuration contains an invalid setting.
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MemoryTooLow(size) => {
                write!(f, "Memory limit too low: {} bytes (minimum 64KB)", size)
            }
            ConfigError::StorageTooLow(size) => {
                write!(f, "Storage limit too low: {} bytes (minimum 256KB)", size)
            }
            ConfigError::Invalid(msg) => write!(f, "Invalid configuration: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_iot_config() {
        let config = Config::iot_mode();
        assert_eq!(config.publish_interval, Duration::ZERO);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_low_power_config() {
        let config = Config::low_power();
        assert_eq!(config.power_mode, PowerMode::Low);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_production_config() {
        let config = Config::production("./test_db");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_power_mode_default() {
        let mode: PowerMode = Default::default();
        assert_eq!(mode, PowerMode::Balanced);
    }

    #[test]
    fn test_power_mode_variants() {
        let modes = vec![
            PowerMode::Full,
            PowerMode::Balanced,
            PowerMode::Low,
            PowerMode::Critical,
        ];

        for mode in modes {
            let debug_str = format!("{:?}", mode);
            assert!(!debug_str.is_empty());
        }
    }

    #[test]
    fn test_transport_config_default() {
        let config: TransportConfig = Default::default();
        if let TransportConfig::Coap { bind_addr, port } = config {
            assert_eq!(bind_addr, "0.0.0.0");
            assert_eq!(port, 5683);
        } else {
            panic!("Expected Coap transport");
        }
    }

    #[test]
    fn test_gossip_config_default() {
        let config = GossipConfig::default();
        assert!(config.loop_delay > Duration::ZERO);
        assert!(config.max_peers > 0);
    }

    #[test]
    fn test_gossip_config_iot() {
        let config = GossipConfig::iot_mode();
        assert!(config.loop_delay < Duration::from_secs(1));
    }

    #[test]
    fn test_gossip_config_low_power() {
        let config = GossipConfig::low_power();
        assert!(config.loop_delay > Duration::from_secs(1));
    }

    #[test]
    fn test_storage_backend_type_default() {
        let backend: StorageBackendType = Default::default();
        assert_eq!(backend, StorageBackendType::Sqlite);
    }

    #[test]
    fn test_storage_backend_type_display() {
        assert_eq!(StorageBackendType::Sqlite.to_string(), "sqlite");
        assert_eq!(StorageBackendType::Rocksdb.to_string(), "rocksdb");
        assert_eq!(StorageBackendType::Memory.to_string(), "memory");
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.backend, StorageBackendType::Sqlite);
        assert!(config.max_size > 0);
        assert!(config.aggressive_pruning);
    }

    #[test]
    fn test_storage_config_sqlite() {
        let config = StorageConfig::sqlite("./test.db");
        assert_eq!(config.backend, StorageBackendType::Sqlite);
        assert_eq!(config.db_path, "./test.db");
    }

    #[test]
    fn test_storage_config_rocksdb() {
        let config = StorageConfig::rocksdb("./rocksdb");
        assert_eq!(config.backend, StorageBackendType::Rocksdb);
        assert_eq!(config.db_path, "./rocksdb");
        assert!(!config.aggressive_pruning);
        assert!(config.max_size > StorageConfig::default().max_size);
    }

    #[test]
    fn test_storage_config_memory() {
        let config = StorageConfig::memory();
        assert_eq!(config.backend, StorageBackendType::Memory);
        assert_eq!(config.db_path, ":memory:");
    }

    #[test]
    fn test_mesh_mode_variants() {
        let modes = vec![MeshMode::WiFiDirect, MeshMode::BluetoothLE, MeshMode::LoRa];

        for mode in modes {
            let debug_str = format!("{:?}", mode);
            assert!(!debug_str.is_empty());
        }
    }

    #[test]
    fn test_config_from_env_no_env() {
        let config = Config::from_env();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_memory() {
        let mut config = Config::default();
        config.memory_limit = 100; // Too small
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_storage() {
        let mut config = Config::default();
        config.storage.max_size = 100; // Too small
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_power_mode_serialization() {
        let mode = PowerMode::Low;
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: PowerMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, parsed);
    }

    #[test]
    fn test_storage_backend_type_serialization() {
        let backend = StorageBackendType::Rocksdb;
        let json = serde_json::to_string(&backend).unwrap();
        assert!(json.contains("rocksdb"));
        let parsed: StorageBackendType = serde_json::from_str(&json).unwrap();
        assert_eq!(backend, parsed);
    }

    #[test]
    fn test_gossip_config_serialization() {
        let config = GossipConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: GossipConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.max_peers, parsed.max_peers);
    }

    #[test]
    fn test_storage_config_serialization() {
        let config = StorageConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: StorageConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.db_path, parsed.db_path);
    }

    #[test]
    fn test_transport_config_serialization() {
        let config = TransportConfig::Coap {
            bind_addr: "0.0.0.0".to_string(),
            port: 5683,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("Coap"));
    }

    #[test]
    fn test_config_error_display() {
        let error = ConfigError::MemoryTooLow(100);
        let display = error.to_string();
        assert!(display.contains("100"));

        let error = ConfigError::StorageTooLow(256);
        let display = error.to_string();
        assert!(display.contains("256"));

        let error = ConfigError::Invalid("bad value".to_string());
        let display = error.to_string();
        assert!(display.contains("bad value"));
    }

    #[test]
    fn test_gossip_config_clone() {
        let config = GossipConfig::iot_mode();
        let cloned = config.clone();
        assert_eq!(config.loop_delay, cloned.loop_delay);
        assert_eq!(config.max_peers, cloned.max_peers);
    }

    #[test]
    fn test_storage_config_clone() {
        let config = StorageConfig::rocksdb("./test");
        let cloned = config.clone();
        assert_eq!(config.db_path, cloned.db_path);
        assert_eq!(config.backend, cloned.backend);
    }

    #[test]
    fn test_config_clone() {
        let config = Config::iot_mode();
        let cloned = config.clone();
        assert_eq!(config.power_mode, cloned.power_mode);
        assert_eq!(config.enable_mdns, cloned.enable_mdns);
    }

    #[test]
    fn test_config_fields() {
        let config = Config::default();
        // Test default values
        assert!(config.memory_limit > 0);
        // node_id is Option<String>
        assert!(config.node_id.is_none() || !config.node_id.as_ref().unwrap().is_empty());
    }
}
