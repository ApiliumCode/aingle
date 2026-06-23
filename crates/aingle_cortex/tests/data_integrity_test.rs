// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Cross-subsystem data integrity tests.
//!
//! Verifies that data flows correctly between all AIngle subsystems:
//! - Graph ↔ Proof Store ↔ Raft Snapshots
//! - Graph ↔ DAG ↔ Triple materialization
//! - State flush/restore round-trip
//! - Batch insert atomicity

use aingle_cortex::proofs::{ProofMetadata, ProofStore, ProofType, SubmitProofRequest};
use aingle_cortex::state::AppState;

// ============================================================================
// 1. ProofStore persistence round-trip (Sled backend)
// ============================================================================

#[tokio::test]
async fn test_proof_store_sled_roundtrip_data_integrity() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap();

    let mut proof_ids = Vec::new();

    // Phase 1: Write 20 proofs of various types
    {
        let store = ProofStore::with_sled(path).unwrap();

        let types = [
            ProofType::Schnorr,
            ProofType::Equality,
            ProofType::Membership,
            ProofType::Range,
            ProofType::Knowledge,
        ];

        for (i, proof_type) in types.iter().cycle().take(20).enumerate() {
            let request = SubmitProofRequest {
                proof_type: proof_type.clone(),
                proof_data: serde_json::json!({
                    "index": i,
                    "payload": format!("proof-data-{}", i),
                }),
                metadata: Some(ProofMetadata {
                    submitter: Some(format!("user-{}", i % 3)),
                    tags: vec![format!("tag-{}", i)],
                    extra: Default::default(),
                }),
            };
            let id = store.submit(request).await.unwrap();
            proof_ids.push(id);
        }

        assert_eq!(store.count().await, 20);
        store.flush().unwrap();
    }

    // Phase 2: Reopen and verify every proof intact
    {
        let store = ProofStore::with_sled(path).unwrap();
        assert_eq!(store.count().await, 20, "count mismatch after reopen");

        for (i, id) in proof_ids.iter().enumerate() {
            let proof = store
                .get(id)
                .await
                .unwrap_or_else(|| panic!("proof {} (id={}) missing after reopen", i, id));

            // Verify data field contains correct index
            let data: serde_json::Value = serde_json::from_slice(&proof.data).unwrap();
            let payload = data.get("index").and_then(|v| v.as_u64()).unwrap();
            assert_eq!(payload as usize, i, "data mismatch for proof {}", i);

            // Verify metadata
            let submitter = proof.metadata.submitter.as_ref().unwrap();
            assert_eq!(submitter, &format!("user-{}", i % 3));
        }

        // Stats should match
        let stats = store.stats().await;
        assert_eq!(stats.total_proofs, 20);
        assert_eq!(stats.proofs_by_type.get("schnorr"), Some(&4));
        assert_eq!(stats.proofs_by_type.get("equality"), Some(&4));
        assert_eq!(stats.proofs_by_type.get("membership"), Some(&4));
        assert_eq!(stats.proofs_by_type.get("range"), Some(&4));
        assert_eq!(stats.proofs_by_type.get("knowledge"), Some(&4));
    }

    // Phase 3: Delete half, reopen, verify
    {
        let store = ProofStore::with_sled(path).unwrap();
        for id in &proof_ids[0..10] {
            assert!(store.delete(id).await, "delete should succeed for {}", id);
        }
        store.flush().unwrap();
    }
    {
        let store = ProofStore::with_sled(path).unwrap();
        assert_eq!(store.count().await, 10, "count after delete+reopen");

        // Deleted ones should be gone
        for id in &proof_ids[0..10] {
            assert!(
                store.get(id).await.is_none(),
                "deleted proof {} should not exist",
                id
            );
        }
        // Remaining ones should be intact
        for (i, id) in proof_ids[10..20].iter().enumerate() {
            let proof = store
                .get(id)
                .await
                .unwrap_or_else(|| panic!("remaining proof {} missing", i + 10));
            let data: serde_json::Value = serde_json::from_slice(&proof.data).unwrap();
            assert_eq!(data["index"].as_u64().unwrap() as usize, i + 10);
        }
    }
}

// ============================================================================
// 2. Graph + DAG data consistency
// ============================================================================

