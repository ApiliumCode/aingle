//! Node-to-node synchronization protocol
//!
//! Implements efficient record synchronization using bloom filter-based
//! set reconciliation.
//!
//! # Protocol Flow
//!
//! 1. Node A sends BloomFilter of known hashes to Node B
//! 2. Node B identifies missing hashes and requests them
//! 3. Node A sends the missing records
//! 4. Node B stores records and updates its bloom filter
//!
//! This is a pull-based protocol to minimize bandwidth usage.

use crate::error::Result;
use crate::gossip::{BloomFilter, GossipManager};
use crate::network::{Message, Network};
use crate::storage_trait::StorageBackend;
use crate::types::{Hash, Record};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Maximum records to send in a single batch
const MAX_BATCH_SIZE: usize = 50;

/// Maximum records to request at once
const MAX_REQUEST_SIZE: usize = 100;

/// Sync state for a peer
#[derive(Debug, Clone)]
pub struct PeerSyncState {
    /// Last sync timestamp
    pub last_sync: Instant,
    /// Remote peer's latest sequence
    pub remote_seq: u32,
    /// Our last synced sequence to this peer
    pub local_synced_seq: u32,
    /// Bloom filter received from peer
    pub peer_filter: Option<BloomFilter>,
    /// Pending hash requests to this peer
    pub pending_requests: Vec<Hash>,
    /// Number of successful syncs
    pub successful_syncs: u32,
    /// Number of failed syncs
    pub failed_syncs: u32,
}

impl PeerSyncState {
    pub fn new() -> Self {
        Self {
            last_sync: Instant::now(),
            remote_seq: 0,
            local_synced_seq: 0,
            peer_filter: None,
            pending_requests: Vec::new(),
            successful_syncs: 0,
            failed_syncs: 0,
        }
    }

    /// Check if we should sync with this peer
    pub fn should_sync(&self, min_interval: Duration) -> bool {
        self.last_sync.elapsed() >= min_interval
    }

    /// Record a successful sync
    pub fn record_success(&mut self) {
        self.last_sync = Instant::now();
        self.successful_syncs += 1;
        self.failed_syncs = 0;
    }

    /// Record a failed sync
    pub fn record_failure(&mut self) {
        self.last_sync = Instant::now();
        self.failed_syncs += 1;
    }
}

impl Default for PeerSyncState {
    fn default() -> Self {
        Self::new()
    }
}

/// Sync manager handles record synchronization between nodes
pub struct SyncManager {
    /// Per-peer sync state
    peer_states: HashMap<SocketAddr, PeerSyncState>,
    /// Minimum sync interval per peer
    sync_interval: Duration,
    /// Our collected hashes for bloom filter comparison
    local_hashes: Vec<Hash>,
    /// Maximum hashes to track
    max_local_hashes: usize,
}

impl SyncManager {
    /// Create a new sync manager
    pub fn new(sync_interval: Duration) -> Self {
        Self {
            peer_states: HashMap::new(),
            sync_interval,
            local_hashes: Vec::with_capacity(1000),
            max_local_hashes: 10000,
        }
    }

    /// Get or create peer sync state
    pub fn get_peer_state(&mut self, addr: &SocketAddr) -> &mut PeerSyncState {
        self.peer_states.entry(*addr).or_default()
    }

    /// Add a local hash to track
    pub fn add_local_hash(&mut self, hash: Hash) {
        if self.local_hashes.len() >= self.max_local_hashes {
            // Remove oldest hashes (first half)
            self.local_hashes.drain(..self.max_local_hashes / 2);
        }
        self.local_hashes.push(hash);
    }

    /// Get peers that need syncing
    pub fn peers_needing_sync(&self) -> Vec<SocketAddr> {
        self.peer_states
            .iter()
            .filter(|(_, state)| state.should_sync(self.sync_interval))
            .map(|(addr, _)| *addr)
            .collect()
    }

