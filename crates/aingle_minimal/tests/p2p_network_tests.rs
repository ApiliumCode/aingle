//! P2P Network Integration Tests
//!
//! Tests for peer-to-peer networking including:
//! - Node creation and configuration
//! - Peer discovery and management
//! - Gossip protocol
//! - Sync operations

use aingle_minimal::{
    config::{GossipConfig, PowerMode, StorageConfig, TransportConfig},
    BloomFilter, Config, DiscoveredPeer, Discovery, GossipManager, Hash, MinimalNode, SyncManager,
};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Helper to create a test configuration (memory storage, memory transport)
fn test_config() -> Config {
    Config {
        node_id: None,
        publish_interval: Duration::ZERO,
        power_mode: PowerMode::Full,
        transport: TransportConfig::Memory,
        gossip: GossipConfig::default(),
        storage: StorageConfig::memory(),
        memory_limit: 256 * 1024,
        enable_metrics: false,
        enable_mdns: false,
        log_level: "debug".to_string(),
    }
}

/// Test that two nodes can be created with different configurations
#[test]
fn test_create_multiple_nodes() {
    let config1 = test_config();
    let config2 = test_config();

    let node1 = MinimalNode::new(config1).unwrap();
    let node2 = MinimalNode::new(config2).unwrap();

    // Nodes should have different public keys
    assert_ne!(node1.public_key(), node2.public_key());
}

/// Test peer management in the network layer
#[test]
fn test_peer_management_workflow() {
    let config = test_config();
    let mut node = MinimalNode::new(config).unwrap();

    // Initially no peers
    let stats = node.stats().unwrap();
    assert_eq!(stats.peer_count, 0);

    // Add peers
    let peer1: SocketAddr = "192.168.1.100:5683".parse().unwrap();
    let peer2: SocketAddr = "192.168.1.101:5683".parse().unwrap();

    node.add_peer(peer1);
    node.add_peer(peer2);

    let stats = node.stats().unwrap();
    assert_eq!(stats.peer_count, 2);
}

/// Test bloom filter for set reconciliation
#[test]
fn test_bloom_filter_reconciliation() {
    let mut filter_a = BloomFilter::new();
    let mut filter_b = BloomFilter::new();

    // Node A has hashes 1, 2, 3
    let hash1 = Hash::from_bytes(&[1; 32]);
    let hash2 = Hash::from_bytes(&[2; 32]);
    let hash3 = Hash::from_bytes(&[3; 32]);

    filter_a.insert(&hash1);
    filter_a.insert(&hash2);
    filter_a.insert(&hash3);

    // Node B has hashes 1, 2
    filter_b.insert(&hash1);
    filter_b.insert(&hash2);

    // Node B can identify hash3 is potentially missing (not in their filter)
    // (Check from A's perspective - what does B not have?)
    assert!(!filter_b.may_contain(&hash3));
    assert!(filter_b.may_contain(&hash1));
    assert!(filter_b.may_contain(&hash2));
}

/// Test gossip manager announcement queue
#[test]
fn test_gossip_announcement_queue() {
    let config = aingle_minimal::config::GossipConfig::default();
    let mut gossip = GossipManager::new(config);

    // Queue some announcements
    let hash1 = Hash::from_bytes(&[1; 32]);
    let hash2 = Hash::from_bytes(&[2; 32]);

    gossip.announce(hash1.clone());
    gossip.announce(hash2.clone());

    // Both should be known
    assert!(gossip.is_known(&hash1));
    assert!(gossip.is_known(&hash2));

    // Unknown hash should not be known
    let unknown = Hash::from_bytes(&[99; 32]);
    assert!(!gossip.is_known(&unknown));
}

/// Test sync manager peer state tracking
#[test]
fn test_sync_manager_peer_tracking() {
    let mut sync = SyncManager::new(Duration::from_secs(60));

    let peer1: SocketAddr = "192.168.1.100:5683".parse().unwrap();
    let peer2: SocketAddr = "192.168.1.101:5683".parse().unwrap();

    // Track interactions with peers
    sync.get_peer_state(&peer1).record_success();
    sync.get_peer_state(&peer1).record_success();
    sync.get_peer_state(&peer2).record_failure();

    let stats = sync.stats();
    assert_eq!(stats.peer_count, 2);
    assert_eq!(stats.total_successful_syncs, 2);
    assert_eq!(stats.total_failed_syncs, 1);
}

/// Test sync manager local hash tracking
#[test]
fn test_sync_local_hash_tracking() {
    let mut sync = SyncManager::new(Duration::from_secs(60));

    // Add some local hashes
    for i in 0..100 {
        let hash = Hash::from_bytes(&[i; 32]);
        sync.add_local_hash(hash);
    }

    let stats = sync.stats();
    assert_eq!(stats.local_hashes, 100);

    // Build filter should contain all hashes
    let filter = sync.build_local_filter();
    let test_hash = Hash::from_bytes(&[50; 32]);
    assert!(filter.may_contain(&test_hash));
}

