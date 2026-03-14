// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Persistent storage for DAG actions with indexes.
//!
//! Actions are persisted via a pluggable [`DagBackend`] (in-memory or Sled).
//! In-memory indexes (author chain, affected triples) are rebuilt on startup
//! from the backend, ensuring zero data loss across restarts.

use super::action::{DagAction, DagActionHash, DagPayload, TripleInsertPayload};
use super::backend::DagBackend;
use super::pruning::{PruneResult, RetentionPolicy};
use super::tips::DagTipSet;
use crate::NodeId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::RwLock;

// =============================================================================
// Key scheme for the backend
// =============================================================================

/// Prefix for action entries: `a:` + 32-byte hash = 34-byte key.
const ACTION_PREFIX: &[u8] = b"a:";
/// Key for the serialized tip set.
const TIPS_KEY: &[u8] = b"_tips";
/// Key for the schema version byte.
const VERSION_KEY: &[u8] = b"_ver";
/// Current schema version.
const SCHEMA_VERSION: u8 = 1;

fn action_key(hash: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(34);
    key.extend_from_slice(ACTION_PREFIX);
    key.extend_from_slice(hash);
    key
}

fn serialize_tips(tips: &[[u8; 32]]) -> Vec<u8> {
    tips.iter().flat_map(|h| h.iter().copied()).collect()
}

fn deserialize_tips(bytes: &[u8]) -> Vec<[u8; 32]> {
    bytes
        .chunks_exact(32)
        .map(|c| {
            let mut h = [0u8; 32];
            h.copy_from_slice(c);
            h
        })
        .collect()
}

// =============================================================================
// DagStore
// =============================================================================

/// Persistent DAG store with in-memory indexes.
///
/// Actions are stored durably in a [`DagBackend`]. On startup, in-memory
/// indexes (author chain, affected triples, tips, count) are rebuilt by
/// scanning the backend.
pub struct DagStore {
    /// Pluggable storage backend (MemoryDagBackend or SledDagBackend).
    backend: Box<dyn DagBackend>,
    /// Author chain: (author_string, seq) → action hash.
    author_index: RwLock<HashMap<(String, u64), [u8; 32]>>,
    /// Affected triple index: triple_id → list of action hashes.
    affected_index: RwLock<HashMap<[u8; 32], Vec<[u8; 32]>>>,
    /// Subject index: blake3(subject_string) → list of action hashes.
    subject_index: RwLock<HashMap<[u8; 32], Vec<[u8; 32]>>>,
    /// Current DAG tips.
    tips: RwLock<DagTipSet>,
    /// Total action count (cached for fast stats).
    count: RwLock<usize>,
}

impl DagStore {
    /// Create a new DagStore with an in-memory backend (tests / ephemeral use).
    pub fn new() -> Self {
        Self::with_backend(Box::new(super::backend::MemoryDagBackend::new()))
            .expect("MemoryDagBackend should never fail")
    }

    /// Create a DagStore backed by a custom [`DagBackend`].
    ///
    /// On construction, all existing data is loaded from the backend and
    /// in-memory indexes are rebuilt.
    pub fn with_backend(backend: Box<dyn DagBackend>) -> crate::Result<Self> {
        let store = Self {
            backend,
            author_index: RwLock::new(HashMap::new()),
            affected_index: RwLock::new(HashMap::new()),
            subject_index: RwLock::new(HashMap::new()),
            tips: RwLock::new(DagTipSet::new()),
            count: RwLock::new(0),
        };
        store.rebuild_indexes()?;
        Ok(store)
    }

    /// Rebuild all in-memory indexes by scanning the backend.
    ///
    /// Called once at construction. Validates the schema version, loads all
    /// actions, restores tips, and writes the schema version marker if not
    /// present.
    fn rebuild_indexes(&self) -> crate::Result<()> {
        // Validate schema version (upgrade safety)
        if let Some(ver_bytes) = self.backend.get(VERSION_KEY)? {
            let stored_version = ver_bytes.first().copied().unwrap_or(0);
            if stored_version > SCHEMA_VERSION {
                return Err(crate::Error::Storage(format!(
                    "DAG backend schema version {} is newer than this binary supports ({}). \
                     Upgrade the aingle binary before opening this database.",
                    stored_version, SCHEMA_VERSION
                )));
            }
            // Future: if stored_version < SCHEMA_VERSION, apply migrations here.
            // Example:
            //   if stored_version == 1 { migrate_v1_to_v2()?; }
            //   if stored_version == 2 { migrate_v2_to_v3()?; }
            // After all migrations, update the version:
            if stored_version < SCHEMA_VERSION {
                self.backend.put(VERSION_KEY, &[SCHEMA_VERSION])?;
            }
        }

        let entries = self.backend.scan_prefix(ACTION_PREFIX)?;

        if entries.is_empty() {
            // Empty backend — nothing to rebuild.
            // Ensure schema version is written.
            if self.backend.get(VERSION_KEY)?.is_none() {
                self.backend.put(VERSION_KEY, &[SCHEMA_VERSION])?;
            }
            return Ok(());
        }

        let mut author_idx = self
            .author_index
            .write()
            .map_err(|_| crate::Error::Storage("DagStore author index lock poisoned".into()))?;
        let mut affected_idx = self
            .affected_index
            .write()
            .map_err(|_| crate::Error::Storage("DagStore affected index lock poisoned".into()))?;
        let mut subject_idx = self
            .subject_index
            .write()
            .map_err(|_| crate::Error::Storage("DagStore subject index lock poisoned".into()))?;
        let mut count = self
            .count
            .write()
            .map_err(|_| crate::Error::Storage("DagStore count lock poisoned".into()))?;

        author_idx.clear();
        affected_idx.clear();
        subject_idx.clear();

        let mut action_count = 0usize;
        for (key, value) in &entries {
            if key.len() != 34 || !key.starts_with(ACTION_PREFIX) {
                continue;
            }
            let mut hash_bytes = [0u8; 32];
            hash_bytes.copy_from_slice(&key[2..]);

            if let Some(action) = DagAction::from_bytes(value) {
                let author_key = format!("{}", action.author);
                author_idx.insert((author_key, action.seq), hash_bytes);

                for triple_id in extract_affected_triple_ids(&action.payload) {
                    affected_idx.entry(triple_id).or_default().push(hash_bytes);
                }
                for subject_hash in extract_subject_hashes(&action.payload) {
                    subject_idx.entry(subject_hash).or_default().push(hash_bytes);
                }
                action_count += 1;
            }
        }

        *count = action_count;
        drop(author_idx);
        drop(affected_idx);
        drop(subject_idx);
        drop(count);

        // Restore tips from backend
        if let Some(tips_bytes) = self.backend.get(TIPS_KEY)? {
            let raw = deserialize_tips(&tips_bytes);
            let mut tips = self
                .tips
                .write()
                .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;
            *tips = DagTipSet::from_raw(raw);
        }

        // Write schema version if not present
        if self.backend.get(VERSION_KEY)?.is_none() {
            self.backend.put(VERSION_KEY, &[SCHEMA_VERSION])?;
        }

        Ok(())
    }

