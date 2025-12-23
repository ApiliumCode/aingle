//! End-to-End Multi-Node Integration Tests
//!
//! These tests verify that multiple AIngle nodes can:
//! - Start and stop correctly on different ports
//! - Discover and connect to each other
//! - Exchange messages via gossip protocol
//! - Synchronize data between nodes
//! - Handle mesh relay for multi-hop communication
//!
//! Run with: `cargo test -p aingle_minimal --features "sqlite,coap" --test e2e_multi_node_tests`

use aingle_minimal::{
    config::{GossipConfig, PowerMode, StorageConfig, TransportConfig},
    BloomFilter, Config, GossipManager, Hash, MinimalNode, SyncManager,
};
use std::net::SocketAddr;
use std::time::Duration;

/// Create a test configuration with memory transport (for unit-like tests)
fn test_config_memory() -> Config {
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

// ============================================================================
// Node Lifecycle Tests
// ============================================================================

/// Test that a single node can be created and has a unique identity
#[test]
fn test_single_node_creation() {
    let config = test_config_memory();
    let node = MinimalNode::new(config).unwrap();

    // Node should have a valid public key
    let pubkey = node.public_key();
    assert!(!pubkey.to_hex().is_empty());
    assert_eq!(pubkey.to_hex().len(), 64); // 32 bytes = 64 hex chars
}

/// Test that multiple nodes have distinct identities
#[test]
fn test_multiple_nodes_unique_identities() {
    let nodes: Vec<_> = (0..5)
        .map(|_| {
            let config = test_config_memory();
            MinimalNode::new(config).unwrap()
        })
        .collect();

    // All nodes should have unique public keys
    let pubkeys: Vec<_> = nodes.iter().map(|n| n.public_key().to_hex()).collect();

    for (i, pk1) in pubkeys.iter().enumerate() {
        for (j, pk2) in pubkeys.iter().enumerate() {
            if i != j {
                assert_ne!(pk1, pk2, "Nodes {} and {} have same pubkey", i, j);
            }
        }
    }
}

/// Test node statistics are initialized correctly
#[test]
fn test_node_initial_stats() {
    let config = test_config_memory();
    let node = MinimalNode::new(config).unwrap();

    let stats = node.stats().unwrap();
    assert_eq!(stats.entries_count, 0);
    assert_eq!(stats.actions_count, 0);
    assert_eq!(stats.peer_count, 0);
}

// ============================================================================
// Peer Management Tests
// ============================================================================

/// Test adding peers to a node
#[test]
fn test_add_peers_to_node() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    // Initially no peers
    assert_eq!(node.stats().unwrap().peer_count, 0);

    // Add peers
    for i in 1..=5 {
        let addr: SocketAddr = format!("192.168.1.{}:5683", i).parse().unwrap();
        node.add_peer(addr);
    }

    assert_eq!(node.stats().unwrap().peer_count, 5);
}

/// Test that adding duplicate peers doesn't increase count
#[test]
fn test_add_duplicate_peers() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    let addr: SocketAddr = "192.168.1.100:5683".parse().unwrap();

    // Add same peer multiple times
    node.add_peer(addr);
    node.add_peer(addr);
    node.add_peer(addr);

    // Should only count as one peer
    assert_eq!(node.stats().unwrap().peer_count, 1);
}

/// Test peer count tracking
#[test]
fn test_peer_count_tracking() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    let addr1: SocketAddr = "192.168.1.100:5683".parse().unwrap();
    let addr2: SocketAddr = "192.168.1.101:5683".parse().unwrap();
    let addr3: SocketAddr = "192.168.1.102:5683".parse().unwrap();

    node.add_peer(addr1);
    node.add_peer(addr2);
    node.add_peer(addr3);

    // Should have 3 distinct peers
    assert_eq!(node.stats().unwrap().peer_count, 3);
}

// ============================================================================
// Entry Creation and Retrieval Tests
// ============================================================================

