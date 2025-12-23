//! Minimal networking for IoT nodes
//!
//! Supports CoAP for lightweight IoT communication and
//! gossip protocol for peer-to-peer synchronization.
//!
//! # Transport Options
//! - **CoAP** (default): Lightweight UDP-based protocol for IoT
//! - **Memory**: In-memory transport for testing
//! - **QUIC**: Future support for reliable transport
//! - **Mesh**: Future support for mesh networking

use crate::config::{GossipConfig, TransportConfig};
use crate::discovery::Discovery;
use crate::error::{Error, Result};
use crate::types::{Hash, Record};
use serde::{Deserialize, Serialize};
use smol::channel::{bounded, Sender};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Counter for generating unique request IDs
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "coap")]
use crate::coap::CoapServer;

#[cfg(feature = "quic")]
use crate::quic::QuicServer;

/// Peer information
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Peer address
    pub addr: SocketAddr,
    /// Last seen timestamp
    pub last_seen: Instant,
    /// Peer's latest sequence number
    pub latest_seq: u32,
    /// Connection quality (0-100)
    pub quality: u8,
}

/// Network message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Ping for liveness
    Ping { node_id: String },
    /// Pong response
    Pong { node_id: String, latest_seq: u32 },
    /// Gossip request (asking for records)
    GossipRequest { from_seq: u32, limit: u32 },
    /// Gossip response (sending records)
    GossipResponse { records: Vec<Record> },
    /// New record announcement
    NewRecord { hash: Hash },
    /// Request specific record
    GetRecord { hash: Hash },
    /// Record data
    RecordData { record: Record },
    /// Remote procedure call to another node
    RemoteCall {
        /// Unique request ID for matching responses
        request_id: String,
        /// Source node ID
        from: String,
        /// Method to invoke
        method: String,
        /// JSON-encoded payload
        payload: Vec<u8>,
    },
    /// Response to a remote call
    RemoteCallResponse {
        /// Request ID this responds to
        request_id: String,
        /// Whether the call succeeded
        success: bool,
        /// Result data (if success) or error message (if failure)
        data: Vec<u8>,
    },
    /// Mesh relay message with TTL for multi-hop delivery
    MeshRelay {
        /// Unique message ID to prevent duplicates
        message_id: String,
        /// Original sender node ID
        origin: String,
        /// Time-to-live (decremented at each hop)
        ttl: u8,
        /// Inner message being relayed
        inner: Box<Message>,
    },
}

/// Pending RPC request tracking
#[derive(Debug)]
struct PendingRpc {
    /// Sender to deliver the response
    response_tx: Sender<RpcResponse>,
    /// When the request was sent
    sent_at: Instant,
    /// Method being called
    method: String,
}

/// RPC Response wrapper
#[derive(Debug, Clone)]
pub struct RpcResponse {
    /// Whether the call succeeded
    pub success: bool,
    /// Response data (or error message)
    pub data: Vec<u8>,
}

/// Default TTL for mesh messages
pub const DEFAULT_MESH_TTL: u8 = 5;

/// Maximum number of seen message IDs to track
const MAX_SEEN_MESSAGES: usize = 10000;

/// Mesh relay manager for multi-hop message delivery
///
/// Tracks seen messages to prevent duplicate processing and handles
/// TTL-based message forwarding across the mesh network.
#[derive(Debug, Default)]
pub struct MeshManager {
    /// Set of recently seen message IDs (for deduplication)
    seen_messages: std::collections::HashSet<String>,
    /// Queue for eviction when limit reached (FIFO order)
    seen_order: std::collections::VecDeque<String>,
    /// Relay statistics
    pub stats: MeshStats,
}

/// Mesh networking statistics
#[derive(Debug, Clone, Default)]
pub struct MeshStats {
    /// Messages relayed to other nodes
    pub messages_relayed: u64,
    /// Messages dropped (TTL expired)
    pub messages_dropped_ttl: u64,
    /// Duplicate messages ignored
    pub duplicates_ignored: u64,
    /// Messages originated by this node
    pub messages_originated: u64,
}

impl MeshManager {
    /// Create a new mesh manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if message was already seen
    pub fn is_seen(&self, message_id: &str) -> bool {
        self.seen_messages.contains(message_id)
    }

    /// Mark a message as seen
    pub fn mark_seen(&mut self, message_id: String) {
        if self.seen_messages.insert(message_id.clone()) {
            self.seen_order.push_back(message_id);

            // Evict oldest if limit reached
            while self.seen_messages.len() > MAX_SEEN_MESSAGES {
                if let Some(old_id) = self.seen_order.pop_front() {
                    self.seen_messages.remove(&old_id);
                }
            }
        }
    }

    /// Process a mesh relay message
    ///
    /// Returns (should_process, should_relay, decremented_ttl)
    pub fn process_relay(&mut self, message_id: &str, ttl: u8) -> (bool, bool, u8) {
        // Check for duplicate
        if self.is_seen(message_id) {
            self.stats.duplicates_ignored += 1;
            return (false, false, 0);
        }

        // Check TTL
        if ttl == 0 {
            self.stats.messages_dropped_ttl += 1;
            return (false, false, 0);
        }

        // Mark as seen
        self.mark_seen(message_id.to_string());

        // Decrement TTL for relay
        let new_ttl = ttl.saturating_sub(1);
        let should_relay = new_ttl > 0;

        (true, should_relay, new_ttl)
    }

    /// Generate a unique message ID
    pub fn generate_message_id(node_id: &str) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{}:{}", node_id, timestamp)
    }

    /// Wrap a message for mesh relay
    pub fn wrap_for_relay(
        node_id: &str,
        message: Message,
        ttl: Option<u8>,
    ) -> Message {
        let message_id = Self::generate_message_id(node_id);
        Message::MeshRelay {
            message_id,
            origin: node_id.to_string(),
            ttl: ttl.unwrap_or(DEFAULT_MESH_TTL),
            inner: Box::new(message),
        }
    }
}