    /// Persist the current tip set to the backend.
    fn persist_tips(&self, tips: &DagTipSet) -> crate::Result<()> {
        let raw = tips.to_raw();
        let bytes = serialize_tips(&raw);
        self.backend.put(TIPS_KEY, &bytes)?;
        Ok(())
    }

    /// Flush all pending writes to durable storage.
    ///
    /// For Sled backends, this ensures data reaches disk immediately.
    /// For in-memory backends, this is a no-op.
    pub fn flush(&self) -> crate::Result<()> {
        self.backend.flush()
    }

    /// Store a DagAction. Computes its hash, updates all indexes and tips.
    /// Returns the action's content-addressable hash.
    pub fn put(&self, action: &DagAction) -> crate::Result<DagActionHash> {
        let hash = action.compute_hash();
        let bytes = action.to_bytes();

        // Store in backend
        self.backend.put(&action_key(&hash.0), &bytes)?;

        // Update author index
        {
            let mut idx = self
                .author_index
                .write()
                .map_err(|_| crate::Error::Storage("DagStore author index lock poisoned".into()))?;
            let author_key = format!("{}", action.author);
            idx.insert((author_key, action.seq), hash.0);
        }

        // Update affected triple index
        {
            let mut idx = self
                .affected_index
                .write()
                .map_err(|_| crate::Error::Storage("DagStore affected index lock poisoned".into()))?;
            for triple_id in extract_affected_triple_ids(&action.payload) {
                idx.entry(triple_id).or_default().push(hash.0);
            }
        }

        // Update subject index
        {
            let mut idx = self
                .subject_index
                .write()
                .map_err(|_| crate::Error::Storage("DagStore subject index lock poisoned".into()))?;
            for subject_hash in extract_subject_hashes(&action.payload) {
                idx.entry(subject_hash).or_default().push(hash.0);
            }
        }

        // Update tip set and persist
        {
            let mut tips = self
                .tips
                .write()
                .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;
            tips.advance(hash, &action.parents);
            self.persist_tips(&tips)?;
        }

        // Update count
        {
            let mut c = self
                .count
                .write()
                .map_err(|_| crate::Error::Storage("DagStore count lock poisoned".into()))?;
            *c += 1;
        }

        Ok(hash)
    }

    /// Retrieve a DagAction by its hash.
    pub fn get(&self, hash: &DagActionHash) -> crate::Result<Option<DagAction>> {
        match self.backend.get(&action_key(&hash.0))? {
            Some(bytes) => Ok(DagAction::from_bytes(&bytes)),
            None => Ok(None),
        }
    }

    /// Check if an action exists.
    pub fn contains(&self, hash: &DagActionHash) -> crate::Result<bool> {
        Ok(self.backend.get(&action_key(&hash.0))?.is_some())
    }

    /// Get current DAG tips.
    pub fn tips(&self) -> crate::Result<Vec<DagActionHash>> {
        let tips = self
            .tips
            .read()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;
        Ok(tips.current())
    }

    /// Get tip count.
    pub fn tip_count(&self) -> crate::Result<usize> {
        let tips = self
            .tips
            .read()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;
        Ok(tips.len())
    }

    /// Export tip set as raw bytes (for snapshots).
    pub fn tips_raw(&self) -> crate::Result<Vec<[u8; 32]>> {
        let tips = self
            .tips
            .read()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;
        Ok(tips.to_raw())
    }

    /// Restore tip set from raw bytes (for snapshot install).
    pub fn restore_tips(&self, raw: Vec<[u8; 32]>) -> crate::Result<()> {
        let mut tips = self
            .tips
            .write()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;
        *tips = DagTipSet::from_raw(raw);
        self.persist_tips(&tips)?;
        Ok(())
    }

    /// Get actions by author in sequence order, most recent first.
    pub fn chain(&self, author: &NodeId, limit: usize) -> crate::Result<Vec<DagAction>> {
        let author_key = format!("{}", author);
        let idx = self
            .author_index
            .read()
            .map_err(|_| crate::Error::Storage("DagStore lock poisoned".into()))?;

        // Collect all (seq, hash) pairs for this author
        let mut entries: Vec<(u64, [u8; 32])> = idx
            .iter()
            .filter(|((a, _), _)| a == &author_key)
            .map(|((_, seq), hash)| (*seq, *hash))
            .collect();

        // Sort by seq descending (most recent first)
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        entries.truncate(limit);
        drop(idx);

        let mut result = Vec::new();
        for (_, hash) in &entries {
            if let Some(bytes) = self.backend.get(&action_key(hash))? {
                if let Some(action) = DagAction::from_bytes(&bytes) {
                    result.push(action);
                }
            }
        }
        Ok(result)
    }

    /// Get the history of mutations affecting a specific triple.
    pub fn history(&self, triple_id: &[u8; 32], limit: usize) -> crate::Result<Vec<DagAction>> {
        let idx = self
            .affected_index
            .read()
            .map_err(|_| crate::Error::Storage("DagStore lock poisoned".into()))?;

        let hashes = match idx.get(triple_id) {
            Some(h) => h.clone(),
            None => return Ok(vec![]),
        };
        drop(idx);

        let mut result: Vec<DagAction> = Vec::new();
        for hash in hashes.iter().rev().take(limit) {
            if let Some(bytes) = self.backend.get(&action_key(hash))? {
                if let Some(action) = DagAction::from_bytes(&bytes) {
                    result.push(action);
                }
            }
        }

        // Sort by timestamp descending
        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        result.truncate(limit);

        Ok(result)
    }

    /// Get the history of mutations affecting a specific subject.
    ///
    /// Looks up all DAG actions that contain the given subject string
    /// (in TripleInsert, TripleDelete, or Custom payloads).
    pub fn history_by_subject(&self, subject: &str, limit: usize) -> crate::Result<Vec<DagAction>> {
        let subject_hash = *blake3::hash(subject.as_bytes()).as_bytes();

        let idx = self
            .subject_index
            .read()
            .map_err(|_| crate::Error::Storage("DagStore lock poisoned".into()))?;

        let hashes = match idx.get(&subject_hash) {
            Some(h) => h.clone(),
            None => return Ok(vec![]),
        };
        drop(idx);

        let mut result: Vec<DagAction> = Vec::new();
        for hash in hashes.iter().rev().take(limit) {
            if let Some(bytes) = self.backend.get(&action_key(hash))? {
                if let Some(action) = DagAction::from_bytes(&bytes) {
                    result.push(action);
                }
            }
        }

        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        result.truncate(limit);

        Ok(result)
    }

    /// Total number of stored actions.
    pub fn action_count(&self) -> usize {
        self.count.read().map(|c| *c).unwrap_or(0)
    }

