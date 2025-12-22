//! Bluetooth LE Mesh Transport for IoT Devices
//!
//! This module enables AIngle nodes to communicate via Bluetooth Low Energy (BLE),
//! which is ideal for low-power IoT deployments where WiFi is not available.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐      BLE       ┌─────────────────┐
//! │   IoT Device A  │◄──────────────►│   IoT Device B  │
//! │   (Peripheral)  │   GATT/ATT     │   (Central)     │
//! └─────────────────┘                └─────────────────┘
//!         │                                  │
//!         └──────────► Mesh ◄────────────────┘
//!                   (Flood/Relay)
//! ```
//!
//! # Features
//!
//! - **Low Power**: Designed for battery-powered devices
//! - **Mesh Networking**: Nodes can relay messages for extended range
//! - **GATT Service**: Standard BLE service for data exchange
//! - **Scanning/Advertising**: Automatic peer discovery
//!
//! # BLE Service Structure
//!
//! - **Service UUID**: `6e400001-b5a3-f393-e0a9-e50e24dcca9e` (Nordic UART)
//! - **TX Characteristic**: Node sends messages
//! - **RX Characteristic**: Node receives messages

use crate::error::{Error, Result};
use crate::network::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Desktop BLE (btleplug) - macOS, Linux, Windows
#[cfg(feature = "ble")]
use btleplug::api::{
    Central, Characteristic, Manager as BtManager, Peripheral as BtPeripheral, ScanFilter,
    WriteType,
};
#[cfg(feature = "ble")]
use btleplug::platform::{Adapter, Manager, Peripheral};
#[cfg(feature = "ble")]
use smol::channel::{Receiver, Sender};
#[cfg(feature = "ble")]
use uuid::Uuid;

// ESP32 BLE (esp32-nimble) - ESP32, ESP32-C3, ESP32-S3
#[cfg(feature = "ble-esp32")]
use esp32_nimble::{uuid128, BLEAdvertisedDevice, BLEClient, BLEDevice, BLEScan};
#[cfg(feature = "ble-esp32")]
use std::sync::Arc;

/// AIngle BLE Service UUID (Nordic UART Service compatible)
pub const AINGLE_SERVICE_UUID: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";

/// TX Characteristic UUID (node sends)
pub const TX_CHAR_UUID: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";

/// RX Characteristic UUID (node receives)
pub const RX_CHAR_UUID: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";

/// Configuration for Bluetooth LE transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleConfig {
    /// Device name to advertise
    pub device_name: String,
    /// Enable mesh relay (forward messages from other nodes)
    pub mesh_relay: bool,
    /// Maximum transmission power in dBm (-40 to +4)
    pub tx_power: i8,
    /// Scan interval in milliseconds
    pub scan_interval_ms: u32,
    /// Scan window in milliseconds (must be <= scan_interval)
    pub scan_window_ms: u32,
    /// Advertising interval in milliseconds
    pub advertising_interval_ms: u32,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Maximum number of simultaneous connections
    pub max_connections: usize,
    /// Enable passive scanning (no scan responses)
    pub passive_scan: bool,
}

impl Default for BleConfig {
    fn default() -> Self {
        Self {
            device_name: "AIngle-Node".to_string(),
            mesh_relay: true,
            tx_power: 0, // 0 dBm (1mW)
            scan_interval_ms: 100,
            scan_window_ms: 50,
            advertising_interval_ms: 100,
            connection_timeout: Duration::from_secs(10),
            max_connections: 4,
            passive_scan: false,
        }
    }
}

impl BleConfig {
    /// Create configuration for low-power mode
    pub fn low_power() -> Self {
        Self {
            device_name: "AIngle-LP".to_string(),
            mesh_relay: false, // Save power by not relaying
            tx_power: -12,     // Lower power consumption
            scan_interval_ms: 1000,
            scan_window_ms: 100,
            advertising_interval_ms: 500,
            connection_timeout: Duration::from_secs(30),
            max_connections: 2,
            passive_scan: true,
        }
    }

    /// Create configuration for mesh relay node
    pub fn mesh_relay() -> Self {
        Self {
            device_name: "AIngle-Relay".to_string(),
            mesh_relay: true,
            tx_power: 4, // Maximum power for relay
            scan_interval_ms: 50,
            scan_window_ms: 30,
            advertising_interval_ms: 50,
            connection_timeout: Duration::from_secs(5),
            max_connections: 8,
            passive_scan: false,
        }
    }
}

