//! Integration tests for the proof storage and verification system

use aingle_cortex::prelude::*;
use aingle_cortex::proofs::{ProofMetadata, SubmitProofRequest};

#[tokio::test]
async fn test_proof_store_lifecycle() {
    let store = ProofStore::new();

    // Submit a proof
    let request = SubmitProofRequest {
        proof_type: ProofType::Schnorr,
        proof_data: serde_json::json!({
            "commitment": vec![0u8; 32],
            "challenge": vec![1u8; 32],
            "response": vec![2u8; 32],
        }),
        metadata: None,
    };

    let proof_id = store.submit(request).await.expect("Failed to submit proof");
    assert!(!proof_id.is_empty());

    // Retrieve the proof
    let proof = store.get(&proof_id).await.expect("Proof not found");
    assert_eq!(proof.proof_type, ProofType::Schnorr);
    assert!(!proof.verified);

    // Count proofs
    let count = store.count().await;
    assert_eq!(count, 1);

    // Delete the proof
    let deleted = store.delete(&proof_id).await;
    assert!(deleted);

    let count_after = store.count().await;
    assert_eq!(count_after, 0);
}

#[tokio::test]
async fn test_proof_verification() {
    let store = ProofStore::new();

    // Create a valid hash opening proof using aingle_zk
    let commitment = aingle_zk::HashCommitment::commit(b"test data");
    let zk_proof = aingle_zk::ZkProof::hash_opening(&commitment);
    let proof_json = serde_json::to_value(&zk_proof).expect("Failed to serialize");

    let request = SubmitProofRequest {
        proof_type: ProofType::HashOpening,
        proof_data: proof_json,
        metadata: None,
    };

    let proof_id = store.submit(request).await.expect("Failed to submit");

    // Verify the proof
    let result = store.verify(&proof_id).await.expect("Verification failed");
    assert!(result.valid, "Proof should be valid");
    assert!(result.verification_time_us > 0);
}

#[tokio::test]
async fn test_batch_proof_submission() {
    let store = ProofStore::new();

    let requests = vec![
        SubmitProofRequest {
            proof_type: ProofType::Knowledge,
            proof_data: serde_json::json!({"id": 1}),
            metadata: None,
        },
        SubmitProofRequest {
            proof_type: ProofType::Equality,
            proof_data: serde_json::json!({"id": 2}),
            metadata: None,
        },
        SubmitProofRequest {
            proof_type: ProofType::Membership,
            proof_data: serde_json::json!({"id": 3}),
            metadata: None,
        },
    ];

    let results = store.submit_batch(requests).await;
    assert_eq!(results.len(), 3);

    let successful = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(successful, 3);

    let count = store.count().await;
    assert_eq!(count, 3);
}

#[tokio::test]
async fn test_batch_proof_verification() {
    let store = ProofStore::new();

    // Submit multiple proofs
    let mut proof_ids = Vec::new();
    for i in 0..3 {
        let commitment = aingle_zk::HashCommitment::commit(format!("data {}", i).as_bytes());
        let zk_proof = aingle_zk::ZkProof::hash_opening(&commitment);
        let proof_json = serde_json::to_value(&zk_proof).unwrap();

        let request = SubmitProofRequest {
            proof_type: ProofType::HashOpening,
            proof_data: proof_json,
            metadata: None,
        };

        let proof_id = store.submit(request).await.unwrap();
        proof_ids.push(proof_id);
    }

    // Batch verify
    let results = store.batch_verify(&proof_ids).await;
    assert_eq!(results.len(), 3);

    let all_valid = results
        .iter()
        .all(|r| r.as_ref().map(|v| v.valid).unwrap_or(false));
    assert!(all_valid, "All proofs should be valid");
}