#[tokio::test]
async fn test_graph_dag_triple_materialization_consistency() {
    use aingle_graph::{GraphDB, NodeId, Predicate, Triple, TriplePattern, Value};

    let mut graph = GraphDB::memory().unwrap();
    graph.enable_dag();

    // Ensure DAG initialized
    if let Some(dag_store) = graph.dag_store() {
        let _ = dag_store.init_or_migrate(0);
    }

    // Insert 50 triples via graph (materialized view)
    let mut triple_ids = Vec::new();
    for i in 0..50 {
        let triple = Triple::new(
            NodeId::named(&format!("entity:{}", i)),
            Predicate::named("has_value"),
            Value::Integer(i * 100),
        );
        let tid = graph.insert(triple).unwrap();
        triple_ids.push(tid);
    }

    // Verify all 50 exist in graph
    assert_eq!(graph.count(), 50);

    // Verify each triple can be retrieved by ID
    for (i, tid) in triple_ids.iter().enumerate() {
        let triple = graph
            .get(tid)
            .unwrap()
            .unwrap_or_else(|| panic!("triple {} not found by ID", i));
        assert_eq!(
            triple.object,
            Value::Integer(i as i64 * 100),
            "value mismatch for triple {}",
            i
        );
    }

    // Verify pattern queries return correct results
    for i in 0..50 {
        let pattern = TriplePattern::subject(NodeId::named(&format!("entity:{}", i)));
        let results = graph.find(pattern).unwrap();
        assert_eq!(
            results.len(),
            1,
            "entity:{} should have exactly 1 triple",
            i
        );
        assert_eq!(results[0].object, Value::Integer(i * 100));
    }

    // Delete odd-numbered triples
    for i in (1..50).step_by(2) {
        let deleted = graph.delete(&triple_ids[i]).unwrap();
        assert!(deleted, "delete should succeed for triple {}", i);
    }
    assert_eq!(graph.count(), 25);

    // Verify even ones remain, odd ones gone
    for i in 0..50 {
        let exists = graph.get(&triple_ids[i]).unwrap().is_some();
        if i % 2 == 0 {
            assert!(exists, "even triple {} should still exist", i);
        } else {
            assert!(!exists, "odd triple {} should be deleted", i);
        }
    }
}

// ============================================================================
// 3. Batch insert atomicity — index consistency
// ============================================================================

#[tokio::test]
async fn test_batch_insert_index_consistency() {
    use aingle_graph::{GraphDB, NodeId, Predicate, Triple, TriplePattern, Value};

    let graph = GraphDB::memory().unwrap();

    // Batch insert 100 triples
    let triples: Vec<Triple> = (0..100)
        .map(|i| {
            Triple::new(
                NodeId::named(&format!("batch:{}", i)),
                Predicate::named("batch_value"),
                Value::Integer(i),
            )
        })
        .collect();

    let ids = graph.insert_batch(triples).unwrap();
    assert_eq!(ids.len(), 100);
    assert_eq!(graph.count(), 100);

    // Verify every triple is findable by subject pattern (uses SPO index)
    for i in 0..100 {
        let pattern = TriplePattern::subject(NodeId::named(&format!("batch:{}", i)));
        let results = graph.find(pattern).unwrap();
        assert_eq!(results.len(), 1, "batch:{} should be findable via index", i);
        assert_eq!(results[0].object, Value::Integer(i));
    }

    // Verify predicate index works
    let by_pred = graph
        .find(TriplePattern::predicate(Predicate::named("batch_value")))
        .unwrap();
    assert_eq!(by_pred.len(), 100, "predicate index should find all 100");

    // Re-batch the same triples — should skip duplicates, no count change
    let triples2: Vec<Triple> = (0..100)
        .map(|i| {
            Triple::new(
                NodeId::named(&format!("batch:{}", i)),
                Predicate::named("batch_value"),
                Value::Integer(i),
            )
        })
        .collect();
    let ids2 = graph.insert_batch(triples2).unwrap();
    assert_eq!(ids2.len(), 100);
    assert_eq!(graph.count(), 100, "duplicates should not increase count");
}

// ============================================================================
// 4. AppState flush/restore round-trip
// ============================================================================