/// BLE connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleState {
    /// Not initialized
    Uninitialized,
    /// Initialized but not connected
    Idle,
    /// Scanning for devices
    Scanning,
    /// Advertising presence
    Advertising,
    /// Connected to one or more peers
    Connected,
    /// Error state
    Error,
}

/// Information about a discovered BLE peer
#[derive(Debug, Clone)]
pub struct BlePeer {
    /// Unique peer identifier (BLE address)
    pub address: String,
    /// Device name (if available)
    pub name: Option<String>,
    /// RSSI (signal strength in dBm)
    pub rssi: i16,
    /// Time when peer was discovered
    pub discovered_at: Instant,
    /// Time of last activity
    pub last_seen: Instant,
    /// Whether peer supports AIngle service
    pub supports_aingle: bool,
    /// Whether currently connected
    pub connected: bool,
}

impl BlePeer {
    /// Create a new BLE peer from discovery
    pub fn new(address: &str, rssi: i16) -> Self {
        let now = Instant::now();
        Self {
            address: address.to_string(),
            name: None,
            rssi,
            discovered_at: now,
            last_seen: now,
            supports_aingle: false,
            connected: false,
        }
    }

    /// Update peer with new RSSI reading
    pub fn update_rssi(&mut self, rssi: i16) {
        self.rssi = rssi;
        self.last_seen = Instant::now();
    }

    /// Check if peer is stale (not seen recently)
    pub fn is_stale(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() > timeout
    }
}

/// Statistics for BLE transport
#[derive(Debug, Clone, Default)]
pub struct BleStats {
    /// Number of messages sent
    pub messages_sent: u64,
    /// Number of messages received
    pub messages_received: u64,
    /// Number of bytes sent
    pub bytes_sent: u64,
    /// Number of bytes received
    pub bytes_received: u64,
    /// Number of messages relayed (mesh)
    pub messages_relayed: u64,
    /// Number of connection attempts
    pub connection_attempts: u64,
    /// Number of successful connections
    pub connections_established: u64,
    /// Number of scan cycles completed
    pub scans_completed: u64,
    /// Average RSSI of connected peers
    pub avg_rssi: i16,
}

/// BLE transport manager
///
/// Manages Bluetooth Low Energy connections and mesh networking.
///
/// Supports two backends:
/// - `ble` feature: Desktop platforms via btleplug (macOS, Linux, Windows)
/// - `ble-esp32` feature: ESP32 devices via esp32-nimble
pub struct BleManager {
    /// Configuration
    config: BleConfig,
    /// Current state
    state: BleState,
    /// Known peers
    peers: HashMap<String, BlePeer>,
    /// Transport statistics
    stats: BleStats,
    /// Local device address
    local_address: Option<String>,
    /// Running state
    running: bool,

    // ========== Desktop (btleplug) fields ==========
    /// BLE adapter (btleplug)
    #[cfg(feature = "ble")]
    adapter: Option<Adapter>,
    /// Connected peripherals (btleplug)
    #[cfg(feature = "ble")]
    peripherals: HashMap<String, Peripheral>,
    /// TX characteristics cache per peripheral
    #[cfg(feature = "ble")]
    tx_chars: HashMap<String, Characteristic>,
    /// Channel for receiving notifications
    #[cfg(feature = "ble")]
    notification_rx: Option<Receiver<(String, Vec<u8>)>>,
    /// Channel for sending notifications (used by notification handler)
    #[cfg(feature = "ble")]
    notification_tx: Option<Sender<(String, Vec<u8>)>>,

    // ========== ESP32 (esp32-nimble) fields ==========
    /// ESP32 BLE device singleton reference
    #[cfg(feature = "ble-esp32")]
    ble_device: Option<&'static BLEDevice>,
    /// Connected BLE clients (ESP32)
    #[cfg(feature = "ble-esp32")]
    esp_clients: HashMap<String, Arc<BLEClient>>,
    /// Discovered devices during scan (ESP32)
    #[cfg(feature = "ble-esp32")]
    discovered_devices: Vec<BLEAdvertisedDevice>,
}