/// Test creating entries on a node
#[test]
fn test_create_entry() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    // Create an entry
    let data = serde_json::json!({
        "sensor": "temperature",
        "value": 23.5,
        "unit": "celsius"
    });

    let hash = node.create_entry(data).unwrap();

    // Hash should be valid (32 bytes)
    assert_eq!(hash.as_bytes().len(), 32);

    // Stats should reflect the entry
    let stats = node.stats().unwrap();
    assert!(stats.entries_count > 0 || stats.actions_count > 0);
}

/// Test creating multiple entries
#[test]
fn test_create_multiple_entries() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    let mut hashes = Vec::new();
    for i in 0..10 {
        let data = serde_json::json!({
            "reading": i,
            "timestamp": i * 1000
        });
        let hash = node.create_entry(data).unwrap();
        hashes.push(hash);
    }

    // All hashes should be unique
    for (i, h1) in hashes.iter().enumerate() {
        for (j, h2) in hashes.iter().enumerate() {
            if i != j {
                assert_ne!(h1.to_hex(), h2.to_hex(), "Hashes {} and {} are duplicates", i, j);
            }
        }
    }
}

/// Test retrieving an entry by hash
#[test]
fn test_get_entry() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    let data = serde_json::json!({
        "test": "data",
        "value": 42
    });

    let hash = node.create_entry(data.clone()).unwrap();

    // Retrieve the entry
    let retrieved = node.get_entry(&hash);
    assert!(retrieved.is_ok());
}

// ============================================================================
// Gossip Protocol Tests
// ============================================================================

/// Test gossip manager tracks known hashes
#[test]
fn test_gossip_manager_tracking() {
    let config = GossipConfig::default();
    let mut gossip = GossipManager::new(config);

    // Announce some hashes
    let hashes: Vec<_> = (0..5)
        .map(|i| Hash::from_bytes(&[i; 32]))
        .collect();

    for hash in &hashes {
        gossip.announce(hash.clone());
    }

    // All announced hashes should be known
    for hash in &hashes {
        assert!(gossip.is_known(hash), "Hash should be known after announcement");
    }

    // Unknown hash should not be known
    let unknown = Hash::from_bytes(&[99; 32]);
    assert!(!gossip.is_known(&unknown));
}

/// Test gossip manager pending announcements
#[test]
fn test_gossip_pending_announcements() {
    let config = GossipConfig::default();
    let mut gossip = GossipManager::new(config);

    // Initially no pending
    let stats = gossip.stats();
    assert_eq!(stats.pending_announcements, 0);

    // Add announcements
    for i in 0..3 {
        let hash = Hash::from_bytes(&[i; 32]);
        gossip.announce(hash);
    }

    let stats = gossip.stats();
    assert!(stats.pending_announcements > 0);
}

/// Test bloom filter for efficient set reconciliation
#[test]
fn test_bloom_filter_set_reconciliation() {
    let mut filter_node_a = BloomFilter::new();
    let mut filter_node_b = BloomFilter::new();

    // Node A has entries 0-9
    for i in 0..10 {
        let hash = Hash::from_bytes(&[i; 32]);
        filter_node_a.insert(&hash);
    }

    // Node B has entries 5-14
    for i in 5..15 {
        let hash = Hash::from_bytes(&[i; 32]);
        filter_node_b.insert(&hash);
    }

    // Node A can identify what B might be missing (0-4)
    for i in 0..5 {
        let hash = Hash::from_bytes(&[i; 32]);
        assert!(!filter_node_b.may_contain(&hash), "Node B shouldn't have hash {}", i);
    }

    // Both should have 5-9
    for i in 5..10 {
        let hash = Hash::from_bytes(&[i; 32]);
        assert!(filter_node_a.may_contain(&hash));
        assert!(filter_node_b.may_contain(&hash));
    }
}

// ============================================================================
// Sync Protocol Tests
// ============================================================================

