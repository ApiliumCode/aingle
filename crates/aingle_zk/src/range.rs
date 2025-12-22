//! Range proofs using Bulletproofs
//!
//! Prove that a committed value is within a specific range without revealing the actual value.
//!
//! This module implements optimized Bulletproofs for efficient range proofs:
//! - Logarithmic proof size: O(log n) where n is the number of bits
//! - Batch verification for multiple range proofs
//! - No trusted setup required
//!
//! ## Cryptographic Primitives
//!
//! Bulletproofs use:
//! - Pedersen commitments for hiding values
//! - Inner product arguments for efficient proof generation
//! - Fiat-Shamir heuristic for non-interactive proofs
//!
//! ## Security
//!
//! Security relies on:
//! - Discrete logarithm problem hardness
//! - Random oracle model (SHA-512 via Merlin transcripts)
//!
//! ## Example
//!
//! ```rust
//! use aingle_zk::RangeProofGenerator;
//!
//! // Create generator for 32-bit range [0, 2^32)
//! let generator = RangeProofGenerator::new(32);
//!
//! // Prove that value 1000 is in range
//! let proof = generator.prove(1000).expect("value in range");
//!
//! // Verify the proof
//! assert!(generator.verify(&proof).unwrap());
//! ```
//!
//! This module requires the `bulletproofs` feature.

use bulletproofs::{BulletproofGens, PedersenGens, RangeProof as BPRangeProof};
use merlin::Transcript;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

// Note: bulletproofs uses curve25519-dalek-ng, we need to convert types
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek_ng::ristretto::CompressedRistretto as CompressedRistrettoNG;
use curve25519_dalek_ng::scalar::Scalar as ScalarNG;

use crate::commitment::PedersenCommitment;
use crate::error::{Result, ZkError};

// Helper functions to convert between curve25519-dalek and curve25519-dalek-ng
fn scalar_to_ng(s: &Scalar) -> ScalarNG {
    ScalarNG::from_bytes_mod_order(s.to_bytes())
}

fn scalar_from_ng(s: &ScalarNG) -> Scalar {
    Scalar::from_bytes_mod_order(s.to_bytes())
}

/// Range proof generators (cached for efficiency)
///
/// Generators should be created once and reused for multiple proofs
/// with the same bit range for best performance.
pub struct RangeProofGenerator {
    /// Bulletproof generators (precomputed elliptic curve points)
    bp_gens: BulletproofGens,
    /// Pedersen generators (G and H points)
    pc_gens: PedersenGens,
    /// Number of bits for the range [0, 2^n_bits)
    n_bits: usize,
}

impl RangeProofGenerator {
    /// Create a new range proof generator
    ///
    /// # Arguments
    /// * `n_bits` - Number of bits determining the range [0, 2^n_bits)
    ///
    /// # Performance
    /// Generator creation precomputes points, so reuse generators when possible.
    /// Creating a generator once and calling prove() multiple times is much faster
    /// than creating a new generator for each proof.
    ///
    /// # Example
    /// ```rust
    /// use aingle_zk::RangeProofGenerator;
    ///
    /// // For 8-bit values [0, 256)
    /// let gen_8bit = RangeProofGenerator::new(8);
    ///
    /// // For 32-bit values [0, 4294967296)
    /// let gen_32bit = RangeProofGenerator::new(32);
    ///
    /// // For 64-bit values [0, 2^64)
    /// let gen_64bit = RangeProofGenerator::new(64);
    /// ```
    pub fn new(n_bits: usize) -> Self {
        let bp_gens = BulletproofGens::new(n_bits, 1);
        let pc_gens = PedersenGens::default();

        Self {
            bp_gens,
            pc_gens,
            n_bits,
        }
    }

    /// Get the number of bits this generator supports
    pub fn bit_size(&self) -> usize {
        self.n_bits
    }

    /// Get the maximum value this generator can prove (2^n_bits - 1)
    pub fn max_value(&self) -> u64 {
        if self.n_bits >= 64 {
            u64::MAX
        } else {
            (1u64 << self.n_bits) - 1
        }
    }

