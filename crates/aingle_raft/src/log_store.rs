// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Raft log storage backed by WAL segments.
//!
//! Implements `RaftLogReader` and `RaftLogStorage` from openraft,
//! persisting entries as `WalEntryKind::RaftEntry` variants and
//! vote/committed state as JSON files alongside the WAL directory.

use crate::types::CortexTypeConfig;
use aingle_wal::{WalEntryKind, WalWriter};
use openraft::alias::{EntryOf, LogIdOf, VoteOf};
use openraft::storage::{IOFlushed, LogState, RaftLogStorage};
use openraft::RaftLogReader;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::{self, Write};
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

type C = CortexTypeConfig;
type Vote = VoteOf<C>;
type LogId = LogIdOf<C>;
type Entry = EntryOf<C>;

/// Durable Raft log store backed by the AIngle WAL.
///
/// In-memory BTreeMap serves reads; WAL provides persistence.
/// Vote and committed state are persisted as JSON files.
pub struct CortexLogStore {
    vote: RwLock<Option<Vote>>,
    committed: RwLock<Option<LogId>>,
    log: RwLock<BTreeMap<u64, Entry>>,
    purged_log_id: RwLock<Option<LogId>>,
    /// Truncation boundary — entries with index > this are invalid.
    truncated_after: RwLock<Option<LogId>>,
    /// WAL writer for durable persistence.
    wal: Arc<WalWriter>,
    /// Directory for vote/committed JSON files.
    wal_dir: PathBuf,
}

impl CortexLogStore {
    /// Open or create a log store backed by the WAL at `wal_dir`.
    ///
    /// On recovery, reads WAL segments, filters `RaftEntry` variants,
    /// rebuilds the in-memory BTreeMap, then applies persisted
    /// truncation/purge boundaries to discard stale entries.
    pub fn open(wal_dir: &Path) -> io::Result<Self> {
        let wal = Arc::new(WalWriter::open(wal_dir)?);

        // Recover vote from disk
        let vote = Self::load_vote(wal_dir)?;

        // Recover committed from disk
        let committed = Self::load_committed(wal_dir)?;

        // Recover purged boundary from disk
        let purged_log_id = Self::load_purged(wal_dir)?;

        // Recover truncation boundary from disk
        let truncated_after = Self::load_truncated_after(wal_dir)?;

        // Rebuild log from WAL
        let reader = aingle_wal::WalReader::open(wal_dir)?;
        let wal_entries = reader.read_from(0)?;
        let mut log = BTreeMap::new();

        for wal_entry in &wal_entries {
            if let WalEntryKind::RaftEntry { index, term: _, data } = &wal_entry.kind {
                match serde_json::from_slice::<Entry>(data) {
                    Ok(entry) => {
                        log.insert(*index, entry);
                    }
                    Err(e) => {
                        tracing::warn!(
                            index = index,
                            "Failed to deserialize RaftEntry from WAL: {}",
                            e
                        );
                    }
                }
            }
        }

        // Apply persisted boundaries: remove entries outside the valid range
        if let Some(ref purged) = purged_log_id {
            log.retain(|idx, _| *idx > purged.index);
        }
        if let Some(ref trunc) = truncated_after {
            log.retain(|idx, _| *idx <= trunc.index);
        }

        tracing::info!(
            entries = log.len(),
            vote = ?vote,
            committed = ?committed,
            purged = ?purged_log_id,
            truncated_after = ?truncated_after,
            "CortexLogStore recovered from WAL"
        );

        Ok(Self {
            vote: RwLock::new(vote),
            committed: RwLock::new(committed),
            log: RwLock::new(log),
            purged_log_id: RwLock::new(purged_log_id),
            truncated_after: RwLock::new(truncated_after),
            wal,
            wal_dir: wal_dir.to_path_buf(),
        })
    }

    /// Get the WAL writer reference.
    pub fn wal(&self) -> &Arc<WalWriter> {
        &self.wal
    }

    // --- Atomic file write ---

    /// Atomically write data to a file: write to .tmp, fsync, rename.
    fn atomic_write(target: &Path, data: &[u8]) -> io::Result<()> {
        let tmp = target.with_extension("tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(data)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, target)?;
        // fsync the parent directory to ensure the rename is durable
        if let Some(parent) = target.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    }

    // --- Persistence helpers ---

    fn vote_path(dir: &Path) -> PathBuf {
        dir.join("raft_vote.json")
    }

    fn committed_path(dir: &Path) -> PathBuf {
        dir.join("raft_committed.json")
    }

    fn purged_path(dir: &Path) -> PathBuf {
        dir.join("raft_purged.json")
    }

    fn truncated_after_path(dir: &Path) -> PathBuf {
        dir.join("raft_truncated_after.json")
    }

    fn persist_vote(dir: &Path, vote: &Vote) -> io::Result<()> {
        let data = serde_json::to_vec_pretty(vote)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Self::atomic_write(&Self::vote_path(dir), &data)
    }

    fn load_vote(dir: &Path) -> io::Result<Option<Vote>> {
        let path = Self::vote_path(dir);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path)?;
        let vote: Vote = serde_json::from_slice(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(vote))
    }

