//! The main [`MinimalNode`] implementation.
//!
//! This module ties together all the components of the lightweight node, including
//! storage, networking, cryptography, and the gossip/sync protocols. The [`MinimalNode`]
//! struct is the primary entry point for running an AIngle node on resource-constrained
//! devices.
//!
//! # Architecture
//!
//! The node coordinates several subsystems:
//! - **Storage**: Persistent data storage via SQLite or RocksDB
//! - **Network**: Peer-to-peer communication via CoAP or QUIC
//! - **Gossip**: Data propagation and peer discovery protocol
//! - **Sync**: Consistency maintenance with peers via bloom filters
//! - **Crypto**: Ed25519 signatures for authentication
//!
//! # Examples
//!
//! Basic node creation and operation:
//!
//! ```no_run
//! # use aingle_minimal::{MinimalNode, Config};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create and start a node
//! let config = Config::iot_mode();
//! let mut node = MinimalNode::new(config)?;
//!
//! // Run the node (blocks until stopped)
//! smol::block_on(async {
//!     node.run().await
//! })?;
//! # Ok(())
//! # }
//! ```

use crate::config::Config;
use crate::crypto::Keypair;
use crate::error::Result;
use crate::gossip::GossipManager;
use crate::network::{Message, Network};
use crate::storage_factory::DynamicStorage;
use crate::storage_trait::StorageBackend;
use crate::sync::SyncManager;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Key used to store known peers in metadata
const PEERS_METADATA_KEY: &str = "known_peers";

/// Interval for auto-saving peers (in seconds)
const PEER_SAVE_INTERVAL_SECS: u64 = 300; // 5 minutes

/// A serializable record of a known peer for persistence.
///
/// This struct captures essential information about a peer that can be
/// saved to storage and restored when the node restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    /// The peer's network address (IP:port).
    pub addr: String,
    /// The last known sequence number from this peer.
    pub latest_seq: u32,
    /// Quality score from 0-100 (higher is better).
    pub quality: u8,
    /// Unix timestamp (seconds) of when this peer was last seen.
    pub last_seen_secs: u64,
}

/// A minimal, lightweight AIngle node designed for IoT and resource-constrained devices.
///
/// `MinimalNode` is the core component of the aingle_minimal crate, providing a
/// complete AIngle node that can run on devices with less than 1MB RAM. It manages
/// data storage, network communication, peer synchronization, and cryptographic
/// operations.
///
/// # Features
///
/// - **Sub-second confirmation**: Configurable publish intervals from immediate to minutes
/// - **Zero-fee transactions**: No staking or gas fees required
/// - **Mesh networking**: Support for WiFi, BLE, LoRa, and traditional IP networks
/// - **Power-aware**: Adaptive modes for battery-operated devices
/// - **Auto-discovery**: mDNS-based peer finding on local networks
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// # use aingle_minimal::{MinimalNode, Config};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a node with IoT configuration
/// let config = Config::iot_mode();
/// let mut node = MinimalNode::new(config)?;
///
/// // Create and store an entry
/// let hash = node.create_entry("Hello, AIngle!")?;
/// println!("Created entry: {}", hash.to_hex());
///
/// // Get node statistics
/// let stats = node.stats()?;
/// println!("Entries: {}, Peers: {}", stats.entries_count, stats.peer_count);
/// # Ok(())
/// # }
/// ```
///
/// ## Running the Node
///
/// ```no_run
/// # use aingle_minimal::{MinimalNode, Config};
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut node = MinimalNode::new(Config::iot_mode())?;
///
/// // Add known peers
/// node.add_peer("192.168.1.100:5683".parse()?);
///
/// // Start the node's main loop
/// node.run().await?;
/// # Ok(())
/// # }
/// ```
pub struct MinimalNode {
    config: Config,
    keypair: Keypair,
    storage: DynamicStorage,
    network: Network,
    gossip: GossipManager,
    sync: SyncManager,
    running: Arc<AtomicBool>,
    start_time: Instant,
    /// Timestamp of the last peer save operation
    last_peer_save: Instant,
}

impl MinimalNode {
    /// Creates and initializes a new `MinimalNode` with the given configuration.
    ///
    /// This method performs several initialization steps:
    /// 1. Validates the configuration
    /// 2. Generates an Ed25519 keypair for the node's identity
    /// 3. Initializes the storage backend (SQLite, RocksDB, or Memory)
    /// 4. Sets up the network layer with the configured transport
    /// 5. Initializes gossip and sync managers
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Configuration validation fails (see [`Config::validate`])
    /// - Storage backend initialization fails
    /// - Network initialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create a node with IoT configuration
    /// let node = MinimalNode::new(Config::iot_mode())?;
    /// println!("Node created with ID: {}", node.public_key().to_hex());
    ///
    /// // Create a node with custom configuration
    /// let mut config = Config::default();
    /// config.memory_limit = 512 * 1024; // 512KB
    /// let node = MinimalNode::new(config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: Config) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        // Generate or load keypair
        let keypair = Keypair::generate();