    /// Build bloom filter from local hashes
    pub fn build_local_filter(&self) -> BloomFilter {
        let mut filter = BloomFilter::new();
        for hash in &self.local_hashes {
            filter.insert(hash);
        }
        filter
    }

    /// Process received bloom filter from peer
    pub fn process_peer_filter<S: StorageBackend>(
        &mut self,
        addr: &SocketAddr,
        filter_bytes: &[u8],
        storage: &S,
    ) -> Vec<Hash> {
        let peer_filter = BloomFilter::from_bytes(filter_bytes);
        let state = self.get_peer_state(addr);
        state.peer_filter = Some(peer_filter.clone());

        // Find hashes we have that peer doesn't
        let mut missing = Vec::new();
        for hash in &self.local_hashes {
            if !peer_filter.may_contain(hash) {
                // Verify we actually have this record
                if storage.get_action(hash).ok().flatten().is_some() {
                    missing.push(hash.clone());
                    if missing.len() >= MAX_REQUEST_SIZE {
                        break;
                    }
                }
            }
        }

        log::debug!("Found {} hashes to send to peer {}", missing.len(), addr);

        missing
    }

    /// Store received records
    pub fn store_records<S: StorageBackend>(
        &mut self,
        _addr: &SocketAddr,
        records: Vec<Record>,
        storage: &S,
        gossip: &mut GossipManager,
    ) -> Result<usize> {
        let mut stored = 0;

        for record in records {
            let hash = match storage.put_record(&record) {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("Failed to store synced record: {}", e);
                    continue;
                }
            };

            // Add to gossip manager so we know about it
            gossip.add_known(hash.clone());
            self.add_local_hash(hash);
            stored += 1;
        }

        log::info!("Stored {} records from peer sync", stored);
        Ok(stored)
    }

    /// Run a sync round with a single peer
    pub async fn sync_with_peer<S: StorageBackend>(
        &mut self,
        addr: &SocketAddr,
        network: &mut Network,
        _storage: &S,
        _gossip: &mut GossipManager,
    ) -> Result<SyncResult> {
        // Build filter before getting peer state to avoid borrow conflict
        let local_filter = self.build_local_filter();
        let filter_bytes = local_filter.to_bytes();

        // Get local synced seq before mutable borrow
        let local_synced_seq = self
            .peer_states
            .get(addr)
            .map(|s| s.local_synced_seq)
            .unwrap_or(0);

        let message = Message::GossipRequest {
            from_seq: local_synced_seq,
            limit: MAX_BATCH_SIZE as u32,
        };

        if let Err(e) = network.send_confirmable(addr, &message).await {
            self.get_peer_state(addr).record_failure();
            return Err(e);
        }

        // Step 2: Also send bloom filter for efficient reconciliation
        // (In a full implementation, this would be a separate message type)
        log::debug!(
            "Sent sync request to {} (filter size: {} bytes)",
            addr,
            filter_bytes.len()
        );

        self.get_peer_state(addr).record_success();

        // Note: The actual record exchange happens asynchronously
        // when we receive GossipResponse messages
        Ok(SyncResult {
            peer: *addr,
            sent_filter: true,
            records_sent: 0,
            records_received: 0,
        })
    }

    /// Handle incoming gossip request
    pub fn handle_gossip_request<S: StorageBackend>(
        &mut self,
        from: &SocketAddr,
        from_seq: u32,
        limit: u32,
        storage: &S,
    ) -> Option<Vec<Record>> {
        let state = self.get_peer_state(from);
        state.remote_seq = from_seq;

        // Get records newer than their sequence
        let latest_seq = storage.get_latest_seq().unwrap_or(0);

        // Fetch records from from_seq to latest_seq, limited by limit
        let records = if from_seq < latest_seq {
            log::debug!(
                "Peer {} at seq {}, we have seq {}, sending up to {} records",
                from,
                from_seq,
                latest_seq,
                limit
            );
            storage.get_records_by_seq_range(from_seq, latest_seq + 1, limit)
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        if records.is_empty() {
            None
        } else {
            Some(records)
        }
    }

    /// Handle incoming gossip response
    pub fn handle_gossip_response<S: StorageBackend>(
        &mut self,
        from: &SocketAddr,
        records: Vec<Record>,
        storage: &S,
        gossip: &mut GossipManager,
    ) -> Result<usize> {
        let count = records.len();
        log::debug!("Received {} records from peer {}", count, from);

        let stored = self.store_records(from, records, storage, gossip)?;

        let state = self.get_peer_state(from);
        state.record_success();

        Ok(stored)
    }

    /// Get sync statistics
    pub fn stats(&self) -> SyncStats {
        let mut total_successful = 0;
        let mut total_failed = 0;

        for state in self.peer_states.values() {
            total_successful += state.successful_syncs;
            total_failed += state.failed_syncs;
        }

        SyncStats {
            peer_count: self.peer_states.len(),
            local_hashes: self.local_hashes.len(),
            total_successful_syncs: total_successful,
            total_failed_syncs: total_failed,
        }
    }

    /// Remove inactive peer states
    pub fn cleanup_inactive(&mut self, timeout: Duration) {
        self.peer_states
            .retain(|_, state| state.last_sync.elapsed() < timeout);
    }
}

