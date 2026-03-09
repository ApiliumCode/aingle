// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Triple synchronization manager.
//!
//! Tracks per-peer sync state and coordinates bloom-filter-based reconciliation
//! against the local `GraphDB`.

use crate::p2p::gossip::BloomFilter;
use aingle_graph::{GraphDB, Triple, TripleId};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Maximum IDs to request in a single reconciliation round.
const MAX_REQUEST_SIZE: usize = 100;

/// Per-peer sync tracking.
#[derive(Debug, Clone)]
pub struct PeerSyncState {
    pub last_sync: Instant,
    pub peer_filter: Option<BloomFilter>,
    pub pending_requests: Vec<[u8; 32]>,
    pub successful_syncs: u32,
    pub failed_syncs: u32,
}

impl PeerSyncState {
    pub fn new() -> Self {
        Self {
            last_sync: Instant::now(),
            peer_filter: None,
            pending_requests: Vec::new(),
            successful_syncs: 0,
            failed_syncs: 0,
        }
    }

    pub fn should_sync(&self, interval: Duration) -> bool {
        self.last_sync.elapsed() >= interval
    }

    pub fn record_success(&mut self) {
        self.last_sync = Instant::now();
        self.successful_syncs += 1;
        self.failed_syncs = 0;
    }

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

/// Manages triple synchronization with peers.
pub struct TripleSyncManager {
    peer_states: HashMap<SocketAddr, PeerSyncState>,
    sync_interval: Duration,
    /// All known local triple IDs.
    local_ids: Vec<[u8; 32]>,
    max_local_ids: usize,
    /// Tombstones: hash -> deletion timestamp_ms.
    pub(crate) tombstones: HashMap<[u8; 32], u64>,
    /// Tombstone time-to-live (default 24h).
    pub(crate) tombstone_ttl: Duration,
}

impl TripleSyncManager {
    pub fn new(sync_interval: Duration) -> Self {
        Self {
            peer_states: HashMap::new(),
            sync_interval,
            local_ids: Vec::with_capacity(1000),
            max_local_ids: 100_000,
            tombstones: HashMap::new(),
            tombstone_ttl: Duration::from_secs(24 * 3600),
        }
    }

    /// Create with a custom tombstone TTL.
    pub fn with_tombstone_ttl(sync_interval: Duration, ttl: Duration) -> Self {
        let mut mgr = Self::new(sync_interval);
        mgr.tombstone_ttl = ttl;
        mgr
    }

    /// Register a new local triple ID.
    pub fn add_local_id(&mut self, id: [u8; 32]) {
        if self.local_ids.len() >= self.max_local_ids {
            self.local_ids.drain(..self.max_local_ids / 2);
        }
        self.local_ids.push(id);
    }

    /// Rebuild the local ID list by scanning the full graph.
    pub fn rebuild_local_ids(&mut self, graph: &GraphDB) {
        self.local_ids.clear();
        if let Ok(triples) = graph.find(aingle_graph::TriplePattern::any()) {
            for triple in &triples {
                self.local_ids.push(TripleId::from_triple(triple).0);
            }
        }
    }

    /// Get a snapshot of all local IDs.
    pub fn local_ids(&self) -> &[[u8; 32]] {
        &self.local_ids
    }

    /// Return peers whose sync interval has elapsed.
    pub fn peers_needing_sync(&self) -> Vec<SocketAddr> {
        self.peer_states
            .iter()
            .filter(|(_, s)| s.should_sync(self.sync_interval))
            .map(|(addr, _)| *addr)
            .collect()
    }

    /// Build a bloom filter from all local IDs.
    pub fn build_local_filter(&self) -> BloomFilter {
        let mut filter = BloomFilter::new();
        for id in &self.local_ids {
            filter.insert(id);
        }
        filter
    }

    /// Given a peer's bloom filter, return IDs we have that the peer is missing (capped).
    pub fn process_peer_filter(&self, peer_filter: &BloomFilter) -> Vec<[u8; 32]> {
        let mut missing = Vec::new();
        for id in &self.local_ids {
            if !peer_filter.may_contain(id) {
                missing.push(*id);
                if missing.len() >= MAX_REQUEST_SIZE {
                    break;
                }
            }
        }
        missing
    }