    fn persist_committed(dir: &Path, committed: &Option<LogId>) -> io::Result<()> {
        let data = serde_json::to_vec_pretty(committed)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Self::atomic_write(&Self::committed_path(dir), &data)
    }

    fn load_committed(dir: &Path) -> io::Result<Option<LogId>> {
        let path = Self::committed_path(dir);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path)?;
        let committed: Option<LogId> = serde_json::from_slice(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(committed)
    }

    fn persist_purged(dir: &Path, purged: &LogId) -> io::Result<()> {
        let data = serde_json::to_vec_pretty(purged)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Self::atomic_write(&Self::purged_path(dir), &data)
    }

    fn load_purged(dir: &Path) -> io::Result<Option<LogId>> {
        let path = Self::purged_path(dir);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path)?;
        let purged: LogId = serde_json::from_slice(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(purged))
    }

    fn persist_truncated_after(dir: &Path, lid: &Option<LogId>) -> io::Result<()> {
        let data = serde_json::to_vec_pretty(lid)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Self::atomic_write(&Self::truncated_after_path(dir), &data)
    }

    fn load_truncated_after(dir: &Path) -> io::Result<Option<LogId>> {
        let path = Self::truncated_after_path(dir);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path)?;
        let lid: Option<LogId> = serde_json::from_slice(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(lid)
    }

    // --- Internal append (for IOFlushed callback pattern) ---

    async fn append_inner<I>(&self, entries: I) -> Result<(), io::Error>
    where
        I: IntoIterator<Item = Entry> + Send,
        I::IntoIter: Send,
    {
        // Collect all entries and serialize them first, then write ALL to
        // WAL before touching the BTreeMap. This prevents a partial batch
        // leaving the in-memory map inconsistent with WAL on failure (#11).
        let batch: Vec<(u64, u64, Vec<u8>, Entry)> = entries
            .into_iter()
            .map(|entry| {
                let index = entry.log_id.index;
                let term = entry.log_id.leader_id.term;
                let data = serde_json::to_vec(&entry)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok((index, term, data, entry))
            })
            .collect::<Result<Vec<_>, io::Error>>()?;

        // Write ALL to WAL first
        for (index, term, ref data, _) in &batch {
            self.wal
                .append(WalEntryKind::RaftEntry { index: *index, term: *term, data: data.clone() })
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        }

        // Only update BTreeMap after all WAL writes succeed
        let mut log = self.log.write().await;
        for (index, _, _, entry) in batch {
            log.insert(index, entry);
        }

        Ok(())
    }

    // --- Legacy convenience methods ---

    pub async fn log_length(&self) -> u64 {
        let log = self.log.read().await;
        log.len() as u64
    }

    pub async fn last_log_id(&self) -> Option<LogId> {
        let log = self.log.read().await;
        log.values().last().map(|e| e.log_id.clone())
    }
}

// ============================================================================
// RaftLogReader implementation
// ============================================================================

impl RaftLogReader<C> for Arc<CortexLogStore> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + Send>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry>, io::Error> {
        let log = self.log.read().await;
        let entries: Vec<Entry> = log.range(range).map(|(_, e)| e.clone()).collect();
        Ok(entries)
    }

    async fn read_vote(&mut self) -> Result<Option<Vote>, io::Error> {
        let v = self.vote.read().await;
        Ok(v.clone())
    }
}

// ============================================================================
// RaftLogStorage implementation
// ============================================================================

impl RaftLogStorage<C> for Arc<CortexLogStore> {
    type LogReader = Arc<CortexLogStore>;