/// Network manager for minimal node
pub struct Network {
    config: TransportConfig,
    gossip_config: GossipConfig,
    peers: HashMap<SocketAddr, PeerInfo>,
    node_id: String,
    /// CoAP server (when coap feature enabled)
    #[cfg(feature = "coap")]
    coap_server: Option<CoapServer>,
    /// QUIC server (when quic feature enabled)
    #[cfg(feature = "quic")]
    quic_server: Option<QuicServer>,
    /// mDNS discovery service
    discovery: Option<Discovery>,
    /// Pending RPC requests waiting for responses
    pending_rpcs: HashMap<String, PendingRpc>,
    /// Mesh relay manager for multi-hop message delivery
    mesh_manager: MeshManager,
}

impl Network {
    /// Create new network manager
    pub fn new(config: TransportConfig, gossip_config: GossipConfig, node_id: String) -> Self {
        Self {
            config,
            gossip_config,
            peers: HashMap::new(),
            node_id,
            #[cfg(feature = "coap")]
            coap_server: None,
            #[cfg(feature = "quic")]
            quic_server: None,
            discovery: None,
            pending_rpcs: HashMap::new(),
            mesh_manager: MeshManager::new(),
        }
    }

    /// Start the network
    pub async fn start(&mut self) -> Result<()> {
        match &self.config {
            TransportConfig::Memory => {
                log::info!("Starting memory transport (testing only)");
                Ok(())
            }
            #[cfg(feature = "coap")]
            TransportConfig::Coap { bind_addr, port } => {
                log::info!("Starting CoAP transport on {}:{}", bind_addr, port);

                let mut server = CoapServer::new(bind_addr.clone(), *port, self.node_id.clone());
                server.start().await?;

                // Join multicast for discovery
                if let Err(e) = server.join_multicast() {
                    log::warn!("Failed to join multicast: {}", e);
                }

                self.coap_server = Some(server);
                Ok(())
            }
            #[cfg(not(feature = "coap"))]
            TransportConfig::Coap { bind_addr, port } => {
                log::warn!("CoAP transport requested but feature not enabled");
                log::info!("Would start CoAP on {}:{}", bind_addr, port);
                Ok(())
            }
            #[cfg(feature = "quic")]
            TransportConfig::Quic { bind_addr, port } => {
                use crate::quic::QuicConfig;

                log::info!("Starting QUIC transport on {}:{}", bind_addr, port);

                let quic_config = QuicConfig {
                    bind_addr: bind_addr.clone(),
                    port: *port,
                    ..QuicConfig::default()
                };

                let mut server = QuicServer::new(quic_config, self.node_id.clone());
                server.start().await?;

                self.quic_server = Some(server);
                Ok(())
            }
            #[cfg(not(feature = "quic"))]
            TransportConfig::Quic { bind_addr, port } => {
                log::warn!("QUIC transport requested but feature not enabled");
                log::info!("Would start QUIC on {}:{}", bind_addr, port);
                Ok(())
            }
            TransportConfig::Mesh { mode } => {
                log::info!("Starting mesh transport: {:?}", mode);
                // Mesh implementation would go here
                Ok(())
            }
            #[cfg(feature = "webrtc")]
            TransportConfig::WebRtc {
                stun_server,
                signaling_port,
                ..
            } => {
                log::info!(
                    "Starting WebRTC transport with STUN {} on signaling port {}",
                    stun_server,
                    signaling_port
                );
                // WebRTC implementation is in webrtc module
                Ok(())
            }
            #[cfg(feature = "ble")]
            TransportConfig::Ble {
                device_name,
                mesh_relay,
                tx_power,
            } => {
                log::info!(
                    "Starting BLE transport: {} (relay: {}, tx_power: {}dBm)",
                    device_name,
                    mesh_relay,
                    tx_power
                );
                // BLE implementation is in bluetooth module
                Ok(())
            }
        }
    }

    /// Stop the network
    pub async fn stop(&mut self) -> Result<()> {
        log::info!("Stopping network");

        // Stop mDNS discovery
        if let Some(ref mut discovery) = self.discovery {
            discovery.stop()?;
        }
        self.discovery = None;

        #[cfg(feature = "coap")]
        if let Some(ref mut server) = self.coap_server {
            server.stop().await?;
            self.coap_server = None;
        }

        self.peers.clear();
        Ok(())
    }

    /// Start mDNS discovery for automatic peer finding
    pub fn start_discovery(&mut self, port: u16) -> Result<()> {
        let mut discovery = Discovery::new(self.node_id.clone(), port)?;
        discovery.register()?;
        discovery.start_browsing()?;
        self.discovery = Some(discovery);
        log::info!("mDNS discovery started");
        Ok(())
    }

    /// Sync discovered peers into the peer list
    pub fn sync_discovered_peers(&mut self) {
        if let Some(ref discovery) = self.discovery {
            let discovered = discovery.get_peer_addrs();
            for addr in discovered {
                if !self.peers.contains_key(&addr) {
                    log::debug!("Adding discovered peer: {}", addr);
                    self.add_peer(addr);
                }
            }
        }
    }

    /// Get discovery peer count
    pub fn discovered_peer_count(&self) -> usize {
        self.discovery.as_ref().map(|d| d.peer_count()).unwrap_or(0)
    }

    /// Receive a message from the network
    #[cfg(feature = "coap")]
    pub async fn recv(&self) -> Result<Option<(SocketAddr, Message)>> {
        if let Some(ref server) = self.coap_server {
            server.recv().await
        } else {
            Ok(None)
        }
    }

    /// Receive without CoAP (fallback)
    #[cfg(not(feature = "coap"))]
    pub async fn recv(&self) -> Result<Option<(SocketAddr, Message)>> {
        Ok(None)
    }

    /// Check if network is running
    pub fn is_running(&self) -> bool {
        #[cfg(feature = "coap")]
        if let Some(ref server) = self.coap_server {
            return server.is_running();
        }
        false
    }