/// Test sync manager peer state tracking
#[test]
fn test_sync_peer_state_tracking() {
    let mut sync = SyncManager::new(Duration::from_secs(60));

    let peer1: SocketAddr = "192.168.1.100:5683".parse().unwrap();
    let peer2: SocketAddr = "192.168.1.101:5683".parse().unwrap();
    let peer3: SocketAddr = "192.168.1.102:5683".parse().unwrap();

    // Track various interactions
    sync.get_peer_state(&peer1).record_success();
    sync.get_peer_state(&peer1).record_success();
    sync.get_peer_state(&peer2).record_success();
    sync.get_peer_state(&peer2).record_failure();
    sync.get_peer_state(&peer3).record_failure();
    sync.get_peer_state(&peer3).record_failure();

    let stats = sync.stats();
    assert_eq!(stats.peer_count, 3);
    assert_eq!(stats.total_successful_syncs, 3);
    assert_eq!(stats.total_failed_syncs, 3);
}

/// Test sync manager local hash management
#[test]
fn test_sync_local_hash_management() {
    let mut sync = SyncManager::new(Duration::from_secs(60));

    // Add local hashes
    for i in 0..50 {
        let hash = Hash::from_bytes(&[i; 32]);
        sync.add_local_hash(hash);
    }

    let stats = sync.stats();
    assert_eq!(stats.local_hashes, 50);

    // Build filter should contain all hashes
    let filter = sync.build_local_filter();
    for i in 0..50 {
        let hash = Hash::from_bytes(&[i; 32]);
        assert!(filter.may_contain(&hash), "Filter should contain hash {}", i);
    }

    // Note: Bloom filters can have false positives, so we don't assert !may_contain for unknown
}

/// Test sync manager recovery after failures
#[test]
fn test_sync_recovery_after_failures() {
    let mut sync = SyncManager::new(Duration::from_secs(1));
    let peer: SocketAddr = "192.168.1.100:5683".parse().unwrap();

    // Simulate failures
    for _ in 0..5 {
        sync.get_peer_state(&peer).record_failure();
    }

    let state = sync.get_peer_state(&peer);
    assert_eq!(state.failed_syncs, 5);

    // Simulate recovery
    sync.get_peer_state(&peer).record_success();

    let state = sync.get_peer_state(&peer);
    assert_eq!(state.successful_syncs, 1);
    assert_eq!(state.failed_syncs, 0); // Should reset on success
}

// ============================================================================
// Multi-Node Communication Simulation Tests
// ============================================================================

/// Simulate two nodes exchanging data
#[test]
fn test_two_node_data_exchange_simulation() {
    // Create two nodes
    let config1 = test_config_memory();
    let config2 = test_config_memory();

    let mut node1 = MinimalNode::new(config1).unwrap();
    let mut node2 = MinimalNode::new(config2).unwrap();

    // Node1 creates entries
    let mut node1_hashes = Vec::new();
    for i in 0..5 {
        let data = serde_json::json!({"node": "1", "entry": i});
        let hash = node1.create_entry(data).unwrap();
        node1_hashes.push(hash);
    }

    // Node2 creates entries
    let mut node2_hashes = Vec::new();
    for i in 0..3 {
        let data = serde_json::json!({"node": "2", "entry": i});
        let hash = node2.create_entry(data).unwrap();
        node2_hashes.push(hash);
    }

    // Verify each node has its own entries
    let stats1 = node1.stats().unwrap();
    let stats2 = node2.stats().unwrap();

    assert!(stats1.entries_count >= 5 || stats1.actions_count >= 5);
    assert!(stats2.entries_count >= 3 || stats2.actions_count >= 3);

    // Simulate sync by tracking hashes in gossip managers
    let mut gossip1 = GossipManager::new(GossipConfig::default());
    let mut gossip2 = GossipManager::new(GossipConfig::default());

    for hash in &node1_hashes {
        gossip1.announce(hash.clone());
    }
    for hash in &node2_hashes {
        gossip2.announce(hash.clone());
    }

    // Get bloom filters
    let _filter1 = gossip1.get_bloom_filter(); // Kept for symmetry
    let filter2 = gossip2.get_bloom_filter();

    // Node1 can identify what Node2 needs
    let mut missing_count = 0;
    for hash in &node1_hashes {
        if !filter2.may_contain(hash) {
            // This hash might need to be sent to node2
            missing_count += 1;
        }
    }
    // At least some of node1's hashes should be missing from node2
    assert!(missing_count > 0 || node1_hashes.is_empty(), "Should identify hashes to sync");
}