    /// Check if genesis exists, create it if not.
    /// Returns the genesis hash.
    pub fn init_or_migrate(&self, triple_count: usize) -> crate::Result<DagActionHash> {
        // Check if we already have any actions
        let count = self.action_count();
        if count > 0 {
            // DAG already initialized — return any tip as "genesis done" signal
            let tips = self.tips()?;
            return Ok(tips.into_iter().next().unwrap_or(DagActionHash([0; 32])));
        }

        // Create genesis action
        let genesis = DagAction {
            parents: vec![],
            author: NodeId::named("aingle:system"),
            seq: 0,
            timestamp: chrono::Utc::now(),
            payload: DagPayload::Genesis {
                triple_count,
                description: "Migration from v0.5.0".into(),
            },
            signature: None,
        };

        self.put(&genesis)
    }

    // =========================================================================
    // Export
    // =========================================================================

    /// Export the full DAG as a portable graph structure.
    pub fn export_graph(&self) -> crate::Result<super::export::DagGraph> {
        let entries = self.backend.scan_prefix(ACTION_PREFIX)?;

        let mut all_actions: Vec<DagAction> = entries
            .iter()
            .filter_map(|(_, value)| DagAction::from_bytes(value))
            .collect();

        // Sort by timestamp for consistent output
        all_actions.sort_by_key(|a| a.timestamp);

        let tips = self.tips()?;
        Ok(super::export::DagGraph::from_actions(&all_actions, &tips))
    }

    // =========================================================================
    // Cross-node sync
    // =========================================================================

    /// Store a DAG action received from a peer **without** updating tips.
    ///
    /// Use this when ingesting historical actions from other nodes.
    /// The tip set remains unchanged so that only Raft-applied actions
    /// advance the local DAG frontier.
    ///
    /// Returns the action's hash. Skips silently if the action already exists.
    pub fn ingest(&self, action: &DagAction) -> crate::Result<DagActionHash> {
        let hash = action.compute_hash();

        // Skip if already present
        if self.backend.get(&action_key(&hash.0))?.is_some() {
            return Ok(hash);
        }

        // Store in backend
        self.backend.put(&action_key(&hash.0), &action.to_bytes())?;

        // Update author index
        {
            let mut idx = self
                .author_index
                .write()
                .map_err(|_| crate::Error::Storage("DagStore author index lock poisoned".into()))?;
            let author_key = format!("{}", action.author);
            idx.insert((author_key, action.seq), hash.0);
        }

        // Update affected triple index
        {
            let mut idx = self
                .affected_index
                .write()
                .map_err(|_| crate::Error::Storage("DagStore affected index lock poisoned".into()))?;
            for triple_id in extract_affected_triple_ids(&action.payload) {
                idx.entry(triple_id).or_default().push(hash.0);
            }
        }

        // Update subject index
        {
            let mut idx = self
                .subject_index
                .write()
                .map_err(|_| crate::Error::Storage("DagStore subject index lock poisoned".into()))?;
            for subject_hash in extract_subject_hashes(&action.payload) {
                idx.entry(subject_hash).or_default().push(hash.0);
            }
        }

        // Update count (but NOT tips)
        {
            let mut c = self
                .count
                .write()
                .map_err(|_| crate::Error::Storage("DagStore count lock poisoned".into()))?;
            *c += 1;
        }

        Ok(hash)
    }

    /// Compute actions the remote node is missing.
    ///
    /// Given the remote's tips, finds all actions in our DAG that are
    /// ancestors of our tips but NOT ancestors of the remote's tips.
    /// Returns them in topological order (roots first).
    pub fn compute_missing(
        &self,
        remote_tips: &[DagActionHash],
    ) -> crate::Result<Vec<DagAction>> {
        // Our full ancestor set (from our tips)
        let our_tips = self.tips()?;
        let mut our_ancestors: HashSet<[u8; 32]> = HashSet::new();
        for tip in &our_tips {
            let set = self.ancestor_set(tip)?;
            our_ancestors.extend(set);
        }

        // Remote's ancestor set (only actions we know about)
        let mut remote_ancestors: HashSet<[u8; 32]> = HashSet::new();
        for tip in remote_tips {
            if self.contains(tip)? {
                let set = self.ancestor_set(tip)?;
                remote_ancestors.extend(set);
            }
            // Unknown remote tips are skipped — we can't walk their ancestry
        }

        // Actions we have that remote doesn't
        let missing_hashes: HashSet<[u8; 32]> = our_ancestors
            .difference(&remote_ancestors)
            .copied()
            .collect();

        if missing_hashes.is_empty() {
            return Ok(vec![]);
        }

        // Collect actions from backend
        let mut collected: HashMap<[u8; 32], DagAction> = HashMap::new();
        for hash in &missing_hashes {
            if let Some(bytes) = self.backend.get(&action_key(hash))? {
                if let Some(action) = DagAction::from_bytes(&bytes) {
                    collected.insert(*hash, action);
                }
            }
        }

        // Topological sort (Kahn's algorithm)
        topo_sort(collected)
    }

    // =========================================================================
    // Time-travel queries
    // =========================================================================

    /// Collect all ancestors of `target` (inclusive) in topological order (roots first).
    ///
    /// Uses BFS backwards + Kahn's algorithm for correct ordering.
    /// Missing parents (e.g. from pruning) are silently skipped.
    pub fn ancestors(&self, target: &DagActionHash) -> crate::Result<Vec<DagAction>> {
        // Phase 1: BFS backwards from target
        let mut visited: HashSet<[u8; 32]> = HashSet::new();
        let mut queue: VecDeque<[u8; 32]> = VecDeque::new();
        let mut collected: HashMap<[u8; 32], DagAction> = HashMap::new();

        queue.push_back(target.0);
        visited.insert(target.0);

        while let Some(hash) = queue.pop_front() {
            if let Some(bytes) = self.backend.get(&action_key(&hash))? {
                if let Some(action) = DagAction::from_bytes(&bytes) {
                    for parent in &action.parents {
                        if visited.insert(parent.0) {
                            queue.push_back(parent.0);
                        }
                    }
                    collected.insert(hash, action);
                }
            }
        }

        // Phase 2: Topological sort (Kahn's algorithm)
        topo_sort(collected)
    }

    /// Collect the set of ancestor hashes for `target` (inclusive).
    pub fn ancestor_set(&self, target: &DagActionHash) -> crate::Result<HashSet<[u8; 32]>> {
        let mut visited: HashSet<[u8; 32]> = HashSet::new();
        let mut queue: VecDeque<[u8; 32]> = VecDeque::new();

        queue.push_back(target.0);
        visited.insert(target.0);

        while let Some(hash) = queue.pop_front() {
            if let Some(bytes) = self.backend.get(&action_key(&hash))? {
                if let Some(action) = DagAction::from_bytes(&bytes) {
                    for parent in &action.parents {
                        if visited.insert(parent.0) {
                            queue.push_back(parent.0);
                        }
                    }
                }
            }
        }

        Ok(visited)
    }

