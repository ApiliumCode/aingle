// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Core DAG action types — the nodes of the Semantic DAG.
//!
//! Every mutation creates a `DagAction` linked to its parent actions by hash,
//! forming a verifiable acyclic graph of all changes.

use crate::NodeId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A content-addressable hash identifying a `DagAction`.
///
/// Computed as `blake3(canonical_serialize(parents, author, seq, timestamp, payload))`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DagActionHash(pub [u8; 32]);

impl DagActionHash {
    /// Create from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Access the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex-encode the hash.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Decode from hex string.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl std::fmt::Display for DagActionHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Payload describing what kind of mutation this action represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DagPayload {
    /// One or more triples were inserted.
    TripleInsert {
        /// Triples in wire format (subject, predicate, object JSON).
        triples: Vec<TripleInsertPayload>,
    },
    /// One or more triples were deleted.
    TripleDelete {
        /// Content-addressable IDs of the deleted triples.
        triple_ids: Vec<[u8; 32]>,
        /// Subjects of the deleted triples (for subject-based indexing).
        /// Empty for actions created before v0.6.2.
        #[serde(default)]
        subjects: Vec<String>,
    },
    /// A memory subsystem operation.
    MemoryOp {
        /// The kind of memory operation.
        kind: MemoryOpKind,
    },
    /// Multiple operations batched into a single action.
    Batch {
        /// The individual payloads.
        ops: Vec<DagPayload>,
    },
    /// Genesis action: marks the root of the DAG (e.g., migration from v0.5).
    Genesis {
        /// Number of triples in the graph at genesis time.
        triple_count: usize,
        /// Human-readable description.
        description: String,
    },
    /// Compaction checkpoint: records that pruning occurred.
    Compact {
        /// Number of actions that were pruned.
        pruned_count: usize,
        /// Number of actions retained after pruning.
        retained_count: usize,
        /// Human-readable description of the policy used.
        policy: String,
    },
    /// No-op action (e.g., for linearizable reads).
    Noop,
    /// Custom user-defined action (audit annotations, checkpoints, decisions).
    Custom {
        /// A descriptive type tag (e.g., "checkpoint", "decision", "annotation").
        payload_type: String,
        /// A human-readable summary of the action.
        payload_summary: String,
        /// Optional arbitrary payload data.
        #[serde(default)]
        payload: Option<serde_json::Value>,
        /// Optional subject for indexing in the DAG history.
        #[serde(default)]
        subject: Option<String>,
    },
}

/// Wire format for a triple insert within a DAG action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TripleInsertPayload {
    pub subject: String,
    pub predicate: String,
    pub object: serde_json::Value,
}

/// Kinds of memory operations tracked in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryOpKind {
    /// A memory entry was stored.
    Store {
        entry_type: String,
        importance: f32,
    },
    /// A memory entry was forgotten.
    Forget { memory_id: String },
    /// Consolidation was triggered.
    Consolidate,
}

/// A single node in the Semantic DAG.
///
/// Each action records its parent action hashes, forming a directed acyclic graph.
/// The hash of this action is computed deterministically from its content fields
/// (excluding `signature`), so any mutation to the content invalidates the hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagAction {
    /// Parent action hashes (the DAG edges).
    /// - Empty for genesis actions.
    /// - 1 parent for linear chains.
    /// - 2+ parents for merge points.
    pub parents: Vec<DagActionHash>,
    /// The author (node) that created this action.
    pub author: NodeId,
    /// Per-author sequence number (monotonically increasing).
    pub seq: u64,
    /// UTC timestamp when this action was created.
    pub timestamp: DateTime<Utc>,
    /// The mutation payload.
    pub payload: DagPayload,
    /// Optional cryptographic signature.
    ///
    /// Marked `#[serde(default)]` so that actions serialized before the
    /// signing feature was added (or by older versions) deserialize
    /// correctly with `None`.
    #[serde(default)]
    pub signature: Option<Vec<u8>>,
}

impl DagAction {
    /// Compute the content-addressable hash of this action.
    ///
    /// Hash = blake3(parents || author || seq || timestamp || payload).
    /// The `signature` field is intentionally excluded.
    pub fn compute_hash(&self) -> DagActionHash {
        let mut hasher = blake3::Hasher::new();

        // Parents
        hasher.update(&(self.parents.len() as u64).to_le_bytes());
        for parent in &self.parents {
            hasher.update(&parent.0);
        }

        // Author — serde_json::to_vec cannot fail for NodeId (no maps with
        // non-string keys, no NaN/Inf floats), so expect() is safe here.
        let author_bytes = serde_json::to_vec(&self.author)
            .expect("NodeId serialization must not fail");
        hasher.update(&(author_bytes.len() as u64).to_le_bytes());
        hasher.update(&author_bytes);

        // Seq
        hasher.update(&self.seq.to_le_bytes());

        // Timestamp
        let ts = self.timestamp.to_rfc3339();
        hasher.update(ts.as_bytes());

        // Payload — same reasoning: DagPayload contains only strings,
        // integers, booleans, and JSON values — all safely serializable.
        let payload_bytes = serde_json::to_vec(&self.payload)
            .expect("DagPayload serialization must not fail");
        hasher.update(&(payload_bytes.len() as u64).to_le_bytes());
        hasher.update(&payload_bytes);

        DagActionHash(*hasher.finalize().as_bytes())
    }