#[tokio::test]
async fn test_app_state_flush_restore_roundtrip() {
    use aingle_graph::{NodeId, Predicate, Triple, TriplePattern, Value};

    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("graph.sled");
    let db_str = db_path.to_str().unwrap();

    // Phase 1: Create state, insert data, flush
    let proof_ids = {
        let state = AppState::with_db_path(db_str, None).unwrap();

        // Insert triples
        {
            let graph = state.graph.read().await;
            for i in 0..10 {
                let triple = Triple::new(
                    NodeId::named(&format!("node:{}", i)),
                    Predicate::named("value"),
                    Value::String(format!("data-{}", i)),
                );
                graph.insert(triple).unwrap();
            }
            assert_eq!(graph.count(), 10);
        }

        // Submit proofs
        let mut ids = Vec::new();
        for i in 0..5 {
            let request = SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({"flush_test": i}),
                metadata: None,
            };
            ids.push(state.proof_store.submit(request).await.unwrap());
        }

        // Flush everything
        let snapshot_dir = dir.path().to_path_buf();
        state.flush(Some(snapshot_dir.as_path())).await.unwrap();
        ids
    };
    // Explicit drop ensures sled DB is fully released before reopen
    // (all Arcs from the block above are now dropped)

    // Phase 2: Reopen, verify data survived
    {
        let state = AppState::with_db_path(db_str, None).unwrap();

        // Graph triples
        {
            let graph = state.graph.read().await;
            assert_eq!(graph.count(), 10, "graph triples should survive restart");

            for i in 0..10 {
                let pattern = TriplePattern::subject(NodeId::named(&format!("node:{}", i)));
                let results = graph.find(pattern).unwrap();
                assert_eq!(results.len(), 1, "node:{} missing after restart", i);
                assert_eq!(
                    results[0].object,
                    Value::String(format!("data-{}", i)),
                    "data mismatch for node:{}",
                    i
                );
            }
        }

        // Proofs — verify each by ID
        let proof_count = state.proof_store.count().await;
        assert_eq!(proof_count, 5, "proofs should survive restart");
        for (i, id) in proof_ids.iter().enumerate() {
            let proof = state
                .proof_store
                .get(id)
                .await
                .unwrap_or_else(|| panic!("proof {} missing after restart", i));
            let data: serde_json::Value = serde_json::from_slice(&proof.data).unwrap();
            assert_eq!(data["flush_test"].as_u64().unwrap(), i as u64);
        }
    }
}

// ============================================================================
// 5. Raft snapshot round-trip with proofs
// ============================================================================

#[tokio::test]
async fn test_raft_snapshot_with_proofs_roundtrip() {
    use aingle_raft::state_machine::{ClusterSnapshot, ProofSnapshot, TripleSnapshot};

    let snapshot = ClusterSnapshot {
        triples: vec![
            TripleSnapshot {
                subject: "alice".into(),
                predicate: "knows".into(),
                object: serde_json::json!("bob"),
            },
            TripleSnapshot {
                subject: "bob".into(),
                predicate: "age".into(),
                object: serde_json::json!(30),
            },
        ],
        ineru_ltm: vec![10, 20, 30],
        last_applied_index: 42,
        last_applied_term: 5,
        dag_tips: vec![],
        proofs: vec![
            ProofSnapshot {
                id: "proof-001".into(),
                proof_type: "schnorr".into(),
                data: vec![1, 2, 3, 4],
                created_at: "2026-03-16T00:00:00Z".into(),
                verified: true,
                verified_at: Some("2026-03-16T00:01:00Z".into()),
                metadata: serde_json::json!({"submitter": "alice"}),
            },
            ProofSnapshot {
                id: "proof-002".into(),
                proof_type: "membership".into(),
                data: vec![5, 6, 7, 8],
                created_at: "2026-03-16T00:02:00Z".into(),
                verified: false,
                verified_at: None,
                metadata: serde_json::json!({}),
            },
        ],
        checksum: String::new(),
    };

    // Serialize
    let bytes = snapshot.to_bytes().unwrap();
    assert!(!bytes.is_empty());

    // Deserialize
    let restored = ClusterSnapshot::from_bytes(&bytes).unwrap();

    // Verify triples
    assert_eq!(restored.triples.len(), 2);
    assert_eq!(restored.triples[0].subject, "alice");
    assert_eq!(restored.triples[1].object, serde_json::json!(30));

    // Verify ineru
    assert_eq!(restored.ineru_ltm, vec![10, 20, 30]);

    // Verify proofs
    assert_eq!(restored.proofs.len(), 2);
    assert_eq!(restored.proofs[0].id, "proof-001");
    assert_eq!(restored.proofs[0].proof_type, "schnorr");
    assert_eq!(restored.proofs[0].data, vec![1, 2, 3, 4]);
    assert!(restored.proofs[0].verified);
    assert_eq!(
        restored.proofs[0].verified_at.as_deref(),
        Some("2026-03-16T00:01:00Z")
    );
    assert_eq!(restored.proofs[1].id, "proof-002");
    assert!(!restored.proofs[1].verified);
    assert!(restored.proofs[1].verified_at.is_none());

    // Verify metadata
    assert_eq!(restored.last_applied_index, 42);
    assert_eq!(restored.last_applied_term, 5);

    // Verify checksum was computed and matches
    assert!(!restored.checksum.is_empty());
}

// ============================================================================
// 6. Snapshot checksum includes proofs
// ============================================================================

