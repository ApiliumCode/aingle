// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Persistent peer storage backed by a JSON file.
//!
//! Stores known peers in `{data_dir}/known_peers.json` so they survive restarts.
//! Peers are infrastructure metadata, not knowledge graph data.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// How a peer was originally discovered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PeerSource {
    Manual,
    Mdns,
    RestApi,
}

/// A peer entry persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPeer {
    pub addr: SocketAddr,
    pub node_id: Option<String>,
    pub last_connected_ms: u64,
    pub source: PeerSource,
}

/// JSON-backed persistent peer list.
pub struct PeerStore {
    path: PathBuf,
    peers: Vec<StoredPeer>,
    max_peers: usize,
}

impl PeerStore {
    /// Load from disk or create empty.
    pub fn load(data_dir: &std::path::Path, max_peers: usize) -> Self {
        let path = data_dir.join("known_peers.json");
        let peers = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
        Self { path, peers, max_peers }
    }

    /// Write the current peer list to disk.
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create peer store dir: {}", e))?;
        }
        let json = serde_json::to_string_pretty(&self.peers)
            .map_err(|e| format!("serialize peers: {}", e))?;
        std::fs::write(&self.path, json)
            .map_err(|e| format!("write peer store: {}", e))?;
        Ok(())
    }

    /// Add a peer. Deduplicates by address. Enforces max_peers limit.
    pub fn add(&mut self, peer: StoredPeer) {
        // Deduplicate
        if self.peers.iter().any(|p| p.addr == peer.addr) {
            return;
        }
        // Enforce capacity
        if self.peers.len() >= self.max_peers {
            // Remove oldest (by last_connected_ms)
            if let Some(oldest_idx) = self.peers.iter().enumerate()
                .min_by_key(|(_, p)| p.last_connected_ms)
                .map(|(i, _)| i)
            {
                self.peers.remove(oldest_idx);
            }
        }
        self.peers.push(peer);
    }

    /// Remove a peer by address.
    pub fn remove(&mut self, addr: &SocketAddr) {
        self.peers.retain(|p| p.addr != *addr);
    }

    /// Get all stored peers.
    pub fn all(&self) -> &[StoredPeer] {
        &self.peers
    }

    /// Update last_connected timestamp for a peer.
    pub fn update_last_connected(&mut self, addr: &SocketAddr, ts_ms: u64) {
        if let Some(peer) = self.peers.iter_mut().find(|p| p.addr == *addr) {
            peer.last_connected_ms = ts_ms;
        }
    }

    /// Remove peers not connected for more than `max_age_ms` milliseconds.
    pub fn cleanup_stale(&mut self, max_age_ms: u64) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.peers.retain(|p| {
            p.last_connected_ms == 0 || now_ms.saturating_sub(p.last_connected_ms) < max_age_ms
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(max_peers: usize) -> PeerStore {
        let dir = tempfile::TempDir::new().unwrap();
        PeerStore::load(dir.path(), max_peers)
    }

    fn addr(port: u16) -> SocketAddr {
        format!("127.0.0.1:{}", port).parse().unwrap()
    }

    fn stored_peer(port: u16, ts: u64) -> StoredPeer {
        StoredPeer {
            addr: addr(port),
            node_id: None,
            last_connected_ms: ts,
            source: PeerSource::Manual,
        }
    }

    #[test]
    fn peer_store_empty_on_first_load() {
        let store = temp_store(100);
        assert!(store.all().is_empty());
    }

    #[test]
    fn peer_store_add_and_save() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut store = PeerStore::load(dir.path(), 100);
        store.add(stored_peer(9000, 1000));
        assert_eq!(store.all().len(), 1);
        assert!(store.save().is_ok());
        assert!(dir.path().join("known_peers.json").exists());
    }

    #[test]
    fn peer_store_load_persisted_data() {
        let dir = tempfile::TempDir::new().unwrap();
        {
            let mut store = PeerStore::load(dir.path(), 100);
            store.add(stored_peer(9000, 1000));
            store.add(stored_peer(9001, 2000));
            store.save().unwrap();
        }
        let store2 = PeerStore::load(dir.path(), 100);
        assert_eq!(store2.all().len(), 2);
    }

    #[test]
    fn peer_store_remove_existing() {
        let mut store = temp_store(100);
        store.add(stored_peer(9000, 1000));
        store.add(stored_peer(9001, 2000));
        store.remove(&addr(9000));
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].addr, addr(9001));
    }

    #[test]
    fn peer_store_deduplicates_same_addr() {
        let mut store = temp_store(100);
        store.add(stored_peer(9000, 1000));
        store.add(stored_peer(9000, 2000));
        assert_eq!(store.all().len(), 1);
    }

    #[test]
    fn peer_store_cleanup_stale() {
        let mut store = temp_store(100);
        // Add a peer with timestamp 0 (never connected) — should be kept
        store.add(stored_peer(9000, 0));
        // Add a peer with an old timestamp
        store.add(stored_peer(9001, 1));
        store.cleanup_stale(1000); // 1 second max age
        // peer with ts=0 is kept (never-connected sentinel), old one removed
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].addr, addr(9000));
    }

    #[test]
    fn peer_store_max_peers_enforced() {
        let mut store = temp_store(2);
        store.add(stored_peer(9000, 100));
        store.add(stored_peer(9001, 200));
        assert_eq!(store.all().len(), 2);
        // Adding a third should evict the oldest (port 9000, ts=100)
        store.add(stored_peer(9002, 300));
        assert_eq!(store.all().len(), 2);
        assert!(!store.all().iter().any(|p| p.addr == addr(9000)));
    }

    #[test]
    fn peer_store_update_last_connected() {
        let mut store = temp_store(100);
        store.add(stored_peer(9000, 1000));
        store.update_last_connected(&addr(9000), 5000);
        assert_eq!(store.all()[0].last_connected_ms, 5000);
    }
}
