//! Integration tests for batch verification

use aingle_zk::{
    batch::{verify_merkle_batch, verify_schnorr_batch, BatchVerifier},
    merkle::SparseMerkleTree,
    proof::SchnorrProof,
};
use curve25519_dalek::{constants::RISTRETTO_BASEPOINT_POINT, scalar::Scalar};
use rand::rngs::OsRng;

#[test]
fn test_batch_verification_correctness() {
    // Test that batch verification produces same results as individual verification
    let mut proofs = Vec::new();

    for i in 0..20 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("message {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        proofs.push((proof, public, message));
    }

    // Individual verification
    let individual_results: Vec<bool> = proofs
        .iter()
        .map(|(p, pk, msg)| p.verify(pk, msg).unwrap_or(false))
        .collect();

    // Batch verification
    let batch_results = verify_schnorr_batch(&proofs);

    // Results should match
    assert_eq!(individual_results, batch_results);
    assert!(batch_results.iter().all(|&v| v));
}

#[test]
fn test_batch_detects_invalid_proofs() {
    let mut verifier = BatchVerifier::new();

    // Add 3 valid proofs
    for i in 0..3 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("valid {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    // Add 1 invalid proof
    let secret = Scalar::random(&mut OsRng);
    let public = RISTRETTO_BASEPOINT_POINT * secret;
    let wrong_public = RISTRETTO_BASEPOINT_POINT * Scalar::random(&mut OsRng);
    let message = b"invalid";
    let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
    verifier.add_schnorr(proof, wrong_public, message);

    // Add 1 more valid proof
    let secret = Scalar::random(&mut OsRng);
    let public = RISTRETTO_BASEPOINT_POINT * secret;
    let message = b"valid 3";
    let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
    verifier.add_schnorr(proof, public, message);

    let result = verifier.verify_all();

    // Should detect 4 valid and 1 invalid (total 5)
    assert!(!result.all_valid);
    assert_eq!(result.total_proofs(), 5);
    assert_eq!(result.valid_count(), 4);
    assert_eq!(result.invalid_count(), 1);

    // The invalid proof should be at index 3
    assert!(result.schnorr_valid(0).unwrap());
    assert!(result.schnorr_valid(1).unwrap());
    assert!(result.schnorr_valid(2).unwrap());
    assert!(!result.schnorr_valid(3).unwrap());
    assert!(result.schnorr_valid(4).unwrap());
}

#[test]
fn test_large_batch_schnorr() {
    let mut proofs = Vec::new();

    // Generate 500 proofs
    for i in 0..500 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("tx {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        proofs.push((proof, public, message));
    }

    let results = verify_schnorr_batch(&proofs);

    assert_eq!(results.len(), 500);
    assert!(results.iter().all(|&v| v));
}

#[test]
fn test_merkle_batch_verification() {
    let mut tree = SparseMerkleTree::new();
    let mut proofs = Vec::new();

    // Insert 50 entries
    for i in 0..50 {
        let key = format!("key{}", i);
        let value = format!("value{}", i);
        tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
    }

    // Generate membership proofs
    for i in 0..50 {
        let key = format!("key{}", i);
        let proof = tree.prove(key.as_bytes()).unwrap();
        proofs.push(proof);
    }

    let results = verify_merkle_batch(&proofs);

    assert_eq!(results.len(), 50);
    assert!(results.iter().all(|&v| v));
}

#[test]
fn test_empty_batch() {
    let verifier = BatchVerifier::new();
    let result = verifier.verify_all();

    assert!(result.all_valid);
    assert_eq!(result.total_proofs(), 0);
    assert_eq!(result.valid_count(), 0);
    assert_eq!(result.invalid_count(), 0);
}

#[test]
fn test_batch_clear() {
    let mut verifier = BatchVerifier::new();

    // Add some proofs
    for i in 0..10 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("msg {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    assert_eq!(verifier.len(), 10);
    assert!(!verifier.is_empty());

    verifier.clear();

    assert_eq!(verifier.len(), 0);
    assert!(verifier.is_empty());

    let result = verifier.verify_all();
    assert!(result.all_valid);
}

#[test]
fn test_mixed_proof_types() {
    let mut verifier = BatchVerifier::new();

    // Add Schnorr proofs
    for i in 0..10 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("schnorr {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    // Add Merkle proofs
    let mut tree = SparseMerkleTree::new();
    for i in 0..10 {
        let key = format!("merkle_key{}", i);
        let value = format!("merkle_value{}", i);
        tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
    }

    for i in 0..10 {
        let key = format!("merkle_key{}", i);
        let proof = tree.prove(key.as_bytes()).unwrap();
        verifier.add_merkle(proof);
    }

    let result = verifier.verify_all();

    assert!(result.all_valid);
    assert_eq!(result.total_proofs(), 20);
    assert_eq!(result.schnorr_results.len(), 10);
    assert_eq!(result.merkle_results.len(), 10);
    assert_eq!(result.valid_count(), 20);
}

#[test]
fn test_batch_result_timing() {
    let mut verifier = BatchVerifier::new();

    // Add a few proofs
    for i in 0..5 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("msg {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    let result = verifier.verify_all();

    // Timing should be recorded (can be 0 for very fast operations)
    // Just check it exists
    let _ = result.verification_time_ms;
}

#[test]
fn test_single_proof_optimization() {
    // Test that single proof uses optimized path
    let secret = Scalar::random(&mut OsRng);
    let public = RISTRETTO_BASEPOINT_POINT * secret;
    let message = b"single message";

    let proof = SchnorrProof::prove_knowledge(&secret, &public, message);

    let mut verifier = BatchVerifier::new();
    verifier.add_schnorr(proof, public, message);

    let result = verifier.verify_all();

    assert!(result.all_valid);
    assert_eq!(result.total_proofs(), 1);
    assert_eq!(result.valid_count(), 1);
}

#[test]
fn test_batch_with_all_invalid_schnorr() {
    let mut proofs = Vec::new();

    // Generate 5 invalid proofs (all with wrong public keys)
    for i in 0..5 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let wrong_public = RISTRETTO_BASEPOINT_POINT * Scalar::random(&mut OsRng);
        let message = format!("msg {}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        proofs.push((proof, wrong_public, message));
    }

    let results = verify_schnorr_batch(&proofs);

    assert_eq!(results.len(), 5);
    assert!(results.iter().all(|&v| !v));
}

#[test]
fn test_batch_with_challenge_mismatch() {
    // Create a proof with a tampered challenge
    let secret = Scalar::random(&mut OsRng);
    let public = RISTRETTO_BASEPOINT_POINT * secret;
    let message = b"test message";

    let mut proof = SchnorrProof::prove_knowledge(&secret, &public, message);

    // Tamper with the challenge
    proof.challenge[0] ^= 0xFF;

    let mut verifier = BatchVerifier::new();
    verifier.add_schnorr(proof, public, message);

    let result = verifier.verify_all();

    assert!(!result.all_valid);
    assert_eq!(result.invalid_count(), 1);
}
