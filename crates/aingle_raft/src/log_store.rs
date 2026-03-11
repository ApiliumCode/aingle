// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft log storage backed by WAL segments.

use crate::types::CortexTypeConfig;
use openraft::alias::{EntryOf, LogIdOf, VoteOf};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type Vote = VoteOf<CortexTypeConfig>;
type LogId = LogIdOf<CortexTypeConfig>;
type Entry = EntryOf<CortexTypeConfig>;

/// In-memory Raft log store with optional WAL backing.
///
/// Handles the Raft protocol's log management needs.
/// WAL entries provide durability on disk.
pub struct CortexLogStore {
    vote: RwLock<Option<Vote>>,
    committed: RwLock<Option<LogId>>,
    log: RwLock<BTreeMap<u64, Entry>>,
    /// WAL writer for durable persistence.
    wal: Option<Arc<aingle_wal::WalWriter>>,
}

impl CortexLogStore {
    /// Create a new log store, optionally backed by a WAL writer.
    pub fn new(wal: Option<Arc<aingle_wal::WalWriter>>) -> Self {
        Self {
            vote: RwLock::new(None),
            committed: RwLock::new(None),
            log: RwLock::new(BTreeMap::new()),
            wal,
        }
    }

    pub async fn save_vote(&self, vote: Vote) {
        let mut v = self.vote.write().await;
        *v = Some(vote);
    }

    pub async fn read_vote(&self) -> Option<Vote> {
        self.vote.read().await.clone()
    }

    pub async fn save_committed(&self, committed: LogId) {
        let mut c = self.committed.write().await;
        *c = Some(committed);
    }

    pub async fn read_committed(&self) -> Option<LogId> {
        let guard = self.committed.read().await;
        guard.clone()
    }

    pub async fn append(&self, entries: Vec<Entry>) {
        let mut log = self.log.write().await;
        for entry in entries {
            let index = entry.log_id.index;
            log.insert(index, entry);
        }
    }

    pub async fn truncate(&self, index: u64) {
        let mut log = self.log.write().await;
        let keys: Vec<u64> = log.range(index..).map(|(k, _)| *k).collect();
        for k in keys {
            log.remove(&k);
        }
    }

    pub async fn get_log_entries(&self, range: std::ops::Range<u64>) -> Vec<Entry> {
        let log = self.log.read().await;
        log.range(range).map(|(_, e)| e.clone()).collect()
    }

    pub async fn last_log_id(&self) -> Option<LogId> {
        let log = self.log.read().await;
        log.values().last().map(|e| e.log_id.clone())
    }

    pub async fn log_length(&self) -> u64 {
        let log = self.log.read().await;
        log.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_store_new() {
        let store = CortexLogStore::new(None);
        assert!(store.read_vote().await.is_none());
        assert!(store.read_committed().await.is_none());
        assert_eq!(store.log_length().await, 0);
    }
}