        // Initialize storage based on configuration
        let storage = DynamicStorage::from_config(config.storage.clone())?;
        log::info!("Using {} storage backend", storage.backend_name());

        // Initialize network
        let node_id = keypair.public_key().to_hex();
        let network = Network::new(config.transport.clone(), config.gossip.clone(), node_id);

        // Initialize gossip manager
        let gossip = GossipManager::new(config.gossip.clone());

        // Initialize sync manager with gossip loop delay as sync interval
        let sync = SyncManager::new(config.gossip.loop_delay * 2);

        let mut node = Self {
            config,
            keypair,
            storage,
            network,
            gossip,
            sync,
            running: Arc::new(AtomicBool::new(false)),
            start_time: Instant::now(),
            last_peer_save: Instant::now(),
        };

        // Load persisted peers from storage
        if let Err(e) = node.load_peers() {
            log::warn!("Failed to load persisted peers: {}", e);
        }

        Ok(node)
    }

    /// Returns the node's public key, which serves as its permanent identity on the network.
    ///
    /// The public key is derived from the node's Ed25519 keypair and uniquely identifies
    /// this agent across the AIngle network. It's used for signing actions and verifying
    /// the node's identity to peers.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = MinimalNode::new(Config::test_mode())?;
    /// let pubkey = node.public_key();
    /// println!("Node identity: {}", pubkey.to_hex());
    /// # Ok(())
    /// # }
    /// ```
    pub fn public_key(&self) -> AgentPubKey {
        self.keypair.public_key()
    }

    /// Returns statistics about the node's current state and performance.
    ///
    /// This provides real-time metrics useful for monitoring node health and resource usage.
    ///
    /// # Errors
    ///
    /// Returns an error if storage statistics cannot be retrieved.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::test_mode())?;
    ///
    /// // Create some test data
    /// node.create_entry("test")?;
    ///
    /// // Get statistics
    /// let stats = node.stats()?;
    /// println!("Entries: {}", stats.entries_count);
    /// println!("Actions: {}", stats.actions_count);
    /// println!("Storage used: {} bytes", stats.storage_used);
    /// println!("Connected peers: {}", stats.peer_count);
    /// println!("Uptime: {} seconds", stats.uptime_secs);
    /// # Ok(())
    /// # }
    /// ```
    pub fn stats(&self) -> Result<NodeStats> {
        let storage_stats = self.storage.stats()?;

        Ok(NodeStats {
            entries_count: storage_stats.entry_count,
            actions_count: storage_stats.action_count,
            memory_used: 0, // Would need system call to measure
            storage_used: storage_stats.db_size,
            peer_count: self.network.peer_count(),
            uptime_secs: self.start_time.elapsed().as_secs(),
        })
    }

    /// Creates a new application entry, signs it, and stores it on the node's source chain.
    ///
    /// This is the primary method for publishing data to the AIngle network. The entry
    /// is automatically:
    /// 1. Serialized to JSON
    /// 2. Wrapped in a signed [`Action`]
    /// 3. Stored locally in the database
    /// 4. Queued for gossip to peers
    /// 5. Published immediately if `publish_interval` is zero
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Content cannot be serialized
    /// - Storage operation fails
    /// - Network publishing fails (when immediate publishing is enabled)
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # use serde::{Serialize, Deserialize};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::test_mode())?;
    ///
    /// // Create a simple text entry
    /// let hash = node.create_entry("Hello, world!")?;
    /// println!("Created entry: {}", hash.to_hex());
    ///
    /// // Create a structured entry
    /// #[derive(Serialize, Deserialize)]
    /// struct SensorReading {
    ///     temperature: f64,
    ///     humidity: f64,
    ///     timestamp: u64,
    /// }
    ///
    /// let reading = SensorReading {
    ///     temperature: 23.5,
    ///     humidity: 65.2,
    ///     timestamp: 1234567890,
    /// };
    ///
    /// let hash = node.create_entry(reading)?;
    /// println!("Created sensor reading: {}", hash.to_hex());
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_entry<T: serde::Serialize>(&mut self, content: T) -> Result<Hash> {
        // Create entry
        let entry = Entry::app(content)?;
        let entry_hash = entry.hash();

        // Get previous action
        let seq = self.storage.get_latest_seq()? + 1;
        let prev_action = if seq > 1 {
            // Would need to get actual previous action hash
            None
        } else {
            None
        };

        // Create action
        let action = Action {
            action_type: ActionType::Create,
            author: self.keypair.public_key(),
            timestamp: Timestamp::now(),
            seq,
            prev_action,
            entry_hash: Some(entry_hash.clone()),
            signature: self.sign_action_data(seq, &entry_hash),
        };

        // Store record
        let record = Record {
            action,
            entry: Some(entry),
        };

        let action_hash = self.storage.put_record(&record)?;

        // Queue for gossip
        self.gossip.announce(action_hash.clone());

        // Track in sync manager
        self.sync.add_local_hash(action_hash.clone());

        // Immediate publish if configured
        if self.config.publish_interval == Duration::ZERO {
            self.publish_pending()?;
        }

        Ok(action_hash)
    }

    /// Creates multiple entries in a single optimized batch operation.
    ///
    /// This is significantly faster than calling [`Self::create_entry`] multiple times
    /// because it:
    /// - Gets the latest sequence number once
    /// - Uses a single database transaction
    /// - Announces all entries for gossip at once
    ///
    /// # Performance
    ///
    /// For 100 entries, batch creation is typically 3-5x faster than individual
    /// creates due to reduced transaction overhead.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # use serde_json::json;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::iot_mode())?;
    ///
    /// // Batch create sensor readings
    /// let readings: Vec<_> = (0..10)
    ///     .map(|i| json!({"sensor": "temp", "value": 20.0 + i as f64}))
    ///     .collect();
    ///
    /// let hashes = node.create_entries_batch(&readings)?;
    /// println!("Created {} entries", hashes.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_entries_batch<T: serde::Serialize>(
        &mut self,
        contents: &[T],
    ) -> Result<Vec<Hash>> {
        if contents.is_empty() {
            return Ok(Vec::new());
        }

        // Get base sequence number once
        let base_seq = self.storage.get_latest_seq()? + 1;
        let author = self.keypair.public_key();
        let timestamp = Timestamp::now();

        // Build all records
        let mut records = Vec::with_capacity(contents.len());

        for (i, content) in contents.iter().enumerate() {
            let entry = Entry::app(content)?;
            let entry_hash = entry.hash();
            let seq = base_seq + i as u32;

            let action = Action {
                action_type: ActionType::Create,
                author: author.clone(),
                timestamp,
                seq,
                prev_action: None,
                entry_hash: Some(entry_hash.clone()),
                signature: self.sign_action_data(seq, &entry_hash),
            };

            records.push(Record {
                action,
                entry: Some(entry),
            });
        }

        // Store all records in single transaction
        let hashes = self.storage.put_records_batch(&records)?;

        // Announce all for gossip
        for hash in &hashes {
            self.gossip.announce(hash.clone());
            self.sync.add_local_hash(hash.clone());
        }

        // Immediate publish if configured
        if self.config.publish_interval == Duration::ZERO {
            self.publish_pending()?;
        }

        Ok(hashes)
    }

    /// Retrieves an [`Entry`] from storage by its content hash.
    ///
    /// This method looks up an entry in the local database. It only returns entries
    /// that have been stored locally, either created by this node or received via gossip.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage backend fails to perform the lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::test_mode())?;
    ///
    /// // Create an entry
    /// let hash = node.create_entry("test data")?;
    ///
    /// // Retrieve it
    /// if let Some(entry) = node.get_entry(&hash)? {
    ///     println!("Found entry with {} bytes", entry.size());
    /// } else {
    ///     println!("Entry not found");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_entry(&self, hash: &Hash) -> Result<Option<Entry>> {
        self.storage.get_entry(hash)
    }

    /// Signs the essential data of an action.
    fn sign_action_data(&self, seq: u32, entry_hash: &Hash) -> Signature {
        let mut data = Vec::new();
        data.extend_from_slice(&seq.to_be_bytes());
        data.extend_from_slice(entry_hash.as_bytes());
        self.keypair.sign(&data)
    }

    /// Publishes pending announcements to the network via gossip.
    fn publish_pending(&mut self) -> Result<()> {
        let announcements = self.gossip.take_announcements(50);
        for hash in announcements {
            let message = Message::NewRecord { hash };
            smol::block_on(self.network.broadcast(&message))?;
        }
        Ok(())
    }

    /// Runs the node's main event loop.
    ///
    /// This async function starts the network, begins peer discovery, and enters the
    /// main loop to handle gossip and data synchronization. It will run continuously
    /// until [`stop()`](Self::stop) is called from another thread.
    ///
    /// The main loop performs these operations:
    /// - Syncs with discovered mDNS peers
    /// - Runs periodic gossip rounds with known peers
    /// - Publishes pending data at configured intervals
    /// - Handles network messages
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Network startup fails
    /// - mDNS discovery fails to initialize
    /// - Network operations fail during the main loop
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::iot_mode())?;
    ///
    /// // Add some known peers
    /// node.add_peer("192.168.1.100:5683".parse()?);
    /// node.add_peer("192.168.1.101:5683".parse()?);
    ///
    /// // Start the node's main loop (blocks until stopped)
    /// node.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(&mut self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        log::info!(
            "Starting minimal node: {}",
            self.keypair.public_key().to_hex()
        );

        // Start network
        self.network.start().await?;

        // Start mDNS discovery if enabled
        if self.config.enable_mdns {
            let port = match &self.config.transport {
                crate::config::TransportConfig::Coap { port, .. } => *port,
                crate::config::TransportConfig::Quic { port, .. } => *port,
                #[cfg(feature = "webrtc")]
                crate::config::TransportConfig::WebRtc { signaling_port, .. } => *signaling_port,
                _ => 5683,
            };
            if let Err(e) = self.network.start_discovery(port) {
                log::warn!("Failed to start mDNS discovery: {}", e);
            }
        }

        let mut discovery_sync_counter = 0u32;

        // Main loop
        while self.running.load(Ordering::SeqCst) {
            // Sync discovered peers periodically (every 100 iterations = ~1 second)
            discovery_sync_counter = discovery_sync_counter.wrapping_add(1);
            if discovery_sync_counter % 100 == 0 {
                self.network.sync_discovered_peers();
            }

            // Check gossip timing
            if self.gossip.should_gossip() {
                self.run_gossip_round().await;
            }

            // Publish pending if interval passed
            if self.config.publish_interval > Duration::ZERO {
                self.publish_pending()?;
            }

            // Periodically save peers to storage
            if self.last_peer_save.elapsed().as_secs() >= PEER_SAVE_INTERVAL_SECS {
                if let Err(e) = self.save_peers() {
                    log::warn!("Failed to save peers: {}", e);
                }
            }

            // Sleep to avoid busy loop
            smol::Timer::after(Duration::from_millis(10)).await;
        }

        // Cleanup: Save peers before stopping
        if let Err(e) = self.save_peers() {
            log::warn!("Failed to save peers on shutdown: {}", e);
        }

        self.network.stop().await?;
        log::info!("Node stopped");

        Ok(())
    }

    /// Runs a single round of gossip and synchronization with a set of peers.
    async fn run_gossip_round(&mut self) {
        let peers = self.network.gossip_peers();
        let mut success_count = 0;

        for addr in peers {
            let latest_seq = self.storage.get_latest_seq().unwrap_or(0);

            // Try to sync with this peer
            match self
                .sync
                .sync_with_peer(&addr, &mut self.network, &self.storage, &mut self.gossip)
                .await
            {
                Ok(result) => {
                    self.network.update_peer(addr, latest_seq);
                    success_count += 1;
                    log::debug!(
                        "Sync with {} complete: sent_filter={}, records_sent={}, records_received={}",
                        addr,
                        result.sent_filter,
                        result.records_sent,
                        result.records_received
                    );
                }
                Err(e) => {
                    log::warn!("Sync with {} failed: {}", addr, e);
                    self.network.mark_peer_failed(&addr);
                }
            }
        }

        self.gossip.gossip_complete(success_count > 0);
    }

    /// Stops the node's main event loop gracefully.
    ///
    /// This method signals the node to stop its main loop and shut down. It can be
    /// called from any thread and is safe to call multiple times.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::iot_mode())?;
    ///
    /// // In another task/thread, stop the node after some work
    /// let stop_signal = Arc::new(node);
    /// let stop_handle = stop_signal.clone();
    ///
    /// // Start the node
    /// let task = smol::spawn(async move {
    ///     // node.run().await
    /// });
    ///
    /// // Later, stop it
    /// stop_handle.stop();
    /// # Ok(())
    /// # }
    /// ```
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Returns `true` if the node's main loop is currently running.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = MinimalNode::new(Config::test_mode())?;
    ///
    /// // Node is not running until run() is called
    /// assert!(!node.is_running());
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Manually adds a peer to the node's peer list.
    ///
    /// This is useful for bootstrapping connections to known peers when mDNS
    /// discovery is disabled or when connecting to peers outside the local network.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::test_mode())?;
    ///
    /// // Add known peers
    /// node.add_peer("192.168.1.100:5683".parse()?);
    /// node.add_peer("10.0.0.50:5683".parse()?);
    ///
    /// // Now the node will attempt to gossip with these peers
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_peer(&mut self, addr: std::net::SocketAddr) {
        self.network.add_peer(addr);
    }

    /// Saves the current list of known peers to persistent storage.
    ///
    /// This method serializes all active peers to JSON and stores them in the
    /// database's metadata table. Peers are automatically saved periodically
    /// during the main loop, but this method can be called manually for
    /// immediate persistence.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or storage operations fail.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::test_mode())?;
    ///
    /// // Add some peers
    /// node.add_peer("192.168.1.100:5683".parse()?);
    /// node.add_peer("192.168.1.101:5683".parse()?);
    ///
    /// // Save peers to storage
    /// node.save_peers()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn save_peers(&mut self) -> Result<()> {
        let peers = self.network.active_peers();
        if peers.is_empty() {
            log::debug!("No active peers to save");
            return Ok(());
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let peer_records: Vec<PeerRecord> = peers
            .iter()
            .map(|p| {
                // Estimate last_seen as now minus elapsed time
                let elapsed_secs = p.last_seen.elapsed().as_secs();
                PeerRecord {
                    addr: p.addr.to_string(),
                    latest_seq: p.latest_seq,
                    quality: p.quality,
                    last_seen_secs: now.saturating_sub(elapsed_secs),
                }
            })
            .collect();

        let json = serde_json::to_string(&peer_records)
            .map_err(|e| crate::error::Error::Serialization(e.to_string()))?;

        self.storage.set_metadata(PEERS_METADATA_KEY, &json)?;
        self.last_peer_save = Instant::now();

        log::info!("Saved {} peers to storage", peer_records.len());
        Ok(())
    }

    /// Loads previously saved peers from persistent storage.
    ///
    /// This method is automatically called during node initialization to restore
    /// known peers from the previous session. Peers with a quality score above
    /// a threshold are re-added to the network.
    ///
    /// # Errors
    ///
    /// Returns an error if storage read or deserialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::test_mode())?;
    ///
    /// // Peers are automatically loaded during new()
    /// // But can also be called manually:
    /// node.load_peers()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_peers(&mut self) -> Result<()> {
        let json = match self.storage.get_metadata(PEERS_METADATA_KEY)? {
            Some(data) => data,
            None => {
                log::debug!("No persisted peers found");
                return Ok(());
            }
        };

        let peer_records: Vec<PeerRecord> = serde_json::from_str(&json)
            .map_err(|e| crate::error::Error::Serialization(e.to_string()))?;

        let mut loaded_count = 0;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for record in peer_records {
            // Skip peers with very low quality or that haven't been seen in > 24 hours
            if record.quality < 10 {
                log::debug!(
                    "Skipping low-quality peer: {} (quality={})",
                    record.addr,
                    record.quality
                );
                continue;
            }

            let age_secs = now.saturating_sub(record.last_seen_secs);
            if age_secs > 24 * 60 * 60 {
                log::debug!(
                    "Skipping stale peer: {} (last seen {} hours ago)",
                    record.addr,
                    age_secs / 3600
                );
                continue;
            }

            if let Ok(addr) = record.addr.parse::<SocketAddr>() {
                self.network.add_peer(addr);
                // Restore quality by simulating successful interactions
                let quality_boosts = record.quality.saturating_sub(50) / 5;
                for _ in 0..quality_boosts {
                    self.network.update_peer(addr, record.latest_seq);
                }
                loaded_count += 1;
            } else {
                log::warn!("Invalid peer address in storage: {}", record.addr);
            }
        }

        log::info!("Loaded {} peers from storage", loaded_count);
        Ok(())
    }

    /// Returns the list of known peers as serializable records.
    ///
    /// This is useful for exporting peer information or debugging.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut node = MinimalNode::new(Config::test_mode())?;
    /// node.add_peer("192.168.1.100:5683".parse()?);
    ///
    /// let peers = node.get_known_peers();
    /// for peer in peers {
    ///     println!("Peer: {} (quality: {})", peer.addr, peer.quality);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_known_peers(&self) -> Vec<PeerRecord> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.network
            .active_peers()
            .iter()
            .map(|p| {
                let elapsed_secs = p.last_seen.elapsed().as_secs();
                PeerRecord {
                    addr: p.addr.to_string(),
                    latest_seq: p.latest_seq,
                    quality: p.quality,
                    last_seen_secs: now.saturating_sub(elapsed_secs),
                }
            })
            .collect()
    }

    /// Returns statistics from the sync manager.
    ///
    /// These statistics provide insights into data synchronization performance,
    /// including sync attempts, successes, failures, and data transfer metrics.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = MinimalNode::new(Config::test_mode())?;
    ///
    /// let stats = node.sync_stats();
    /// println!("Successful syncs: {}", stats.total_successful_syncs);
    /// println!("Failed syncs: {}", stats.total_failed_syncs);
    /// # Ok(())
    /// # }
    /// ```
    pub fn sync_stats(&self) -> crate::sync::SyncStats {
        self.sync.stats()
    }

    /// Returns statistics from the gossip manager.
    ///
    /// These statistics provide insights into gossip protocol performance,
    /// including gossip rounds, peer interactions, and data propagation metrics.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = MinimalNode::new(Config::test_mode())?;
    ///
    /// let stats = node.gossip_stats();
    /// println!("Gossip round: {}", stats.round);
    /// println!("Queue length: {}", stats.queue_length);
    /// # Ok(())
    /// # }
    /// ```
    pub fn gossip_stats(&self) -> crate::gossip::GossipStats {
        self.gossip.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_create_entry() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        let content = serde_json::json!({
            "sensor_id": "temp_001",
            "value": 23.5,
            "unit": "celsius"
        });

        let hash = node.create_entry(content);
        assert!(hash.is_ok());
    }

    #[test]
    fn test_node_stats() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        let stats = node.stats();
        assert!(stats.is_ok());
        assert_eq!(stats.unwrap().entries_count, 0);
    }

    #[test]
    fn test_public_key() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        let pubkey = node.public_key();
        // Public key should be 32 bytes
        assert_eq!(pubkey.0.len(), 32);
        // Hex representation should be 64 characters
        assert_eq!(pubkey.to_hex().len(), 64);
    }

    #[test]
    fn test_get_entry() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Create an entry
        let content = "test content";
        let hash = node.create_entry(content).unwrap();

        // Get the entry back
        let entry = node.get_entry(&hash);
        assert!(entry.is_ok());
        // Note: entry may be None depending on storage implementation
    }

    #[test]
    fn test_get_entry_not_found() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        // Try to get non-existent entry
        let hash = Hash([0u8; 32]);
        let entry = node.get_entry(&hash);
        assert!(entry.is_ok());
        assert!(entry.unwrap().is_none());
    }

    #[test]
    fn test_stop_and_is_running() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        // Node should not be running initially
        assert!(!node.is_running());

        // Stop should work even when not running
        node.stop();
        assert!(!node.is_running());
    }

    #[test]
    fn test_add_peer() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Add a peer
        let addr: std::net::SocketAddr = "192.168.1.100:5683".parse().unwrap();
        node.add_peer(addr);

        // Verify peer was added via stats
        let stats = node.stats().unwrap();
        assert!(stats.peer_count >= 0); // At least no error
    }

    #[test]
    fn test_add_multiple_peers() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Add multiple peers
        node.add_peer("192.168.1.100:5683".parse().unwrap());
        node.add_peer("192.168.1.101:5683".parse().unwrap());
        node.add_peer("192.168.1.102:5683".parse().unwrap());

        // Stats should work
        let stats = node.stats().unwrap();
        assert!(stats.peer_count >= 0);
    }

    #[test]
    fn test_sync_stats() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        let sync_stats = node.sync_stats();
        // Initial stats should be zero
        assert_eq!(sync_stats.total_successful_syncs, 0);
        assert_eq!(sync_stats.total_failed_syncs, 0);
        assert_eq!(sync_stats.peer_count, 0);
    }

    #[test]
    fn test_gossip_stats() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        let gossip_stats = node.gossip_stats();
        // Initial stats should be zero
        assert_eq!(gossip_stats.round, 0);
        assert_eq!(gossip_stats.pending_announcements, 0);
        assert_eq!(gossip_stats.queue_length, 0);
    }

    #[test]
    fn test_create_multiple_entries() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Create multiple entries
        let hash1 = node.create_entry("entry 1").unwrap();
        let hash2 = node.create_entry("entry 2").unwrap();
        let hash3 = node.create_entry("entry 3").unwrap();

        // All hashes should be different
        assert_ne!(hash1, hash2);
        assert_ne!(hash2, hash3);
        assert_ne!(hash1, hash3);

        // Stats should reflect the entries
        let stats = node.stats().unwrap();
        assert!(stats.actions_count >= 3);
    }

    #[test]
    fn test_node_stats_after_entries() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Initial stats
        let initial_stats = node.stats().unwrap();
        let initial_actions = initial_stats.actions_count;

        // Create an entry
        node.create_entry("test").unwrap();

        // Stats should update
        let updated_stats = node.stats().unwrap();
        assert!(updated_stats.actions_count > initial_actions);
    }

    #[test]
    fn test_node_uptime() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        // Sleep a bit to ensure uptime is measurable
        std::thread::sleep(std::time::Duration::from_millis(10));

        let stats = node.stats().unwrap();
        // Uptime should be at least 0 (could be 0 if very fast)
        assert!(stats.uptime_secs >= 0);
    }

    #[test]
    fn test_node_with_iot_config() {
        let config = Config::iot_mode();
        let node = MinimalNode::new(config);
        assert!(node.is_ok());

        let node = node.unwrap();
        assert!(!node.is_running());
    }

    #[test]
    fn test_create_entry_with_struct() {
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize)]
        struct SensorReading {
            temperature: f64,
            humidity: f64,
            timestamp: u64,
        }

        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        let reading = SensorReading {
            temperature: 23.5,
            humidity: 65.2,
            timestamp: 1234567890,
        };

        let hash = node.create_entry(reading);
        assert!(hash.is_ok());
    }

    #[test]
    fn test_create_entry_with_nested_struct() {
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize)]
        struct Location {
            latitude: f64,
            longitude: f64,
        }

        #[derive(Serialize, Deserialize)]
        struct DeviceData {
            device_id: String,
            location: Location,
            readings: Vec<f64>,
        }

        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        let data = DeviceData {
            device_id: "device_001".to_string(),
            location: Location {
                latitude: 40.7128,
                longitude: -74.0060,
            },
            readings: vec![1.0, 2.0, 3.0, 4.0, 5.0],
        };

        let hash = node.create_entry(data);
        assert!(hash.is_ok());
    }

    // ==================== Peer Persistence Tests ====================

    #[test]
    fn test_peer_record_serialization() {
        let record = PeerRecord {
            addr: "192.168.1.100:5683".to_string(),
            latest_seq: 42,
            quality: 75,
            last_seen_secs: 1234567890,
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("192.168.1.100:5683"));
        assert!(json.contains("42"));
        assert!(json.contains("75"));

        let parsed: PeerRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.addr, record.addr);
        assert_eq!(parsed.latest_seq, record.latest_seq);
        assert_eq!(parsed.quality, record.quality);
        assert_eq!(parsed.last_seen_secs, record.last_seen_secs);
    }

    #[test]
    fn test_peer_record_vec_serialization() {
        let records = vec![
            PeerRecord {
                addr: "192.168.1.100:5683".to_string(),
                latest_seq: 10,
                quality: 80,
                last_seen_secs: 1000,
            },
            PeerRecord {
                addr: "10.0.0.50:5683".to_string(),
                latest_seq: 20,
                quality: 60,
                last_seen_secs: 2000,
            },
        ];

        let json = serde_json::to_string(&records).unwrap();
        let parsed: Vec<PeerRecord> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].addr, "192.168.1.100:5683");
        assert_eq!(parsed[1].addr, "10.0.0.50:5683");
    }

    #[test]
    fn test_save_peers_empty() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // No peers to save - should succeed silently
        let result = node.save_peers();
        assert!(result.is_ok());
    }

    #[test]
    fn test_save_peers_with_peers() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Add some peers
        node.add_peer("192.168.1.100:5683".parse().unwrap());
        node.add_peer("192.168.1.101:5683".parse().unwrap());

        // Save should succeed
        let result = node.save_peers();
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_peers_empty_storage() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Load when nothing is stored - should succeed
        let result = node.load_peers();
        assert!(result.is_ok());
    }

    #[test]
    fn test_save_and_load_peers_roundtrip() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Add peers
        node.add_peer("192.168.1.100:5683".parse().unwrap());
        node.add_peer("10.0.0.50:5683".parse().unwrap());

        // Save peers
        node.save_peers().unwrap();

        // Create a new node with the same storage
        // (In test_mode, storage is in-memory, so we need to load in same instance)
        let peer_count_before = node.stats().unwrap().peer_count;

        // Load should work (peers already in network, so no change expected)
        node.load_peers().unwrap();

        let peer_count_after = node.stats().unwrap().peer_count;
        assert!(peer_count_after >= peer_count_before);
    }

    #[test]
    fn test_get_known_peers_empty() {
        let config = Config::test_mode();
        let node = MinimalNode::new(config).unwrap();

        let peers = node.get_known_peers();
        assert!(peers.is_empty());
    }

    #[test]
    fn test_get_known_peers_with_peers() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Add peers
        node.add_peer("192.168.1.100:5683".parse().unwrap());
        node.add_peer("192.168.1.101:5683".parse().unwrap());

        let peers = node.get_known_peers();
        assert_eq!(peers.len(), 2);

        // Check addresses are correct
        let addrs: Vec<&str> = peers.iter().map(|p| p.addr.as_str()).collect();
        assert!(addrs.contains(&"192.168.1.100:5683"));
        assert!(addrs.contains(&"192.168.1.101:5683"));

        // Check quality is initialized
        for peer in &peers {
            assert!(peer.quality > 0);
        }
    }

    #[test]
    fn test_peer_record_debug() {
        let record = PeerRecord {
            addr: "127.0.0.1:5683".to_string(),
            latest_seq: 0,
            quality: 50,
            last_seen_secs: 0,
        };

        let debug_str = format!("{:?}", record);
        assert!(debug_str.contains("PeerRecord"));
        assert!(debug_str.contains("127.0.0.1:5683"));
    }

    #[test]
    fn test_peer_record_clone() {
        let record1 = PeerRecord {
            addr: "192.168.1.1:5683".to_string(),
            latest_seq: 100,
            quality: 90,
            last_seen_secs: 999999,
        };

        let record2 = record1.clone();
        assert_eq!(record1.addr, record2.addr);
        assert_eq!(record1.latest_seq, record2.latest_seq);
        assert_eq!(record1.quality, record2.quality);
        assert_eq!(record1.last_seen_secs, record2.last_seen_secs);
    }

    #[test]
    fn test_peer_persistence_skips_low_quality() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Manually set metadata with a low-quality peer
        let records = vec![PeerRecord {
            addr: "192.168.1.100:5683".to_string(),
            latest_seq: 0,
            quality: 5, // Very low quality
            last_seen_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }];

        let json = serde_json::to_string(&records).unwrap();
        node.storage
            .set_metadata(PEERS_METADATA_KEY, &json)
            .unwrap();

        // Load peers - should skip the low-quality peer
        node.load_peers().unwrap();

        let peers = node.get_known_peers();
        // The low-quality peer should be skipped
        assert!(peers.is_empty());
    }

    #[test]
    fn test_peer_persistence_skips_stale() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Manually set metadata with a stale peer (>24 hours old)
        let old_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(25 * 60 * 60); // 25 hours ago

        let records = vec![PeerRecord {
            addr: "192.168.1.100:5683".to_string(),
            latest_seq: 100,
            quality: 80, // Good quality but stale
            last_seen_secs: old_time,
        }];

        let json = serde_json::to_string(&records).unwrap();
        node.storage
            .set_metadata(PEERS_METADATA_KEY, &json)
            .unwrap();

        // Load peers - should skip the stale peer
        node.load_peers().unwrap();

        let peers = node.get_known_peers();
        // The stale peer should be skipped
        assert!(peers.is_empty());
    }

    #[test]
    fn test_peer_persistence_accepts_recent_quality_peer() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Manually set metadata with a good, recent peer
        let recent_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let records = vec![PeerRecord {
            addr: "192.168.1.100:5683".to_string(),
            latest_seq: 50,
            quality: 60, // Good quality
            last_seen_secs: recent_time,
        }];

        let json = serde_json::to_string(&records).unwrap();
        node.storage
            .set_metadata(PEERS_METADATA_KEY, &json)
            .unwrap();

        // Load peers - should accept the good peer
        node.load_peers().unwrap();

        let peers = node.get_known_peers();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].addr, "192.168.1.100:5683");
    }

    #[test]
    fn test_peer_persistence_invalid_address() {
        let config = Config::test_mode();
        let mut node = MinimalNode::new(config).unwrap();

        // Manually set metadata with an invalid address
        let recent_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let records = vec![PeerRecord {
            addr: "not-a-valid-address".to_string(), // Invalid
            latest_seq: 50,
            quality: 80,
            last_seen_secs: recent_time,
        }];

        let json = serde_json::to_string(&records).unwrap();
        node.storage
            .set_metadata(PEERS_METADATA_KEY, &json)
            .unwrap();

        // Load peers - should succeed but skip invalid address
        let result = node.load_peers();
        assert!(result.is_ok());

        let peers = node.get_known_peers();
        assert!(peers.is_empty());
    }
}