    /// Insert triples received from a peer into the graph. Duplicates are counted, not errors.
    pub fn store_received_triples(
        &mut self,
        triples: Vec<Triple>,
        graph: &GraphDB,
    ) -> StoreResult {
        let mut result = StoreResult::default();
        for triple in triples {
            let id = TripleId::from_triple(&triple);
            match graph.insert(triple) {
                Ok(_) => {
                    self.add_local_id(id.0);
                    result.inserted += 1;
                }
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("duplicate") || msg.contains("exists") || msg.contains("already") {
                        result.duplicates += 1;
                    } else {
                        result.errors += 1;
                    }
                }
            }
        }
        result
    }

    /// Record the outcome of a sync round for a given peer.
    pub fn record_sync_result(&mut self, peer: SocketAddr, success: bool, _triples_synced: usize) {
        let state = self.peer_states.entry(peer).or_default();
        if success {
            state.record_success();
        } else {
            state.record_failure();
        }
    }

    /// Get or create state entry for a peer.
    pub fn get_peer_state(&mut self, addr: &SocketAddr) -> &mut PeerSyncState {
        self.peer_states.entry(*addr).or_default()
    }

    /// Remove peers that haven't synced within `timeout`.
    pub fn cleanup_inactive(&mut self, timeout: Duration) {
        self.peer_states
            .retain(|_, s| s.last_sync.elapsed() < timeout);
    }

    /// Remove a local ID (used when a triple is deleted).
    pub fn remove_local_id(&mut self, id: &[u8; 32]) {
        self.local_ids.retain(|existing| existing != id);
    }

    /// Add a tombstone marker for a deleted triple.
    pub fn add_tombstone(&mut self, id: [u8; 32], ts_ms: u64) {
        self.tombstones.insert(id, ts_ms);
    }

    /// Check if a tombstone exists for the given ID.
    pub fn has_tombstone(&self, id: &[u8; 32]) -> bool {
        self.tombstones.contains_key(id)
    }

    /// Remove expired tombstones (older than TTL).
    pub fn cleanup_expired_tombstones(&mut self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let ttl_ms = self.tombstone_ttl.as_millis() as u64;
        self.tombstones.retain(|_, ts| now_ms.saturating_sub(*ts) < ttl_ms);
    }

    /// Return all active tombstones as (hash, timestamp_ms) pairs.
    pub fn active_tombstones(&self) -> Vec<([u8; 32], u64)> {
        self.tombstones.iter().map(|(k, v)| (*k, *v)).collect()
    }

    /// Aggregate sync statistics.
    pub fn stats(&self) -> SyncStats {
        let mut total_successful = 0;
        let mut total_failed = 0;
        for s in self.peer_states.values() {
            total_successful += s.successful_syncs;
            total_failed += s.failed_syncs;
        }
        SyncStats {
            peer_count: self.peer_states.len(),
            local_ids: self.local_ids.len(),
            total_successful_syncs: total_successful,
            total_failed_syncs: total_failed,
        }
    }
}

/// Result of a `store_received_triples` operation.
#[derive(Debug, Default)]
pub struct StoreResult {
    pub inserted: usize,
    pub duplicates: usize,
    pub errors: usize,
}

