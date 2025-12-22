//! Batch Verification Example
//!
//! This example demonstrates the efficiency gains from batch verification.
//! Run with: cargo run --example batch_verification --release

use aingle_zk::{
    batch::{verify_schnorr_batch, BatchVerifier},
    merkle::SparseMerkleTree,
    proof::{EqualityProof, SchnorrProof},
};
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT, ristretto::RistrettoPoint, scalar::Scalar,
};
use rand::rngs::OsRng;
use sha2::{Digest, Sha512};
use std::time::Instant;

/// Helper function to get second generator H
fn generator_h() -> RistrettoPoint {
    let mut hasher = Sha512::new();
    hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
    hasher.update(b"aingle_zk_pedersen_h");
    RistrettoPoint::from_uniform_bytes(&hasher.finalize().into())
}

fn main() {
    println!("=== AIngle ZK Batch Verification Example ===\n");

    // Example 1: Schnorr Proof Batch Verification
    println!("1. Schnorr Proof Verification");
    println!("   Generating 100 Schnorr proofs...");

    let mut schnorr_proofs = Vec::new();
    for i in 0..100 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("Transaction #{}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        schnorr_proofs.push((proof, public, message));
    }

    // Individual verification
    let start = Instant::now();
    let mut valid_count = 0;
    for (proof, pubkey, message) in &schnorr_proofs {
        if proof.verify(pubkey, message).unwrap_or(false) {
            valid_count += 1;
        }
    }
    let individual_time = start.elapsed();
    println!(
        "   Individual verification: {} valid in {:?}",
        valid_count, individual_time
    );

    // Batch verification
    let start = Instant::now();
    let results = verify_schnorr_batch(&schnorr_proofs);
    let batch_time = start.elapsed();
    let valid_count = results.iter().filter(|&&v| v).count();
    println!(
        "   Batch verification: {} valid in {:?}",
        valid_count, batch_time
    );

    let speedup = individual_time.as_micros() as f64 / batch_time.as_micros() as f64;
    println!("   ⚡ Speedup: {:.2}x faster\n", speedup);

    // Example 2: Mixed Batch Verification
    println!("2. Mixed Batch Verification (Schnorr + Equality + Merkle)");

    let mut verifier = BatchVerifier::new();

    // Add Schnorr proofs
    println!("   Adding 30 Schnorr proofs...");
    for i in 0..30 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("Signature #{}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    // Add Equality proofs
    println!("   Adding 30 Equality proofs...");
    let h = generator_h();
    for i in 0..30 {
        let value = 1000u64 + i;
        let r1 = Scalar::random(&mut OsRng);
        let r2 = Scalar::random(&mut OsRng);

        let g = RISTRETTO_BASEPOINT_POINT;
        let v = Scalar::from(value);

        let c1 = g * v + h * r1;
        let c2 = g * v + h * r2;

        let proof = EqualityProof::prove_equality(value, &r1, &r2, &c1, &c2);
        verifier.add_equality(proof, c1, c2);
    }

    // Add Merkle proofs
    println!("   Adding 40 Merkle membership proofs...");
    let mut tree = SparseMerkleTree::new();
    for i in 0..40 {
        let key = format!("account_{}", i);
        let value = format!("balance_{}", i * 100);
        tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
    }

    for i in 0..40 {
        let key = format!("account_{}", i);
        let proof = tree.prove(key.as_bytes()).unwrap();
        verifier.add_merkle(proof);
    }

    println!("   Verifying all 100 proofs...");
    let result = verifier.verify_all();

    println!("\n   Results:");
    println!("   - Total proofs: {}", result.total_proofs());
    println!("   - Valid proofs: {}", result.valid_count());
    println!("   - Invalid proofs: {}", result.invalid_count());
    println!("   - Verification time: {}ms", result.verification_time_ms);
    println!("   - All valid: {}", result.all_valid);

    println!("\n   Breakdown:");
    println!(
        "   - Schnorr: {}/{} valid",
        result.schnorr_results.iter().filter(|&&v| v).count(),
        result.schnorr_results.len()
    );
    println!(
        "   - Equality: {}/{} valid",
        result.equality_results.iter().filter(|&&v| v).count(),
        result.equality_results.len()
    );
    println!(
        "   - Merkle: {}/{} valid",
        result.merkle_results.iter().filter(|&&v| v).count(),
        result.merkle_results.len()
    );

    // Example 3: Detecting Invalid Proofs
    println!("\n3. Detecting Invalid Proofs");

    let mut verifier = BatchVerifier::new();

    // Add 5 valid proofs
    for i in 0..5 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("Valid #{}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    // Add 1 invalid proof (wrong public key)
    let secret = Scalar::random(&mut OsRng);
    let public = RISTRETTO_BASEPOINT_POINT * secret;
    let wrong_public = RISTRETTO_BASEPOINT_POINT * Scalar::random(&mut OsRng);
    let message = b"Invalid proof";
    let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
    verifier.add_schnorr(proof, wrong_public, message);

    // Add 4 more valid proofs
    for i in 5..9 {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = format!("Valid #{}", i).into_bytes();
        let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
        verifier.add_schnorr(proof, public, &message);
    }

    let result = verifier.verify_all();

    println!("   Added 10 proofs (9 valid, 1 invalid)");
    println!(
        "   Batch verification detected: {} valid, {} invalid",
        result.valid_count(),
        result.invalid_count()
    );

    println!("   Individual results:");
    for (i, valid) in result.schnorr_results.iter().enumerate() {
        println!(
            "     Proof #{}: {}",
            i,
            if *valid { "✓ valid" } else { "✗ INVALID" }
        );
    }

    // Example 4: Performance Scaling
    println!("\n4. Performance Scaling Test");
    println!("   Testing batch sizes: 10, 50, 100, 200, 500\n");

    for size in [10, 50, 100, 200, 500].iter() {
        let proofs: Vec<_> = (0..*size)
            .map(|i| {
                let secret = Scalar::random(&mut OsRng);
                let public = RISTRETTO_BASEPOINT_POINT * secret;
                let message = format!("msg {}", i).into_bytes();
                let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
                (proof, public, message)
            })
            .collect();

        let start = Instant::now();
        let results = verify_schnorr_batch(&proofs);
        let batch_time = start.elapsed();

        let valid_count = results.iter().filter(|&&v| v).count();
        println!(
            "   Size {:3}: {}ms for {} proofs ({:.2}µs per proof)",
            size,
            batch_time.as_millis(),
            valid_count,
            batch_time.as_micros() as f64 / *size as f64
        );
    }

    println!("\n=== Example Complete ===");
}