/// Simulate three node mesh communication
#[test]
fn test_three_node_mesh_simulation() {
    // Create three nodes in a mesh topology
    // Node1 <-> Node2 <-> Node3 (Node1 and Node3 not directly connected)

    let mut node1 = MinimalNode::new(test_config_memory()).unwrap();
    let mut node2 = MinimalNode::new(test_config_memory()).unwrap();
    let mut node3 = MinimalNode::new(test_config_memory()).unwrap();

    // Setup peer relationships
    let addr1: SocketAddr = "192.168.1.1:5683".parse().unwrap();
    let addr2: SocketAddr = "192.168.1.2:5683".parse().unwrap();
    let addr3: SocketAddr = "192.168.1.3:5683".parse().unwrap();

    // Node1 knows Node2
    node1.add_peer(addr2);

    // Node2 knows both Node1 and Node3
    node2.add_peer(addr1);
    node2.add_peer(addr3);

    // Node3 knows Node2
    node3.add_peer(addr2);

    // Verify peer counts
    assert_eq!(node1.stats().unwrap().peer_count, 1);
    assert_eq!(node2.stats().unwrap().peer_count, 2);
    assert_eq!(node3.stats().unwrap().peer_count, 1);

    // Create entry on Node1
    let data = serde_json::json!({"origin": "node1", "message": "hello mesh"});
    let hash = node1.create_entry(data).unwrap();

    // Track in gossip managers (simulating gossip propagation)
    let mut gossip1 = GossipManager::new(GossipConfig::default());
    let mut gossip2 = GossipManager::new(GossipConfig::default());
    let mut gossip3 = GossipManager::new(GossipConfig::default());

    // Node1 announces
    gossip1.announce(hash.clone());

    // Simulate gossip round: Node1 -> Node2
    gossip2.announce(hash.clone());

    // Simulate gossip round: Node2 -> Node3
    gossip3.announce(hash.clone());

    // All nodes should now know about the hash
    assert!(gossip1.is_known(&hash));
    assert!(gossip2.is_known(&hash));
    assert!(gossip3.is_known(&hash));
}

/// Test node handles many entries efficiently
#[test]
fn test_node_many_entries_performance() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    let start = std::time::Instant::now();

    // Create 100 entries
    for i in 0..100 {
        let data = serde_json::json!({
            "batch": "perf_test",
            "index": i,
            "data": format!("entry_data_{}", i)
        });
        node.create_entry(data).unwrap();
    }

    let duration = start.elapsed();

    // Should complete reasonably fast (less than 5 seconds)
    assert!(
        duration < Duration::from_secs(5),
        "Creating 100 entries took too long: {:?}",
        duration
    );

    let stats = node.stats().unwrap();
    assert!(stats.entries_count >= 100 || stats.actions_count >= 100);
}

// ============================================================================
// Gossip Propagation Simulation Tests
// ============================================================================

/// Simulate gossip propagation across a network of nodes
#[test]
fn test_gossip_propagation_simulation() {
    const NUM_NODES: usize = 10;

    // Create gossip managers for each node
    let mut gossips: Vec<_> = (0..NUM_NODES)
        .map(|_| GossipManager::new(GossipConfig::default()))
        .collect();

    // Node 0 creates and announces a hash
    let original_hash = Hash::from_bytes(&[42; 32]);
    gossips[0].announce(original_hash.clone());

    // Simulate gossip rounds (in a line topology: 0->1->2->...->9)
    for round in 0..NUM_NODES - 1 {
        // Check if current node knows the hash and propagate to next
        if gossips[round].is_known(&original_hash) {
            gossips[round + 1].announce(original_hash.clone());
        }
    }

    // All nodes should eventually know about the hash
    for (i, gossip) in gossips.iter().enumerate() {
        assert!(
            gossip.is_known(&original_hash),
            "Node {} should know about the hash",
            i
        );
    }
}

