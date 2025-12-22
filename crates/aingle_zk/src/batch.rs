//! Batch verification for zero-knowledge proofs
//!
//! Batch verification allows verifying multiple proofs more efficiently than
//! verifying them individually. This is especially important for Schnorr proofs
//! where we can use random linear combination to verify multiple proofs with
//! a single elliptic curve equation check.
//!
//! ## Efficiency Gains
//!
//! - **Schnorr proofs**: O(n) individual verifications → O(1) batch verification
//! - **Parallel processing**: Different proof types verified in parallel using rayon
//! - **Merkle proofs**: Independent proofs verified in parallel
//!
//! ## Example
//!
//! ```rust
//! use aingle_zk::batch::BatchVerifier;
//! use aingle_zk::proof::SchnorrProof;
//! use curve25519_dalek::{constants::RISTRETTO_BASEPOINT_POINT, scalar::Scalar};
//! use rand::rngs::OsRng;
//!
//! let mut verifier = BatchVerifier::new();
//!
//! // Add multiple Schnorr proofs
//! for _ in 0..100 {
//!     let secret = Scalar::random(&mut OsRng);
//!     let public = RISTRETTO_BASEPOINT_POINT * secret;
//!     let message = b"test message";
//!     let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
//!     verifier.add_schnorr(proof, public, message);
//! }
//!
//! // Verify all at once (much faster than individual verification)
//! let result = verifier.verify_all();
//! assert!(result.all_valid);
//! ```

use crate::merkle::SparseMerkleProof;
use crate::proof::{EqualityProof, SchnorrProof};

use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
    traits::MultiscalarMul,
};
use rand::{rngs::OsRng, Rng};
use rayon::prelude::*;
#[cfg(test)]
use sha2::Sha512;
use sha2::{Digest, Sha256};
use std::time::Instant;

/// Helper function to get second generator H (same as in commitment.rs and proof.rs)
#[cfg(test)]
fn generator_h() -> RistrettoPoint {
    let mut hasher = Sha512::new();
    hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
    hasher.update(b"aingle_zk_pedersen_h");
    RistrettoPoint::from_uniform_bytes(&hasher.finalize().into())
}

/// Batch verifier for zero-knowledge proofs
///
/// Collects multiple proofs and verifies them efficiently using batch
/// verification techniques and parallel processing.
#[derive(Default)]
pub struct BatchVerifier {
    schnorr_proofs: Vec<(SchnorrProof, RistrettoPoint, Vec<u8>)>,
    equality_proofs: Vec<(EqualityProof, RistrettoPoint, RistrettoPoint)>,
    merkle_proofs: Vec<SparseMerkleProof>,
}

impl BatchVerifier {
    /// Create a new batch verifier
    pub fn new() -> Self {
        Self {
            schnorr_proofs: Vec::new(),
            equality_proofs: Vec::new(),
            merkle_proofs: Vec::new(),
        }
    }

    /// Add a Schnorr proof to the batch
    ///
    /// # Arguments
    /// * `proof` - The Schnorr proof to verify
    /// * `pubkey` - The public key (P = x*G where x is the secret)
    /// * `message` - The message that was signed
    pub fn add_schnorr(&mut self, proof: SchnorrProof, pubkey: RistrettoPoint, message: &[u8]) {
        self.schnorr_proofs.push((proof, pubkey, message.to_vec()));
    }

    /// Add an equality proof to the batch
    ///
    /// # Arguments
    /// * `proof` - The equality proof
    /// * `c1` - First commitment
    /// * `c2` - Second commitment
    pub fn add_equality(&mut self, proof: EqualityProof, c1: RistrettoPoint, c2: RistrettoPoint) {
        self.equality_proofs.push((proof, c1, c2));
    }

    /// Add a sparse Merkle proof to the batch
    pub fn add_merkle(&mut self, proof: SparseMerkleProof) {
        self.merkle_proofs.push(proof);
    }