/// Aggregate sync statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStats {
    pub peer_count: usize,
    pub local_ids: usize,
    pub total_successful_syncs: u32,
    pub total_failed_syncs: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_empty() {
        let sm = TripleSyncManager::new(Duration::from_secs(60));
        assert!(sm.local_ids().is_empty());
        assert!(sm.peers_needing_sync().is_empty());
    }

    #[test]
    fn add_local_id() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        sm.add_local_id([1u8; 32]);
        assert_eq!(sm.local_ids().len(), 1);
    }

    #[test]
    fn build_local_filter_contains_ids() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let id = [5u8; 32];
        sm.add_local_id(id);
        let filter = sm.build_local_filter();
        assert!(filter.may_contain(&id));
        assert!(!filter.may_contain(&[99u8; 32]));
    }

    #[test]
    fn process_peer_filter_finds_missing() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let a = [1u8; 32];
        let b = [2u8; 32];
        let c = [3u8; 32];
        sm.add_local_id(a);
        sm.add_local_id(b);
        sm.add_local_id(c);

        let mut peer = BloomFilter::new();
        peer.insert(&a);

        let missing = sm.process_peer_filter(&peer);
        assert!(missing.contains(&b));
        assert!(missing.contains(&c));
        assert!(!missing.contains(&a));
    }

    #[test]
    fn peers_needing_sync_respects_interval() {
        let mut sm = TripleSyncManager::new(Duration::from_millis(10));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        sm.get_peer_state(&addr);

        std::thread::sleep(Duration::from_millis(20));
        let peers = sm.peers_needing_sync();
        assert_eq!(peers.len(), 1);
    }

    #[test]
    fn store_received_triples_inserts() {
        let graph = GraphDB::memory().unwrap();
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));

        let triple = Triple::new(
            aingle_graph::NodeId::named("test:a"),
            aingle_graph::Predicate::named("test:rel"),
            aingle_graph::Value::String("val".into()),
        );

        let result = sm.store_received_triples(vec![triple], &graph);
        assert_eq!(result.inserted, 1);
        assert_eq!(result.duplicates, 0);
    }

    #[test]
    fn store_received_triples_skips_duplicates() {
        let graph = GraphDB::memory().unwrap();
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));

        let triple = Triple::new(
            aingle_graph::NodeId::named("test:a"),
            aingle_graph::Predicate::named("test:rel"),
            aingle_graph::Value::String("val".into()),
        );

        // Insert once directly.
        let _ = graph.insert(triple.clone());

        // Attempt to insert again via sync.
        let result = sm.store_received_triples(vec![triple], &graph);
        assert_eq!(result.inserted, 0);
        // Depending on GraphDB error message, counted as duplicate or error.
        assert!(result.duplicates > 0 || result.errors > 0);
    }

    #[test]
    fn record_sync_result_updates_state() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        sm.record_sync_result(addr, true, 10);
        sm.record_sync_result(addr, true, 5);

        let stats = sm.stats();
        assert_eq!(stats.total_successful_syncs, 2);
    }

    #[test]
    fn cleanup_inactive_removes_old_peers() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        sm.get_peer_state(&addr);

        std::thread::sleep(Duration::from_millis(10));
        sm.cleanup_inactive(Duration::from_millis(1));
        assert_eq!(sm.stats().peer_count, 0);
    }

    #[test]
    fn remove_local_id_works() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let a = [1u8; 32];
        let b = [2u8; 32];
        sm.add_local_id(a);
        sm.add_local_id(b);
        assert_eq!(sm.local_ids().len(), 2);
        sm.remove_local_id(&a);
        assert_eq!(sm.local_ids().len(), 1);
        assert_eq!(sm.local_ids()[0], b);
    }

    #[test]
    fn add_tombstone_and_check() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let id = [42u8; 32];
        assert!(!sm.has_tombstone(&id));
        sm.add_tombstone(id, 1700000000000);
        assert!(sm.has_tombstone(&id));
    }

    #[test]
    fn cleanup_expired_tombstones() {
        let mut sm = TripleSyncManager::with_tombstone_ttl(
            Duration::from_secs(60),
            Duration::from_millis(50),
        );
        let id = [1u8; 32];
        // Use a timestamp far in the past
        sm.add_tombstone(id, 0);
        sm.cleanup_expired_tombstones();
        assert!(!sm.has_tombstone(&id));
    }

    #[test]
    fn tombstone_ttl_configurable() {
        let sm = TripleSyncManager::with_tombstone_ttl(
            Duration::from_secs(60),
            Duration::from_secs(3600),
        );
        assert_eq!(sm.tombstone_ttl, Duration::from_secs(3600));
    }

    #[test]
    fn active_tombstones_returns_all() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        sm.add_tombstone([1u8; 32], 100);
        sm.add_tombstone([2u8; 32], 200);
        let active = sm.active_tombstones();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn duplicate_tombstone_is_noop() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let id = [1u8; 32];
        sm.add_tombstone(id, 100);
        sm.add_tombstone(id, 200);
        assert_eq!(sm.active_tombstones().len(), 1);
        // Second write overwrites timestamp
        let ts = sm.tombstones.get(&id).unwrap();
        assert_eq!(*ts, 200);
    }

    #[test]
    fn stats_are_accurate() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        sm.add_local_id([1u8; 32]);
        sm.add_local_id([2u8; 32]);

        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        sm.record_sync_result(addr, true, 1);
        sm.record_sync_result(addr, false, 0);

        let stats = sm.stats();
        assert_eq!(stats.local_ids, 2);
        assert_eq!(stats.peer_count, 1);
        assert_eq!(stats.total_successful_syncs, 1);
        assert_eq!(stats.total_failed_syncs, 1);
    }
}