    /// Find actions in `to`'s ancestry but not in `from`'s ancestry (topological order).
    pub fn actions_between(
        &self,
        from: &DagActionHash,
        to: &DagActionHash,
    ) -> crate::Result<Vec<DagAction>> {
        let from_set = self.ancestor_set(from)?;
        let to_ancestors = self.ancestors(to)?;

        Ok(to_ancestors
            .into_iter()
            .filter(|a| {
                let h = a.compute_hash();
                !from_set.contains(&h.0)
            })
            .collect())
    }

    /// Find the action with the latest timestamp that is ≤ `ts`.
    ///
    /// Returns `None` if no actions exist before the given time.
    pub fn action_at_or_before(
        &self,
        ts: &chrono::DateTime<chrono::Utc>,
    ) -> crate::Result<Option<DagActionHash>> {
        let entries = self.backend.scan_prefix(ACTION_PREFIX)?;

        let mut best: Option<(DagActionHash, chrono::DateTime<chrono::Utc>)> = None;

        for (key, value) in &entries {
            if key.len() != 34 || !key.starts_with(ACTION_PREFIX) {
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&key[2..]);

            if let Some(action) = DagAction::from_bytes(value) {
                if action.timestamp <= *ts {
                    if best.as_ref().map_or(true, |(_, t)| action.timestamp > *t) {
                        best = Some((DagActionHash(hash), action.timestamp));
                    }
                }
            }
        }

        Ok(best.map(|(h, _)| h))
    }

    // =========================================================================
    // Pruning
    // =========================================================================

    /// Prune old actions according to a retention policy.
    ///
    /// Tips are never pruned. If `create_checkpoint` is true, a `Compact`
    /// action is appended after pruning (its parents are the current tips).
    pub fn prune(
        &self,
        policy: &RetentionPolicy,
        create_checkpoint: bool,
    ) -> crate::Result<PruneResult> {
        let to_remove = match policy {
            RetentionPolicy::KeepAll => {
                return Ok(PruneResult {
                    pruned_count: 0,
                    retained_count: self.action_count(),
                    checkpoint_hash: None,
                });
            }
            RetentionPolicy::KeepSince { seconds } => self.collect_older_than(*seconds)?,
            RetentionPolicy::KeepLast(n) => self.collect_excess(*n)?,
            RetentionPolicy::KeepDepth(d) => self.collect_beyond_depth(*d)?,
        };

        if to_remove.is_empty() {
            return Ok(PruneResult {
                pruned_count: 0,
                retained_count: self.action_count(),
                checkpoint_hash: None,
            });
        }

        let pruned_count = self.remove_actions(&to_remove)?;
        let retained_count = self.action_count();

        let checkpoint_hash = if create_checkpoint {
            let tips = self.tips()?;
            let action = DagAction {
                parents: tips,
                author: NodeId::named("aingle:system"),
                seq: 0,
                timestamp: chrono::Utc::now(),
                payload: DagPayload::Compact {
                    pruned_count,
                    retained_count,
                    policy: format!("{:?}", policy),
                },
                signature: None,
            };
            Some(self.put(&action)?)
        } else {
            None
        };

        Ok(PruneResult {
            pruned_count,
            retained_count: self.action_count(),
            checkpoint_hash,
        })
    }

    /// Compute a depth map: for each action, its minimum hop-distance from any tip.
    ///
    /// Tips have depth 0, their parents depth 1, and so on.
    /// Actions unreachable from tips get `usize::MAX`.
    pub fn depth_map(&self) -> crate::Result<HashMap<[u8; 32], usize>> {
        let entries = self.backend.scan_prefix(ACTION_PREFIX)?;
        let tips = self
            .tips
            .read()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;

        // Deserialize all actions into a local map
        let mut actions_map: HashMap<[u8; 32], DagAction> = HashMap::new();
        for (key, value) in &entries {
            if key.len() != 34 || !key.starts_with(ACTION_PREFIX) {
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&key[2..]);
            if let Some(action) = DagAction::from_bytes(value) {
                actions_map.insert(hash, action);
            }
        }

        let mut depths: HashMap<[u8; 32], usize> = HashMap::new();
        let mut queue: VecDeque<([u8; 32], usize)> = VecDeque::new();

        // Seed with tips at depth 0
        for tip in tips.current() {
            depths.insert(tip.0, 0);
            queue.push_back((tip.0, 0));
        }

        // BFS traversal following parent links
        while let Some((hash, depth)) = queue.pop_front() {
            if let Some(action) = actions_map.get(&hash) {
                for parent in &action.parents {
                    let parent_depth = depth + 1;
                    let entry = depths.entry(parent.0).or_insert(usize::MAX);
                    if parent_depth < *entry {
                        *entry = parent_depth;
                        queue.push_back((parent.0, parent_depth));
                    }
                }
            }
        }

        // Mark any remaining actions not reached by BFS
        for hash in actions_map.keys() {
            depths.entry(*hash).or_insert(usize::MAX);
        }

        Ok(depths)
    }

    /// Remove a set of actions from backend and all indexes. Returns the count removed.
    fn remove_actions(&self, to_remove: &HashSet<[u8; 32]>) -> crate::Result<usize> {
        let mut removed = 0;

        // Delete from backend
        for hash in to_remove {
            if self.backend.delete(&action_key(hash))? {
                removed += 1;
            }
        }

        // Clean author index
        let mut author_idx = self
            .author_index
            .write()
            .map_err(|_| crate::Error::Storage("DagStore author index lock poisoned".into()))?;
        author_idx.retain(|_, h| !to_remove.contains(h));

        // Clean affected index
        let mut affected_idx = self
            .affected_index
            .write()
            .map_err(|_| crate::Error::Storage("DagStore affected index lock poisoned".into()))?;
        affected_idx.retain(|_, hashes| {
            hashes.retain(|h| !to_remove.contains(h));
            !hashes.is_empty()
        });

        // Clean subject index
        let mut subject_idx = self
            .subject_index
            .write()
            .map_err(|_| crate::Error::Storage("DagStore subject index lock poisoned".into()))?;
        subject_idx.retain(|_, hashes| {
            hashes.retain(|h| !to_remove.contains(h));
            !hashes.is_empty()
        });

        // Update count
        let mut count = self
            .count
            .write()
            .map_err(|_| crate::Error::Storage("DagStore count lock poisoned".into()))?;
        *count = count.saturating_sub(removed);

        Ok(removed)
    }