    /// Get CoAP server reference (for advanced operations)
    #[cfg(feature = "coap")]
    pub fn coap_server(&self) -> Option<&CoapServer> {
        self.coap_server.as_ref()
    }

    /// Get mutable CoAP server reference
    #[cfg(feature = "coap")]
    pub fn coap_server_mut(&mut self) -> Option<&mut CoapServer> {
        self.coap_server.as_mut()
    }

    /// Add a peer
    pub fn add_peer(&mut self, addr: SocketAddr) {
        self.peers.insert(
            addr,
            PeerInfo {
                addr,
                last_seen: Instant::now(),
                latest_seq: 0,
                quality: 50,
            },
        );
    }

    /// Remove a peer
    pub fn remove_peer(&mut self, addr: &SocketAddr) {
        self.peers.remove(addr);
    }

    /// Get active peers
    pub fn active_peers(&self) -> Vec<&PeerInfo> {
        let timeout = Duration::from_secs(300); // 5 minutes
        self.peers
            .values()
            .filter(|p| p.last_seen.elapsed() < timeout)
            .collect()
    }

    /// Update peer info
    pub fn update_peer(&mut self, addr: SocketAddr, latest_seq: u32) {
        if let Some(peer) = self.peers.get_mut(&addr) {
            peer.last_seen = Instant::now();
            peer.latest_seq = latest_seq;
            // Increase quality on successful interaction
            peer.quality = (peer.quality + 5).min(100);
        }
    }

    /// Mark peer as failed
    pub fn mark_peer_failed(&mut self, addr: &SocketAddr) {
        if let Some(peer) = self.peers.get_mut(addr) {
            // Decrease quality on failure
            peer.quality = peer.quality.saturating_sub(10);
        }
    }

    /// Send message to peer
    ///
    /// Uses CoAP transport when the feature is enabled, otherwise logs a debug message.
    /// By default, messages are sent as non-confirmable (NON) for efficiency.
    /// Use `send_confirmable` for reliable delivery.
    #[cfg(feature = "coap")]
    pub async fn send(&mut self, addr: &SocketAddr, message: &Message) -> Result<()> {
        if let Some(ref mut server) = self.coap_server {
            server.send(addr, message, false).await
        } else {
            let _data = serde_json::to_vec(message)?;
            log::debug!("Sending message to {} (no transport): {:?}", addr, message);
            Ok(())
        }
    }

    /// Send message to peer (fallback without CoAP)
    #[cfg(not(feature = "coap"))]
    pub async fn send(&mut self, addr: &SocketAddr, message: &Message) -> Result<()> {
        let _data = serde_json::to_vec(message)?;
        log::debug!("Sending message to {}: {:?}", addr, message);
        Ok(())
    }

    /// Send message with confirmation (reliable delivery)
    #[cfg(feature = "coap")]
    pub async fn send_confirmable(&mut self, addr: &SocketAddr, message: &Message) -> Result<()> {
        if let Some(ref mut server) = self.coap_server {
            server.send(addr, message, true).await
        } else {
            self.send(addr, message).await
        }
    }

    /// Send confirmable (fallback)
    #[cfg(not(feature = "coap"))]
    pub async fn send_confirmable(&mut self, addr: &SocketAddr, message: &Message) -> Result<()> {
        self.send(addr, message).await
    }

    /// Broadcast message to all peers
    pub async fn broadcast(&mut self, message: &Message) -> Result<()> {
        let peers: Vec<SocketAddr> = self.active_peers().iter().map(|p| p.addr).collect();
        for addr in peers {
            if let Err(e) = self.send(&addr, message).await {
                log::warn!("Failed to send to {}: {}", addr, e);
            }
        }
        Ok(())
    }

    /// Send discovery request (multicast)
    #[cfg(feature = "coap")]
    pub async fn send_discovery(&mut self) -> Result<()> {
        if let Some(ref mut server) = self.coap_server {
            server.send_discovery().await
        } else {
            Ok(())
        }
    }

    /// Send discovery (fallback)
    #[cfg(not(feature = "coap"))]
    pub async fn send_discovery(&mut self) -> Result<()> {
        log::debug!("Discovery not available without CoAP");
        Ok(())
    }

    /// Handle retransmissions for pending messages
    #[cfg(feature = "coap")]
    pub async fn handle_retransmissions(&mut self) -> Result<()> {
        if let Some(ref mut server) = self.coap_server {
            server.handle_retransmissions().await
        } else {
            Ok(())
        }
    }

    /// Handle retransmissions (fallback)
    #[cfg(not(feature = "coap"))]
    pub async fn handle_retransmissions(&mut self) -> Result<()> {
        Ok(())
    }

    /// Get peers for gossip (best quality first)
    pub fn gossip_peers(&self) -> Vec<SocketAddr> {
        let mut peers: Vec<_> = self.active_peers().into_iter().collect();
        peers.sort_by(|a, b| b.quality.cmp(&a.quality));
        peers
            .into_iter()
            .take(self.gossip_config.max_peers)
            .map(|p| p.addr)
            .collect()
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.active_peers().len()
    }

    // ==================== RPC Methods ====================

    /// Generate a unique request ID for RPC calls
    fn generate_request_id() -> String {
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("rpc-{}-{}", std::process::id(), id)
    }