    /// Serialize this action to bytes (JSON).
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("DagAction serialization must not fail")
    }

    /// Deserialize an action from bytes (JSON).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// Returns true if this is a genesis action (no parents).
    pub fn is_genesis(&self) -> bool {
        self.parents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    fn make_test_action(seq: u64, parents: Vec<DagActionHash>) -> DagAction {
        DagAction {
            parents,
            author: NodeId::named("node:1"),
            seq,
            timestamp: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
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
    fn test_hash_deterministic() {
        let action = make_test_action(1, vec![]);
        let h1 = action.compute_hash();
        let h2 = action.compute_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_differs_on_seq() {
        let a1 = make_test_action(1, vec![]);
        let a2 = make_test_action(2, vec![]);
        assert_ne!(a1.compute_hash(), a2.compute_hash());
    }

    #[test]
    fn test_hash_differs_on_parents() {
        let a1 = make_test_action(1, vec![]);
        let a2 = make_test_action(1, vec![DagActionHash([0xAB; 32])]);
        assert_ne!(a1.compute_hash(), a2.compute_hash());
    }

    #[test]
    fn test_hash_hex_roundtrip() {
        let hash = DagActionHash([0xDE; 32]);
        let hex = hash.to_hex();
        assert_eq!(hex.len(), 64);
        let restored = DagActionHash::from_hex(&hex).unwrap();
        assert_eq!(hash, restored);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let action = make_test_action(5, vec![DagActionHash([1; 32])]);
        let bytes = action.to_bytes();
        let restored = DagAction::from_bytes(&bytes).unwrap();
        assert_eq!(restored.seq, 5);
        assert_eq!(restored.parents.len(), 1);
    }

    #[test]
    fn test_genesis_action() {
        let genesis = DagAction {
            parents: vec![],
            author: NodeId::named("aingle:system"),
            seq: 0,
            timestamp: Utc::now(),
            payload: DagPayload::Genesis {
                triple_count: 42,
                description: "Migration from v0.5.0".into(),
            },
            signature: None,
        };
        assert!(genesis.is_genesis());

        let child = make_test_action(1, vec![genesis.compute_hash()]);
        assert!(!child.is_genesis());
    }

    #[test]
    fn test_batch_payload() {
        let action = DagAction {
            parents: vec![],
            author: NodeId::named("node:1"),
            seq: 1,
            timestamp: Utc::now(),
            payload: DagPayload::Batch {
                ops: vec![
                    DagPayload::TripleInsert {
                        triples: vec![TripleInsertPayload {
                            subject: "a".into(),
                            predicate: "b".into(),
                            object: serde_json::json!("c"),
                        }],
                    },
                    DagPayload::TripleDelete {
                        triple_ids: vec![[0u8; 32]],
                        subjects: vec![],
                    },
                ],
            },
            signature: None,
        };
        let bytes = action.to_bytes();
        let restored = DagAction::from_bytes(&bytes).unwrap();
        assert!(matches!(restored.payload, DagPayload::Batch { ops } if ops.len() == 2));
    }

    #[test]
    fn test_signature_excluded_from_hash() {
        let mut a1 = make_test_action(1, vec![]);
        a1.signature = None;
        let h1 = a1.compute_hash();

        a1.signature = Some(vec![1, 2, 3, 4]);
        let h2 = a1.compute_hash();

        assert_eq!(h1, h2, "signature must not affect hash");
    }

    #[test]
    fn test_forward_compat_unknown_fields_ignored() {
        // Simulate a v0.6.1 action with an extra field unknown to v0.6.0.
        // Serde must silently ignore it without errors.
        let json = r#"{
            "parents": [],
            "author": {"Named":"node:1"},
            "seq": 42,
            "timestamp": "2026-01-01T00:00:00Z",
            "payload": "Noop",
            "signature": null,
            "future_field": "some_new_data",
            "another_future": 123
        }"#;

        let action: DagAction = serde_json::from_str(json).expect(
            "must deserialize actions with unknown fields (forward compat)"
        );
        assert_eq!(action.seq, 42);
        assert!(matches!(action.payload, DagPayload::Noop));
    }

    #[test]
    fn test_forward_compat_unknown_payload_variant() {
        // Simulate a v0.6.1 payload variant unknown to v0.6.0.
        // This WILL fail deserialization — which is expected and safe,
        // because DagAction::from_bytes returns None.
        let json = r#"{
            "parents": [],
            "author": {"Named":"node:1"},
            "seq": 1,
            "timestamp": "2026-01-01T00:00:00Z",
            "payload": {"FutureVariant": {"data": "xyz"}},
            "signature": null
        }"#;

        // from_bytes returns None for unrecognized payloads — safe failure
        let result = DagAction::from_bytes(json.as_bytes());
        assert!(
            result.is_none(),
            "unknown payload variants must fail gracefully (None, not panic)"
        );
    }

    #[test]
    fn test_backward_compat_missing_signature() {
        // Simulate a v0.5.0 action that was serialized WITHOUT the signature field.
        // #[serde(default)] ensures this deserializes to None.
        let json = r#"{
            "parents": [],
            "author": {"Named":"node:1"},
            "seq": 1,
            "timestamp": "2026-01-01T00:00:00Z",
            "payload": "Noop"
        }"#;

        let action: DagAction = serde_json::from_str(json).expect(
            "must deserialize actions without signature field (backward compat)"
        );
        assert!(action.signature.is_none());
    }
}