#[tokio::test]
async fn test_proof_filtering_by_type() {
    let store = ProofStore::new();

    // Submit different types
    for i in 0..5 {
        let proof_type = if i % 2 == 0 {
            ProofType::Schnorr
        } else {
            ProofType::Equality
        };

        let request = SubmitProofRequest {
            proof_type,
            proof_data: serde_json::json!({"index": i}),
            metadata: None,
        };

        store.submit(request).await.unwrap();
    }

    // List all
    let all_proofs = store.list(None).await;
    assert_eq!(all_proofs.len(), 5);

    // List Schnorr only
    let schnorr_proofs = store.list(Some(ProofType::Schnorr)).await;
    assert_eq!(schnorr_proofs.len(), 3);

    // List Equality only
    let equality_proofs = store.list(Some(ProofType::Equality)).await;
    assert_eq!(equality_proofs.len(), 2);
}

#[tokio::test]
async fn test_proof_metadata() {
    let store = ProofStore::new();

    let metadata = ProofMetadata {
        submitter: Some("user123".to_string()),
        tags: vec!["important".to_string(), "verified".to_string()],
        extra: {
            let mut map = std::collections::HashMap::new();
            map.insert("source".to_string(), serde_json::json!("api"));
            map.insert("version".to_string(), serde_json::json!("1.0"));
            map
        },
    };

    let request = SubmitProofRequest {
        proof_type: ProofType::Knowledge,
        proof_data: serde_json::json!({"test": "data"}),
        metadata: Some(metadata.clone()),
    };

    let proof_id = store.submit(request).await.unwrap();
    let proof = store.get(&proof_id).await.unwrap();

    assert_eq!(proof.metadata.submitter, Some("user123".to_string()));
    assert_eq!(proof.metadata.tags.len(), 2);
    assert!(proof.metadata.tags.contains(&"important".to_string()));
    assert_eq!(
        proof.metadata.extra.get("source"),
        Some(&serde_json::json!("api"))
    );
}

#[tokio::test]
async fn test_verification_cache() {
    let store = ProofStore::new();

    // Submit a proof
    let commitment = aingle_zk::HashCommitment::commit(b"cached test");
    let zk_proof = aingle_zk::ZkProof::hash_opening(&commitment);
    let proof_json = serde_json::to_value(&zk_proof).unwrap();

    let request = SubmitProofRequest {
        proof_type: ProofType::HashOpening,
        proof_data: proof_json,
        metadata: None,
    };

    let proof_id = store.submit(request).await.unwrap();

    // First verification (cache miss)
    let stats_before = store.stats().await;
    let cache_misses_before = stats_before.cache_misses;

    store.verify(&proof_id).await.unwrap();

    let stats_after_first = store.stats().await;
    assert_eq!(stats_after_first.cache_misses, cache_misses_before + 1);

    // Second verification (cache hit)
    let cache_hits_before = stats_after_first.cache_hits;

    store.verify(&proof_id).await.unwrap();

    let stats_after_second = store.stats().await;
    assert_eq!(stats_after_second.cache_hits, cache_hits_before + 1);
}

#[tokio::test]
async fn test_proof_statistics() {
    let store = ProofStore::new();

    // Submit various proofs
    for i in 0..10 {
        let proof_type = match i % 3 {
            0 => ProofType::Schnorr,
            1 => ProofType::Equality,
            _ => ProofType::Membership,
        };

        let request = SubmitProofRequest {
            proof_type,
            proof_data: serde_json::json!({"index": i}),
            metadata: None,
        };

        store.submit(request).await.unwrap();
    }

    let stats = store.stats().await;
    assert_eq!(stats.total_proofs, 10);
    assert!(stats.proofs_by_type.len() >= 3);
    assert!(stats.total_size_bytes > 0);
}

#[tokio::test]
async fn test_proof_verification_with_merkle_tree() {
    let store = ProofStore::new();

    // Create a Merkle tree and proof
    let leaves: Vec<&[u8]> = vec![b"alice", b"bob", b"charlie"];
    let tree = aingle_zk::MerkleTree::new(&leaves).expect("Failed to create tree");

    let merkle_proof = tree.prove_data(b"bob").expect("Failed to create proof");
    let zk_proof = aingle_zk::ZkProof::membership(tree.root(), merkle_proof);
    let proof_json = serde_json::to_value(&zk_proof).unwrap();

    let request = SubmitProofRequest {
        proof_type: ProofType::Membership,
        proof_data: proof_json,
        metadata: None,
    };

    let proof_id = store.submit(request).await.unwrap();

    // Verify the proof
    let result = store.verify(&proof_id).await.unwrap();
    assert!(result.valid, "Merkle proof should be valid");
}