    /// Send a remote procedure call and wait for response
    ///
    /// This method sends a RemoteCall message to the target peer and waits
    /// for a RemoteCallResponse with matching request_id.
    ///
    /// # Arguments
    /// * `addr` - Target peer address
    /// * `method` - RPC method name
    /// * `payload` - JSON-encoded payload
    /// * `timeout` - Maximum time to wait for response
    ///
    /// # Returns
    /// * `Ok(RpcResponse)` - The response from the remote peer
    /// * `Err(Error::Timeout)` - If no response within timeout
    /// * `Err(Error::Network)` - If send fails
    #[cfg(feature = "coap")]
    pub async fn send_remote_call(
        &mut self,
        addr: &SocketAddr,
        method: &str,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> Result<RpcResponse> {
        let request_id = Self::generate_request_id();

        // Create channel for receiving response
        let (tx, rx) = bounded::<RpcResponse>(1);

        // Track pending RPC
        self.pending_rpcs.insert(
            request_id.clone(),
            PendingRpc {
                response_tx: tx,
                sent_at: Instant::now(),
                method: method.to_string(),
            },
        );

        // Create and send the RemoteCall message
        let message = Message::RemoteCall {
            request_id: request_id.clone(),
            from: self.node_id.clone(),
            method: method.to_string(),
            payload,
        };

        // Send using confirmable message for reliability
        if let Err(e) = self.send_confirmable(addr, &message).await {
            self.pending_rpcs.remove(&request_id);
            return Err(e);
        }

        log::debug!(
            "RPC call {} to {}: method={}",
            request_id,
            addr,
            method
        );

        // Wait for response with timeout
        match smol::future::or(
            async {
                rx.recv()
                    .await
                    .map_err(|_| Error::network("RPC channel closed".to_string()))
            },
            async {
                smol::Timer::after(timeout).await;
                Err(Error::Timeout(format!(
                    "RPC {} timed out after {:?}",
                    request_id, timeout
                )))
            },
        )
        .await
        {
            Ok(response) => {
                self.pending_rpcs.remove(&request_id);
                log::debug!("RPC {} completed: success={}", request_id, response.success);
                Ok(response)
            }
            Err(e) => {
                self.pending_rpcs.remove(&request_id);
                log::warn!("RPC {} failed: {}", request_id, e);
                Err(e)
            }
        }
    }

    /// Send remote call (fallback without CoAP)
    #[cfg(not(feature = "coap"))]
    pub async fn send_remote_call(
        &mut self,
        _addr: &SocketAddr,
        _method: &str,
        _payload: Vec<u8>,
        _timeout: Duration,
    ) -> Result<RpcResponse> {
        Err(Error::network("RPC requires CoAP feature".to_string()))
    }

    /// Handle an incoming RPC response
    ///
    /// Call this when receiving a RemoteCallResponse message to deliver
    /// the response to the waiting caller.
    pub fn handle_rpc_response(&mut self, request_id: &str, success: bool, data: Vec<u8>) {
        if let Some(pending) = self.pending_rpcs.remove(request_id) {
            let response = RpcResponse { success, data };
            let elapsed = pending.sent_at.elapsed();

            if let Err(e) = pending.response_tx.try_send(response) {
                log::warn!(
                    "Failed to deliver RPC response {}: {:?}",
                    request_id,
                    e
                );
            } else {
                log::debug!(
                    "RPC {} response delivered (method={}, elapsed={:?})",
                    request_id,
                    pending.method,
                    elapsed
                );
            }
        } else {
            log::warn!(
                "Received response for unknown RPC: {} (may have timed out)",
                request_id
            );
        }
    }

    /// Clean up timed out RPC requests
    ///
    /// Call this periodically to clean up RPC requests that have exceeded
    /// the maximum timeout without a response.
    pub fn cleanup_stale_rpcs(&mut self, max_age: Duration) {
        let now = Instant::now();
        let stale: Vec<String> = self
            .pending_rpcs
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.sent_at) > max_age)
            .map(|(id, _)| id.clone())
            .collect();

        for request_id in stale {
            if let Some(pending) = self.pending_rpcs.remove(&request_id) {
                log::warn!(
                    "RPC {} timed out (method={}, age={:?})",
                    request_id,
                    pending.method,
                    now.duration_since(pending.sent_at)
                );
            }
        }
    }

    /// Get count of pending RPC requests
    pub fn pending_rpc_count(&self) -> usize {
        self.pending_rpcs.len()
    }

    // ==================== Mesh Relay Methods ====================

    /// Process a mesh relay message
    ///
    /// Returns the inner message if it should be processed locally,
    /// and handles relaying to other peers if TTL > 0.
    pub async fn process_mesh_message(
        &mut self,
        from: SocketAddr,
        message_id: &str,
        origin: &str,
        ttl: u8,
        inner: Message,
    ) -> Option<Message> {
        // Process through mesh manager
        let (should_process, should_relay, new_ttl) =
            self.mesh_manager.process_relay(message_id, ttl);

        if !should_process {
            log::trace!("Mesh message {} skipped (duplicate or TTL=0)", message_id);
            return None;
        }

        // Relay to other peers if TTL > 0
        if should_relay {
            let relay_msg = Message::MeshRelay {
                message_id: message_id.to_string(),
                origin: origin.to_string(),
                ttl: new_ttl,
                inner: Box::new(inner.clone()),
            };

            // Send to all peers except the one we received from
            let peers_to_relay: Vec<SocketAddr> = self
                .active_peers()
                .iter()
                .filter(|p| p.addr != from)
                .map(|p| p.addr)
                .collect();

            for peer_addr in peers_to_relay {
                if let Err(e) = self.send(&peer_addr, &relay_msg).await {
                    log::warn!("Failed to relay to {}: {}", peer_addr, e);
                } else {
                    self.mesh_manager.stats.messages_relayed += 1;
                }
            }
        }

        Some(inner)
    }

    /// Broadcast a message through the mesh network
    ///
    /// Wraps the message with TTL and sends to all connected peers.
    pub async fn mesh_broadcast(&mut self, message: Message) -> Result<usize> {
        let wrapped = MeshManager::wrap_for_relay(&self.node_id, message, None);

        // Mark as seen so we don't process our own broadcast
        if let Message::MeshRelay { ref message_id, .. } = wrapped {
            self.mesh_manager.mark_seen(message_id.clone());
        }

        self.mesh_manager.stats.messages_originated += 1;

        let peers: Vec<SocketAddr> = self.active_peers().iter().map(|p| p.addr).collect();
        let mut sent_count = 0;

        for peer_addr in peers {
            if let Err(e) = self.send(&peer_addr, &wrapped).await {
                log::warn!("Failed to broadcast to {}: {}", peer_addr, e);
            } else {
                sent_count += 1;
            }
        }

        Ok(sent_count)
    }

    /// Get mesh manager reference
    pub fn mesh_manager(&self) -> &MeshManager {
        &self.mesh_manager
    }

    /// Get mutable mesh manager reference
    pub fn mesh_manager_mut(&mut self) -> &mut MeshManager {
        &mut self.mesh_manager
    }

    /// Get mesh statistics
    pub fn mesh_stats(&self) -> &MeshStats {
        &self.mesh_manager.stats
    }
}