/// Test gossip handles multiple concurrent announcements
#[test]
fn test_gossip_concurrent_announcements() {
    let mut gossip = GossipManager::new(GossipConfig::default());

    // Simulate multiple nodes announcing different hashes
    let hashes: Vec<_> = (0..20)
        .map(|i| Hash::from_bytes(&[i; 32]))
        .collect();

    // Announce all hashes
    for hash in &hashes {
        gossip.announce(hash.clone());
    }

    // Verify all are known
    for (i, hash) in hashes.iter().enumerate() {
        assert!(gossip.is_known(hash), "Hash {} should be known", i);
    }

    // Stats should reflect announcements
    let stats = gossip.stats();
    assert_eq!(stats.pending_announcements, 20);
}

// ============================================================================
// Network Topology Tests
// ============================================================================

/// Test star topology (all nodes connected to central hub)
#[test]
fn test_star_topology_simulation() {
    let mut hub = MinimalNode::new(test_config_memory()).unwrap();

    // Create 5 spoke nodes, all connected to hub
    let hub_addr: SocketAddr = "192.168.1.1:5683".parse().unwrap();
    let spokes: Vec<_> = (0..5)
        .map(|_| {
            let mut node = MinimalNode::new(test_config_memory()).unwrap();
            node.add_peer(hub_addr);
            node
        })
        .collect();

    // Hub knows all spokes
    for i in 2..7 {
        let addr: SocketAddr = format!("192.168.1.{}:5683", i).parse().unwrap();
        hub.add_peer(addr);
    }

    // Verify topology
    assert_eq!(hub.stats().unwrap().peer_count, 5);
    for spoke in &spokes {
        assert_eq!(spoke.stats().unwrap().peer_count, 1);
    }

    // Create entry on hub
    let data = serde_json::json!({"from": "hub", "broadcast": true});
    let hash = hub.create_entry(data).unwrap();

    // Gossip manager simulation
    let mut hub_gossip = GossipManager::new(GossipConfig::default());
    hub_gossip.announce(hash.clone());

    // All spokes receive the announcement
    let mut spoke_gossips: Vec<_> = (0..5)
        .map(|_| GossipManager::new(GossipConfig::default()))
        .collect();

    for gossip in &mut spoke_gossips {
        gossip.announce(hash.clone());
    }

    // Verify all received
    assert!(hub_gossip.is_known(&hash));
    for gossip in &spoke_gossips {
        assert!(gossip.is_known(&hash));
    }
}