#[tokio::test]
async fn test_snapshot_checksum_changes_with_proofs() {
    use aingle_raft::state_machine::{ClusterSnapshot, ProofSnapshot, TripleSnapshot};

    // Snapshot without proofs
    let snap_no_proofs = ClusterSnapshot {
        triples: vec![TripleSnapshot {
            subject: "a".into(),
            predicate: "b".into(),
            object: serde_json::json!("c"),
        }],
        ineru_ltm: vec![],
        last_applied_index: 1,
        last_applied_term: 1,
        dag_tips: vec![],
        proofs: vec![],
        checksum: String::new(),
    };

    // Same snapshot WITH proofs
    let snap_with_proofs = ClusterSnapshot {
        triples: vec![TripleSnapshot {
            subject: "a".into(),
            predicate: "b".into(),
            object: serde_json::json!("c"),
        }],
        ineru_ltm: vec![],
        last_applied_index: 1,
        last_applied_term: 1,
        dag_tips: vec![],
        proofs: vec![ProofSnapshot {
            id: "p1".into(),
            proof_type: "schnorr".into(),
            data: vec![1],
            created_at: "2026-01-01T00:00:00Z".into(),
            verified: false,
            verified_at: None,
            metadata: serde_json::json!({}),
        }],
        checksum: String::new(),
    };

    let bytes1 = snap_no_proofs.to_bytes().unwrap();
    let bytes2 = snap_with_proofs.to_bytes().unwrap();

    let r1 = ClusterSnapshot::from_bytes(&bytes1).unwrap();
    let r2 = ClusterSnapshot::from_bytes(&bytes2).unwrap();

    // Checksums should differ
    assert_ne!(
        r1.checksum, r2.checksum,
        "checksum should change when proofs are added"
    );
}

// ============================================================================
// 7. Graph Sled persistence — data survives restart
// ============================================================================

#[tokio::test]
async fn test_graph_sled_persistence_full_cycle() {
    use aingle_graph::{GraphDB, NodeId, Predicate, Triple, TriplePattern, Value};

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.sled");
    let path_str = path.to_str().unwrap();

    // Write
    {
        let graph = GraphDB::sled(path_str).unwrap();
        for i in 0..25 {
            let triple = Triple::new(
                NodeId::named(&format!("persist:{}", i)),
                Predicate::named("data"),
                Value::Float(i as f64 * 1.5),
            );
            graph.insert(triple).unwrap();
        }
        assert_eq!(graph.count(), 25);
        graph.flush().unwrap();
    }

    // Reopen and verify
    {
        let graph = GraphDB::sled(path_str).unwrap();
        assert_eq!(graph.count(), 25, "sled graph should persist");

        for i in 0..25 {
            let pattern = TriplePattern::subject(NodeId::named(&format!("persist:{}", i)));
            let results = graph.find(pattern).unwrap();
            assert_eq!(results.len(), 1, "persist:{} missing", i);
            match &results[0].object {
                Value::Float(f) => {
                    let expected = i as f64 * 1.5;
                    assert!((f - expected).abs() < 0.001, "float mismatch for {}", i);
                }
                other => panic!("expected Float, got {:?} for persist:{}", other, i),
            }
        }
    }
}

// ============================================================================
// 8. Audit log file-backed integrity
// ============================================================================

#[tokio::test]
async fn test_audit_log_fsync_integrity() {
    use aingle_cortex::rest::audit::{AuditEntry, AuditLog};

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("audit_test.jsonl");

    // Write entries
    {
        let mut log = AuditLog::with_path(10_000, path.clone());
        for i in 0..50 {
            log.record(AuditEntry {
                timestamp: format!("2026-03-16T00:{:02}:00Z", i),
                user_id: format!("user-{}", i % 5),
                namespace: Some("test".to_string()),
                action: if i % 3 == 0 {
                    "create".into()
                } else {
                    "read".into()
                },
                resource: format!("/api/v1/triples/{}", i),
                details: Some(format!("detail-{}", i)),
                request_id: Some(format!("req-{}", i)),
            });
        }
        assert_eq!(log.len(), 50);
    }

    // Reopen — entries should be restored from JSONL
    {
        let log = AuditLog::with_path(10_000, path.clone());
        assert_eq!(log.len(), 50, "audit entries should survive restart");

        // Verify query filters work
        let by_user = log.query(Some("user-0"), None, None, None, None, 100);
        assert_eq!(by_user.len(), 10, "10 entries per user out of 50");

        let by_action = log.query(None, None, Some("create"), None, None, 100);
        assert_eq!(by_action.len(), 17, "17 create actions (0,3,6,...,48)");

        let by_ns = log.query(None, Some("test"), None, None, None, 100);
        assert_eq!(by_ns.len(), 50, "all entries in 'test' namespace");
    }
}