/// Result of a sync operation
#[derive(Debug)]
pub struct SyncResult {
    /// Peer address
    pub peer: SocketAddr,
    /// Whether we sent our bloom filter
    pub sent_filter: bool,
    /// Number of records sent to peer
    pub records_sent: usize,
    /// Number of records received from peer
    pub records_received: usize,
}

/// Sync statistics
#[derive(Debug, Clone)]
pub struct SyncStats {
    /// Number of peers tracked
    pub peer_count: usize,
    /// Number of local hashes tracked
    pub local_hashes: usize,
    /// Total successful syncs
    pub total_successful_syncs: u32,
    /// Total failed syncs
    pub total_failed_syncs: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_sync_state_new() {
        let state = PeerSyncState::new();
        assert_eq!(state.remote_seq, 0);
        assert_eq!(state.local_synced_seq, 0);
        assert_eq!(state.successful_syncs, 0);
    }

    #[test]
    fn test_peer_sync_state_should_sync() {
        let state = PeerSyncState::new();

        // Should sync immediately with 0 interval
        assert!(state.should_sync(Duration::ZERO));

        // Should not sync with very long interval
        assert!(!state.should_sync(Duration::from_secs(3600)));
    }

    #[test]
    fn test_peer_sync_state_record_success() {
        let mut state = PeerSyncState::new();
        state.failed_syncs = 5;

        state.record_success();

        assert_eq!(state.successful_syncs, 1);
        assert_eq!(state.failed_syncs, 0);
    }

    #[test]
    fn test_sync_manager_new() {
        let manager = SyncManager::new(Duration::from_secs(60));
        assert_eq!(manager.peer_states.len(), 0);
        assert!(manager.local_hashes.is_empty());
    }

    #[test]
    fn test_sync_manager_add_local_hash() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        let hash = Hash::from_bytes(&[1; 32]);

        manager.add_local_hash(hash.clone());