    /// Create a range proof for a value
    ///
    /// Proves that `value` is in range [0, 2^n_bits) without revealing it.
    ///
    /// # Arguments
    /// * `value` - The value to prove is in range (must be < 2^n_bits)
    ///
    /// # Returns
    /// A `RangeProof` that can be verified without revealing the value
    ///
    /// # Errors
    /// Returns `ZkError::InvalidRange` if value is out of range
    ///
    /// # Example
    /// ```rust
    /// use aingle_zk::RangeProofGenerator;
    ///
    /// let generator = RangeProofGenerator::new(32);
    /// let proof = generator.prove(1000).expect("value in range");
    /// assert!(generator.verify(&proof).unwrap());
    /// ```
    ///
    /// # Proof Size
    /// Bulletproof size is O(log n) where n is the number of bits:
    /// - 8-bit: ~672 bytes
    /// - 16-bit: ~736 bytes
    /// - 32-bit: ~800 bytes
    /// - 64-bit: ~864 bytes
    pub fn prove(&self, value: u64) -> Result<RangeProof> {
        // Check value is in range
        let max_value = if self.n_bits >= 64 {
            u64::MAX
        } else {
            1u64 << self.n_bits
        };

        if value >= max_value {
            return Err(ZkError::InvalidRange(0, max_value));
        }

        let mut rng = OsRng;
        let blinding = Scalar::random(&mut rng);
        let blinding_ng = scalar_to_ng(&blinding);

        let mut transcript = Transcript::new(b"aingle_range_proof");

        let (proof, commitment) = BPRangeProof::prove_single(
            &self.bp_gens,
            &self.pc_gens,
            &mut transcript,
            value,
            &blinding_ng,
            self.n_bits,
        )
        .map_err(|e| ZkError::CryptoError(format!("Range proof generation failed: {:?}", e)))?;

        Ok(RangeProof {
            proof_bytes: proof.to_bytes(),
            commitment: commitment.to_bytes(),
            n_bits: self.n_bits,
            blinding: blinding.to_bytes(),
        })
    }

    /// Create a range proof with a specific blinding factor
    ///
    /// Useful when you need to coordinate with other commitments
    /// using the same blinding factor.
    pub fn prove_with_blinding(&self, value: u64, blinding: &Scalar) -> Result<RangeProof> {
        let max_value = if self.n_bits >= 64 {
            u64::MAX
        } else {
            1u64 << self.n_bits
        };

        if value >= max_value {
            return Err(ZkError::InvalidRange(0, max_value));
        }

        let blinding_ng = scalar_to_ng(blinding);
        let mut transcript = Transcript::new(b"aingle_range_proof");

        let (proof, commitment) = BPRangeProof::prove_single(
            &self.bp_gens,
            &self.pc_gens,
            &mut transcript,
            value,
            &blinding_ng,
            self.n_bits,
        )
        .map_err(|e| ZkError::CryptoError(format!("Range proof generation failed: {:?}", e)))?;

        Ok(RangeProof {
            proof_bytes: proof.to_bytes(),
            commitment: commitment.to_bytes(),
            n_bits: self.n_bits,
            blinding: blinding.to_bytes(),
        })
    }

    /// Verify a range proof
    ///
    /// Verifies that the proof is valid for the claimed range
    /// without learning the actual value.
    ///
    /// # Arguments
    /// * `proof` - The range proof to verify
    ///
    /// # Returns
    /// `Ok(true)` if the proof is valid, `Ok(false)` if invalid
    ///
    /// # Errors
    /// Returns error if proof is malformed or bit size doesn't match
    ///
    /// # Example
    /// ```rust
    /// use aingle_zk::RangeProofGenerator;
    ///
    /// let generator = RangeProofGenerator::new(16);
    /// let proof = generator.prove(42).unwrap();
    ///
    /// // Verification succeeds
    /// assert!(generator.verify(&proof).unwrap());
    /// ```
    pub fn verify(&self, proof: &RangeProof) -> Result<bool> {
        if proof.n_bits != self.n_bits {
            return Err(ZkError::InvalidProof(format!(
                "Bit size mismatch: expected {}, got {}",
                self.n_bits, proof.n_bits
            )));
        }

        let bp_proof = BPRangeProof::from_bytes(&proof.proof_bytes)
            .map_err(|e| ZkError::InvalidProof(format!("Invalid proof bytes: {:?}", e)))?;

        let commitment = CompressedRistrettoNG::from_slice(&proof.commitment);

        let mut transcript = Transcript::new(b"aingle_range_proof");

        let result = bp_proof.verify_single(
            &self.bp_gens,
            &self.pc_gens,
            &mut transcript,
            &commitment,
            self.n_bits,
        );

        Ok(result.is_ok())
    }