    /// Get the total number of proofs in the batch
    pub fn len(&self) -> usize {
        self.schnorr_proofs.len() + self.equality_proofs.len() + self.merkle_proofs.len()
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Verify all proofs in the batch
    ///
    /// Uses optimized batch verification for Schnorr proofs and parallel
    /// processing for different proof types.
    ///
    /// Returns detailed results for each proof type.
    pub fn verify_all(&self) -> BatchResult {
        let start = Instant::now();

        // Verify different proof types in parallel using rayon
        let (schnorr_results, (equality_results, merkle_results)) = rayon::join(
            || self.verify_schnorr_batch(),
            || {
                rayon::join(
                    || self.verify_equality_batch(),
                    || self.verify_merkle_batch(),
                )
            },
        );

        let all_valid = schnorr_results.iter().all(|&v| v)
            && equality_results.iter().all(|&v| v)
            && merkle_results.iter().all(|&v| v);

        let verification_time_ms = start.elapsed().as_millis() as u64;

        BatchResult {
            all_valid,
            schnorr_results,
            equality_results,
            merkle_results,
            verification_time_ms,
        }
    }

    /// Verify all Schnorr proofs using batch verification
    ///
    /// This is the most important optimization. Instead of verifying each proof
    /// individually (N elliptic curve multiplications), we use random linear
    /// combination to verify all proofs with a single equation check.
    ///
    /// Mathematical principle:
    /// Instead of checking: s_i*G == R_i + c_i*P_i for each i
    /// We check: sum(z_i*s_i)*G == sum(z_i*R_i) + sum(z_i*c_i*P_i)
    /// where z_i are random coefficients
    ///
    /// This is secure under the discrete logarithm assumption with overwhelming
    /// probability (probability of false acceptance ≈ 1/2^128).
    fn verify_schnorr_batch(&self) -> Vec<bool> {
        if self.schnorr_proofs.is_empty() {
            return Vec::new();
        }

        // Single proof can use optimized path
        if self.schnorr_proofs.len() == 1 {
            let (proof, pubkey, message) = &self.schnorr_proofs[0];
            return vec![proof.verify(pubkey, message).unwrap_or(false)];
        }

        // First pass: validate all challenges and parse all values
        // This must be done sequentially to catch any invalid proofs
        let mut parsed_proofs = Vec::with_capacity(self.schnorr_proofs.len());

        for (proof, pubkey, message) in &self.schnorr_proofs {
            // Verify challenge: c == H(R || P || message)
            let mut hasher = Sha256::new();
            hasher.update(proof.commitment);
            hasher.update(pubkey.compress().as_bytes());
            hasher.update(message);
            let expected_challenge: [u8; 32] = hasher.finalize().into();

            if expected_challenge != proof.challenge {
                // Challenge mismatch - this proof is invalid
                // Return individual results for all proofs
                return self
                    .schnorr_proofs
                    .iter()
                    .map(|(p, pk, msg)| p.verify(pk, msg).unwrap_or(false))
                    .collect();
            }

            // Parse commitment point
            let r = match CompressedRistretto::from_slice(&proof.commitment)
                .ok()
                .and_then(|c| c.decompress())
            {
                Some(r) => r,
                None => {
                    // Invalid point - fall back to individual verification
                    return self
                        .schnorr_proofs
                        .iter()
                        .map(|(p, pk, msg)| p.verify(pk, msg).unwrap_or(false))
                        .collect();
                }
            };

            let c = Scalar::from_bytes_mod_order(proof.challenge);
            let s = Scalar::from_bytes_mod_order(proof.response);

            parsed_proofs.push((r, c, s, *pubkey));
        }

        // Generate random coefficients z_i for linear combination
        // These must be cryptographically secure random values
        let mut rng = OsRng;
        let random_coefficients: Vec<Scalar> = (0..parsed_proofs.len())
            .map(|_| {
                let mut bytes = [0u8; 64];
                rng.fill(&mut bytes);
                Scalar::from_bytes_mod_order_wide(&bytes)
            })
            .collect();

        // Compute batch verification equation:
        // sum(z_i*s_i)*G == sum(z_i*R_i) + sum(z_i*c_i*P_i)

        // Left side: sum(z_i*s_i)*G
        let g = RISTRETTO_BASEPOINT_POINT;
        let mut sum_zs = Scalar::ZERO;
        for (i, (_, _, s, _)) in parsed_proofs.iter().enumerate() {
            sum_zs += random_coefficients[i] * s;
        }
        let lhs = g * sum_zs;

        // Right side: sum(z_i*R_i) + sum(z_i*c_i*P_i)
        // Use multiscalar multiplication for efficiency
        let mut scalars = Vec::with_capacity(parsed_proofs.len() * 2);
        let mut points = Vec::with_capacity(parsed_proofs.len() * 2);

        for (i, (r, c, _, p)) in parsed_proofs.iter().enumerate() {
            let z_i = random_coefficients[i];
            // Add z_i*R_i
            scalars.push(z_i);
            points.push(*r);
            // Add z_i*c_i*P_i
            scalars.push(z_i * c);
            points.push(*p);
        }

        let rhs = RistrettoPoint::multiscalar_mul(scalars.iter(), points.iter());

        // If batch verification passes, all proofs are valid
        let batch_valid = lhs == rhs;

        // Return vector of results - all true if batch valid, otherwise verify individually
        if batch_valid {
            vec![true; self.schnorr_proofs.len()]
        } else {
            // Batch failed - verify each proof individually to identify which ones failed
            self.schnorr_proofs
                .par_iter()
                .map(|(p, pk, msg)| p.verify(pk, msg).unwrap_or(false))
                .collect()
        }
    }

    /// Verify all equality proofs
    ///
    /// Equality proofs are verified individually but in parallel.
    fn verify_equality_batch(&self) -> Vec<bool> {
        if self.equality_proofs.is_empty() {
            return Vec::new();
        }

        // Verify in parallel
        self.equality_proofs
            .par_iter()
            .map(|(proof, _c1, _c2)| proof.verify().unwrap_or(false))
            .collect()
    }

    /// Verify all Merkle proofs in parallel
    ///
    /// Merkle proofs are independent and can be verified in parallel.
    fn verify_merkle_batch(&self) -> Vec<bool> {
        if self.merkle_proofs.is_empty() {
            return Vec::new();
        }

        use crate::merkle::SparseMerkleTree;

        // Verify in parallel
        self.merkle_proofs
            .par_iter()
            .map(|proof| SparseMerkleTree::verify_proof(proof, &proof.root))
            .collect()
    }

    /// Clear all proofs from the batch
    pub fn clear(&mut self) {
        self.schnorr_proofs.clear();
        self.equality_proofs.clear();
        self.merkle_proofs.clear();
    }
}

/// Result of batch verification
#[derive(Debug, Clone)]
pub struct BatchResult {
    /// True if all proofs are valid
    pub all_valid: bool,
    /// Individual results for Schnorr proofs
    pub schnorr_results: Vec<bool>,
    /// Individual results for equality proofs
    pub equality_results: Vec<bool>,
    /// Individual results for Merkle proofs
    pub merkle_results: Vec<bool>,
    /// Time taken to verify all proofs (in milliseconds)
    pub verification_time_ms: u64,
}

impl BatchResult {
    /// Get the total number of proofs verified
    pub fn total_proofs(&self) -> usize {
        self.schnorr_results.len() + self.equality_results.len() + self.merkle_results.len()
    }