/// Test discovered peer data structure
#[test]
fn test_discovered_peer_properties() {
    use std::collections::HashMap;
    use std::net::IpAddr;

    let peer = DiscoveredPeer {
        node_id: "abc123".to_string(),
        addresses: vec![
            "192.168.1.100".parse::<IpAddr>().unwrap(),
            "10.0.0.100".parse::<IpAddr>().unwrap(),
        ],
        port: 5683,
        discovered_at: Instant::now(),
        last_seen: Instant::now(),
        properties: {
            let mut props = HashMap::new();
            props.insert("version".to_string(), "0.1.0".to_string());
            props
        },
    };

    // Should generate socket addresses for both IPs
    let addrs = peer.socket_addrs();
    assert_eq!(addrs.len(), 2);
    assert_eq!(addrs[0].port(), 5683);
    assert_eq!(addrs[1].port(), 5683);

    // Should be alive (just created)
    assert!(peer.is_alive(Duration::from_secs(60)));
}

/// Test node statistics after operations
#[test]
fn test_node_stats_after_operations() {
    let config = test_config();
    let mut node = MinimalNode::new(config).unwrap();

    // Initial stats
    let stats = node.stats().unwrap();
    assert_eq!(stats.entries_count, 0);
    assert_eq!(stats.actions_count, 0);

    // Create an entry
    let data = serde_json::json!({
        "sensor": "temperature",
        "value": 23.5
    });
    node.create_entry(data).unwrap();

    // Stats should update
    let stats = node.stats().unwrap();
    assert!(stats.entries_count > 0 || stats.actions_count > 0);
}

/// Test gossip statistics tracking
#[test]
fn test_gossip_stats() {
    let config = test_config();
    let node = MinimalNode::new(config).unwrap();

    let gossip_stats = node.gossip_stats();
    assert_eq!(gossip_stats.round, 0);
    assert_eq!(gossip_stats.pending_announcements, 0);
}

/// Test sync statistics tracking
#[test]
fn test_sync_stats() {
    let config = test_config();
    let node = MinimalNode::new(config).unwrap();

    let sync_stats = node.sync_stats();
    assert_eq!(sync_stats.peer_count, 0);
    assert_eq!(sync_stats.local_hashes, 0);
}

/// Test discovery stub behavior (without mDNS feature active in tests)
#[test]
#[cfg(not(feature = "mdns"))]
fn test_discovery_stub() {
    let mut discovery = Discovery::new("test-node".to_string(), 5683).unwrap();

    // Should work but do nothing without mdns feature
    assert!(discovery.register().is_ok());
    assert!(discovery.start_browsing().is_ok());
    assert_eq!(discovery.get_peers().len(), 0);
    assert_eq!(discovery.peer_count(), 0);
    assert!(discovery.stop().is_ok());
}

/// Test IoT mode configuration for P2P
#[test]
fn test_iot_mode_p2p_config() {
    let config = Config::iot_mode();

    // IoT mode should enable mDNS for auto-discovery
    assert!(config.enable_mdns);

    // Should use CoAP transport
    match config.transport {
        aingle_minimal::config::TransportConfig::Coap { port, .. } => {
            assert_eq!(port, 5683);
        }
        _ => panic!("Expected CoAP transport for IoT mode"),
    }

    // Should have aggressive gossip
    assert!(config.gossip.loop_delay.as_millis() < 1000);
}

/// Test low power mode P2P configuration
#[test]
fn test_low_power_p2p_config() {
    let config = Config::low_power();

    // Low power should disable mDNS to save power
    assert!(!config.enable_mdns);

    // Should have slower gossip to save power
    assert!(config.gossip.loop_delay.as_secs() >= 5);
}

/// Test that sync manager handles missing peers gracefully
#[test]
fn test_sync_manager_graceful_handling() {
    let mut sync = SyncManager::new(Duration::from_secs(1));

    let peer: SocketAddr = "192.168.1.100:5683".parse().unwrap();

    // First access creates state
    {
        let state = sync.get_peer_state(&peer);
        assert_eq!(state.successful_syncs, 0);
    }

    // Simulate failures
    for _ in 0..5 {
        sync.get_peer_state(&peer).record_failure();
    }

    let stats = sync.stats();
    assert_eq!(stats.total_failed_syncs, 5);

    // Recovery
    sync.get_peer_state(&peer).record_success();

    let state = sync.get_peer_state(&peer);
    assert_eq!(state.successful_syncs, 1);
    assert_eq!(state.failed_syncs, 0); // Reset on success
}

/// Test that nodes can be configured for production use
#[test]
fn test_production_p2p_config() {
    let config = Config::production("/tmp/aingle_test_db");

    // Production should enable mDNS
    assert!(config.enable_mdns);

    // Should use QUIC transport
    match config.transport {
        aingle_minimal::config::TransportConfig::Quic { port, .. } => {
            assert_eq!(port, 8443);
        }
        _ => panic!("Expected QUIC transport for production mode"),
    }
}