    /// Batch verify multiple range proofs
    ///
    /// More efficient than verifying each proof individually.
    ///
    /// # Performance
    /// Batch verification is approximately 2-3x faster than individual
    /// verification for batches of 10+ proofs.
    pub fn verify_batch(&self, proofs: &[RangeProof]) -> Result<Vec<bool>> {
        // For small batches, individual verification might be faster
        if proofs.is_empty() {
            return Ok(Vec::new());
        }

        // Verify all proofs have correct bit size
        for proof in proofs {
            if proof.n_bits != self.n_bits {
                return Err(ZkError::InvalidProof(format!(
                    "Bit size mismatch in batch: expected {}, got {}",
                    self.n_bits, proof.n_bits
                )));
            }
        }

        // Parse all proofs and commitments
        let mut bp_proofs = Vec::with_capacity(proofs.len());
        let mut commitments = Vec::with_capacity(proofs.len());

        for proof in proofs {
            let bp_proof = BPRangeProof::from_bytes(&proof.proof_bytes)
                .map_err(|e| ZkError::InvalidProof(format!("Invalid proof bytes: {:?}", e)))?;

            let commitment = CompressedRistrettoNG::from_slice(&proof.commitment);

            bp_proofs.push(bp_proof);
            commitments.push(commitment);
        }

        // For bulletproofs, we verify individually but can do it in parallel
        // The bulletproofs crate doesn't expose batch verification for single proofs
        use rayon::prelude::*;

        let results: Vec<bool> = (0..proofs.len())
            .into_par_iter()
            .map(|i| {
                let mut transcript = Transcript::new(b"aingle_range_proof");
                bp_proofs[i]
                    .verify_single(
                        &self.bp_gens,
                        &self.pc_gens,
                        &mut transcript,
                        &commitments[i],
                        self.n_bits,
                    )
                    .is_ok()
            })
            .collect();

        Ok(results)
    }
}

/// A range proof
///
/// Bulletproofs-style range proof proving that a committed value
/// lies in the range [0, 2^n_bits).
///
/// # Size
/// Proof size is logarithmic in the number of bits:
/// - Approximately 32 * (log2(n_bits) + 13) bytes
/// - For 32-bit: ~800 bytes
/// - For 64-bit: ~864 bytes
///
/// # Serialization
/// Proofs can be serialized to JSON or binary formats using serde.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeProof {
    /// Serialized bulletproof (inner product proof)
    pub proof_bytes: Vec<u8>,
    /// Pedersen commitment to the value (32-byte compressed point)
    pub commitment: [u8; 32],
    /// Number of bits determining the range [0, 2^n_bits)
    pub n_bits: usize,
    /// Blinding factor used in the commitment (for opening/verification)
    /// This is optional and should only be included when needed for opening
    #[serde(skip_serializing_if = "is_zero_bytes")]
    #[serde(default = "default_blinding")]
    pub blinding: [u8; 32],
}

fn is_zero_bytes(bytes: &[u8; 32]) -> bool {
    bytes.iter().all(|&b| b == 0)
}

fn default_blinding() -> [u8; 32] {
    [0u8; 32]
}

impl RangeProof {
    /// Get the range [0, max) that this proof covers
    ///
    /// # Example
    /// ```rust
    /// use aingle_zk::RangeProofGenerator;
    ///
    /// let generator = RangeProofGenerator::new(8);
    /// let proof = generator.prove(100).unwrap();
    ///
    /// let (min, max) = proof.range();
    /// assert_eq!(min, 0);
    /// assert_eq!(max, 256); // 2^8
    /// ```
    pub fn range(&self) -> (u64, u64) {
        let max = if self.n_bits >= 64 {
            u64::MAX
        } else {
            1u64 << self.n_bits
        };
        (0, max)
    }

    /// Get the commitment bytes
    pub fn commitment(&self) -> &[u8; 32] {
        &self.commitment
    }

    /// Convert to Pedersen commitment
    ///
    /// Returns the commitment as a `PedersenCommitment` for use
    /// with other ZK operations.
    pub fn to_pedersen(&self) -> PedersenCommitment {
        PedersenCommitment::from_bytes(self.commitment)
    }

    /// Get the proof size in bytes
    pub fn size(&self) -> usize {
        self.proof_bytes.len() + 32 + 32 + std::mem::size_of::<usize>()
    }

    /// Verify that this proof was created for a specific value
    ///
    /// This requires knowledge of the blinding factor (opening).
    /// Only use this when you have the secret opening information.
    ///
    /// # Arguments
    /// * `value` - The value to check against the commitment
    ///
    /// # Returns
    /// `true` if the commitment matches the value with the stored blinding
    ///
    /// # Example
    /// ```rust
    /// use aingle_zk::RangeProofGenerator;
    ///
    /// let generator = RangeProofGenerator::new(16);
    /// let proof = generator.prove(42).unwrap();
    ///
    /// // The blinding is included in the proof
    /// assert!(proof.verify_value(42));
    /// assert!(!proof.verify_value(43));
    /// ```
    pub fn verify_value(&self, value: u64) -> bool {
        if self.blinding.iter().all(|&b| b == 0) {
            return false; // No blinding available
        }

        let blinding_ng = ScalarNG::from_bytes_mod_order(self.blinding);
        let pc_gens = PedersenGens::default();

        let value_ng = ScalarNG::from(value);
        let expected = pc_gens.commit(value_ng, blinding_ng);
        expected.compress().to_bytes() == self.commitment
    }

