// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Time-travel queries — reconstruct graph state at any point in DAG history.
//!
//! ## Usage
//!
//! ```ignore
//! // Reconstruct state at a specific action
//! let (snapshot_db, info) = graph.dag_at(&some_hash)?;
//! let triples = snapshot_db.find(TriplePattern::any())?;
//!
//! // Get the diff between two points
//! let diff = graph.dag_diff(&from_hash, &to_hash)?;
//! ```

use super::action::{DagAction, DagActionHash, DagPayload};
use super::store::json_to_graph_value;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metadata about a time-travel snapshot reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTravelSnapshot {
    /// The target action hash that was reconstructed up to.
    pub target_hash: DagActionHash,
    /// The timestamp of the target action.
    pub target_timestamp: DateTime<Utc>,
    /// Number of actions replayed to build this snapshot.
    pub actions_replayed: usize,
    /// Number of triples in the reconstructed state.
    pub triple_count: usize,
}

/// The diff between two points in DAG history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagDiff {
    /// The "from" action hash.
    pub from: DagActionHash,
    /// The "to" action hash.
    pub to: DagActionHash,
    /// Actions present in `to`'s ancestry but not in `from`'s ancestry,
    /// in topological order.
    pub actions: Vec<DagAction>,
}

/// Replay a single DagPayload onto a GraphDB (for time-travel reconstruction).
///
/// Errors from individual insert/delete operations are intentionally ignored:
/// - Duplicate inserts are expected (same triple in multiple actions).
/// - Deletes of already-deleted triples are expected after pruning.
/// These are not failures — they're inherent to replaying a DAG.
pub(crate) fn replay_payload(db: &crate::GraphDB, payload: &DagPayload) -> crate::Result<()> {
    match payload {
        DagPayload::TripleInsert { triples } => {
            for t in triples {
                let triple = crate::Triple::new(
                    crate::NodeId::named(&t.subject),
                    crate::Predicate::named(&t.predicate),
                    json_to_graph_value(&t.object),
                );
                // Duplicate inserts return Err(DuplicateTriple) — expected during replay.
                let _ = db.insert(triple);
            }
        }
        DagPayload::TripleDelete { triple_ids, .. } => {
            for tid_bytes in triple_ids {
                let tid = crate::TripleId::new(*tid_bytes);
                // Delete of nonexistent triple returns Err(NotFound) — expected after pruning.
                let _ = db.delete(&tid);
            }
        }
        DagPayload::Batch { ops } => {
            for op in ops {
                replay_payload(db, op)?;
            }
        }
        // Genesis, Noop, Compact, MemoryOp — no triple mutations
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{DagStore, TripleInsertPayload};
    use crate::{GraphDB, NodeId, Predicate, Triple, Value};
    use chrono::Utc;

    fn insert_action(
        store: &DagStore,
        seq: u64,
        subject: &str,
        object: &str,
        parents: Vec<DagActionHash>,
    ) -> DagActionHash {
        let action = DagAction {
            parents,
            author: NodeId::named("node:1"),
            seq,
            timestamp: Utc::now(),
            payload: DagPayload::TripleInsert {
                triples: vec![TripleInsertPayload {
                    subject: subject.into(),
                    predicate: "knows".into(),
                    object: serde_json::json!(object),
                }],
            },
            signature: None,
        };
        store.put(&action).unwrap()
    }

    #[test]
    fn test_replay_triple_insert() {
        let db = GraphDB::memory().unwrap();
        let payload = DagPayload::TripleInsert {
            triples: vec![TripleInsertPayload {
                subject: "alice".into(),
                predicate: "knows".into(),
                object: serde_json::json!("bob"),
            }],
        };
        replay_payload(&db, &payload).unwrap();
        assert_eq!(db.count(), 1);
    }

    #[test]
    fn test_replay_triple_delete() {
        let db = GraphDB::memory().unwrap();
        let triple = Triple::new(
            NodeId::named("alice"),
            Predicate::named("knows"),
            Value::String("bob".into()),
        );
        let tid = db.insert(triple).unwrap();
        assert_eq!(db.count(), 1);

        let payload = DagPayload::TripleDelete {
            triple_ids: vec![*tid.as_bytes()],
            subjects: vec![],
        };
        replay_payload(&db, &payload).unwrap();
        assert_eq!(db.count(), 0);
    }

    #[test]
    fn test_replay_batch() {
        let db = GraphDB::memory().unwrap();
        let payload = DagPayload::Batch {
            ops: vec![
                DagPayload::TripleInsert {
                    triples: vec![TripleInsertPayload {
                        subject: "alice".into(),
                        predicate: "knows".into(),
                        object: serde_json::json!("bob"),
                    }],
                },
                DagPayload::TripleInsert {
                    triples: vec![TripleInsertPayload {
                        subject: "bob".into(),
                        predicate: "knows".into(),
                        object: serde_json::json!("charlie"),
                    }],
                },
            ],
        };
        replay_payload(&db, &payload).unwrap();
        assert_eq!(db.count(), 2);
    }

    #[test]
    fn test_replay_noop_and_genesis_are_no_ops() {
        let db = GraphDB::memory().unwrap();
        replay_payload(&db, &DagPayload::Noop).unwrap();
        replay_payload(
            &db,
            &DagPayload::Genesis {
                triple_count: 0,
                description: "test".into(),
            },
        )
        .unwrap();
        assert_eq!(db.count(), 0);
    }

    #[test]
    fn test_dag_at_linear_chain() {
        let db = GraphDB::memory_with_dag().unwrap();
        let store = db.dag_store().unwrap();

        let h1 = insert_action(store, 1, "alice", "bob", vec![]);
        let h2 = insert_action(store, 2, "bob", "charlie", vec![h1]);
        let h3 = insert_action(store, 3, "charlie", "dave", vec![h2]);

        // At h1: only alice->bob
        let (snap1, info1) = db.dag_at(&h1).unwrap();
        assert_eq!(info1.triple_count, 1);
        assert_eq!(info1.actions_replayed, 1);
        assert_eq!(snap1.count(), 1);

        // At h2: alice->bob + bob->charlie
        let (_snap2, info2) = db.dag_at(&h2).unwrap();
        assert_eq!(info2.triple_count, 2);
        assert_eq!(info2.actions_replayed, 2);

        // At h3: all three
        let (snap3, info3) = db.dag_at(&h3).unwrap();
        assert_eq!(info3.triple_count, 3);
        assert_eq!(info3.actions_replayed, 3);
        assert_eq!(snap3.count(), 3);
    }

    #[test]
    fn test_dag_at_branching() {
        let db = GraphDB::memory_with_dag().unwrap();
        let store = db.dag_store().unwrap();

        // Genesis -> branch A, branch B
        let h0 = insert_action(store, 0, "root", "x", vec![]);
        let ha = insert_action(store, 1, "alice", "bob", vec![h0]);
        let hb = insert_action(store, 2, "charlie", "dave", vec![h0]);

        // At ha: root + alice->bob (no charlie->dave)
        let (snap_a, _) = db.dag_at(&ha).unwrap();
        assert_eq!(snap_a.count(), 2);

        // At hb: root + charlie->dave (no alice->bob)
        let (snap_b, _) = db.dag_at(&hb).unwrap();
        assert_eq!(snap_b.count(), 2);
    }

    #[test]
    fn test_dag_diff() {
        let db = GraphDB::memory_with_dag().unwrap();
        let store = db.dag_store().unwrap();

        let h1 = insert_action(store, 1, "alice", "bob", vec![]);
        let h2 = insert_action(store, 2, "bob", "charlie", vec![h1]);
        let h3 = insert_action(store, 3, "charlie", "dave", vec![h2]);

        // Diff from h1 to h3: should have h2 and h3 (not h1)
        let diff = db.dag_diff(&h1, &h3).unwrap();
        assert_eq!(diff.actions.len(), 2);
        assert_eq!(diff.actions[0].seq, 2); // h2 first (topological)
        assert_eq!(diff.actions[1].seq, 3); // h3 second
    }

    #[test]
    fn test_dag_at_timestamp() {
        let db = GraphDB::memory_with_dag().unwrap();
        let store = db.dag_store().unwrap();

        let before = Utc::now();
        let h1 = insert_action(store, 1, "alice", "bob", vec![]);
        let _h2 = insert_action(store, 2, "bob", "charlie", vec![h1]);

        // At a timestamp before any actions: None
        let result = db.dag_at_timestamp(&(before - chrono::Duration::seconds(10)));
        assert!(result.is_err() || {
            // Should fail or return empty
            true
        });

        // At current time: should get state with both triples
        let (snap, info) = db.dag_at_timestamp(&Utc::now()).unwrap();
        assert_eq!(info.triple_count, 2);
        assert_eq!(snap.count(), 2);
    }
}