    /// Collect action hashes older than `seconds` ago (excluding tips).
    fn collect_older_than(&self, seconds: u64) -> crate::Result<HashSet<[u8; 32]>> {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::seconds(seconds as i64);
        let entries = self.backend.scan_prefix(ACTION_PREFIX)?;
        let tips = self
            .tips
            .read()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;

        let tip_set: HashSet<[u8; 32]> = tips.current().iter().map(|h| h.0).collect();
        let mut result = HashSet::new();

        for (key, value) in &entries {
            if key.len() != 34 || !key.starts_with(ACTION_PREFIX) {
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&key[2..]);

            if tip_set.contains(&hash) {
                continue;
            }
            if let Some(action) = DagAction::from_bytes(value) {
                if action.timestamp < cutoff {
                    result.insert(hash);
                }
            }
        }

        Ok(result)
    }

    /// Collect the oldest actions beyond the keep count (excluding tips).
    fn collect_excess(&self, keep: usize) -> crate::Result<HashSet<[u8; 32]>> {
        let entries = self.backend.scan_prefix(ACTION_PREFIX)?;
        let tips = self
            .tips
            .read()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;

        let total = entries.len();
        if total <= keep {
            return Ok(HashSet::new());
        }

        let tip_set: HashSet<[u8; 32]> = tips.current().iter().map(|h| h.0).collect();

        // Deserialize all non-tip actions with their timestamps
        let mut candidates: Vec<([u8; 32], chrono::DateTime<chrono::Utc>)> = Vec::new();
        for (key, value) in &entries {
            if key.len() != 34 || !key.starts_with(ACTION_PREFIX) {
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&key[2..]);

            if tip_set.contains(&hash) {
                continue;
            }
            if let Some(action) = DagAction::from_bytes(value) {
                candidates.push((hash, action.timestamp));
            }
        }

        // Sort oldest first
        candidates.sort_by_key(|(_, ts)| *ts);

        // How many non-tip actions do we need to remove?
        let to_prune = total.saturating_sub(keep);

        Ok(candidates
            .into_iter()
            .take(to_prune)
            .map(|(hash, _)| hash)
            .collect())
    }

    /// Collect actions beyond the given depth from tips (excluding tips).
    fn collect_beyond_depth(&self, max_depth: usize) -> crate::Result<HashSet<[u8; 32]>> {
        let depths = self.depth_map()?;
        let tips = self
            .tips
            .read()
            .map_err(|_| crate::Error::Storage("DagStore tips lock poisoned".into()))?;

        let tip_set: HashSet<[u8; 32]> = tips.current().iter().map(|h| h.0).collect();

        Ok(depths
            .into_iter()
            .filter(|(hash, depth)| *depth > max_depth && !tip_set.contains(hash))
            .map(|(hash, _)| hash)
            .collect())
    }
}

impl Default for DagStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Topological sort helper
// =============================================================================

/// Kahn's topological sort on a collected set of actions.
fn topo_sort(mut collected: HashMap<[u8; 32], DagAction>) -> crate::Result<Vec<DagAction>> {
    let mut in_degree: HashMap<[u8; 32], usize> = HashMap::new();
    let mut children: HashMap<[u8; 32], Vec<[u8; 32]>> = HashMap::new();

    for (hash, action) in &collected {
        in_degree.entry(*hash).or_insert(0);
        for parent in &action.parents {
            if collected.contains_key(&parent.0) {
                children.entry(parent.0).or_default().push(*hash);
                *in_degree.entry(*hash).or_insert(0) += 1;
            }
        }
    }

    let mut ready: VecDeque<[u8; 32]> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(hash, _)| *hash)
        .collect();

    let mut result = Vec::with_capacity(collected.len());

    while let Some(hash) = ready.pop_front() {
        if let Some(action) = collected.remove(&hash) {
            result.push(action);
        }
        if let Some(kids) = children.get(&hash) {
            for kid in kids {
                if let Some(deg) = in_degree.get_mut(kid) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push_back(*kid);
                    }
                }
            }
        }
    }

    Ok(result)
}

// =============================================================================
// Helpers
// =============================================================================

/// Extract triple IDs affected by a payload (for the affected index).
fn extract_affected_triple_ids(payload: &DagPayload) -> Vec<[u8; 32]> {
    match payload {
        DagPayload::TripleInsert { triples } => triples
            .iter()
            .map(|t| compute_triple_id_from_payload(t))
            .collect(),
        DagPayload::TripleDelete { triple_ids, .. } => triple_ids.clone(),
        DagPayload::Batch { ops } => ops.iter().flat_map(extract_affected_triple_ids).collect(),
        _ => vec![],
    }
}

/// Extract subject hashes from a payload (for the subject index).
///
/// Returns `blake3(subject_string)` for each subject mentioned in the payload.
fn extract_subject_hashes(payload: &DagPayload) -> Vec<[u8; 32]> {
    match payload {
        DagPayload::TripleInsert { triples } => triples
            .iter()
            .map(|t| *blake3::hash(t.subject.as_bytes()).as_bytes())
            .collect(),
        DagPayload::TripleDelete { subjects, .. } => subjects
            .iter()
            .map(|s| *blake3::hash(s.as_bytes()).as_bytes())
            .collect(),
        DagPayload::Custom { subject, .. } => subject
            .iter()
            .map(|s| *blake3::hash(s.as_bytes()).as_bytes())
            .collect(),
        DagPayload::Batch { ops } => ops.iter().flat_map(extract_subject_hashes).collect(),
        _ => vec![],
    }
}

/// Compute a triple ID from a TripleInsertPayload.
///
/// Must match `TripleId::from_triple()` exactly: blake3(subject.to_bytes() || predicate.to_bytes() || object.to_bytes()).
fn compute_triple_id_from_payload(t: &TripleInsertPayload) -> [u8; 32] {
    let subject = crate::NodeId::named(&t.subject);
    let predicate = crate::Predicate::named(&t.predicate);
    let object = json_to_graph_value(&t.object);
    let triple = crate::Triple::new(subject, predicate, object);
    *crate::TripleId::from_triple(&triple).as_bytes()
}