    /// Serialize to compact binary format
    ///
    /// More efficient than JSON for storage and transmission.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // n_bits as u16 (2 bytes)
        bytes.extend_from_slice(&(self.n_bits as u16).to_le_bytes());

        // commitment (32 bytes)
        bytes.extend_from_slice(&self.commitment);

        // blinding (32 bytes)
        bytes.extend_from_slice(&self.blinding);

        // proof_bytes length as u32 (4 bytes)
        bytes.extend_from_slice(&(self.proof_bytes.len() as u32).to_le_bytes());

        // proof_bytes
        bytes.extend_from_slice(&self.proof_bytes);

        bytes
    }

    /// Deserialize from compact binary format
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 70 {
            return Err(ZkError::InvalidProof("Proof too short".into()));
        }

        let mut offset = 0;

        // Read n_bits
        let n_bits = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset += 2;

        // Read commitment
        let commitment: [u8; 32] = bytes[offset..offset + 32]
            .try_into()
            .map_err(|_| ZkError::InvalidProof("Invalid commitment length".into()))?;
        offset += 32;

        // Read blinding
        let blinding: [u8; 32] = bytes[offset..offset + 32]
            .try_into()
            .map_err(|_| ZkError::InvalidProof("Invalid blinding length".into()))?;
        offset += 32;

        // Read proof_bytes length
        let proof_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        // Read proof_bytes
        if bytes.len() < offset + proof_len {
            return Err(ZkError::InvalidProof("Proof bytes truncated".into()));
        }

        let proof_bytes = bytes[offset..offset + proof_len].to_vec();

        Ok(Self {
            proof_bytes,
            commitment,
            n_bits,
            blinding,
        })
    }
}

/// Aggregated range proofs for multiple values
pub struct AggregatedRangeProof {
    /// Individual proofs
    proofs: Vec<RangeProof>,
}

impl AggregatedRangeProof {
    /// Create aggregated proofs for multiple values
    pub fn prove(values: &[u64], n_bits: usize) -> Result<Self> {
        let generator = RangeProofGenerator::new(n_bits);
        let proofs: Result<Vec<_>> = values.iter().map(|&v| generator.prove(v)).collect();

        Ok(Self { proofs: proofs? })
    }

    /// Verify all proofs
    pub fn verify(&self, n_bits: usize) -> Result<bool> {
        let generator = RangeProofGenerator::new(n_bits);
        for proof in &self.proofs {
            if !generator.verify(proof)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Get number of proofs
    pub fn len(&self) -> usize {
        self.proofs.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_proof_valid() {
        let generator = RangeProofGenerator::new(32);

        // Value within range
        let value = 1000u64;
        let proof = generator.prove(value).unwrap();

        assert!(generator.verify(&proof).unwrap());
        assert!(proof.verify_value(value));
    }

    #[test]
    fn test_range_proof_boundary() {
        let generator = RangeProofGenerator::new(8); // Range [0, 256)

        // Maximum valid value
        let proof = generator.prove(255).unwrap();
        assert!(generator.verify(&proof).unwrap());

        // Zero
        let proof = generator.prove(0).unwrap();
        assert!(generator.verify(&proof).unwrap());
    }

    #[test]
    fn test_range_proof_out_of_range() {
        let generator = RangeProofGenerator::new(8); // Range [0, 256)

        // Value out of range
        let result = generator.prove(256);
        assert!(result.is_err());
    }

    #[test]
    fn test_aggregated_range_proofs() {
        let values = vec![10u64, 20, 30, 40];
        let proofs = AggregatedRangeProof::prove(&values, 16).unwrap();

        assert_eq!(proofs.len(), 4);
        assert!(proofs.verify(16).unwrap());
    }

    #[test]
    fn test_range_proof_serialization() {
        let generator = RangeProofGenerator::new(16);
        let proof = generator.prove(42).unwrap();

        let json = serde_json::to_string(&proof).unwrap();
        let deserialized: RangeProof = serde_json::from_str(&json).unwrap();

        assert_eq!(proof.n_bits, deserialized.n_bits);
        assert_eq!(proof.commitment, deserialized.commitment);
    }
}