/// Gossip manager
pub struct GossipManager {
    config: GossipConfig,
    last_gossip: Instant,
    pending_announcements: Vec<Hash>,
}

impl GossipManager {
    /// Create new gossip manager
    pub fn new(config: GossipConfig) -> Self {
        Self {
            config,
            last_gossip: Instant::now(),
            pending_announcements: Vec::new(),
        }
    }

    /// Check if gossip should run
    pub fn should_gossip(&self) -> bool {
        self.last_gossip.elapsed() >= self.config.loop_delay
    }

    /// Mark gossip as complete
    pub fn gossip_complete(&mut self, success: bool) {
        self.last_gossip = Instant::now();
        if success {
            // Use success delay
        } else {
            // Use error delay
        }
    }

    /// Queue record for announcement
    pub fn announce(&mut self, hash: Hash) {
        self.pending_announcements.push(hash);
    }

    /// Get pending announcements
    pub fn take_announcements(&mut self) -> Vec<Hash> {
        std::mem::take(&mut self.pending_announcements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_creation() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let network = Network::new(config, gossip, "test-node".to_string());
        assert_eq!(network.peer_count(), 0);
    }

    #[test]
    fn test_peer_management() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        network.add_peer(addr);
        assert_eq!(network.peer_count(), 1);

        network.remove_peer(&addr);
        assert_eq!(network.peer_count(), 0);
    }

    #[test]
    fn test_gossip_timing() {
        let mut config = GossipConfig::default();
        config.loop_delay = Duration::from_millis(10);

        let mut gossip = GossipManager::new(config);
        assert!(!gossip.should_gossip()); // Just created

        std::thread::sleep(Duration::from_millis(20));
        assert!(gossip.should_gossip());

        gossip.gossip_complete(true);
        assert!(!gossip.should_gossip());
    }

    #[test]
    fn test_peer_info_struct() {
        let addr: SocketAddr = "192.168.1.1:5683".parse().unwrap();
        let peer = PeerInfo {
            addr,
            last_seen: Instant::now(),
            latest_seq: 42,
            quality: 75,
        };

        assert_eq!(peer.addr, addr);
        assert_eq!(peer.latest_seq, 42);
        assert_eq!(peer.quality, 75);
    }

    #[test]
    fn test_peer_info_clone() {
        let addr: SocketAddr = "10.0.0.1:1234".parse().unwrap();
        let peer1 = PeerInfo {
            addr,
            last_seen: Instant::now(),
            latest_seq: 100,
            quality: 90,
        };

        let peer2 = peer1.clone();
        assert_eq!(peer1.addr, peer2.addr);
        assert_eq!(peer1.latest_seq, peer2.latest_seq);
        assert_eq!(peer1.quality, peer2.quality);
    }

    #[test]
    fn test_peer_info_debug() {
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let peer = PeerInfo {
            addr,
            last_seen: Instant::now(),
            latest_seq: 0,
            quality: 50,
        };
        let debug_str = format!("{:?}", peer);
        assert!(debug_str.contains("PeerInfo"));
        assert!(debug_str.contains("127.0.0.1:5683"));
    }