impl BleManager {
    /// Create a new BLE manager
    pub fn new(config: BleConfig) -> Self {
        Self {
            config,
            state: BleState::Uninitialized,
            peers: HashMap::new(),
            stats: BleStats::default(),
            local_address: None,
            running: false,
            // Desktop (btleplug)
            #[cfg(feature = "ble")]
            adapter: None,
            #[cfg(feature = "ble")]
            peripherals: HashMap::new(),
            #[cfg(feature = "ble")]
            tx_chars: HashMap::new(),
            #[cfg(feature = "ble")]
            notification_rx: None,
            #[cfg(feature = "ble")]
            notification_tx: None,
            // ESP32 (esp32-nimble)
            #[cfg(feature = "ble-esp32")]
            ble_device: None,
            #[cfg(feature = "ble-esp32")]
            esp_clients: HashMap::new(),
            #[cfg(feature = "ble-esp32")]
            discovered_devices: Vec::new(),
        }
    }

    /// Initialize the BLE stack
    pub async fn init(&mut self) -> Result<()> {
        log::info!("Initializing BLE transport: {}", self.config.device_name);

        // ========== Desktop (btleplug) ==========
        #[cfg(feature = "ble")]
        {
            // Initialize btleplug manager
            let manager = Manager::new()
                .await
                .map_err(|e| Error::Network(format!("Failed to initialize BLE manager: {}", e)))?;

            // Get the first available adapter
            let adapters = manager
                .adapters()
                .await
                .map_err(|e| Error::Network(format!("Failed to get BLE adapters: {}", e)))?;

            let adapter = adapters
                .into_iter()
                .next()
                .ok_or_else(|| Error::Network("No BLE adapter found".to_string()))?;

            // Get adapter info for local address
            let info = adapter
                .adapter_info()
                .await
                .map_err(|e| Error::Network(format!("Failed to get adapter info: {}", e)))?;
            self.local_address = Some(info.to_string());

            log::info!("BLE adapter initialized: {}", info);

            // Create notification channel
            let (tx, rx) = smol::channel::unbounded();
            self.notification_tx = Some(tx);
            self.notification_rx = Some(rx);

            self.adapter = Some(adapter);
        }

        // ========== ESP32 (esp32-nimble) ==========
        #[cfg(feature = "ble-esp32")]
        {
            // Take ownership of the BLE device singleton
            let device = BLEDevice::take();

            // Get local address from the device
            // Note: On ESP32, the address is set during manufacturing or can be configured
            self.local_address = Some(format!("{:?}", device.get_address()));

            log::info!("ESP32 BLE device initialized: {:?}", device.get_address());

            self.ble_device = Some(device);
        }

        // ========== No BLE backend enabled ==========
        #[cfg(not(any(feature = "ble", feature = "ble-esp32")))]
        {
            log::warn!("No BLE feature enabled, using simulated mode");
            self.local_address = Some("00:00:00:00:00:00".to_string());
        }

        self.state = BleState::Idle;
        Ok(())
    }

