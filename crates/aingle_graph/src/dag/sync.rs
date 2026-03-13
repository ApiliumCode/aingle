// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Cross-node DAG synchronization protocol.
//!
//! Nodes exchange DAG actions using a pull-based protocol:
//!
//! 1. Node A sends its tips to Node B via `SyncRequest`
//! 2. Node B computes which actions A is missing
//! 3. Node B responds with those actions in topological order
//! 4. Node A ingests them into its local DagStore

use super::action::{DagAction, DagActionHash};
use serde::{Deserialize, Serialize};

/// Request sent by a node to synchronize DAG actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    /// The requesting node's current DAG tips.
    pub local_tips: Vec<DagActionHash>,
    /// Specific action hashes to request (if known).
    /// When non-empty, the responder returns only these actions.
    #[serde(default)]
    pub want: Vec<DagActionHash>,
}

/// Response containing DAG actions the requester is missing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    /// Actions the requester is missing, in topological order.
    pub actions: Vec<DagAction>,
    /// The responding node's current tips.
    pub remote_tips: Vec<DagActionHash>,
    /// Number of actions sent.
    pub action_count: usize,
}

/// Result of a pull sync operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResult {
    /// Number of new actions ingested.
    pub ingested: usize,
    /// Number of actions that were already present locally.
    pub already_had: usize,
    /// The remote node's tips after sync.
    pub remote_tips: Vec<DagActionHash>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{DagPayload, DagStore, TripleInsertPayload};
    use crate::NodeId;
    use chrono::Utc;

    fn make_action(seq: u64, subject: &str, parents: Vec<DagActionHash>) -> DagAction {
        DagAction {
            parents,
            author: NodeId::named("node:1"),
            seq,
            timestamp: Utc::now(),
            payload: DagPayload::TripleInsert {
                triples: vec![TripleInsertPayload {
                    subject: subject.into(),
                    predicate: "knows".into(),
                    object: serde_json::json!("x"),
                }],
            },
            signature: None,
        }
    }

    #[test]
    fn test_compute_missing_linear() {
        // Node B has: a1 -> a2 -> a3
        // Node A has: a1 -> a2 (tips = [a2])
        // Missing for A: [a3]
        let store_b = DagStore::new();
        let a1 = make_action(1, "s1", vec![]);
        let h1 = store_b.put(&a1).unwrap();
        let a2 = make_action(2, "s2", vec![h1]);
        let h2 = store_b.put(&a2).unwrap();
        let a3 = make_action(3, "s3", vec![h2]);
        store_b.put(&a3).unwrap();

        let missing = store_b.compute_missing(&[h2]).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].seq, 3);
    }

    #[test]
    fn test_compute_missing_branching() {
        // Node B: a1 -> a2, a1 -> a3
        // Node A: has a1, a2 (tips = [a2])
        // Missing: a3
        let store_b = DagStore::new();
        let a1 = make_action(1, "s1", vec![]);
        let h1 = store_b.put(&a1).unwrap();
        let a2 = make_action(2, "s2", vec![h1]);
        let h2 = store_b.put(&a2).unwrap();
        let a3 = make_action(3, "s3", vec![h1]);
        store_b.put(&a3).unwrap();

        let missing = store_b.compute_missing(&[h2]).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].seq, 3);
    }

    #[test]
    fn test_compute_missing_fully_synced() {
        let store = DagStore::new();
        let a1 = make_action(1, "s1", vec![]);
        let h1 = store.put(&a1).unwrap();
        let a2 = make_action(2, "s2", vec![h1]);
        let h2 = store.put(&a2).unwrap();

        let missing = store.compute_missing(&[h2]).unwrap();
        assert!(missing.is_empty());
    }

    #[test]
    fn test_compute_missing_unknown_remote_tip() {
        // Remote tip is unknown to us — we send everything
        let store = DagStore::new();
        let a1 = make_action(1, "s1", vec![]);
        store.put(&a1).unwrap();

        let unknown = DagActionHash([0xFF; 32]);
        let missing = store.compute_missing(&[unknown]).unwrap();
        assert_eq!(missing.len(), 1);
    }

    #[test]
    fn test_ingest_stores_without_touching_tips() {
        let store = DagStore::new();

        // Put a1 as the "real" tip
        let a1 = make_action(1, "s1", vec![]);
        let h1 = store.put(&a1).unwrap();
        assert_eq!(store.tip_count().unwrap(), 1);

        // Ingest a historical action (a0, parent of a1)
        let a0 = DagAction {
            parents: vec![],
            author: NodeId::named("node:1"),
            seq: 0,
            timestamp: Utc::now(),
            payload: DagPayload::Noop,
            signature: None,
        };
        let h0 = store.ingest(&a0).unwrap();
        assert_ne!(h0, h1);

        // Tips should still be [h1], not changed by ingest
        let tips = store.tips().unwrap();
        assert_eq!(tips.len(), 1);
        assert_eq!(tips[0], h1);

        // But the action is stored and retrievable
        assert!(store.get(&h0).unwrap().is_some());
        assert_eq!(store.action_count(), 2);
    }

    #[test]
    fn test_ingest_skips_duplicates() {
        let store = DagStore::new();
        let a1 = make_action(1, "s1", vec![]);
        let h1 = store.put(&a1).unwrap();

        // Ingest same action again
        let h1_again = store.ingest(&a1).unwrap();
        assert_eq!(h1, h1_again);
        assert_eq!(store.action_count(), 1); // no duplicate
    }

    #[test]
    fn test_sync_request_serialization() {
        let req = SyncRequest {
            local_tips: vec![DagActionHash([1; 32])],
            want: vec![],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: SyncRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.local_tips.len(), 1);
    }

    #[test]
    fn test_sync_response_serialization() {
        let resp = SyncResponse {
            actions: vec![],
            remote_tips: vec![DagActionHash([2; 32])],
            action_count: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: SyncResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.remote_tips.len(), 1);
    }
}
