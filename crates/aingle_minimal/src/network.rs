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
use crate::error::Result;
use crate::types::{Hash, Record};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

#[cfg(feature = "coap")]
use crate::coap::CoapServer;

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
    /// mDNS discovery service
    discovery: Option<Discovery>,
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
            discovery: None,
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
            TransportConfig::Quic { bind_addr, port } => {
                log::info!("Starting QUIC transport on {}:{}", bind_addr, port);
                // QUIC implementation would go here
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
            bind_addr: "0.0.0.0".to_string(),
            port: 5684,
        };
        let gossip = GossipConfig::default();
        let mut network = Network::new(config, gossip, "test-node".to_string());

        smol::block_on(async {
            let result = network.start().await;
            assert!(result.is_ok());
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
}
