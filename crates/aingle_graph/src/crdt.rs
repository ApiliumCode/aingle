// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! CRDT conflict resolution for distributed triple stores.
//!
//! Provides Last-Writer-Wins (LWW) registers and Observed-Remove Sets
//! for deterministic conflict resolution when gossip-synced nodes have
//! concurrent writes.

use crate::triple::{Triple, TripleId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// Last-Writer-Wins Register for triple conflicts.
///
/// When two nodes write to the same triple ID concurrently,
/// the write with the latest timestamp wins. Ties are broken
/// deterministically by node ID (higher ID wins).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LwwTriple {
    pub triple: Triple,
    pub timestamp: DateTime<Utc>,
    pub node_id: u64,
}

impl LwwTriple {
    /// Create a new LWW-tagged triple.
    pub fn new(triple: Triple, node_id: u64) -> Self {
        Self {
            triple,
            timestamp: Utc::now(),
            node_id,
        }
    }

    /// Create with an explicit timestamp.
    pub fn with_timestamp(triple: Triple, timestamp: DateTime<Utc>, node_id: u64) -> Self {
        Self {
            triple,
            timestamp,
            node_id,
        }
    }

    /// Merge two conflicting versions. Returns the winner.
    pub fn merge(a: &LwwTriple, b: &LwwTriple) -> LwwTriple {
        if a.timestamp > b.timestamp {
            a.clone()
        } else if b.timestamp > a.timestamp {
            b.clone()
        } else {
            // Tie-break by node ID (deterministic: higher ID wins)
            if a.node_id >= b.node_id {
                a.clone()
            } else {
                b.clone()
            }
        }
    }
}

/// Observed-Remove Set for triple existence.
///
/// Handles the case where one node inserts and another deletes
/// the same triple concurrently. Each insert generates a unique
/// tag; a remove only affects the tags that were observed at the
/// time of removal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrSet {
    /// (triple_id bytes, add_tag) pairs — unique per insert operation.
    adds: HashSet<([u8; 32], Uuid)>,
    /// (triple_id bytes, add_tag) pairs that have been removed.
    removes: HashSet<([u8; 32], Uuid)>,
}

impl OrSet {
    /// Create a new empty OR-Set.
    pub fn new() -> Self {
        Self {
            adds: HashSet::new(),
            removes: HashSet::new(),
        }
    }

    /// Insert a triple ID into the set, returning a unique tag.
    pub fn insert(&mut self, id: &TripleId) -> Uuid {
        let tag = Uuid::new_v4();
        self.adds.insert((*id.as_bytes(), tag));
        tag
    }

    /// Remove all observed add-tags for this triple ID.
    pub fn remove(&mut self, id: &TripleId) {
        let id_bytes = *id.as_bytes();
        let to_remove: Vec<_> = self
            .adds
            .iter()
            .filter(|(tid, _)| *tid == id_bytes)
            .cloned()
            .collect();
        for pair in to_remove {
            self.adds.remove(&pair);
            self.removes.insert(pair);
        }
    }

    /// Check if a triple ID is in the set (has at least one
    /// non-removed add-tag).
    pub fn contains(&self, id: &TripleId) -> bool {
        let id_bytes = *id.as_bytes();
        self.adds
            .iter()
            .any(|(tid, tag)| *tid == id_bytes && !self.removes.contains(&(id_bytes, *tag)))
    }

    /// Merge another OR-Set into this one.
    ///
    /// Union of adds and removes. Idempotent.
    pub fn merge(&mut self, other: &OrSet) {
        self.adds = self.adds.union(&other.adds).cloned().collect();
        self.removes = self.removes.union(&other.removes).cloned().collect();
    }

    /// Number of active (non-removed) entries.
    pub fn len(&self) -> usize {
        self.adds
            .iter()
            .filter(|pair| !self.removes.contains(pair))
            .count()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for OrSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeId, Predicate, Value};
    use chrono::Duration;

    fn make_triple(subject: &str) -> Triple {
        Triple::new(
            NodeId::named(subject),
            Predicate::named("knows"),
            Value::String("bob".into()),
        )
    }