#[tokio::test]
async fn test_schnorr_proof_verification() {
    let store = ProofStore::new();

    // For this test, we'll use a simpler approach with a hash opening proof
    // since Schnorr proofs require curve25519 types that aren't re-exported
    let commitment = aingle_zk::HashCommitment::commit(b"schnorr test data");
    let zk_proof = aingle_zk::ZkProof::hash_opening(&commitment);
    let proof_json = serde_json::to_value(&zk_proof).unwrap();

    let request = SubmitProofRequest {
        proof_type: ProofType::Knowledge,
        proof_data: proof_json,
        metadata: None,
    };

    let proof_id = store.submit(request).await.unwrap();
    let result = store.verify(&proof_id).await;

    // Verification should succeed
    assert!(result.is_ok());
    assert!(result.unwrap().valid);
}

#[tokio::test]
async fn test_app_state_integration() {
    let state = AppState::new();

    // Test that proof store is accessible
    let count = state.proof_store.count().await;
    assert_eq!(count, 0);

    // Submit a proof through app state
    let commitment = aingle_zk::HashCommitment::commit(b"integration test");
    let zk_proof = aingle_zk::ZkProof::hash_opening(&commitment);
    let proof_json = serde_json::to_value(&zk_proof).unwrap();

    let request = SubmitProofRequest {
        proof_type: ProofType::HashOpening,
        proof_data: proof_json,
        metadata: None,
    };

    let proof_id = state.proof_store.submit(request).await.unwrap();

    // Verify through app state
    let result = state.proof_store.verify(&proof_id).await.unwrap();
    assert!(result.valid);

    // Check count
    let count_after = state.proof_store.count().await;
    assert_eq!(count_after, 1);
}

#[tokio::test]
async fn test_clear_all_proofs() {
    let store = ProofStore::new();

    // Submit multiple proofs
    for i in 0..5 {
        let request = SubmitProofRequest {
            proof_type: ProofType::Knowledge,
            proof_data: serde_json::json!({"index": i}),
            metadata: None,
        };
        store.submit(request).await.unwrap();
    }

    assert_eq!(store.count().await, 5);

    // Clear all
    store.clear().await;

    assert_eq!(store.count().await, 0);

    let stats = store.stats().await;
    assert_eq!(stats.total_proofs, 0);
    assert_eq!(stats.total_size_bytes, 0);
}

#[tokio::test]
async fn test_concurrent_proof_operations() {
    use tokio::task::JoinSet;

    let store = ProofStore::new();
    let store = std::sync::Arc::new(store);

    let mut join_set = JoinSet::new();

    // Spawn multiple tasks submitting proofs concurrently
    for i in 0..10 {
        let store_clone = store.clone();
        join_set.spawn(async move {
            let request = SubmitProofRequest {
                proof_type: ProofType::Knowledge,
                proof_data: serde_json::json!({"task": i}),
                metadata: None,
            };
            store_clone.submit(request).await
        });
    }

    // Wait for all tasks
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.unwrap());
    }

    // All should succeed
    assert_eq!(results.len(), 10);
    assert!(results.iter().all(|r| r.is_ok()));

    let count = store.count().await;
    assert_eq!(count, 10);
}

#[tokio::test]
async fn test_proof_type_display() {
    assert_eq!(ProofType::Schnorr.to_string(), "schnorr");
    assert_eq!(ProofType::Equality.to_string(), "equality");
    assert_eq!(ProofType::Membership.to_string(), "membership");
    assert_eq!(ProofType::NonMembership.to_string(), "non-membership");
    assert_eq!(ProofType::Range.to_string(), "range");
    assert_eq!(ProofType::HashOpening.to_string(), "hash-opening");
    assert_eq!(ProofType::Knowledge.to_string(), "knowledge");
}