    #[test]
    fn test_message_ping_serialize() {
        let msg = Message::Ping {
            node_id: "node123".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Ping"));
        assert!(json.contains("node123"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::Ping { node_id } = parsed {
            assert_eq!(node_id, "node123");
        } else {
            panic!("Expected Ping message");
        }
    }

    #[test]
    fn test_message_pong_serialize() {
        let msg = Message::Pong {
            node_id: "node456".to_string(),
            latest_seq: 42,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Pong"));
        assert!(json.contains("42"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::Pong {
            node_id,
            latest_seq,
        } = parsed
        {
            assert_eq!(node_id, "node456");
            assert_eq!(latest_seq, 42);
        } else {
            panic!("Expected Pong message");
        }
    }

    #[test]
    fn test_message_gossip_request_serialize() {
        let msg = Message::GossipRequest {
            from_seq: 10,
            limit: 50,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("GossipRequest"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::GossipRequest { from_seq, limit } = parsed {
            assert_eq!(from_seq, 10);
            assert_eq!(limit, 50);
        } else {
            panic!("Expected GossipRequest message");
        }
    }

    #[test]
    fn test_message_gossip_response_serialize() {
        let msg = Message::GossipResponse { records: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("GossipResponse"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::GossipResponse { records } = parsed {
            assert!(records.is_empty());
        } else {
            panic!("Expected GossipResponse message");
        }
    }

    #[test]
    fn test_message_new_record_serialize() {
        let hash = Hash::from_raw(&[0u8; 32]);
        let msg = Message::NewRecord { hash: hash.clone() };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("NewRecord"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::NewRecord { hash: h } = parsed {
            assert_eq!(h, hash);
        } else {
            panic!("Expected NewRecord message");
        }
    }

    #[test]
    fn test_message_get_record_serialize() {
        let hash = Hash::from_raw(&[0u8; 32]);
        let msg = Message::GetRecord { hash: hash.clone() };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("GetRecord"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::GetRecord { hash: h } = parsed {
            assert_eq!(h, hash);
        } else {
            panic!("Expected GetRecord message");
        }
    }

    #[test]
    fn test_message_clone() {
        let msg1 = Message::Ping {
            node_id: "test".to_string(),
        };
        let msg2 = msg1.clone();

        if let (Message::Ping { node_id: n1 }, Message::Ping { node_id: n2 }) = (&msg1, &msg2) {
            assert_eq!(n1, n2);
        } else {
            panic!("Clone failed");
        }
    }

    #[test]
    fn test_message_debug() {
        let msg = Message::Ping {
            node_id: "debug-test".to_string(),
        };
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("Ping"));
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_network_update_peer() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        network.add_peer(addr);

        // Update peer with new seq
        network.update_peer(addr, 100);

        let peers = network.active_peers();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].latest_seq, 100);
        // Quality should increase
        assert!(peers[0].quality > 50);
    }

    #[test]
    fn test_network_update_nonexistent_peer() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        // Update peer that doesn't exist - should not panic
        network.update_peer(addr, 100);
        assert_eq!(network.peer_count(), 0);
    }

    #[test]
    fn test_network_mark_peer_failed() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        network.add_peer(addr);

        let initial_quality = network.active_peers()[0].quality;

        // Mark peer as failed
        network.mark_peer_failed(&addr);

        let peers = network.active_peers();
        assert!(peers[0].quality < initial_quality);
    }

    #[test]
    fn test_network_mark_nonexistent_peer_failed() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        // Mark peer that doesn't exist - should not panic
        network.mark_peer_failed(&addr);
    }

    #[test]
    fn test_network_active_peers_timeout() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        network.add_peer(addr);

        // Active peers should include the just-added peer
        let active = network.active_peers();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_network_gossip_peers_sorting() {
        let config = TransportConfig::Memory;
        let mut gossip = GossipConfig::default();
        gossip.max_peers = 10;
        let mut network = Network::new(config, gossip, "test-node".to_string());

        // Add multiple peers
        for i in 0..5 {
            let addr: SocketAddr = format!("127.0.0.1:808{}", i).parse().unwrap();
            network.add_peer(addr);
        }

        // Improve quality for some peers
        let addr1: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        network.update_peer(addr1, 10);
        network.update_peer(addr1, 20);

        let gossip_peers = network.gossip_peers();
        assert!(!gossip_peers.is_empty());
        // First peer should have highest quality
    }

    #[test]
    fn test_network_gossip_peers_limited() {
        let config = TransportConfig::Memory;
        let mut gossip = GossipConfig::default();
        gossip.max_peers = 2;
        let mut network = Network::new(config, gossip, "test-node".to_string());

        // Add more peers than limit
        for i in 0..5 {
            let addr: SocketAddr = format!("127.0.0.1:808{}", i).parse().unwrap();
            network.add_peer(addr);
        }

        let gossip_peers = network.gossip_peers();
        assert_eq!(gossip_peers.len(), 2); // Limited to max_peers
    }

    #[test]
    fn test_network_is_running_false() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let network = Network::new(config, gossip, "test-node".to_string());

        assert!(!network.is_running());
    }

    #[test]
    fn test_network_discovered_peer_count_no_discovery() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let network = Network::new(config, gossip, "test-node".to_string());

        assert_eq!(network.discovered_peer_count(), 0);
    }

    #[test]
    fn test_gossip_manager_creation() {
        let config = GossipConfig::default();
        let manager = GossipManager::new(config);

        assert!(manager.pending_announcements.is_empty());
    }

    #[test]
    fn test_gossip_manager_announce() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        let hash = Hash::from_raw(&[1u8; 32]);
        manager.announce(hash.clone());

        assert_eq!(manager.pending_announcements.len(), 1);
    }

    #[test]
    fn test_gossip_manager_take_announcements() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        let hash1 = Hash::from_raw(&[1u8; 32]);
        let hash2 = Hash::from_raw(&[2u8; 32]);
        manager.announce(hash1);
        manager.announce(hash2);

        let announcements = manager.take_announcements();
        assert_eq!(announcements.len(), 2);

        // Should be empty after taking
        assert!(manager.pending_announcements.is_empty());
        assert!(manager.take_announcements().is_empty());
    }

    #[test]
    fn test_gossip_manager_complete_failure() {
        let mut config = GossipConfig::default();
        config.loop_delay = Duration::from_millis(10);

        let mut manager = GossipManager::new(config);

        std::thread::sleep(Duration::from_millis(20));
        assert!(manager.should_gossip());

        // Complete with failure
        manager.gossip_complete(false);
        assert!(!manager.should_gossip());
    }

    #[test]
    fn test_network_quality_caps_at_100() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        network.add_peer(addr);

        // Update many times to try to exceed 100
        for _ in 0..50 {
            network.update_peer(addr, 1);
        }

        let peers = network.active_peers();
        assert_eq!(peers[0].quality, 100); // Capped at 100
    }

    #[test]
    fn test_network_quality_floor_at_0() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        network.add_peer(addr);

        // Mark failed many times to try to go below 0
        for _ in 0..20 {
            network.mark_peer_failed(&addr);
        }

        let peers = network.active_peers();
        assert_eq!(peers[0].quality, 0); // Floored at 0
    }

    #[test]
    fn test_network_multiple_peers() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        let addrs: Vec<SocketAddr> = vec![
            "127.0.0.1:8080".parse().unwrap(),
            "127.0.0.1:8081".parse().unwrap(),
            "127.0.0.1:8082".parse().unwrap(),
        ];

        for addr in &addrs {
            network.add_peer(*addr);
        }

        assert_eq!(network.peer_count(), 3);