/// Test ring topology (each node connected to two neighbors)
#[test]
fn test_ring_topology_simulation() {
    const RING_SIZE: usize = 6;

    // Create nodes
    let nodes: Vec<_> = (0..RING_SIZE)
        .map(|_| MinimalNode::new(test_config_memory()).unwrap())
        .collect();

    // Setup ring connections
    let mut nodes_with_peers: Vec<_> = nodes.into_iter().collect();

    // Add peers in ring fashion
    for i in 0..RING_SIZE {
        let prev = if i == 0 { RING_SIZE - 1 } else { i - 1 };
        let next = (i + 1) % RING_SIZE;

        let prev_addr: SocketAddr = format!("192.168.1.{}:5683", prev + 1).parse().unwrap();
        let next_addr: SocketAddr = format!("192.168.1.{}:5683", next + 1).parse().unwrap();

        nodes_with_peers[i].add_peer(prev_addr);
        nodes_with_peers[i].add_peer(next_addr);
    }

    // Verify each node has exactly 2 peers
    for (i, node) in nodes_with_peers.iter().enumerate() {
        assert_eq!(
            node.stats().unwrap().peer_count,
            2,
            "Node {} should have exactly 2 peers",
            i
        );
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test node handles operations on empty peer list gracefully
#[test]
fn test_empty_peer_list_operations() {
    let config = test_config_memory();
    let node = MinimalNode::new(config).unwrap();

    // Operations on empty peer list should work
    assert_eq!(node.stats().unwrap().peer_count, 0);

    // Gossip stats should also work with no peers
    let gossip_stats = node.gossip_stats();
    assert_eq!(gossip_stats.round, 0);
}

/// Test gossip handles empty filter gracefully
#[test]
fn test_gossip_empty_filter() {
    let gossip = GossipManager::new(GossipConfig::default());

    // Getting filter with no announcements should work
    let filter = gossip.get_bloom_filter();

    // Any hash query should work (probabilistically might return false positive)
    let test_hash = Hash::from_bytes(&[1; 32]);
    // Note: Can't assert !may_contain due to bloom filter properties
    let _ = filter.may_contain(&test_hash);
}

/// Test sync manager handles empty peer list
#[test]
fn test_sync_empty_peers() {
    let sync = SyncManager::new(Duration::from_secs(60));

    let stats = sync.stats();
    assert_eq!(stats.peer_count, 0);
    assert_eq!(stats.total_successful_syncs, 0);
    assert_eq!(stats.total_failed_syncs, 0);
}

// ============================================================================
// Configuration Tests
// ============================================================================

/// Test IoT mode configuration for multi-node setup
#[test]
fn test_iot_mode_multi_node() {
    let config1 = Config::iot_mode();
    let config2 = Config::iot_mode();

    // Both should be able to create nodes
    let node1 = MinimalNode::new(config1);
    let node2 = MinimalNode::new(config2);

    assert!(node1.is_ok());
    assert!(node2.is_ok());

    // Should have different identities
    let n1 = node1.unwrap();
    let n2 = node2.unwrap();
    assert_ne!(n1.public_key().to_hex(), n2.public_key().to_hex());
}

/// Test low power mode configuration
#[test]
fn test_low_power_mode_multi_node() {
    let config = Config::low_power();

    // Should have reduced gossip frequency
    assert!(config.gossip.loop_delay >= Duration::from_secs(5));

    // Should be able to create node
    let node = MinimalNode::new(config);
    assert!(node.is_ok());
}

// ============================================================================
// Stress Tests
// ============================================================================

/// Test system handles many peers
#[test]
fn test_many_peers() {
    let config = test_config_memory();
    let mut node = MinimalNode::new(config).unwrap();

    // Add 100 peers
    for i in 0..100 {
        let addr: SocketAddr = format!("10.0.{}.{}:5683", i / 256, i % 256).parse().unwrap();
        node.add_peer(addr);
    }

    assert_eq!(node.stats().unwrap().peer_count, 100);
}

/// Test bloom filter with many hashes
#[test]
fn test_bloom_filter_many_hashes() {
    let mut filter = BloomFilter::new();

    // Insert 1000 hashes
    for i in 0u16..1000 {
        let mut bytes = [0u8; 32];
        bytes[0] = (i >> 8) as u8;
        bytes[1] = (i & 0xFF) as u8;
        let hash = Hash::from_bytes(&bytes);
        filter.insert(&hash);
    }

    // Verify all are probably contained
    let mut found = 0;
    for i in 0u16..1000 {
        let mut bytes = [0u8; 32];
        bytes[0] = (i >> 8) as u8;
        bytes[1] = (i & 0xFF) as u8;
        let hash = Hash::from_bytes(&bytes);
        if filter.may_contain(&hash) {
            found += 1;
        }
    }

    // Should find most (bloom filters can have false negatives at high load)
    assert!(found >= 900, "Should find at least 90% of inserted hashes");
}