    #[test]
    fn test_lww_later_timestamp_wins() {
        let triple = make_triple("alice");
        let now = Utc::now();
        let earlier = now - Duration::seconds(10);

        let a = LwwTriple::with_timestamp(triple.clone(), earlier, 1);
        let b = LwwTriple::with_timestamp(triple, now, 2);

        let winner = LwwTriple::merge(&a, &b);
        assert_eq!(winner.node_id, 2); // b wins (later timestamp)
    }

    #[test]
    fn test_lww_tiebreak_by_node_id() {
        let triple = make_triple("alice");
        let same_time = Utc::now();

        let a = LwwTriple::with_timestamp(triple.clone(), same_time, 1);
        let b = LwwTriple::with_timestamp(triple, same_time, 2);

        let winner = LwwTriple::merge(&a, &b);
        assert_eq!(winner.node_id, 2); // Higher node_id wins tie
    }

    #[test]
    fn test_lww_merge_commutative() {
        let triple = make_triple("test");
        let now = Utc::now();

        let a = LwwTriple::with_timestamp(triple.clone(), now, 1);
        let b = LwwTriple::with_timestamp(triple, now - Duration::seconds(1), 2);

        let winner1 = LwwTriple::merge(&a, &b);
        let winner2 = LwwTriple::merge(&b, &a);
        assert_eq!(winner1.node_id, winner2.node_id); // Same result regardless of order
    }

    #[test]
    fn test_or_set_insert_contains() {
        let mut set = OrSet::new();
        let triple = make_triple("alice");
        let id = TripleId::from_triple(&triple);

        set.insert(&id);
        assert!(set.contains(&id));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_or_set_remove() {
        let mut set = OrSet::new();
        let triple = make_triple("alice");
        let id = TripleId::from_triple(&triple);

        set.insert(&id);
        assert!(set.contains(&id));

        set.remove(&id);
        assert!(!set.contains(&id));
        assert!(set.is_empty());
    }

    #[test]
    fn test_or_set_concurrent_insert_remove() {
        // Simulates: Node A inserts, Node B removes same ID (with tag from A),
        // then Node A inserts again. The second insert should survive.
        let mut set = OrSet::new();
        let triple = make_triple("alice");
        let id = TripleId::from_triple(&triple);

        // First insert
        set.insert(&id);
        assert!(set.contains(&id));

        // Remove (only removes tags observed so far)
        set.remove(&id);
        assert!(!set.contains(&id));

        // Re-insert generates new tag
        set.insert(&id);
        assert!(set.contains(&id));
    }

    #[test]
    fn test_or_set_merge() {
        let triple = make_triple("alice");
        let id = TripleId::from_triple(&triple);

        let mut set_a = OrSet::new();
        set_a.insert(&id);

        let mut set_b = OrSet::new();
        let triple2 = make_triple("bob");
        let id2 = TripleId::from_triple(&triple2);
        set_b.insert(&id2);

        // Merge B into A
        set_a.merge(&set_b);
        assert!(set_a.contains(&id));
        assert!(set_a.contains(&id2));
    }

    #[test]
    fn test_or_set_merge_idempotent() {
        let triple = make_triple("alice");
        let id = TripleId::from_triple(&triple);

        let mut set = OrSet::new();
        set.insert(&id);

        let snapshot = set.clone();
        set.merge(&snapshot);

        assert_eq!(set.len(), 1); // Merging with self doesn't duplicate
    }

    #[test]
    fn test_or_set_merge_with_removes() {
        let triple = make_triple("alice");
        let id = TripleId::from_triple(&triple);

        let mut set_a = OrSet::new();
        set_a.insert(&id);

        // B gets A's state and removes the entry
        let mut set_b = set_a.clone();
        set_b.remove(&id);

        // A doesn't know about the remove yet
        assert!(set_a.contains(&id));
        assert!(!set_b.contains(&id));

        // After merge, the remove propagates
        set_a.merge(&set_b);
        assert!(!set_a.contains(&id));
    }
}