    /// Get the number of valid proofs
    pub fn valid_count(&self) -> usize {
        self.schnorr_results.iter().filter(|&&v| v).count()
            + self.equality_results.iter().filter(|&&v| v).count()
            + self.merkle_results.iter().filter(|&&v| v).count()
    }

    /// Get the number of invalid proofs
    pub fn invalid_count(&self) -> usize {
        self.total_proofs() - self.valid_count()
    }

    /// Check if a specific Schnorr proof is valid
    pub fn schnorr_valid(&self, index: usize) -> Option<bool> {
        self.schnorr_results.get(index).copied()
    }

    /// Check if a specific equality proof is valid
    pub fn equality_valid(&self, index: usize) -> Option<bool> {
        self.equality_results.get(index).copied()
    }

    /// Check if a specific Merkle proof is valid
    pub fn merkle_valid(&self, index: usize) -> Option<bool> {
        self.merkle_results.get(index).copied()
    }
}

/// Verify a batch of Schnorr proofs
///
/// Convenience function for verifying only Schnorr proofs without
/// creating a BatchVerifier.
pub fn verify_schnorr_batch(proofs: &[(SchnorrProof, RistrettoPoint, Vec<u8>)]) -> Vec<bool> {
    let mut verifier = BatchVerifier::new();
    for (proof, pubkey, message) in proofs {
        verifier.add_schnorr(proof.clone(), *pubkey, message);
    }
    verifier.verify_all().schnorr_results
}

/// Verify a batch of equality proofs
///
/// Convenience function for verifying only equality proofs.
pub fn verify_equality_batch(
    proofs: &[(EqualityProof, RistrettoPoint, RistrettoPoint)],
) -> Vec<bool> {
    let mut verifier = BatchVerifier::new();
    for (proof, c1, c2) in proofs {
        verifier.add_equality(proof.clone(), *c1, *c2);
    }
    verifier.verify_all().equality_results
}

/// Verify a batch of Merkle proofs
///
/// Convenience function for verifying only Merkle proofs.
pub fn verify_merkle_batch(proofs: &[SparseMerkleProof]) -> Vec<bool> {
    let mut verifier = BatchVerifier::new();
    for proof in proofs {
        verifier.add_merkle(proof.clone());
    }
    verifier.verify_all().merkle_results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle::SparseMerkleTree;
    use curve25519_dalek::scalar::Scalar;

    #[test]
    fn test_batch_verifier_empty() {
        let verifier = BatchVerifier::new();
        assert_eq!(verifier.len(), 0);
        assert!(verifier.is_empty());

        let result = verifier.verify_all();
        assert!(result.all_valid);
        assert_eq!(result.total_proofs(), 0);
    }

    #[test]
    fn test_single_schnorr_proof() {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = b"test message";

        let proof = SchnorrProof::prove_knowledge(&secret, &public, message);

        let mut verifier = BatchVerifier::new();
        verifier.add_schnorr(proof, public, message);

        let result = verifier.verify_all();
        assert!(result.all_valid);
        assert_eq!(result.schnorr_results.len(), 1);
        assert!(result.schnorr_results[0]);
    }

    #[test]
    fn test_batch_100_schnorr_proofs() {
        let mut verifier = BatchVerifier::new();

        // Generate 100 valid Schnorr proofs
        for i in 0..100 {
            let secret = Scalar::random(&mut OsRng);
            let public = RISTRETTO_BASEPOINT_POINT * secret;
            let message = format!("message {}", i);

            let proof = SchnorrProof::prove_knowledge(&secret, &public, message.as_bytes());
            verifier.add_schnorr(proof, public, message.as_bytes());
        }

        let result = verifier.verify_all();
        assert!(result.all_valid);
        assert_eq!(result.schnorr_results.len(), 100);
        assert!(result.schnorr_results.iter().all(|&v| v));
        assert_eq!(result.valid_count(), 100);
        assert_eq!(result.invalid_count(), 0);
    }

    #[test]
    fn test_batch_with_invalid_schnorr_proof() {
        let mut verifier = BatchVerifier::new();

        // Add valid proofs
        for i in 0..5 {
            let secret = Scalar::random(&mut OsRng);
            let public = RISTRETTO_BASEPOINT_POINT * secret;
            let message = format!("message {}", i);

            let proof = SchnorrProof::prove_knowledge(&secret, &public, message.as_bytes());
            verifier.add_schnorr(proof, public, message.as_bytes());
        }

        // Add an invalid proof (wrong public key)
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let wrong_public = RISTRETTO_BASEPOINT_POINT * Scalar::random(&mut OsRng);
        let message = b"invalid message";

        let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
        verifier.add_schnorr(proof, wrong_public, message); // Wrong public key

        let result = verifier.verify_all();
        assert!(!result.all_valid);
        assert_eq!(result.schnorr_results.len(), 6);

        // First 5 should be valid, last one invalid
        assert!(result.schnorr_results[0..5].iter().all(|&v| v));
        assert!(!result.schnorr_results[5]);
    }

    #[test]
    fn test_batch_100_merkle_proofs() {
        let mut tree = SparseMerkleTree::new();
        let mut verifier = BatchVerifier::new();

        // Insert 100 key-value pairs and generate proofs
        for i in 0..100 {
            let key = format!("key{}", i);
            let value = format!("value{}", i);
            tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
        }

        for i in 0..100 {
            let key = format!("key{}", i);
            let proof = tree.prove(key.as_bytes()).unwrap();
            verifier.add_merkle(proof);
        }

        let result = verifier.verify_all();
        assert!(result.all_valid);
        assert_eq!(result.merkle_results.len(), 100);
        assert!(result.merkle_results.iter().all(|&v| v));
    }

    #[test]
    fn test_mixed_batch() {
        let mut verifier = BatchVerifier::new();

        // Add Schnorr proofs
        for i in 0..10 {
            let secret = Scalar::random(&mut OsRng);
            let public = RISTRETTO_BASEPOINT_POINT * secret;
            let message = format!("schnorr {}", i);

            let proof = SchnorrProof::prove_knowledge(&secret, &public, message.as_bytes());
            verifier.add_schnorr(proof, public, message.as_bytes());
        }

        // Add equality proofs
        let h = generator_h();
        for i in 0..10 {
            let value = 42u64 + i;
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
        assert_eq!(result.total_proofs(), 30);
        assert_eq!(result.schnorr_results.len(), 10);
        assert_eq!(result.equality_results.len(), 10);
        assert_eq!(result.merkle_results.len(), 10);
        assert_eq!(result.valid_count(), 30);
    }

    #[test]
    fn test_convenience_functions() {
        // Test verify_schnorr_batch
        let mut schnorr_proofs = Vec::new();
        for i in 0..10 {
            let secret = Scalar::random(&mut OsRng);
            let public = RISTRETTO_BASEPOINT_POINT * secret;
            let message = format!("message {}", i).into_bytes();

            let proof = SchnorrProof::prove_knowledge(&secret, &public, &message);
            schnorr_proofs.push((proof, public, message));
        }

        let results = verify_schnorr_batch(&schnorr_proofs);
        assert_eq!(results.len(), 10);
        assert!(results.iter().all(|&v| v));

        // Test verify_merkle_batch
        let mut tree = SparseMerkleTree::new();
        let mut merkle_proofs = Vec::new();

        for i in 0..10 {
            let key = format!("key{}", i);
            let value = format!("value{}", i);
            tree.insert(key.as_bytes(), value.as_bytes()).unwrap();
        }

        for i in 0..10 {
            let key = format!("key{}", i);
            let proof = tree.prove(key.as_bytes()).unwrap();
            merkle_proofs.push(proof);
        }

        let results = verify_merkle_batch(&merkle_proofs);
        assert_eq!(results.len(), 10);
        assert!(results.iter().all(|&v| v));
    }

    #[test]
    fn test_batch_clear() {
        let mut verifier = BatchVerifier::new();

        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = b"test";

        let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
        verifier.add_schnorr(proof, public, message);

        assert_eq!(verifier.len(), 1);

        verifier.clear();
        assert_eq!(verifier.len(), 0);
        assert!(verifier.is_empty());
    }

    #[test]
    fn test_batch_result_methods() {
        let mut verifier = BatchVerifier::new();

        // Add 3 Schnorr proofs (2 valid, 1 invalid)
        for i in 0..2 {
            let secret = Scalar::random(&mut OsRng);
            let public = RISTRETTO_BASEPOINT_POINT * secret;
            let message = format!("message {}", i);

            let proof = SchnorrProof::prove_knowledge(&secret, &public, message.as_bytes());
            verifier.add_schnorr(proof, public, message.as_bytes());
        }

        // Add invalid proof
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let wrong_public = RISTRETTO_BASEPOINT_POINT * Scalar::random(&mut OsRng);
        let message = b"invalid";

        let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
        verifier.add_schnorr(proof, wrong_public, message);

        let result = verifier.verify_all();

        assert_eq!(result.total_proofs(), 3);
        assert_eq!(result.valid_count(), 2);
        assert_eq!(result.invalid_count(), 1);
        assert!(result.schnorr_valid(0).unwrap());
        assert!(result.schnorr_valid(1).unwrap());
        assert!(!result.schnorr_valid(2).unwrap());
        assert!(result.schnorr_valid(3).is_none());
    }

    #[test]
    fn test_batch_verification_timing() {
        let mut verifier = BatchVerifier::new();

        // Generate 50 proofs
        for i in 0..50 {
            let secret = Scalar::random(&mut OsRng);
            let public = RISTRETTO_BASEPOINT_POINT * secret;
            let message = format!("message {}", i);

            let proof = SchnorrProof::prove_knowledge(&secret, &public, message.as_bytes());
            verifier.add_schnorr(proof, public, message.as_bytes());
        }

        let result = verifier.verify_all();
        assert!(result.all_valid);
        assert!(result.verification_time_ms > 0);
        println!(
            "Verified {} proofs in {}ms",
            result.total_proofs(),
            result.verification_time_ms
        );
    }

    #[test]
    fn test_equality_batch() {
        let mut verifier = BatchVerifier::new();
        let h = generator_h();

        // Add 10 equality proofs
        for i in 0..10 {
            let value = 100u64 + i;
            let r1 = Scalar::random(&mut OsRng);
            let r2 = Scalar::random(&mut OsRng);

            let g = RISTRETTO_BASEPOINT_POINT;
            let v = Scalar::from(value);

            let c1 = g * v + h * r1;
            let c2 = g * v + h * r2;

            let proof = EqualityProof::prove_equality(value, &r1, &r2, &c1, &c2);
            verifier.add_equality(proof, c1, c2);
        }

        let result = verifier.verify_all();
        assert!(result.all_valid);
        assert_eq!(result.equality_results.len(), 10);
        assert!(result.equality_results.iter().all(|&v| v));
    }

    #[test]
    fn test_verify_equality_batch_convenience() {
        let h = generator_h();
        let mut equality_proofs = Vec::new();

        for i in 0..10 {
            let value = 100u64 + i;
            let r1 = Scalar::random(&mut OsRng);
            let r2 = Scalar::random(&mut OsRng);

            let g = RISTRETTO_BASEPOINT_POINT;
            let v = Scalar::from(value);

            let c1 = g * v + h * r1;
            let c2 = g * v + h * r2;

            let proof = EqualityProof::prove_equality(value, &r1, &r2, &c1, &c2);
            equality_proofs.push((proof, c1, c2));
        }

        let results = verify_equality_batch(&equality_proofs);
        assert_eq!(results.len(), 10);
        assert!(results.iter().all(|&v| v));
    }
}