    async fn get_log_state(&mut self) -> Result<LogState<C>, io::Error> {
        // Hold both locks simultaneously to avoid TOCTOU race
        let log = self.log.read().await;
        let purged = self.purged_log_id.read().await;

        let last_log_id = log
            .values()
            .last()
            .map(|e| e.log_id.clone())
            .or_else(|| purged.clone());

        Ok(LogState {
            last_purged_log_id: purged.clone(),
            last_log_id,
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        Arc::clone(self)
    }

    async fn save_vote(&mut self, vote: &Vote) -> Result<(), io::Error> {
        CortexLogStore::persist_vote(&self.wal_dir, vote)?;
        let mut v = self.vote.write().await;
        *v = Some(vote.clone());
        Ok(())
    }

    async fn save_committed(&mut self, committed: Option<LogId>) -> Result<(), io::Error> {
        CortexLogStore::persist_committed(&self.wal_dir, &committed)?;
        let mut c = self.committed.write().await;
        *c = committed;
        Ok(())
    }

    async fn read_committed(&mut self) -> Result<Option<LogId>, io::Error> {
        let c = self.committed.read().await;
        Ok(c.clone())
    }

    async fn append<I>(&mut self, entries: I, callback: IOFlushed<C>) -> Result<(), io::Error>
    where
        I: IntoIterator<Item = Entry> + Send,
        I::IntoIter: Send,
    {
        // Always invoke the callback, even on error, to prevent openraft hangs.
        let result = self.append_inner(entries).await;
        callback.io_completed(result.as_ref().map(|_| ()).map_err(|e| {
            io::Error::new(e.kind(), e.to_string())
        }));
        result
    }

    async fn truncate_after(&mut self, last_log_id: Option<LogId>) -> Result<(), io::Error> {
        let mut log = self.log.write().await;

        match last_log_id {
            Some(ref lid) => {
                let keys_to_remove: Vec<u64> =
                    log.range((lid.index + 1)..).map(|(k, _)| *k).collect();
                for k in keys_to_remove {
                    log.remove(&k);
                }
            }
            None => {
                log.clear();
            }
        }

        // Persist truncation boundary so recovery filters out stale entries
        let mut trunc = self.truncated_after.write().await;
        *trunc = last_log_id.clone();
        CortexLogStore::persist_truncated_after(&self.wal_dir, &last_log_id)?;

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId) -> Result<(), io::Error> {
        let mut log = self.log.write().await;

        let keys_to_remove: Vec<u64> = log
            .range(..=log_id.index)
            .map(|(k, _)| *k)
            .collect();
        for k in keys_to_remove {
            log.remove(&k);
        }

        // Persist purge boundary
        let mut purged = self.purged_log_id.write().await;
        *purged = Some(log_id.clone());
        CortexLogStore::persist_purged(&self.wal_dir, &log_id)?;

        // Clean up old WAL segments that are entirely below the purge point
        let _ = self.wal.truncate_before(log_id.index);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openraft::entry::RaftEntry;
    use openraft::vote::leader_id_adv::CommittedLeaderId;
    use openraft::vote::RaftLeaderId;

    fn make_entry(index: u64, term: u64) -> Entry {
        Entry::new_blank(openraft::LogId::new(
            CommittedLeaderId::new(term, 0),
            index,
        ))
    }

    #[tokio::test]
    async fn test_log_store_open_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = CortexLogStore::open(dir.path()).unwrap();
        let store = Arc::new(store);

        let mut reader = store.clone();
        assert!(reader.read_vote().await.unwrap().is_none());
        assert_eq!(store.log_length().await, 0);
    }

    #[tokio::test]
    async fn test_append_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
        let mut store_mut = store.clone();

        let entries = vec![make_entry(1, 1), make_entry(2, 1), make_entry(3, 1)];

        store_mut
            .append(entries, IOFlushed::noop())
            .await
            .unwrap();

        let mut reader = store.clone();
        let result = reader.try_get_log_entries(1..4).await.unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].log_id.index, 1);
        assert_eq!(result[2].log_id.index, 3);
    }

    #[tokio::test]
    async fn test_vote_persistence() {
        let dir = tempfile::tempdir().unwrap();

        // Write vote
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut store_mut = store.clone();
            let vote = openraft::Vote::new(1, 0);
            store_mut.save_vote(&vote).await.unwrap();
        }

        // Reopen and verify
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut reader = store.clone();
            let vote = reader.read_vote().await.unwrap();
            assert!(vote.is_some());
            assert_eq!(vote.unwrap().leader_id().term, 1);
        }
    }

    #[tokio::test]
    async fn test_truncate_after() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
        let mut store_mut = store.clone();

        let entries = vec![
            make_entry(1, 1),
            make_entry(2, 1),
            make_entry(3, 1),
            make_entry(4, 1),
        ];
        store_mut
            .append(entries, IOFlushed::noop())
            .await
            .unwrap();

        // Truncate after index 2
        let lid = openraft::LogId::new(CommittedLeaderId::new(1, 0), 2);
        store_mut.truncate_after(Some(lid)).await.unwrap();

        let mut reader = store.clone();
        let result = reader.try_get_log_entries(1..5).await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_truncate_survives_restart() {
        let dir = tempfile::tempdir().unwrap();

        // Write entries and truncate
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut store_mut = store.clone();

            let entries = vec![
                make_entry(1, 1),
                make_entry(2, 1),
                make_entry(3, 1),
                make_entry(4, 1),
            ];
            store_mut
                .append(entries, IOFlushed::noop())
                .await
                .unwrap();

            let lid = openraft::LogId::new(CommittedLeaderId::new(1, 0), 2);
            store_mut.truncate_after(Some(lid)).await.unwrap();
        }

        // Reopen — truncated entries must NOT reappear
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut reader = store.clone();
            let result = reader.try_get_log_entries(1..5).await.unwrap();
            assert_eq!(result.len(), 2, "truncated entries must not survive restart");
        }
    }

    #[tokio::test]
    async fn test_purge() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
        let mut store_mut = store.clone();

        let entries = vec![
            make_entry(1, 1),
            make_entry(2, 1),
            make_entry(3, 1),
        ];
        store_mut
            .append(entries, IOFlushed::noop())
            .await
            .unwrap();

        let purge_id = openraft::LogId::new(CommittedLeaderId::new(1, 0), 2);
        store_mut.purge(purge_id).await.unwrap();

        let mut reader = store.clone();
        let result = reader.try_get_log_entries(1..4).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].log_id.index, 3);
    }

    #[tokio::test]
    async fn test_purge_survives_restart() {
        let dir = tempfile::tempdir().unwrap();

        // Write entries and purge
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut store_mut = store.clone();

            let entries = vec![
                make_entry(1, 1),
                make_entry(2, 1),
                make_entry(3, 1),
            ];
            store_mut
                .append(entries, IOFlushed::noop())
                .await
                .unwrap();

            let purge_id = openraft::LogId::new(CommittedLeaderId::new(1, 0), 2);
            store_mut.purge(purge_id).await.unwrap();
        }

        // Reopen — purged entries must NOT reappear
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut reader = store.clone();
            let result = reader.try_get_log_entries(1..4).await.unwrap();
            assert_eq!(result.len(), 1, "purged entries must not survive restart");
            assert_eq!(result[0].log_id.index, 3);

            // purged_log_id should also be restored
            let mut store_mut = store.clone();
            let state = store_mut.get_log_state().await.unwrap();
            assert!(state.last_purged_log_id.is_some());
            assert_eq!(state.last_purged_log_id.unwrap().index, 2);
        }
    }

    #[tokio::test]
    async fn test_reopen_recovery() {
        let dir = tempfile::tempdir().unwrap();

        // Write entries
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut store_mut = store.clone();

            let entries = vec![make_entry(1, 1), make_entry(2, 1)];
            store_mut
                .append(entries, IOFlushed::noop())
                .await
                .unwrap();
        }

        // Reopen and verify entries are recovered
        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut reader = store.clone();
            let result = reader.try_get_log_entries(1..3).await.unwrap();
            assert_eq!(result.len(), 2);
            assert_eq!(result[0].log_id.index, 1);
        }
    }

    #[tokio::test]
    async fn test_committed_persistence() {
        let dir = tempfile::tempdir().unwrap();

        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut store_mut = store.clone();
            let lid = openraft::LogId::new(CommittedLeaderId::new(1, 0), 5);
            store_mut.save_committed(Some(lid)).await.unwrap();
        }

        {
            let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
            let mut store_mut = store.clone();
            let committed = store_mut.read_committed().await.unwrap();
            assert!(committed.is_some());
            assert_eq!(committed.unwrap().index, 5);
        }
    }

    #[tokio::test]
    async fn test_get_log_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(CortexLogStore::open(dir.path()).unwrap());
        let mut store_mut = store.clone();

        let entries = vec![make_entry(1, 1), make_entry(2, 1)];
        store_mut
            .append(entries, IOFlushed::noop())
            .await
            .unwrap();

        let state = store_mut.get_log_state().await.unwrap();
        assert!(state.last_purged_log_id.is_none());
        assert_eq!(state.last_log_id.unwrap().index, 2);
    }

    #[tokio::test]
    async fn test_atomic_write_persists() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("test_atomic.json");
        CortexLogStore::atomic_write(&target, b"hello world").unwrap();
        let data = std::fs::read(&target).unwrap();
        assert_eq!(data, b"hello world");
        // tmp file should not exist
        assert!(!target.with_extension("tmp").exists());
    }
}