        // Remove one
        network.remove_peer(&addrs[1]);
        assert_eq!(network.peer_count(), 2);
    }

    #[test]
    fn test_message_record_data_serialize() {
        use crate::types::{
            Action, ActionType, AgentPubKey, Entry, EntryType, Signature, Timestamp,
        };

        let record = Record {
            action: Action {
                action_type: ActionType::Create,
                author: AgentPubKey([0u8; 32]),
                timestamp: Timestamp(1234567890),
                seq: 1,
                prev_action: None,
                entry_hash: None,
                signature: Signature([0u8; 64]),
            },
            entry: Some(Entry {
                entry_type: EntryType::App,
                content: vec![1, 2, 3],
            }),
        };

        let msg = Message::RecordData { record };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("RecordData"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Message::RecordData { .. }));
    }

    #[test]
    fn test_network_sync_discovered_peers_no_discovery() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        // Should not panic when discovery is None
        network.sync_discovered_peers();
        assert_eq!(network.peer_count(), 0);
    }

    #[test]
    fn test_transport_config_memory() {
        let config = TransportConfig::Memory;
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("Memory"));
    }

    #[test]
    fn test_transport_config_coap() {
        let config = TransportConfig::Coap {
            bind_addr: "0.0.0.0".to_string(),
            port: 5683,
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("Coap"));
        assert!(debug_str.contains("5683"));
    }

    #[test]
    fn test_transport_config_quic() {
        let config = TransportConfig::Quic {
            bind_addr: "0.0.0.0".to_string(),
            port: 5684,
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("Quic"));
    }

    #[test]
    fn test_transport_config_mesh() {
        use crate::config::MeshMode;

        let config = TransportConfig::Mesh {
            mode: MeshMode::WiFiDirect,
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("Mesh"));
        assert!(debug_str.contains("WiFiDirect"));
    }

    #[test]
    fn test_network_gossip_manager_multiple_announces() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        for i in 0..10 {
            let mut bytes = [0u8; 32];
            bytes[0] = i;
            manager.announce(Hash::from_raw(&bytes));
        }

        assert_eq!(manager.pending_announcements.len(), 10);

        let taken = manager.take_announcements();
        assert_eq!(taken.len(), 10);
        assert!(manager.pending_announcements.is_empty());
    }

    #[test]
    fn test_network_start_memory_transport() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let result = network.start().await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_network_stop() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            network.start().await.unwrap();
            let result = network.stop().await;
            assert!(result.is_ok());
            assert_eq!(network.peer_count(), 0);
        });
    }

    #[test]
    fn test_network_recv_memory() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let result = network.recv().await;
            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
        });
    }

    #[test]
    fn test_network_send_memory() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
            let msg = Message::Ping {
                node_id: "test".to_string(),
            };
            let result = network.send(&addr, &msg).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_network_send_confirmable_memory() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
            let msg = Message::Ping {
                node_id: "test".to_string(),
            };
            let result = network.send_confirmable(&addr, &msg).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_network_broadcast() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        // Add some peers
        let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        network.add_peer(addr1);
        network.add_peer(addr2);

        smol::block_on(async {
            let msg = Message::Ping {
                node_id: "broadcast".to_string(),
            };
            let result = network.broadcast(&msg).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_network_send_discovery_memory() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let result = network.send_discovery().await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_network_handle_retransmissions_memory() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let result = network.handle_retransmissions().await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_network_start_quic() {
        let config = TransportConfig::Quic {
            bind_addr: "127.0.0.1".to_string(),
            port: 15684, // Use high port to avoid conflicts
        };
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let result = network.start().await;
            // With quic feature, this may succeed or fail depending on environment
            // Without quic feature, this should always succeed (no-op)
            #[cfg(not(feature = "quic"))]
            assert!(result.is_ok());
            #[cfg(feature = "quic")]
            {
                // In test environment, QUIC may fail due to cert/binding issues
                // Just verify it doesn't panic
                let _ = result;
            }
        });
    }

    #[test]
    fn test_network_start_mesh() {
        use crate::config::MeshMode;

        let config = TransportConfig::Mesh {
            mode: MeshMode::BluetoothLE,
        };
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let result = network.start().await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_gossip_manager_config() {
        let mut config = GossipConfig::default();
        config.loop_delay = Duration::from_millis(100);
        config.max_peers = 5;

        let manager = GossipManager::new(config.clone());

        // Verify config was stored
        assert_eq!(manager.config.loop_delay, Duration::from_millis(100));
        assert_eq!(manager.config.max_peers, 5);
    }

    // ==================== RPC Tests ====================

    #[test]
    fn test_rpc_response_struct() {
        let response = RpcResponse {
            success: true,
            data: vec![1, 2, 3, 4],
        };
        assert!(response.success);
        assert_eq!(response.data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_rpc_response_clone() {
        let response1 = RpcResponse {
            success: false,
            data: b"error message".to_vec(),
        };
        let response2 = response1.clone();
        assert_eq!(response1.success, response2.success);
        assert_eq!(response1.data, response2.data);
    }

    #[test]
    fn test_rpc_response_debug() {
        let response = RpcResponse {
            success: true,
            data: vec![],
        };
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("RpcResponse"));
        assert!(debug_str.contains("success: true"));
    }

    #[test]
    fn test_generate_request_id() {
        let id1 = Network::generate_request_id();
        let id2 = Network::generate_request_id();

        // IDs should be unique
        assert_ne!(id1, id2);

        // IDs should have the expected format
        assert!(id1.starts_with("rpc-"));
        assert!(id2.starts_with("rpc-"));
    }

    #[test]
    fn test_pending_rpc_count() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let network = Network::new(config, gossip, "test-node".to_string());

        assert_eq!(network.pending_rpc_count(), 0);
    }

    #[test]
    fn test_handle_rpc_response_unknown_request() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        // Should not panic on unknown request ID
        network.handle_rpc_response("unknown-id", true, vec![1, 2, 3]);
        assert_eq!(network.pending_rpc_count(), 0);
    }

    #[test]
    fn test_cleanup_stale_rpcs_empty() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        // Should not panic with empty pending RPCs
        network.cleanup_stale_rpcs(Duration::from_secs(10));
        assert_eq!(network.pending_rpc_count(), 0);
    }

    #[test]
    fn test_message_remote_call_serialize() {
        let msg = Message::RemoteCall {
            request_id: "rpc-123".to_string(),
            from: "node-abc".to_string(),
            method: "get_status".to_string(),
            payload: b"{}".to_vec(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("RemoteCall"));
        assert!(json.contains("rpc-123"));
        assert!(json.contains("get_status"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::RemoteCall { request_id, from, method, payload } = parsed {
            assert_eq!(request_id, "rpc-123");
            assert_eq!(from, "node-abc");
            assert_eq!(method, "get_status");
            assert_eq!(payload, b"{}".to_vec());
        } else {
            panic!("Expected RemoteCall message");
        }
    }

    #[test]
    fn test_message_remote_call_response_serialize() {
        let msg = Message::RemoteCallResponse {
            request_id: "rpc-456".to_string(),
            success: true,
            data: b"result data".to_vec(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("RemoteCallResponse"));
        assert!(json.contains("rpc-456"));
        assert!(json.contains("true"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::RemoteCallResponse { request_id, success, data } = parsed {
            assert_eq!(request_id, "rpc-456");
            assert!(success);
            assert_eq!(data, b"result data".to_vec());
        } else {
            panic!("Expected RemoteCallResponse message");
        }
    }

    #[test]
    fn test_message_remote_call_response_failure() {
        let msg = Message::RemoteCallResponse {
            request_id: "rpc-789".to_string(),
            success: false,
            data: b"method not found".to_vec(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();

        if let Message::RemoteCallResponse { success, data, .. } = parsed {
            assert!(!success);
            assert_eq!(String::from_utf8_lossy(&data), "method not found");
        } else {
            panic!("Expected RemoteCallResponse message");
        }
    }

    #[test]
    fn test_send_remote_call_without_coap() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
            let result = network.send_remote_call(
                &addr,
                "test_method",
                vec![],
                Duration::from_secs(1),
            ).await;

            // Without CoAP feature, should return error
            assert!(result.is_err());
        });
    }

    // ==================== Mesh Networking Tests ====================

    #[test]
    fn test_mesh_manager_new() {
        let mesh = MeshManager::new();
        assert_eq!(mesh.stats.messages_relayed, 0);
        assert_eq!(mesh.stats.duplicates_ignored, 0);
    }

    #[test]
    fn test_mesh_manager_seen_tracking() {
        let mut mesh = MeshManager::new();

        assert!(!mesh.is_seen("msg-1"));
        mesh.mark_seen("msg-1".to_string());
        assert!(mesh.is_seen("msg-1"));
        assert!(!mesh.is_seen("msg-2"));
    }

    #[test]
    fn test_mesh_manager_process_relay_new() {
        let mut mesh = MeshManager::new();

        let (should_process, should_relay, new_ttl) =
            mesh.process_relay("msg-1", 5);

        assert!(should_process);
        assert!(should_relay);
        assert_eq!(new_ttl, 4);
    }

    #[test]
    fn test_mesh_manager_process_relay_duplicate() {
        let mut mesh = MeshManager::new();

        // First time - process and relay
        let (should_process, _, _) = mesh.process_relay("msg-1", 5);
        assert!(should_process);

        // Second time - duplicate, skip
        let (should_process, should_relay, _) = mesh.process_relay("msg-1", 5);
        assert!(!should_process);
        assert!(!should_relay);
        assert_eq!(mesh.stats.duplicates_ignored, 1);
    }

    #[test]
    fn test_mesh_manager_process_relay_ttl_zero() {
        let mut mesh = MeshManager::new();

        let (should_process, should_relay, _) = mesh.process_relay("msg-1", 0);
        assert!(!should_process);
        assert!(!should_relay);
        assert_eq!(mesh.stats.messages_dropped_ttl, 1);
    }

    #[test]
    fn test_mesh_manager_process_relay_ttl_one() {
        let mut mesh = MeshManager::new();

        let (should_process, should_relay, new_ttl) =
            mesh.process_relay("msg-1", 1);

        assert!(should_process);
        assert!(!should_relay); // TTL=0 after decrement, no relay
        assert_eq!(new_ttl, 0);
    }

    #[test]
    fn test_mesh_manager_generate_message_id() {
        let id1 = MeshManager::generate_message_id("node-1");
        let id2 = MeshManager::generate_message_id("node-1");

        assert!(id1.starts_with("node-1:"));
        assert!(id2.starts_with("node-1:"));
        assert_ne!(id1, id2); // Should be unique
    }

    #[test]
    fn test_mesh_manager_wrap_for_relay() {
        let inner = Message::Ping {
            node_id: "source".to_string(),
        };

        let wrapped = MeshManager::wrap_for_relay("relay-node", inner, Some(3));

        if let Message::MeshRelay {
            message_id,
            origin,
            ttl,
            inner,
        } = wrapped
        {
            assert!(message_id.starts_with("relay-node:"));
            assert_eq!(origin, "relay-node");
            assert_eq!(ttl, 3);
            assert!(matches!(*inner, Message::Ping { .. }));
        } else {
            panic!("Expected MeshRelay message");
        }
    }

    #[test]
    fn test_mesh_stats_default() {
        let stats = MeshStats::default();
        assert_eq!(stats.messages_relayed, 0);
        assert_eq!(stats.messages_dropped_ttl, 0);
        assert_eq!(stats.duplicates_ignored, 0);
        assert_eq!(stats.messages_originated, 0);
    }

    #[test]
    fn test_network_mesh_manager_access() {
        let config = TransportConfig::Memory;
        let gossip = GossipConfig::default();
        let network = Network::new(config, gossip, "test-node".to_string());

        let mesh = network.mesh_manager();
        assert_eq!(mesh.stats.messages_relayed, 0);
    }

    #[test]
    fn test_message_mesh_relay_serialize() {
        let inner = Message::NewRecord {
            hash: Hash([0u8; 32]),
        };

        let msg = Message::MeshRelay {
            message_id: "node-1:123".to_string(),
            origin: "node-1".to_string(),
            ttl: 4,
            inner: Box::new(inner),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("MeshRelay"));
        assert!(json.contains("node-1:123"));
        assert!(json.contains("ttl"));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        if let Message::MeshRelay {
            message_id,
            origin,
            ttl,
            inner,
        } = parsed
        {
            assert_eq!(message_id, "node-1:123");
            assert_eq!(origin, "node-1");
            assert_eq!(ttl, 4);
            assert!(matches!(*inner, Message::NewRecord { .. }));
        } else {
            panic!("Expected MeshRelay message");
        }
    }
}
