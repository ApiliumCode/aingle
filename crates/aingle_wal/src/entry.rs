// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! WAL entry types and serialization.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single WAL entry representing one mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// Wall-clock timestamp (UTC).
    pub timestamp: DateTime<Utc>,
    /// The mutation kind.
    pub kind: WalEntryKind,
    /// blake3 hash of the previous entry (chain integrity).
    pub prev_hash: [u8; 32],
    /// blake3 hash of this entry's payload.
    pub hash: [u8; 32],
}

impl WalEntry {
    /// Compute the hash for this entry's payload (kind + seq + timestamp + prev_hash).
    pub fn compute_hash(seq: u64, timestamp: &DateTime<Utc>, kind: &WalEntryKind, prev_hash: &[u8; 32]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&seq.to_le_bytes());
        hasher.update(timestamp.to_rfc3339().as_bytes());
        // Hash the serialized kind
        if let Ok(kind_bytes) = serde_json::to_vec(kind) {
            hasher.update(&kind_bytes);
        }
        hasher.update(prev_hash);
        *hasher.finalize().as_bytes()
    }
}

/// The kind of mutation recorded in a WAL entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WalEntryKind {
    /// Triple inserted into GraphDB.
    TripleInsert {
        subject: String,
        predicate: String,
        object: serde_json::Value,
        triple_id: [u8; 32],
    },
    /// Triple deleted from GraphDB.
    TripleDelete {
        triple_id: [u8; 32],
    },
    /// Memory entry stored in Ineru STM.
    MemoryStore {
        memory_id: String,
        entry_type: String,
        data: serde_json::Value,
        importance: f32,
    },
    /// Memory entry forgotten.
    MemoryForget {
        memory_id: String,
    },
    /// STM → LTM consolidation occurred.
    MemoryConsolidate {
        consolidated_count: usize,
    },
    /// Proof submitted.
    ProofSubmit {
        proof_id: String,
        proof_type: String,
    },
    /// Snapshot checkpoint marker.
    Checkpoint {
        graph_triple_count: usize,
        ineru_stm_count: usize,
        ineru_ltm_entity_count: usize,
    },
    /// LTM entity created (for Ineru replication).
    LtmEntityCreate {
        entity_id: String,
        name: String,
        entity_type: String,
    },
    /// LTM link created (for Ineru replication).
    LtmLinkCreate {
        from_entity: String,
        to_entity: String,
        relation: String,
        weight: f32,
    },
    /// LTM entity deleted (for Ineru replication).
    LtmEntityDelete {
        entity_id: String,
    },
    /// Serialized openraft Raft log entry.
    RaftEntry {
        index: u64,
        term: u64,
        data: Vec<u8>,
    },
    /// DAG action (serialized bytes to avoid circular deps with aingle_graph).
    DagAction {
        /// Serialized DagAction bytes (JSON).
        action_bytes: Vec<u8>,
    },
    /// No-op entry for linearizable reads.
    Noop,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_kind_serialization() {
        let kind = WalEntryKind::TripleInsert {
            subject: "alice".into(),
            predicate: "knows".into(),
            object: serde_json::json!("bob"),
            triple_id: [0u8; 32],
        };
        let json = serde_json::to_string(&kind).unwrap();
        let back: WalEntryKind = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, WalEntryKind::TripleInsert { .. }));
    }

    #[test]
    fn test_compute_hash_deterministic() {
        let ts = Utc::now();
        let kind = WalEntryKind::TripleDelete { triple_id: [1u8; 32] };
        let prev = [0u8; 32];

        let h1 = WalEntry::compute_hash(1, &ts, &kind, &prev);
        let h2 = WalEntry::compute_hash(1, &ts, &kind, &prev);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_compute_hash_differs_on_seq() {
        let ts = Utc::now();
        let kind = WalEntryKind::TripleDelete { triple_id: [1u8; 32] };
        let prev = [0u8; 32];

        let h1 = WalEntry::compute_hash(1, &ts, &kind, &prev);
        let h2 = WalEntry::compute_hash(2, &ts, &kind, &prev);
        assert_ne!(h1, h2);
    }
}