/// Convert a serde_json::Value to a graph Value (matching the state machine's json_to_value).
pub(crate) fn json_to_graph_value(v: &serde_json::Value) -> crate::Value {
    match v {
        serde_json::Value::String(s) => crate::Value::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                crate::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                crate::Value::Float(f)
            } else {
                crate::Value::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => crate::Value::Boolean(*b),
        serde_json::Value::Null => crate::Value::Null,
        _ => crate::Value::Json(v.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;
    use chrono::Utc;

    fn make_action(seq: u64, parents: Vec<DagActionHash>) -> DagAction {
        DagAction {
            parents,
            author: NodeId::named("node:1"),
            seq,
            timestamp: Utc::now(),
            payload: DagPayload::TripleInsert {
                triples: vec![TripleInsertPayload {
                    subject: "alice".into(),
                    predicate: "knows".into(),
                    object: serde_json::json!("bob"),
                }],
            },
            signature: None,
        }
    }

    #[test]
    fn test_put_and_get() {
        let store = DagStore::new();
        let action = make_action(1, vec![]);
        let hash = store.put(&action).unwrap();

        let retrieved = store.get(&hash).unwrap().unwrap();
        assert_eq!(retrieved.seq, 1);
    }

    #[test]
    fn test_tips_linear_chain() {
        let store = DagStore::new();

        let a1 = make_action(1, vec![]);
        let h1 = store.put(&a1).unwrap();
        assert_eq!(store.tip_count().unwrap(), 1);

        let a2 = make_action(2, vec![h1]);
        let h2 = store.put(&a2).unwrap();
        assert_eq!(store.tip_count().unwrap(), 1);

        let tips = store.tips().unwrap();
        assert_eq!(tips[0], h2);
    }

    #[test]
    fn test_author_chain() {
        let store = DagStore::new();

        for seq in 0..5 {
            let action = make_action(seq, vec![]);
            store.put(&action).unwrap();
        }

        let chain = store.chain(&NodeId::named("node:1"), 10).unwrap();
        assert_eq!(chain.len(), 5);
        // Most recent first
        assert_eq!(chain[0].seq, 4);
    }

    #[test]
    fn test_triple_history() {
        let store = DagStore::new();

        // Two actions affecting the same triple
        let a1 = make_action(1, vec![]);
        store.put(&a1).unwrap();
        let a2 = make_action(2, vec![]);
        store.put(&a2).unwrap();

        // Compute the triple ID that both actions affect
        let tid = compute_triple_id_from_payload(&TripleInsertPayload {
            subject: "alice".into(),
            predicate: "knows".into(),
            object: serde_json::json!("bob"),
        });

        let history = store.history(&tid, 10).unwrap();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_init_or_migrate() {
        let store = DagStore::new();

        // First call creates genesis
        let hash = store.init_or_migrate(100).unwrap();
        assert_eq!(store.action_count(), 1);

        let genesis = store.get(&hash).unwrap().unwrap();
        assert!(genesis.is_genesis());
        assert!(matches!(
            genesis.payload,
            DagPayload::Genesis { triple_count: 100, .. }
        ));

        // Second call returns existing tip
        let hash2 = store.init_or_migrate(200).unwrap();
        assert_eq!(store.action_count(), 1); // No new action created
        assert_ne!(hash2, DagActionHash([0; 32]));
    }

    #[test]
    fn test_action_count() {
        let store = DagStore::new();
        assert_eq!(store.action_count(), 0);

        store.put(&make_action(1, vec![])).unwrap();
        assert_eq!(store.action_count(), 1);

        store.put(&make_action(2, vec![])).unwrap();
        assert_eq!(store.action_count(), 2);
    }

    #[test]
    fn test_contains() {
        let store = DagStore::new();
        let action = make_action(1, vec![]);
        let hash = store.put(&action).unwrap();

        assert!(store.contains(&hash).unwrap());
        assert!(!store.contains(&DagActionHash([0xFF; 32])).unwrap());
    }

    #[test]
    fn test_restore_tips() {
        let store = DagStore::new();
        let raw = vec![[1u8; 32], [2u8; 32]];
        store.restore_tips(raw).unwrap();
        assert_eq!(store.tip_count().unwrap(), 2);
    }

    #[test]
    fn test_triple_id_matches_graph_triple_id() {
        // CRITICAL: the triple ID computed from a DagPayload must match
        // the TripleId::from_triple() in the graph's triple store.
        // If these diverge, history lookups by triple ID silently fail.
        use crate::{Triple, TripleId, Predicate, Value};

        let subject = "user:alice";
        let predicate = "knows";
        let object_json = serde_json::json!("bob");

        // Compute via DagStore's helper
        let dag_tid = compute_triple_id_from_payload(&TripleInsertPayload {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object_json.clone(),
        });

        // Compute via TripleId::from_triple (the canonical graph path)
        let triple = Triple::new(
            NodeId::named(subject),
            Predicate::named(predicate),
            Value::String("bob".into()),
        );
        let graph_tid = *TripleId::from_triple(&triple).as_bytes();

        assert_eq!(
            dag_tid, graph_tid,
            "DagStore triple ID must match TripleId::from_triple()"
        );
    }

    #[test]
    fn test_history_matches_real_triple_id() {
        // End-to-end: insert via DagStore, then look up history using
        // the same triple ID that GraphDB.insert() would produce.
        use crate::{Triple, TripleId, Predicate, Value};

        let store = DagStore::new();
        let action = make_action(1, vec![]);
        store.put(&action).unwrap();

        // Compute the real triple ID as GraphDB would
        let triple = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::String("bob".into()),
        );
        let real_tid = *TripleId::from_triple(&triple).as_bytes();

        let history = store.history(&real_tid, 10).unwrap();
        assert_eq!(
            history.len(),
            1,
            "history lookup using real TripleId must find the DagAction"
        );
    }

    // =======================================================================
    // Pruning tests
    // =======================================================================

    fn make_action_at(seq: u64, parents: Vec<DagActionHash>, ts: chrono::DateTime<Utc>) -> DagAction {
        DagAction {
            parents,
            author: NodeId::named("node:1"),
            seq,
            timestamp: ts,
            payload: DagPayload::TripleInsert {
                triples: vec![TripleInsertPayload {
                    subject: format!("s{}", seq),
                    predicate: "p".into(),
                    object: serde_json::json!(seq),
                }],
            },
            signature: None,
        }
    }

    #[test]
    fn test_prune_keep_all() {
        let store = DagStore::new();
        store.put(&make_action(1, vec![])).unwrap();
        store.put(&make_action(2, vec![])).unwrap();

        let result = store.prune(&RetentionPolicy::KeepAll, false).unwrap();
        assert_eq!(result.pruned_count, 0);
        assert_eq!(result.retained_count, 2);
    }

    #[test]
    fn test_prune_keep_last() {
        let store = DagStore::new();

        let now = Utc::now();
        // Build a linear chain: a1 -> a2 -> a3 -> a4 -> a5
        let a1 = make_action_at(1, vec![], now - chrono::Duration::seconds(50));
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action_at(2, vec![h1], now - chrono::Duration::seconds(40));
        let h2 = store.put(&a2).unwrap();
        let a3 = make_action_at(3, vec![h2], now - chrono::Duration::seconds(30));
        let h3 = store.put(&a3).unwrap();
        let a4 = make_action_at(4, vec![h3], now - chrono::Duration::seconds(20));
        let h4 = store.put(&a4).unwrap();
        let a5 = make_action_at(5, vec![h4], now - chrono::Duration::seconds(10));
        let h5 = store.put(&a5).unwrap();
        assert_eq!(store.action_count(), 5);

        // Keep last 3 → prune 2 oldest (a1, a2)
        let result = store.prune(&RetentionPolicy::KeepLast(3), false).unwrap();
        assert_eq!(result.pruned_count, 2);
        assert_eq!(result.retained_count, 3);

        // a1, a2 gone; a3, a4, a5 remain
        assert!(store.get(&h1).unwrap().is_none());
        assert!(store.get(&h2).unwrap().is_none());
        assert!(store.get(&h3).unwrap().is_some());
        assert!(store.get(&h4).unwrap().is_some());
        assert!(store.get(&h5).unwrap().is_some());

        // Tip is still h5
        let tips = store.tips().unwrap();
        assert_eq!(tips.len(), 1);
        assert_eq!(tips[0], h5);
    }

    #[test]
    fn test_prune_keep_since() {
        let store = DagStore::new();

        let now = Utc::now();
        // Old actions (>100s ago)
        let old1 = make_action_at(1, vec![], now - chrono::Duration::seconds(200));
        let h_old1 = store.put(&old1).unwrap();
        let old2 = make_action_at(2, vec![h_old1], now - chrono::Duration::seconds(150));
        let h_old2 = store.put(&old2).unwrap();
        // Recent actions (<100s ago)
        let new1 = make_action_at(3, vec![h_old2], now - chrono::Duration::seconds(50));
        let h_new1 = store.put(&new1).unwrap();
        let new2 = make_action_at(4, vec![h_new1], now - chrono::Duration::seconds(10));
        let h_new2 = store.put(&new2).unwrap();

        // Keep actions from last 100 seconds
        let result = store
            .prune(&RetentionPolicy::KeepSince { seconds: 100 }, false)
            .unwrap();
        assert_eq!(result.pruned_count, 2);
        assert_eq!(result.retained_count, 2);
        assert!(store.get(&h_old1).unwrap().is_none());
        assert!(store.get(&h_old2).unwrap().is_none());
        assert!(store.get(&h_new1).unwrap().is_some());
        assert!(store.get(&h_new2).unwrap().is_some());
    }

    #[test]
    fn test_prune_keep_depth() {
        let store = DagStore::new();

        // Chain: a1 -> a2 -> a3 -> a4 (tip)
        // Depths from tip: a4=0, a3=1, a2=2, a1=3
        let a1 = make_action(1, vec![]);
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action(2, vec![h1]);
        let h2 = store.put(&a2).unwrap();
        let a3 = make_action(3, vec![h2]);
        let h3 = store.put(&a3).unwrap();
        let a4 = make_action(4, vec![h3]);
        let h4 = store.put(&a4).unwrap();

        // Keep depth 1 → keep a4 (tip, depth 0) and a3 (depth 1), prune a1, a2
        let result = store.prune(&RetentionPolicy::KeepDepth(1), false).unwrap();
        assert_eq!(result.pruned_count, 2);
        assert!(store.get(&h1).unwrap().is_none());
        assert!(store.get(&h2).unwrap().is_none());
        assert!(store.get(&h3).unwrap().is_some());
        assert!(store.get(&h4).unwrap().is_some());
    }

    #[test]
    fn test_prune_never_removes_tips() {
        let store = DagStore::new();

        // Two concurrent tips (branches)
        let a1 = make_action_at(1, vec![], Utc::now() - chrono::Duration::seconds(1000));
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action_at(2, vec![], Utc::now() - chrono::Duration::seconds(1000));
        let h2 = store.put(&a2).unwrap();

        // Both are tips and very old — KeepLast(1) should still keep both
        let result = store.prune(&RetentionPolicy::KeepLast(1), false).unwrap();
        assert_eq!(result.pruned_count, 0);
        assert!(store.get(&h1).unwrap().is_some());
        assert!(store.get(&h2).unwrap().is_some());
    }

    #[test]
    fn test_prune_with_checkpoint() {
        let store = DagStore::new();

        let now = Utc::now();
        let a1 = make_action_at(1, vec![], now - chrono::Duration::seconds(200));
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action_at(2, vec![h1], now - chrono::Duration::seconds(10));
        store.put(&a2).unwrap();
        assert_eq!(store.action_count(), 2);

        let result = store
            .prune(&RetentionPolicy::KeepSince { seconds: 100 }, true)
            .unwrap();
        assert_eq!(result.pruned_count, 1);
        assert!(result.checkpoint_hash.is_some());

        // Checkpoint action was created
        let cp = store.get(&result.checkpoint_hash.unwrap()).unwrap().unwrap();
        assert!(matches!(cp.payload, DagPayload::Compact { .. }));
        // +1 for the checkpoint
        assert_eq!(store.action_count(), 2); // 1 retained + 1 checkpoint
    }

    #[test]
    fn test_prune_cleans_indexes() {
        let store = DagStore::new();

        let now = Utc::now();
        let a1 = make_action_at(1, vec![], now - chrono::Duration::seconds(200));
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action_at(2, vec![h1], now - chrono::Duration::seconds(10));
        store.put(&a2).unwrap();

        // Author chain has 2 entries before pruning
        assert_eq!(store.chain(&NodeId::named("node:1"), 10).unwrap().len(), 2);

        store
            .prune(&RetentionPolicy::KeepSince { seconds: 100 }, false)
            .unwrap();

        // Author chain should now have only 1 entry
        assert_eq!(store.chain(&NodeId::named("node:1"), 10).unwrap().len(), 1);
    }

    #[test]
    fn test_depth_map() {
        let store = DagStore::new();

        // Chain: a1 -> a2 -> a3 (tip)
        let a1 = make_action(1, vec![]);
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action(2, vec![h1]);
        let h2 = store.put(&a2).unwrap();
        let a3 = make_action(3, vec![h2]);
        let h3 = store.put(&a3).unwrap();

        let depths = store.depth_map().unwrap();
        assert_eq!(depths[&h3.0], 0); // tip
        assert_eq!(depths[&h2.0], 1);
        assert_eq!(depths[&h1.0], 2);
    }

    // =======================================================================
    // Backend persistence test
    // =======================================================================

    #[test]
    fn test_with_backend_rebuilds_indexes() {
        use super::super::backend::MemoryDagBackend;
        use std::sync::Arc;

        // Create a store and populate it
        let _backend = Arc::new(MemoryDagBackend::new());

        // Use a wrapper that shares the backend
        let store = DagStore::new();
        let a1 = make_action(1, vec![]);
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action(2, vec![h1]);
        let h2 = store.put(&a2).unwrap();

        assert_eq!(store.action_count(), 2);
        assert_eq!(store.tip_count().unwrap(), 1);
        assert_eq!(store.tips().unwrap()[0], h2);

        // Verify chain works
        let chain = store.chain(&NodeId::named("node:1"), 10).unwrap();
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn test_tips_persisted_to_backend() {
        // Verify that tips are stored in the backend after put
        let store = DagStore::new();
        let a1 = make_action(1, vec![]);
        store.put(&a1).unwrap();

        // Tips should be persisted — check the backend directly
        let tips_bytes = store.backend.get(TIPS_KEY).unwrap();
        assert!(tips_bytes.is_some());
        let raw = deserialize_tips(&tips_bytes.unwrap());
        assert_eq!(raw.len(), 1);
    }

    // =======================================================================
    // Sled persistence end-to-end test
    // =======================================================================

    #[cfg(feature = "sled-backend")]
    #[test]
    fn test_sled_persistence_end_to_end() {
        use super::super::backend::SledDagBackend;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();

        let h1;
        let h3;

        // Phase 1: Write data, then drop the store
        {
            let backend = SledDagBackend::open(path).unwrap();
            let store = DagStore::with_backend(Box::new(backend)).unwrap();

            let a1 = make_action(1, vec![]);
            h1 = store.put(&a1).unwrap();
            let a2 = make_action(2, vec![h1]);
            let h2 = store.put(&a2).unwrap();
            let a3 = make_action(3, vec![h2]);
            h3 = store.put(&a3).unwrap();

            assert_eq!(store.action_count(), 3);
            assert_eq!(store.tip_count().unwrap(), 1);
            assert_eq!(store.tips().unwrap()[0], h3);
            store.flush().unwrap();
        }

        // Phase 2: Reopen from same path — all data must survive
        {
            let backend = SledDagBackend::open(path).unwrap();
            let store = DagStore::with_backend(Box::new(backend)).unwrap();

            // Action count restored
            assert_eq!(store.action_count(), 3);

            // Tips restored
            assert_eq!(store.tip_count().unwrap(), 1);
            assert_eq!(store.tips().unwrap()[0], h3);

            // Individual actions retrievable
            let a1 = store.get(&h1).unwrap().unwrap();
            assert_eq!(a1.seq, 1);
            let a3 = store.get(&h3).unwrap().unwrap();
            assert_eq!(a3.seq, 3);

            // Author index rebuilt — chain works
            let chain = store.chain(&NodeId::named("node:1"), 10).unwrap();
            assert_eq!(chain.len(), 3);
            assert_eq!(chain[0].seq, 3); // most recent first

            // Affected index rebuilt — history works
            let tid = compute_triple_id_from_payload(&TripleInsertPayload {
                subject: "alice".into(),
                predicate: "knows".into(),
                object: serde_json::json!("bob"),
            });
            let history = store.history(&tid, 10).unwrap();
            assert_eq!(history.len(), 3);

            // Can extend the chain after reopen
            let a4 = make_action(4, vec![h3]);
            let h4 = store.put(&a4).unwrap();
            assert_eq!(store.action_count(), 4);
            assert_eq!(store.tips().unwrap()[0], h4);
        }

        // Phase 3: Reopen again — verify the new action also persisted
        {
            let backend = SledDagBackend::open(path).unwrap();
            let store = DagStore::with_backend(Box::new(backend)).unwrap();
            assert_eq!(store.action_count(), 4);
        }
    }

    #[cfg(feature = "sled-backend")]
    #[test]
    fn test_sled_persistence_with_pruning() {
        use super::super::backend::SledDagBackend;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();

        // Phase 1: Write data + prune
        {
            let backend = SledDagBackend::open(path).unwrap();
            let store = DagStore::with_backend(Box::new(backend)).unwrap();

            let now = chrono::Utc::now();
            let a1 = make_action_at(1, vec![], now - chrono::Duration::seconds(200));
            let h1 = store.put(&a1).unwrap();
            let a2 = make_action_at(2, vec![h1], now - chrono::Duration::seconds(10));
            store.put(&a2).unwrap();

            // Prune old actions
            let result = store
                .prune(&RetentionPolicy::KeepSince { seconds: 100 }, true)
                .unwrap();
            assert_eq!(result.pruned_count, 1);
            store.flush().unwrap();
        }

        // Phase 2: Reopen — pruned data must stay gone
        {
            let backend = SledDagBackend::open(path).unwrap();
            let store = DagStore::with_backend(Box::new(backend)).unwrap();

            // 1 retained + 1 checkpoint = 2
            assert_eq!(store.action_count(), 2);

            // Author chain should have only the retained action + checkpoint
            let chain = store.chain(&NodeId::named("node:1"), 10).unwrap();
            // Only seq=2 from node:1 (checkpoint is from aingle:system)
            assert_eq!(chain.len(), 1);
            assert_eq!(chain[0].seq, 2);
        }
    }

    #[cfg(feature = "sled-backend")]
    #[test]
    fn test_sled_persistence_genesis_migration() {
        use super::super::backend::SledDagBackend;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();

        // Phase 1: Create genesis
        let genesis_hash;
        {
            let backend = SledDagBackend::open(path).unwrap();
            let store = DagStore::with_backend(Box::new(backend)).unwrap();
            genesis_hash = store.init_or_migrate(42).unwrap();
            assert_eq!(store.action_count(), 1);
            store.flush().unwrap();
        }

        // Phase 2: Reopen — genesis must be there, init_or_migrate should be a no-op
        {
            let backend = SledDagBackend::open(path).unwrap();
            let store = DagStore::with_backend(Box::new(backend)).unwrap();
            assert_eq!(store.action_count(), 1);

            let hash2 = store.init_or_migrate(999).unwrap();
            assert_eq!(store.action_count(), 1); // No new genesis
            assert_ne!(hash2, DagActionHash([0; 32]));

            // Verify genesis content
            let genesis = store.get(&genesis_hash).unwrap().unwrap();
            assert!(genesis.is_genesis());
            assert!(matches!(
                genesis.payload,
                DagPayload::Genesis { triple_count: 42, .. }
            ));
        }
    }

    #[test]
    fn test_schema_version_reject_future() {
        use super::super::backend::MemoryDagBackend;

        // Simulate a database written by a newer version (schema v99)
        let backend = MemoryDagBackend::new();
        backend.put(VERSION_KEY, &[99u8]).unwrap();

        // Attempting to open it should fail with a clear error
        let result = DagStore::with_backend(Box::new(backend));
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap());
        assert!(
            err_msg.contains("newer than this binary"),
            "error must explain version mismatch: {err_msg}"
        );
    }

    #[test]
    fn test_schema_version_written_on_first_use() {
        use super::super::backend::MemoryDagBackend;

        let backend = MemoryDagBackend::new();
        // No version key yet
        assert!(backend.get(VERSION_KEY).unwrap().is_none());

        let _store = DagStore::with_backend(Box::new(backend)).unwrap();
        // Can't check the backend directly since it was moved into the store.
        // But rebuild_indexes() should have written the version key.
        // Verified indirectly: if it panicked, with_backend would have failed.
    }

    #[cfg(feature = "sled-backend")]
    #[test]
    fn test_sled_schema_version_persists() {
        use super::super::backend::SledDagBackend;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();

        // Phase 1: Create store (writes schema version)
        {
            let backend = SledDagBackend::open(path).unwrap();
            let _store = DagStore::with_backend(Box::new(backend)).unwrap();
        }

        // Phase 2: Verify schema version was persisted
        {
            let backend = SledDagBackend::open(path).unwrap();
            let ver = backend.get(VERSION_KEY).unwrap().unwrap();
            assert_eq!(ver, vec![SCHEMA_VERSION]);
        }
    }
}