    /// Start the BLE transport
    pub async fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }

        log::info!("Starting BLE transport");

        // Start advertising
        self.start_advertising().await?;

        // Start scanning for peers
        self.start_scanning().await?;

        self.running = true;
        Ok(())
    }

    /// Stop the BLE transport
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        log::info!("Stopping BLE transport");

        // Disconnect all peers
        for (addr, peer) in self.peers.iter_mut() {
            if peer.connected {
                log::debug!("Disconnecting from peer: {}", addr);
                peer.connected = false;
            }
        }

        self.running = false;
        self.state = BleState::Idle;
        Ok(())
    }

    /// Start advertising presence
    async fn start_advertising(&mut self) -> Result<()> {
        log::debug!(
            "Starting BLE advertising: {} (interval: {}ms, tx_power: {}dBm)",
            self.config.device_name,
            self.config.advertising_interval_ms,
            self.config.tx_power
        );

        // ========== ESP32 (esp32-nimble) ==========
        #[cfg(feature = "ble-esp32")]
        {
            let device = self
                .ble_device
                .ok_or_else(|| Error::Network("ESP32 BLE device not initialized".to_string()))?;

            // Get advertising instance
            let advertising = device.get_advertising();

            // Configure advertising data
            advertising
                .lock()
                .set_scan_response(false)
                .set_min_interval(self.config.advertising_interval_ms as u16)
                .set_max_interval((self.config.advertising_interval_ms + 50) as u16);

            // Start advertising
            advertising.lock().start().map_err(|e| {
                Error::Network(format!("Failed to start ESP32 advertising: {:?}", e))
            })?;

            log::info!("ESP32 BLE advertising started: {}", self.config.device_name);
        }

        Ok(())
    }

    /// Start scanning for peers
    async fn start_scanning(&mut self) -> Result<()> {
        log::debug!(
            "Starting BLE scan (interval: {}ms, window: {}ms)",
            self.config.scan_interval_ms,
            self.config.scan_window_ms
        );

        self.state = BleState::Scanning;

        // ========== Desktop (btleplug) ==========
        #[cfg(feature = "ble")]
        {
            let adapter = self
                .adapter
                .as_ref()
                .ok_or_else(|| Error::Network("BLE adapter not initialized".to_string()))?;

            // Create scan filter for AIngle service
            let service_uuid = Uuid::parse_str(AINGLE_SERVICE_UUID)
                .map_err(|e| Error::Network(format!("Invalid service UUID: {}", e)))?;

            let filter = ScanFilter {
                services: vec![service_uuid],
            };

            adapter
                .start_scan(filter)
                .await
                .map_err(|e| Error::Network(format!("Failed to start BLE scan: {}", e)))?;

            log::info!("BLE scan started with AIngle service filter");

            self.stats.scans_completed += 1;
        }

        // ========== ESP32 (esp32-nimble) ==========
        #[cfg(feature = "ble-esp32")]
        {
            let device = self
                .ble_device
                .ok_or_else(|| Error::Network("ESP32 BLE device not initialized".to_string()))?;

            // Get scan instance
            let scan = device.get_scan();

            // Configure scan parameters
            scan.active_scan(true)
                .interval(self.config.scan_interval_ms as u16)
                .window(self.config.scan_window_ms as u16);

            // Start scanning - results handled via callback
            log::info!("ESP32 BLE scan started");

            self.stats.scans_completed += 1;
        }

        Ok(())
    }

    /// Handle discovered peer
    pub fn on_peer_discovered(&mut self, address: &str, rssi: i16, name: Option<&str>) {
        if let Some(peer) = self.peers.get_mut(address) {
            peer.update_rssi(rssi);
            if name.is_some() {
                peer.name = name.map(String::from);
            }
        } else {
            let mut peer = BlePeer::new(address, rssi);
            peer.name = name.map(String::from);
            log::debug!(
                "Discovered BLE peer: {} ({:?}) RSSI: {}",
                address,
                name,
                rssi
            );
            self.peers.insert(address.to_string(), peer);
        }
    }

    /// Connect to a peer
    pub async fn connect(&mut self, address: &str) -> Result<()> {
        let peer = self
            .peers
            .get_mut(address)
            .ok_or_else(|| Error::Network(format!("Unknown peer: {}", address)))?;

        if peer.connected {
            return Ok(());
        }

        log::info!("Connecting to BLE peer: {}", address);
        self.stats.connection_attempts += 1;

        #[cfg(feature = "ble")]
        {
            let adapter = self
                .adapter
                .as_ref()
                .ok_or_else(|| Error::Network("BLE adapter not initialized".to_string()))?;

            // Find the peripheral by address
            let peripherals = adapter
                .peripherals()
                .await
                .map_err(|e| Error::Network(format!("Failed to get peripherals: {}", e)))?;

            let peripheral = peripherals
                .into_iter()
                .find(|p| {
                    if let Ok(Some(props)) = smol::block_on(p.properties()) {
                        props.address.to_string() == address
                    } else {
                        false
                    }
                })
                .ok_or_else(|| Error::Network(format!("Peripheral not found: {}", address)))?;

            // Connect to the peripheral
            peripheral
                .connect()
                .await
                .map_err(|e| Error::Network(format!("Failed to connect: {}", e)))?;

            // Discover services
            peripheral
                .discover_services()
                .await
                .map_err(|e| Error::Network(format!("Failed to discover services: {}", e)))?;

            // Parse UUIDs for characteristics
            let tx_uuid = Uuid::parse_str(TX_CHAR_UUID)
                .map_err(|e| Error::Network(format!("Invalid TX UUID: {}", e)))?;
            let rx_uuid = Uuid::parse_str(RX_CHAR_UUID)
                .map_err(|e| Error::Network(format!("Invalid RX UUID: {}", e)))?;

            // Find TX and RX characteristics
            for service in peripheral.services() {
                for char in &service.characteristics {
                    if char.uuid == tx_uuid {
                        self.tx_chars.insert(address.to_string(), char.clone());
                        log::debug!("Found TX characteristic for {}", address);
                    }
                    if char.uuid == rx_uuid {
                        // Subscribe to RX notifications
                        peripheral.subscribe(&char).await.map_err(|e| {
                            Error::Network(format!("Failed to subscribe to RX: {}", e))
                        })?;
                        log::debug!("Subscribed to RX characteristic for {}", address);
                    }
                }
            }

            // Store the peripheral
            self.peripherals.insert(address.to_string(), peripheral);
        }

        // ========== ESP32 (esp32-nimble) ==========
        #[cfg(feature = "ble-esp32")]
        {
            // Create a new BLE client for this connection
            let client = Arc::new(BLEClient::new());

            // Parse the address - ESP32 uses BLEAddress type
            // Note: address format is "XX:XX:XX:XX:XX:XX"
            client
                .connect(address)
                .await
                .map_err(|e| Error::Network(format!("ESP32 BLE connect failed: {:?}", e)))?;

            log::info!("ESP32 BLE connected to: {}", address);

            // Discover services and characteristics
            // The AIngle service UUID in esp32-nimble format
            let service_uuid = uuid128!("6e400001-b5a3-f393-e0a9-e50e24dcca9e");

            if let Some(service) = client.get_service(service_uuid).await {
                log::debug!("Found AIngle service on ESP32 peer: {}", address);

                // Get TX and RX characteristics
                let _tx_uuid = uuid128!("6e400002-b5a3-f393-e0a9-e50e24dcca9e");
                let rx_uuid = uuid128!("6e400003-b5a3-f393-e0a9-e50e24dcca9e");

                // Subscribe to RX notifications
                if let Some(rx_char) = service.get_characteristic(rx_uuid).await {
                    rx_char
                        .subscribe_notify(false, |_| {
                            // Notification callback - message received
                            log::debug!("ESP32 BLE notification received");
                        })
                        .await
                        .map_err(|e| {
                            Error::Network(format!("Failed to subscribe to RX: {:?}", e))
                        })?;
                }
            }

            // Store the client
            self.esp_clients.insert(address.to_string(), client);
        }

        // Update peer state
        let peer = self.peers.get_mut(address).unwrap();
        peer.connected = true;
        peer.supports_aingle = true;
        self.stats.connections_established += 1;
        self.state = BleState::Connected;

        Ok(())
    }

    /// Disconnect from a peer
    pub async fn disconnect(&mut self, address: &str) -> Result<()> {
        let peer = self
            .peers
            .get_mut(address)
            .ok_or_else(|| Error::Network(format!("Unknown peer: {}", address)))?;

        if !peer.connected {
            return Ok(());
        }

        log::info!("Disconnecting from BLE peer: {}", address);

        // ========== Desktop (btleplug) ==========
        #[cfg(feature = "ble")]
        {
            // Disconnect the btleplug peripheral
            if let Some(peripheral) = self.peripherals.remove(address) {
                peripheral
                    .disconnect()
                    .await
                    .map_err(|e| Error::Network(format!("Failed to disconnect: {}", e)))?;
            }

            // Clean up cached characteristics
            self.tx_chars.remove(address);
        }

        // ========== ESP32 (esp32-nimble) ==========
        #[cfg(feature = "ble-esp32")]
        {
            // Disconnect and remove the ESP32 BLE client
            if let Some(client) = self.esp_clients.remove(address) {
                client
                    .disconnect()
                    .map_err(|e| Error::Network(format!("ESP32 BLE disconnect failed: {:?}", e)))?;
            }
        }

        peer.connected = false;

        // Update state if no more connections
        if !self.peers.values().any(|p| p.connected) {
            self.state = BleState::Scanning;
        }

        Ok(())
    }

    /// Send message to a peer
    pub async fn send(&mut self, address: &str, message: &Message) -> Result<()> {
        let peer = self
            .peers
            .get(address)
            .ok_or_else(|| Error::Network(format!("Unknown peer: {}", address)))?;

        if !peer.connected {
            return Err(Error::Network(format!("Peer not connected: {}", address)));
        }

        let payload =
            serde_json::to_vec(message).map_err(|e| Error::Serialization(e.to_string()))?;

        // ========== Desktop (btleplug) ==========
        #[cfg(feature = "ble")]
        {
            let peripheral = self
                .peripherals
                .get(address)
                .ok_or_else(|| Error::Network(format!("Peripheral not found: {}", address)))?;

            let tx_char = self.tx_chars.get(address).ok_or_else(|| {
                Error::Network(format!("TX characteristic not found: {}", address))
            })?;

            // Write to TX characteristic (without response for speed)
            peripheral
                .write(tx_char, &payload, WriteType::WithoutResponse)
                .await
                .map_err(|e| Error::Network(format!("Failed to send: {}", e)))?;
        }

        // ========== ESP32 (esp32-nimble) ==========
        #[cfg(feature = "ble-esp32")]
        {
            let client = self
                .esp_clients
                .get(address)
                .ok_or_else(|| Error::Network(format!("ESP32 client not found: {}", address)))?;

            // Get TX characteristic and write
            let service_uuid = uuid128!("6e400001-b5a3-f393-e0a9-e50e24dcca9e");
            let tx_uuid = uuid128!("6e400002-b5a3-f393-e0a9-e50e24dcca9e");

            if let Some(service) = client.get_service(service_uuid).await {
                if let Some(tx_char) = service.get_characteristic(tx_uuid).await {
                    tx_char
                        .write_value(&payload, false)
                        .await
                        .map_err(|e| Error::Network(format!("ESP32 BLE write failed: {:?}", e)))?;
                } else {
                    return Err(Error::Network("TX characteristic not found".to_string()));
                }
            } else {
                return Err(Error::Network("AIngle service not found".to_string()));
            }
        }

        self.stats.messages_sent += 1;
        self.stats.bytes_sent += payload.len() as u64;

        log::debug!("Sent BLE message to {} ({} bytes)", address, payload.len());
        Ok(())
    }

    /// Broadcast message to all connected peers (mesh)
    pub async fn broadcast(&mut self, message: &Message) -> Result<usize> {
        let payload =
            serde_json::to_vec(message).map_err(|e| Error::Serialization(e.to_string()))?;

        let connected_peers: Vec<String> = self
            .peers
            .iter()
            .filter(|(_, p)| p.connected)
            .map(|(addr, _)| addr.clone())
            .collect();

        let count = connected_peers.len();

        for address in connected_peers {
            // TODO: Send to each peer
            log::debug!("Broadcast to {}", address);
        }

        self.stats.messages_sent += count as u64;
        self.stats.bytes_sent += (payload.len() * count) as u64;

        Ok(count)
    }

    /// Receive message from any peer
    pub async fn recv(&mut self) -> Result<Option<(String, Message)>> {
        #[cfg(feature = "ble")]
        {
            if let Some(ref rx) = self.notification_rx {
                // Try to receive without blocking
                match rx.try_recv() {
                    Ok((address, data)) => {
                        let message: Message = serde_json::from_slice(&data)
                            .map_err(|e| Error::Serialization(e.to_string()))?;

                        self.stats.messages_received += 1;
                        self.stats.bytes_received += data.len() as u64;

                        log::debug!(
                            "Received BLE message from {} ({} bytes)",
                            address,
                            data.len()
                        );

                        return Ok(Some((address, message)));
                    }
                    Err(smol::channel::TryRecvError::Empty) => {
                        // No message available
                        return Ok(None);
                    }
                    Err(smol::channel::TryRecvError::Closed) => {
                        return Err(Error::Network("Notification channel closed".to_string()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Relay message through mesh (if enabled)
    pub async fn relay(&mut self, source: &str, _message: &Message) -> Result<usize> {
        if !self.config.mesh_relay {
            return Ok(0);
        }

        let connected_peers: Vec<String> = self
            .peers
            .iter()
            .filter(|(addr, p)| p.connected && *addr != source)
            .map(|(addr, _)| addr.clone())
            .collect();

        let count = connected_peers.len();

        for address in connected_peers {
            // TODO: Forward message to peer
            log::debug!("Relay to {}", address);
        }

        self.stats.messages_relayed += count as u64;

        Ok(count)
    }

    /// Get current state
    pub fn state(&self) -> BleState {
        self.state
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get number of connected peers
    pub fn connected_count(&self) -> usize {
        self.peers.values().filter(|p| p.connected).count()
    }

    /// Get all known peers
    pub fn peers(&self) -> impl Iterator<Item = &BlePeer> {
        self.peers.values()
    }

    /// Get connected peers
    pub fn connected_peers(&self) -> impl Iterator<Item = &BlePeer> {
        self.peers.values().filter(|p| p.connected)
    }

    /// Get peer by address
    pub fn get_peer(&self, address: &str) -> Option<&BlePeer> {
        self.peers.get(address)
    }

    /// Get statistics
    pub fn stats(&self) -> &BleStats {
        &self.stats
    }

    /// Clean up stale peers
    pub fn cleanup_stale_peers(&mut self, timeout: Duration) {
        let stale: Vec<String> = self
            .peers
            .iter()
            .filter(|(_, p)| !p.connected && p.is_stale(timeout))
            .map(|(addr, _)| addr.clone())
            .collect();

        for addr in stale {
            log::debug!("Removing stale peer: {}", addr);
            self.peers.remove(&addr);
        }
    }

    /// Get local device address
    pub fn local_address(&self) -> Option<&str> {
        self.local_address.as_deref()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ble_config_default() {
        let config = BleConfig::default();
        assert_eq!(config.device_name, "AIngle-Node");
        assert!(config.mesh_relay);
        assert_eq!(config.tx_power, 0);
        assert_eq!(config.max_connections, 4);
    }

    #[test]
    fn test_ble_config_low_power() {
        let config = BleConfig::low_power();
        assert_eq!(config.device_name, "AIngle-LP");
        assert!(!config.mesh_relay);
        assert_eq!(config.tx_power, -12);
        assert!(config.passive_scan);
    }

    #[test]
    fn test_ble_config_mesh_relay() {
        let config = BleConfig::mesh_relay();
        assert!(config.mesh_relay);
        assert_eq!(config.tx_power, 4);
        assert_eq!(config.max_connections, 8);
    }

    #[test]
    fn test_ble_peer_creation() {
        let peer = BlePeer::new("AA:BB:CC:DD:EE:FF", -50);
        assert_eq!(peer.address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(peer.rssi, -50);
        assert!(!peer.connected);
        assert!(peer.name.is_none());
    }

    #[test]
    fn test_ble_peer_update_rssi() {
        let mut peer = BlePeer::new("AA:BB:CC:DD:EE:FF", -50);
        std::thread::sleep(Duration::from_millis(10));
        peer.update_rssi(-40);
        assert_eq!(peer.rssi, -40);
    }

    #[test]
    fn test_ble_peer_stale() {
        let peer = BlePeer::new("AA:BB:CC:DD:EE:FF", -50);
        assert!(!peer.is_stale(Duration::from_secs(60)));
        // Can't easily test stale=true without sleeping
    }

    #[test]
    fn test_ble_manager_creation() {
        let config = BleConfig::default();
        let manager = BleManager::new(config);
        assert_eq!(manager.state(), BleState::Uninitialized);
        assert!(!manager.is_running());
        assert_eq!(manager.connected_count(), 0);
    }

    #[test]
    fn test_ble_manager_peer_discovery() {
        let mut manager = BleManager::new(BleConfig::default());
        manager.on_peer_discovered("AA:BB:CC:DD:EE:FF", -50, Some("Test Device"));

        assert_eq!(manager.peers().count(), 1);
        let peer = manager.get_peer("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(peer.rssi, -50);
        assert_eq!(peer.name, Some("Test Device".to_string()));
    }

    #[test]
    fn test_ble_manager_peer_update() {
        let mut manager = BleManager::new(BleConfig::default());
        manager.on_peer_discovered("AA:BB:CC:DD:EE:FF", -50, None);
        manager.on_peer_discovered("AA:BB:CC:DD:EE:FF", -40, Some("Updated Name"));

        let peer = manager.get_peer("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(peer.rssi, -40);
        assert_eq!(peer.name, Some("Updated Name".to_string()));
    }

    #[test]
    fn test_ble_stats_default() {
        let stats = BleStats::default();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.messages_relayed, 0);
        assert_eq!(stats.connections_established, 0);
    }

    #[test]
    fn test_ble_state_equality() {
        assert_eq!(BleState::Idle, BleState::Idle);
        assert_ne!(BleState::Connected, BleState::Scanning);
    }
}