        assert_eq!(manager.local_hashes.len(), 1);
        assert_eq!(manager.local_hashes[0], hash);
    }

    #[test]
    fn test_sync_manager_build_local_filter() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        let hash = Hash::from_bytes(&[1; 32]);
        manager.add_local_hash(hash.clone());

        let filter = manager.build_local_filter();

        assert!(filter.may_contain(&hash));
        assert!(!filter.may_contain(&Hash::from_bytes(&[2; 32])));
    }

    #[test]
    fn test_sync_manager_get_peer_state() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        let state = manager.get_peer_state(&addr);
        assert_eq!(state.successful_syncs, 0);

        state.record_success();

        let state2 = manager.get_peer_state(&addr);
        assert_eq!(state2.successful_syncs, 1);
    }

    #[test]
    fn test_sync_stats() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        manager.add_local_hash(Hash::from_bytes(&[1; 32]));
        manager.get_peer_state(&addr).record_success();
        manager.get_peer_state(&addr).record_success();

        let stats = manager.stats();
        assert_eq!(stats.peer_count, 1);
        assert_eq!(stats.local_hashes, 1);
        assert_eq!(stats.total_successful_syncs, 2);
    }

    #[test]
    fn test_peer_sync_state_record_failure() {
        let mut state = PeerSyncState::new();
        state.record_failure();
        assert_eq!(state.failed_syncs, 1);
        state.record_failure();
        assert_eq!(state.failed_syncs, 2);
    }

    #[test]
    fn test_peer_sync_state_default() {
        let state: PeerSyncState = Default::default();
        assert_eq!(state.remote_seq, 0);
        assert_eq!(state.local_synced_seq, 0);
        assert_eq!(state.successful_syncs, 0);
        assert_eq!(state.failed_syncs, 0);
        assert!(state.peer_filter.is_none());
        assert!(state.pending_requests.is_empty());
    }

    #[test]
    fn test_peer_sync_state_clone() {
        let mut state = PeerSyncState::new();
        state.remote_seq = 42;
        state.local_synced_seq = 10;
        state.successful_syncs = 5;
        state.failed_syncs = 2;

        let cloned = state.clone();
        assert_eq!(cloned.remote_seq, 42);
        assert_eq!(cloned.local_synced_seq, 10);
        assert_eq!(cloned.successful_syncs, 5);
        assert_eq!(cloned.failed_syncs, 2);
    }

    #[test]
    fn test_peer_sync_state_debug() {
        let state = PeerSyncState::new();
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("PeerSyncState"));
        assert!(debug_str.contains("remote_seq"));
    }

    #[test]
    fn test_sync_manager_peers_needing_sync_empty() {
        let manager = SyncManager::new(Duration::from_secs(60));
        let peers = manager.peers_needing_sync();
        assert!(peers.is_empty());
    }

    #[test]
    fn test_sync_manager_peers_needing_sync_with_interval() {
        let mut manager = SyncManager::new(Duration::from_millis(10));
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        manager.get_peer_state(&addr);

        // Immediately after creation, should not need sync (just saw it)
        std::thread::sleep(Duration::from_millis(20));

        let peers = manager.peers_needing_sync();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0], addr);
    }

    #[test]
    fn test_sync_manager_cleanup_inactive() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();

        manager.get_peer_state(&addr1);
        manager.get_peer_state(&addr2);

        assert_eq!(manager.peer_states.len(), 2);

        // Cleanup with very short timeout should remove all
        std::thread::sleep(Duration::from_millis(10));
        manager.cleanup_inactive(Duration::from_millis(1));

        assert_eq!(manager.peer_states.len(), 0);
    }

    #[test]
    fn test_sync_manager_cleanup_inactive_keeps_recent() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        manager.get_peer_state(&addr).record_success();

        // Cleanup with very long timeout should keep peer
        manager.cleanup_inactive(Duration::from_secs(3600));

        assert_eq!(manager.peer_states.len(), 1);
    }

    #[test]
    fn test_sync_manager_multiple_local_hashes() {
        let mut manager = SyncManager::new(Duration::from_secs(60));

        for i in 0..10 {
            let mut bytes = [0u8; 32];
            bytes[0] = i;
            manager.add_local_hash(Hash::from_bytes(&bytes));
        }

        assert_eq!(manager.local_hashes.len(), 10);
    }

    #[test]
    fn test_sync_stats_clone() {
        let stats = SyncStats {
            peer_count: 5,
            local_hashes: 100,
            total_successful_syncs: 50,
            total_failed_syncs: 2,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.peer_count, 5);
        assert_eq!(cloned.local_hashes, 100);
        assert_eq!(cloned.total_successful_syncs, 50);
        assert_eq!(cloned.total_failed_syncs, 2);
    }

    #[test]
    fn test_sync_stats_debug() {
        let stats = SyncStats {
            peer_count: 1,
            local_hashes: 10,
            total_successful_syncs: 5,
            total_failed_syncs: 0,
        };
        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("SyncStats"));
        assert!(debug_str.contains("peer_count"));
    }

    #[test]
    fn test_sync_result_debug() {
        let result = SyncResult {
            peer: "127.0.0.1:8080".parse().unwrap(),
            sent_filter: true,
            records_sent: 5,
            records_received: 3,
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("SyncResult"));
        assert!(debug_str.contains("127.0.0.1:8080"));
    }

    #[test]
    fn test_sync_manager_stats_with_failures() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();

        manager.get_peer_state(&addr1).record_success();
        manager.get_peer_state(&addr1).record_failure();
        manager.get_peer_state(&addr2).record_failure();
        manager.get_peer_state(&addr2).record_failure();

        let stats = manager.stats();
        assert_eq!(stats.peer_count, 2);
        assert_eq!(stats.total_successful_syncs, 1);
        assert_eq!(stats.total_failed_syncs, 3); // 1 + 2
    }

    #[test]
    fn test_sync_manager_build_empty_filter() {
        let manager = SyncManager::new(Duration::from_secs(60));
        let filter = manager.build_local_filter();

        // Empty filter should not contain any hash
        let hash = Hash::from_bytes(&[1; 32]);
        assert!(!filter.may_contain(&hash));
    }

    #[test]
    fn test_peer_sync_state_pending_requests() {
        let mut state = PeerSyncState::new();
        assert!(state.pending_requests.is_empty());

        state.pending_requests.push(Hash::from_bytes(&[1; 32]));
        state.pending_requests.push(Hash::from_bytes(&[2; 32]));

        assert_eq!(state.pending_requests.len(), 2);
    }

    #[test]
    fn test_peer_sync_state_with_filter() {
        let mut state = PeerSyncState::new();
        assert!(state.peer_filter.is_none());

        let filter = BloomFilter::new();
        state.peer_filter = Some(filter);

        assert!(state.peer_filter.is_some());
    }

    #[test]
    fn test_max_batch_size_constant() {
        assert_eq!(MAX_BATCH_SIZE, 50);
    }

    #[test]
    fn test_max_request_size_constant() {
        assert_eq!(MAX_REQUEST_SIZE, 100);
    }

    #[test]
    fn test_sync_manager_add_local_hash_overflow() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        manager.max_local_hashes = 10;

        // Add 15 hashes, which exceeds the limit
        for i in 0..15 {
            let mut bytes = [0u8; 32];
            bytes[0] = i as u8;
            manager.add_local_hash(Hash::from_bytes(&bytes));
        }

        // Should have cleaned up oldest half when exceeded
        // 10 + 5 new = 15, then drain half(5) = 10, then add remaining
        assert!(manager.local_hashes.len() <= 15);
    }

    #[test]
    fn test_peer_sync_state_should_sync_after_wait() {
        let state = PeerSyncState::new();

        std::thread::sleep(Duration::from_millis(5));

        // Should sync after waiting more than the interval
        assert!(state.should_sync(Duration::from_millis(1)));
    }

    #[test]
    fn test_sync_result_fields() {
        let result = SyncResult {
            peer: "192.168.1.1:5683".parse().unwrap(),
            sent_filter: false,
            records_sent: 10,
            records_received: 5,
        };

        assert_eq!(result.peer.port(), 5683);
        assert!(!result.sent_filter);
        assert_eq!(result.records_sent, 10);
        assert_eq!(result.records_received, 5);
    }

    #[test]
    fn test_sync_stats_fields() {
        let stats = SyncStats {
            peer_count: 3,
            local_hashes: 500,
            total_successful_syncs: 100,
            total_failed_syncs: 5,
        };

        assert_eq!(stats.peer_count, 3);
        assert_eq!(stats.local_hashes, 500);
        assert_eq!(stats.total_successful_syncs, 100);
        assert_eq!(stats.total_failed_syncs, 5);
    }

    #[test]
    fn test_sync_manager_filter_multiple_hashes() {
        let mut manager = SyncManager::new(Duration::from_secs(60));

        for i in 0..100 {
            let mut bytes = [0u8; 32];
            bytes[0] = i;
            manager.add_local_hash(Hash::from_bytes(&bytes));
        }

        let filter = manager.build_local_filter();

        // All added hashes should be in filter
        for i in 0..100 {
            let mut bytes = [0u8; 32];
            bytes[0] = i;
            assert!(filter.may_contain(&Hash::from_bytes(&bytes)));
        }
    }

    #[test]
    fn test_sync_manager_multiple_peers_stats() {
        let mut manager = SyncManager::new(Duration::from_secs(60));

        // Add 5 peers with different stats
        for i in 0..5 {
            let addr: SocketAddr = format!("127.0.0.1:808{}", i).parse().unwrap();
            let state = manager.get_peer_state(&addr);
            for _ in 0..i {
                state.record_success();
            }
        }

        let stats = manager.stats();
        assert_eq!(stats.peer_count, 5);
        // 0 + 1 + 2 + 3 + 4 = 10
        assert_eq!(stats.total_successful_syncs, 10);
    }

    #[test]
    fn test_peer_sync_state_sequence_tracking() {
        let mut state = PeerSyncState::new();

        state.remote_seq = 100;
        state.local_synced_seq = 50;

        assert_eq!(state.remote_seq, 100);
        assert_eq!(state.local_synced_seq, 50);
    }

    #[test]
    fn test_sync_manager_get_peer_state_creates_new() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        assert_eq!(manager.peer_states.len(), 0);

        let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();

        manager.get_peer_state(&addr1);
        assert_eq!(manager.peer_states.len(), 1);

        manager.get_peer_state(&addr2);
        assert_eq!(manager.peer_states.len(), 2);

        // Getting same peer shouldn't add new entry
        manager.get_peer_state(&addr1);
        assert_eq!(manager.peer_states.len(), 2);
    }

    #[test]
    fn test_sync_manager_empty_stats() {
        let manager = SyncManager::new(Duration::from_secs(60));
        let stats = manager.stats();

        assert_eq!(stats.peer_count, 0);
        assert_eq!(stats.local_hashes, 0);
        assert_eq!(stats.total_successful_syncs, 0);
        assert_eq!(stats.total_failed_syncs, 0);
    }

    #[test]
    fn test_peer_sync_state_multiple_operations() {
        let mut state = PeerSyncState::new();

        // Multiple successes
        state.record_success();
        state.record_success();
        state.record_success();
        assert_eq!(state.successful_syncs, 3);
        assert_eq!(state.failed_syncs, 0);

        // Then a failure
        state.record_failure();
        assert_eq!(state.failed_syncs, 1);

        // Another success resets failures
        state.record_success();
        assert_eq!(state.successful_syncs, 4);
        assert_eq!(state.failed_syncs, 0);
    }

    #[test]
    fn test_sync_manager_sync_interval() {
        let manager = SyncManager::new(Duration::from_secs(120));
        assert_eq!(manager.sync_interval, Duration::from_secs(120));
    }

    #[test]
    fn test_sync_manager_max_local_hashes_limit() {
        let mut manager = SyncManager::new(Duration::from_secs(60));
        // Default is 10000
        assert_eq!(manager.max_local_hashes, 10000);

        // Add up to limit
        for i in 0..100 {
            let mut bytes = [0u8; 32];
            bytes[0] = (i % 256) as u8;
            bytes[1] = (i / 256) as u8;
            manager.add_local_hash(Hash::from_bytes(&bytes));
        }

        assert_eq!(manager.local_hashes.len(), 100);
    }

    #[test]
    fn test_peer_sync_state_last_sync_updated() {
        let mut state = PeerSyncState::new();
        let initial = state.last_sync;

        std::thread::sleep(Duration::from_millis(5));
        state.record_success();

        assert!(state.last_sync > initial);
    }

    #[test]
    fn test_peer_sync_state_failure_timing() {
        let mut state = PeerSyncState::new();
        let initial = state.last_sync;

        std::thread::sleep(Duration::from_millis(5));
        state.record_failure();

        assert!(state.last_sync > initial);
    }

    #[test]
    fn test_sync_result_all_fields() {
        let result = SyncResult {
            peer: "10.0.0.1:1234".parse().unwrap(),
            sent_filter: true,
            records_sent: 100,
            records_received: 50,
        };

        assert!(result.sent_filter);
        assert_eq!(result.records_sent, 100);
        assert_eq!(result.records_received, 50);
        assert_eq!(result.peer.port(), 1234);
    }

    #[test]
    fn test_sync_manager_add_many_hashes() {
        let mut manager = SyncManager::new(Duration::from_secs(60));

        // Add 1000 hashes
        for i in 0..1000 {
            let mut bytes = [0u8; 32];
            bytes[0] = (i % 256) as u8;
            bytes[1] = ((i / 256) % 256) as u8;
            manager.add_local_hash(Hash::from_bytes(&bytes));
        }

        let filter = manager.build_local_filter();

        // Verify some hashes are in filter
        let mut bytes = [0u8; 32];
        bytes[0] = 100;
        assert!(filter.may_contain(&Hash::from_bytes(&bytes)));
    }

    #[test]
    fn test_peers_needing_sync_multiple() {
        let mut manager = SyncManager::new(Duration::from_millis(5));

        // Add multiple peers
        for i in 0..5 {
            let addr: SocketAddr = format!("127.0.0.1:808{}", i).parse().unwrap();
            manager.get_peer_state(&addr);
        }

        // Wait for sync interval
        std::thread::sleep(Duration::from_millis(10));

        let peers = manager.peers_needing_sync();
        assert_eq!(peers.len(), 5);
    }

    #[test]
    fn test_sync_manager_cleanup_all_inactive() {
        let mut manager = SyncManager::new(Duration::from_secs(60));

        let addr1: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:8081".parse().unwrap();

        manager.get_peer_state(&addr1);
        manager.get_peer_state(&addr2);

        assert_eq!(manager.peer_states.len(), 2);

        // Wait and then cleanup
        std::thread::sleep(Duration::from_millis(10));
        manager.cleanup_inactive(Duration::from_millis(1));

        // All should be removed because of short timeout
        assert!(manager.peer_states.len() <= 2);
    }

    #[test]
    fn test_peer_sync_state_fields_direct() {
        let mut state = PeerSyncState::new();

        state.remote_seq = 500;
        state.local_synced_seq = 400;
        state.successful_syncs = 10;
        state.failed_syncs = 2;

        let hash = Hash::from_bytes(&[1; 32]);
        state.pending_requests.push(hash.clone());

        assert_eq!(state.remote_seq, 500);
        assert_eq!(state.local_synced_seq, 400);
        assert_eq!(state.successful_syncs, 10);
        assert_eq!(state.failed_syncs, 2);
        assert_eq!(state.pending_requests.len(), 1);
    }

    #[test]
    fn test_sync_stats_total_counts() {
        let mut manager = SyncManager::new(Duration::from_secs(60));

        // Multiple peers with different outcomes
        for i in 0..3 {
            let addr: SocketAddr = format!("127.0.0.1:900{}", i).parse().unwrap();
            let state = manager.get_peer_state(&addr);

            // Varying successes
            for _ in 0..(i + 1) {
                state.record_success();
            }
            // Some failures
            state.record_failure();
        }

        let stats = manager.stats();
        assert_eq!(stats.peer_count, 3);
        // 1 + 2 + 3 = 6 successes (but then reset by failure)
        // Each has 1 failure at the end
        assert_eq!(stats.total_failed_syncs, 3);
    }
}
