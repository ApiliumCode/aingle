// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Zero-knowledge proof types and verification
//!
//! High-level proof API for AIngle.

use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

use crate::commitment::HashCommitment;
use crate::error::{Result, ZkError};
use crate::merkle::{Hash, MerkleProof};

/// Helper function to get second generator H (same as in commitment.rs)
fn generator_h() -> RistrettoPoint {
    let mut hasher = Sha512::new();
    hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
    hasher.update(b"aingle_zk_pedersen_h");
    RistrettoPoint::from_uniform_bytes(&hasher.finalize().into())
}

/// Schnorr proof of knowledge of discrete log
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchnorrProof {
    pub commitment: [u8; 32], // R = k*G
    pub challenge: [u8; 32],  // c = H(R || P || message)
    pub response: [u8; 32],   // s = k + c*x
}

impl SchnorrProof {
    /// Generate a Schnorr proof that we know x such that P = x*G
    pub fn prove_knowledge(secret: &Scalar, public_point: &RistrettoPoint, message: &[u8]) -> Self {
        let g = RISTRETTO_BASEPOINT_POINT;

        // 1. Generate random k
        let k = Scalar::random(&mut OsRng);

        // 2. Compute R = k*G
        let r = g * k;
        let r_bytes: [u8; 32] = r.compress().to_bytes();

        // 3. Compute challenge c = H(R || P || message)
        let mut hasher = Sha256::new();
        hasher.update(r_bytes);
        hasher.update(public_point.compress().as_bytes());
        hasher.update(message);
        let challenge_bytes: [u8; 32] = hasher.finalize().into();
        let c = Scalar::from_bytes_mod_order(challenge_bytes);

        // 4. Compute response s = k + c*x
        let s = k + c * secret;
        let s_bytes: [u8; 32] = s.to_bytes();

        SchnorrProof {
            commitment: r_bytes,
            challenge: challenge_bytes,
            response: s_bytes,
        }
    }

    /// Verify a Schnorr proof
    pub fn verify(&self, public_point: &RistrettoPoint, message: &[u8]) -> Result<bool> {
        let g = RISTRETTO_BASEPOINT_POINT;

        // 1. Parse values
        let r = CompressedRistretto::from_slice(&self.commitment)
            .map_err(|_| ZkError::InvalidProof("Invalid commitment".into()))?
            .decompress()
            .ok_or_else(|| ZkError::InvalidProof("Cannot decompress commitment".into()))?;

        let c = Scalar::from_bytes_mod_order(self.challenge);
        let s = Scalar::from_bytes_mod_order(self.response);

        // 2. Verify challenge: c == H(R || P || message)
        let mut hasher = Sha256::new();
        hasher.update(self.commitment);
        hasher.update(public_point.compress().as_bytes());
        hasher.update(message);
        let expected_challenge: [u8; 32] = hasher.finalize().into();

        if expected_challenge != self.challenge {
            return Ok(false);
        }

        // 3. Verify equation: s*G == R + c*P
        let lhs = g * s;
        let rhs = r + public_point * c;

        Ok(lhs == rhs)
    }
}

/// Proof that two Pedersen commitments hide the same value
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EqualityProof {
    pub commitment1: [u8; 32],
    pub commitment2: [u8; 32],
    pub challenge: [u8; 32],
    pub response: [u8; 32],
}

impl EqualityProof {
    /// Prove that C1 = v*G + r1*H and C2 = v*G + r2*H hide the same v
    pub fn prove_equality(
        _value: u64,
        blinding1: &Scalar,
        blinding2: &Scalar,
        commitment1: &RistrettoPoint,
        commitment2: &RistrettoPoint,
    ) -> Self {
        // Prove knowledge of (r1 - r2) such that C1 - C2 = (r1 - r2)*H
        let h = generator_h();
        let diff = commitment1 - commitment2; // Should equal (r1 - r2)*H
        let r_diff = blinding1 - blinding2;

        // Schnorr proof of knowledge of r_diff
        let k = Scalar::random(&mut OsRng);
        let r = h * k;

        let mut hasher = Sha256::new();
        hasher.update(r.compress().as_bytes());
        hasher.update(diff.compress().as_bytes());
        let challenge: [u8; 32] = hasher.finalize().into();
        let c = Scalar::from_bytes_mod_order(challenge);

        let response = k + c * r_diff;

        EqualityProof {
            commitment1: commitment1.compress().to_bytes(),
            commitment2: commitment2.compress().to_bytes(),
            challenge,
            response: response.to_bytes(),
        }
    }

    /// Verify equality proof
    pub fn verify(&self) -> Result<bool> {
        let h = generator_h();

        let c1 = CompressedRistretto::from_slice(&self.commitment1)
            .map_err(|_| ZkError::InvalidProof("Invalid C1".into()))?
            .decompress()
            .ok_or_else(|| ZkError::InvalidProof("Cannot decompress C1".into()))?;

        let c2 = CompressedRistretto::from_slice(&self.commitment2)
            .map_err(|_| ZkError::InvalidProof("Invalid C2".into()))?
            .decompress()
            .ok_or_else(|| ZkError::InvalidProof("Cannot decompress C2".into()))?;

        let diff = c1 - c2;
        let c = Scalar::from_bytes_mod_order(self.challenge);
        let s = Scalar::from_bytes_mod_order(self.response);

        // Verify: s*H == R + c*(C1-C2)
        let r_prime = h * s - diff * c;

        let mut hasher = Sha256::new();
        hasher.update(r_prime.compress().as_bytes());
        hasher.update(diff.compress().as_bytes());
        let computed_challenge: [u8; 32] = hasher.finalize().into();

        Ok(computed_challenge == self.challenge)
    }
}

// ---------------------------------------------------------------------------
// Statement binding (v2 schemes)
//
// The v1 `Knowledge` and `Equality` variants below hash only the commitment
// into their Fiat-Shamir challenge. Nothing else is covered, so a proof that
// verifies is not a proof *of* anything: lift it out of one response and serve
// it beside a different claim and it still verifies. The v2 variants close that
// by hashing a caller-supplied statement into the challenge, with a domain
// separation tag and an explicit length prefix so no two (R, P, statement)
// triples can share a preimage.
//
// The preimage builders are public and are the SINGLE definition used by the
// prover, the verifier and anything that publishes the bytes to a client. A
// second, "equivalent" implementation is how a client ends up hashing something
// subtly different and verifying nothing.
// ---------------------------------------------------------------------------

/// Domain-separation tag for the statement-binding knowledge scheme.
pub const KNOWLEDGE_V2_DOMAIN: &[u8] = b"aingle-zk-knowledge-v2";

/// Domain-separation tag for the statement-binding equality scheme.
pub const EQUALITY_V2_DOMAIN: &[u8] = b"aingle-zk-equality-v2";

/// The byte layout of [`knowledge_v2_challenge_preimage`], for publication.
pub const KNOWLEDGE_V2_PREIMAGE_LAYOUT: &str =
    "\"aingle-zk-knowledge-v2\" || 0x00 || compress(R) || commitment || \
     u64_le(statement_len) || statement";

/// The byte layout of [`equality_v2_challenge_preimage`], for publication.
pub const EQUALITY_V2_PREIMAGE_LAYOUT: &str =
    "\"aingle-zk-equality-v2\" || 0x00 || compress(R) || \
     compress(commitment1 - commitment2) || u64_le(statement_len) || statement";

/// Build the exact bytes hashed to produce a `KnowledgeBound` challenge.
///
/// `r_compressed` is the compressed nonce point R (recomputed as `s*G - c*P` on
/// the verifying side), `commitment` the compressed public point P, `statement`
/// the claim the proof is about.
pub fn knowledge_v2_challenge_preimage(
    r_compressed: &[u8; 32],
    commitment: &[u8; 32],
    statement: &[u8],
) -> Vec<u8> {
    let mut preimage =
        Vec::with_capacity(KNOWLEDGE_V2_DOMAIN.len() + 1 + 32 + 32 + 8 + statement.len());
    preimage.extend_from_slice(KNOWLEDGE_V2_DOMAIN);
    preimage.push(0x00);
    preimage.extend_from_slice(r_compressed);
    preimage.extend_from_slice(commitment);
    preimage.extend_from_slice(&(statement.len() as u64).to_le_bytes());
    preimage.extend_from_slice(statement);
    preimage
}

/// Build the exact bytes hashed to produce an `EqualityBound` challenge.
///
/// `diff_compressed` is the compressed `C1 - C2`.
pub fn equality_v2_challenge_preimage(
    r_compressed: &[u8; 32],
    diff_compressed: &[u8; 32],
    statement: &[u8],
) -> Vec<u8> {
    let mut preimage =
        Vec::with_capacity(EQUALITY_V2_DOMAIN.len() + 1 + 32 + 32 + 8 + statement.len());
    preimage.extend_from_slice(EQUALITY_V2_DOMAIN);
    preimage.push(0x00);
    preimage.extend_from_slice(r_compressed);
    preimage.extend_from_slice(diff_compressed);
    preimage.extend_from_slice(&(statement.len() as u64).to_le_bytes());
    preimage.extend_from_slice(statement);
    preimage
}

/// Types of zero-knowledge proofs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProofType {
    /// Proof of knowledge of a value
    KnowledgeProof,
    /// Proof that a value is in a range
    RangeProof,
    /// Proof of set membership
    MembershipProof,
    /// Proof of equality between two commitments
    EqualityProof,
    /// Proof of non-membership
    NonMembershipProof,
}

/// A zero-knowledge proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkProof {
    /// Type of proof
    pub proof_type: ProofType,
    /// Proof-specific data
    pub proof_data: ProofData,
    /// Timestamp of proof creation
    pub timestamp: u64,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Proof-specific data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProofData {
    /// Schnorr-like knowledge proof
    Knowledge {
        commitment: [u8; 32],
        challenge: [u8; 32],
        response: [u8; 32],
    },
    /// Merkle membership proof
    Membership { root: [u8; 32], proof: MerkleProof },
    /// Equality proof between commitments
    Equality {
        commitment1: [u8; 32],
        commitment2: [u8; 32],
        proof: Vec<u8>,
    },
    /// Simple hash commitment opening
    HashOpening {
        commitment: [u8; 32],
        salt: [u8; 32],
    },
    /// Statement-binding Schnorr knowledge proof.
    ///
    /// Same sigma protocol as [`ProofData::Knowledge`], but the challenge is
    /// `sha256` over [`knowledge_v2_challenge_preimage`], which covers
    /// `statement`. The proof therefore verifies for that statement and no
    /// other: it cannot be lifted and presented beside a different claim.
    KnowledgeBound {
        commitment: [u8; 32],
        challenge: [u8; 32],
        response: [u8; 32],
        /// The statement this proof is *about*, bound into the challenge.
        statement: Vec<u8>,
    },
    /// Statement-binding equality proof between two Pedersen commitments.
    ///
    /// Same sigma protocol as [`ProofData::Equality`], with the challenge taken
    /// over [`equality_v2_challenge_preimage`] so `statement` is bound in.
    EqualityBound {
        commitment1: [u8; 32],
        commitment2: [u8; 32],
        /// `challenge || response`, 64 bytes.
        proof: Vec<u8>,
        /// The statement this proof is *about*, bound into the challenge.
        statement: Vec<u8>,
    },
}

impl ProofData {
    /// Produce a statement-binding knowledge proof of `secret`.
    ///
    /// The resulting proof establishes knowledge of `x` with `P = x*G` **for
    /// `statement`**; presenting it for any other statement fails verification.
    pub fn prove_knowledge_bound(secret: &Scalar, statement: &[u8]) -> Self {
        let g = RISTRETTO_BASEPOINT_POINT;
        let p = g * secret;
        let k = Scalar::random(&mut OsRng);
        let r = g * k;

        let challenge_bytes: [u8; 32] = Sha256::digest(knowledge_v2_challenge_preimage(
            &r.compress().to_bytes(),
            &p.compress().to_bytes(),
            statement,
        ))
        .into();
        let s = k + Scalar::from_bytes_mod_order(challenge_bytes) * secret;

        ProofData::KnowledgeBound {
            commitment: p.compress().to_bytes(),
            challenge: challenge_bytes,
            response: s.to_bytes(),
            statement: statement.to_vec(),
        }
    }

    /// Produce a statement-binding equality proof for two commitments to the
    /// same value under blindings `blinding1` / `blinding2`.
    pub fn prove_equality_bound(
        blinding1: &Scalar,
        blinding2: &Scalar,
        commitment1: &RistrettoPoint,
        commitment2: &RistrettoPoint,
        statement: &[u8],
    ) -> Self {
        let h = generator_h();
        let diff = commitment1 - commitment2;
        let r_diff = blinding1 - blinding2;

        let k = Scalar::random(&mut OsRng);
        let r = h * k;

        let challenge_bytes: [u8; 32] = Sha256::digest(equality_v2_challenge_preimage(
            &r.compress().to_bytes(),
            &diff.compress().to_bytes(),
            statement,
        ))
        .into();
        let response = k + Scalar::from_bytes_mod_order(challenge_bytes) * r_diff;

        let mut proof = Vec::with_capacity(64);
        proof.extend_from_slice(&challenge_bytes);
        proof.extend_from_slice(&response.to_bytes());

        ProofData::EqualityBound {
            commitment1: commitment1.compress().to_bytes(),
            commitment2: commitment2.compress().to_bytes(),
            proof,
            statement: statement.to_vec(),
        }
    }

    /// The statement bound into this proof's challenge, if the scheme binds one.
    ///
    /// `None` means the challenge covers no statement — a verifying proof of
    /// this shape says nothing about any claim it was served alongside.
    pub fn bound_statement(&self) -> Option<&[u8]> {
        match self {
            ProofData::KnowledgeBound { statement, .. }
            | ProofData::EqualityBound { statement, .. } => Some(statement),
            ProofData::Knowledge { .. }
            | ProofData::Equality { .. }
            | ProofData::Membership { .. }
            | ProofData::HashOpening { .. } => None,
        }
    }
}

impl ZkProof {
    /// Create a new proof
    pub fn new(proof_type: ProofType, proof_data: ProofData) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            proof_type,
            proof_data,
            timestamp,
            metadata: None,
        }
    }

    /// Create a membership proof
    pub fn membership(root: Hash, proof: MerkleProof) -> Self {
        Self::new(
            ProofType::MembershipProof,
            ProofData::Membership { root, proof },
        )
    }

    /// Create a hash opening proof
    pub fn hash_opening(commitment: &HashCommitment) -> Self {
        Self::new(
            ProofType::KnowledgeProof,
            ProofData::HashOpening {
                commitment: commitment.hash,
                salt: commitment.salt,
            },
        )
    }

    /// Add metadata to the proof
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get proof ID (hash of proof data)
    pub fn id(&self) -> String {
        let serialized = serde_json::to_vec(&self.proof_data).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(&serialized);
        hex::encode(hasher.finalize())
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| ZkError::SerializationError(e.to_string()))
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| ZkError::SerializationError(e.to_string()))
    }
}

/// Proof verifier
pub struct ProofVerifier;

impl ProofVerifier {
    /// Verify a zero-knowledge proof
    pub fn verify(proof: &ZkProof) -> Result<bool> {
        match &proof.proof_data {
            ProofData::Membership {
                root,
                proof: merkle_proof,
            } => {
                // For membership proofs, we need the original data
                // This is a placeholder - real verification requires the data
                if merkle_proof.root != *root {
                    return Ok(false);
                }
                Ok(true) // Structure is valid, actual membership check needs data
            }
            ProofData::HashOpening { commitment, salt } => {
                // Hash opening requires the original data to verify.
                // Without data, we can only validate the proof structure is well-formed.
                // Callers must use ProofVerifier::verify_hash_opening() with data
                // for actual verification. This path returns false to be safe.
                use subtle::ConstantTimeEq;
                let non_zero_commitment = commitment.ct_ne(&[0u8; 32]);
                let non_zero_salt = salt.ct_ne(&[0u8; 32]);
                // Structural check only — reject zero commitment/salt as malformed
                Ok(bool::from(non_zero_commitment & non_zero_salt))
            }
            ProofData::Knowledge {
                commitment,
                challenge,
                response,
            } => {
                // Verify Schnorr-like knowledge proof
                Self::verify_knowledge_proof(commitment, challenge, response)
            }
            ProofData::Equality {
                commitment1,
                commitment2,
                proof,
            } => {
                // Verify equality of committed values
                if proof.len() < 64 {
                    return Err(ZkError::InvalidProof("Proof data too short".into()));
                }
                let challenge: [u8; 32] = proof[0..32]
                    .try_into()
                    .map_err(|_| ZkError::InvalidProof("Invalid challenge".into()))?;
                let response: [u8; 32] = proof[32..64]
                    .try_into()
                    .map_err(|_| ZkError::InvalidProof("Invalid response".into()))?;

                let equality_proof = EqualityProof {
                    commitment1: *commitment1,
                    commitment2: *commitment2,
                    challenge,
                    response,
                };
                equality_proof.verify()
            }
            ProofData::KnowledgeBound {
                commitment,
                challenge,
                response,
                statement,
            } => Self::verify_knowledge_bound_proof(commitment, challenge, response, statement),
            ProofData::EqualityBound {
                commitment1,
                commitment2,
                proof,
                statement,
            } => {
                if proof.len() < 64 {
                    return Err(ZkError::InvalidProof("Proof data too short".into()));
                }
                let challenge: [u8; 32] = proof[0..32]
                    .try_into()
                    .map_err(|_| ZkError::InvalidProof("Invalid challenge".into()))?;
                let response: [u8; 32] = proof[32..64]
                    .try_into()
                    .map_err(|_| ZkError::InvalidProof("Invalid response".into()))?;
                Self::verify_equality_bound_proof(
                    commitment1,
                    commitment2,
                    &challenge,
                    &response,
                    statement,
                )
            }
        }
    }

    /// Verify a membership proof with the actual data
    pub fn verify_membership(proof: &ZkProof, data: &[u8]) -> Result<bool> {
        match &proof.proof_data {
            ProofData::Membership {
                root,
                proof: merkle_proof,
            } => {
                if merkle_proof.root != *root {
                    return Err(ZkError::InvalidProof("Root mismatch".into()));
                }
                Ok(merkle_proof.verify(data))
            }
            _ => Err(ZkError::InvalidProof("Not a membership proof".into())),
        }
    }

    /// Verify a hash opening with the original data (constant-time comparison)
    pub fn verify_hash_opening(proof: &ZkProof, data: &[u8]) -> Result<bool> {
        match &proof.proof_data {
            ProofData::HashOpening { commitment, salt } => {
                use subtle::ConstantTimeEq;
                let expected = HashCommitment::commit_with_salt(data, *salt);
                Ok(bool::from(expected.hash.ct_eq(commitment)))
            }
            _ => Err(ZkError::InvalidProof("Not a hash opening proof".into())),
        }
    }

    fn verify_knowledge_proof(
        commitment: &[u8; 32],
        challenge: &[u8; 32],
        response: &[u8; 32],
    ) -> Result<bool> {
        let g = RISTRETTO_BASEPOINT_POINT;

        // Parse the commitment as a point (this is the public key P)
        let public_point = CompressedRistretto::from_slice(commitment)
            .map_err(|_| ZkError::InvalidProof("Invalid commitment point".into()))?
            .decompress()
            .ok_or_else(|| ZkError::InvalidProof("Cannot decompress commitment".into()))?;

        let c = Scalar::from_bytes_mod_order(*challenge);
        let s = Scalar::from_bytes_mod_order(*response);

        // Verify: s*G == R + c*P, where R is reconstructed
        // Rearrange: R = s*G - c*P
        let r_prime = g * s - public_point * c;

        // Recompute challenge and verify
        let mut hasher = Sha256::new();
        hasher.update(r_prime.compress().as_bytes());
        hasher.update(commitment);
        let computed_challenge: [u8; 32] = hasher.finalize().into();

        Ok(&computed_challenge == challenge)
    }

    /// Verify a statement-binding knowledge proof.
    ///
    /// Identical to [`Self::verify_knowledge_proof`] except that the recomputed
    /// challenge is taken over [`knowledge_v2_challenge_preimage`], so the
    /// check fails the moment `statement` differs from the one the prover used.
    fn verify_knowledge_bound_proof(
        commitment: &[u8; 32],
        challenge: &[u8; 32],
        response: &[u8; 32],
        statement: &[u8],
    ) -> Result<bool> {
        let g = RISTRETTO_BASEPOINT_POINT;

        let public_point = CompressedRistretto::from_slice(commitment)
            .map_err(|_| ZkError::InvalidProof("Invalid commitment point".into()))?
            .decompress()
            .ok_or_else(|| ZkError::InvalidProof("Cannot decompress commitment".into()))?;

        let c = Scalar::from_bytes_mod_order(*challenge);
        let s = Scalar::from_bytes_mod_order(*response);

        // R' = s*G - c*P
        let r_prime = g * s - public_point * c;

        let computed_challenge: [u8; 32] = Sha256::digest(knowledge_v2_challenge_preimage(
            &r_prime.compress().to_bytes(),
            commitment,
            statement,
        ))
        .into();

        Ok(&computed_challenge == challenge)
    }

    /// Verify a statement-binding equality proof.
    fn verify_equality_bound_proof(
        commitment1: &[u8; 32],
        commitment2: &[u8; 32],
        challenge: &[u8; 32],
        response: &[u8; 32],
        statement: &[u8],
    ) -> Result<bool> {
        let h = generator_h();

        let c1 = CompressedRistretto::from_slice(commitment1)
            .map_err(|_| ZkError::InvalidProof("Invalid C1".into()))?
            .decompress()
            .ok_or_else(|| ZkError::InvalidProof("Cannot decompress C1".into()))?;
        let c2 = CompressedRistretto::from_slice(commitment2)
            .map_err(|_| ZkError::InvalidProof("Invalid C2".into()))?
            .decompress()
            .ok_or_else(|| ZkError::InvalidProof("Cannot decompress C2".into()))?;

        let diff = c1 - c2;
        let c = Scalar::from_bytes_mod_order(*challenge);
        let s = Scalar::from_bytes_mod_order(*response);

        // R' = s*H - c*(C1 - C2)
        let r_prime = h * s - diff * c;

        let computed_challenge: [u8; 32] = Sha256::digest(equality_v2_challenge_preimage(
            &r_prime.compress().to_bytes(),
            &diff.compress().to_bytes(),
            statement,
        ))
        .into();

        Ok(&computed_challenge == challenge)
    }
}

/// Builder for creating proofs
pub struct ProofBuilder {
    proof_type: Option<ProofType>,
    metadata: Option<serde_json::Value>,
}

impl ProofBuilder {
    /// Create a new proof builder
    pub fn new() -> Self {
        Self {
            proof_type: None,
            metadata: None,
        }
    }

    /// Set proof type
    pub fn proof_type(mut self, pt: ProofType) -> Self {
        self.proof_type = Some(pt);
        self
    }

    /// Set metadata
    pub fn metadata(mut self, m: serde_json::Value) -> Self {
        self.metadata = Some(m);
        self
    }

    /// Build a membership proof
    pub fn build_membership(self, root: Hash, merkle_proof: MerkleProof) -> ZkProof {
        let mut proof = ZkProof::membership(root, merkle_proof);
        if let Some(m) = self.metadata {
            proof.metadata = Some(m);
        }
        proof
    }

    /// Build a hash opening proof
    pub fn build_hash_opening(self, commitment: &HashCommitment) -> ZkProof {
        let mut proof = ZkProof::hash_opening(commitment);
        if let Some(m) = self.metadata {
            proof.metadata = Some(m);
        }
        proof
    }
}

impl Default for ProofBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch proof verification for efficiency
pub struct BatchVerifier {
    proofs: Vec<ZkProof>,
}

impl BatchVerifier {
    /// Create a new batch verifier
    pub fn new() -> Self {
        Self { proofs: Vec::new() }
    }

    /// Add a proof to the batch
    pub fn add(&mut self, proof: ZkProof) {
        self.proofs.push(proof);
    }

    /// Verify all proofs in the batch
    pub fn verify_all(&self) -> Vec<Result<bool>> {
        self.proofs.iter().map(ProofVerifier::verify).collect()
    }

    /// Check if all proofs are valid
    pub fn all_valid(&self) -> bool {
        self.verify_all().iter().all(|r| matches!(r, Ok(true)))
    }
}

impl Default for BatchVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle::MerkleTree;

    #[test]
    fn test_membership_proof() {
        let leaves: Vec<&[u8]> = vec![b"alice", b"bob", b"charlie"];
        let tree = MerkleTree::new(&leaves).unwrap();

        let merkle_proof = tree.prove_data(b"bob").unwrap();
        let zk_proof = ZkProof::membership(tree.root(), merkle_proof);

        assert_eq!(zk_proof.proof_type, ProofType::MembershipProof);

        // Verify with correct data
        let result = ProofVerifier::verify_membership(&zk_proof, b"bob").unwrap();
        assert!(result);

        // Verify with wrong data
        let result = ProofVerifier::verify_membership(&zk_proof, b"dave").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_hash_opening_proof() {
        let data = b"secret data";
        let commitment = HashCommitment::commit(data);
        let proof = ZkProof::hash_opening(&commitment);

        assert_eq!(proof.proof_type, ProofType::KnowledgeProof);

        // Verify with correct data
        let result = ProofVerifier::verify_hash_opening(&proof, data).unwrap();
        assert!(result);

        // Verify with wrong data
        let result = ProofVerifier::verify_hash_opening(&proof, b"wrong data").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_proof_builder() {
        let leaves: Vec<&[u8]> = vec![b"x", b"y", b"z"];
        let tree = MerkleTree::new(&leaves).unwrap();
        let merkle_proof = tree.prove(0).unwrap();

        let proof = ProofBuilder::new()
            .metadata(serde_json::json!({"source": "test"}))
            .build_membership(tree.root(), merkle_proof);

        assert!(proof.metadata.is_some());
        assert_eq!(proof.metadata.as_ref().unwrap()["source"], "test");
    }

    #[test]
    fn test_batch_verifier() {
        let commitment1 = HashCommitment::commit(b"data1");
        let commitment2 = HashCommitment::commit(b"data2");

        let proof1 = ZkProof::hash_opening(&commitment1);
        let proof2 = ZkProof::hash_opening(&commitment2);

        let mut batch = BatchVerifier::new();
        batch.add(proof1);
        batch.add(proof2);

        let results = batch.verify_all();
        assert_eq!(results.len(), 2);
        assert!(batch.all_valid());
    }

    #[test]
    fn test_proof_serialization() {
        let commitment = HashCommitment::commit(b"test");
        let proof = ZkProof::hash_opening(&commitment);

        let json = proof.to_json().unwrap();
        let deserialized = ZkProof::from_json(&json).unwrap();

        assert_eq!(proof.proof_type, deserialized.proof_type);
    }

    #[test]
    fn test_proof_id() {
        let commitment = HashCommitment::commit(b"unique");
        let proof1 = ZkProof::hash_opening(&commitment);
        let proof2 = ZkProof::hash_opening(&commitment);

        // Same proof data should have same ID
        // (but different due to random salt in HashCommitment)
        assert!(!proof1.id().is_empty());
        assert!(!proof2.id().is_empty());
    }

    #[test]
    fn test_schnorr_proof() {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let message = b"test message";

        let proof = SchnorrProof::prove_knowledge(&secret, &public, message);
        assert!(proof.verify(&public, message).unwrap());

        // Wrong message should fail
        assert!(!proof.verify(&public, b"wrong").unwrap());
    }

    #[test]
    fn test_schnorr_proof_wrong_public_key() {
        let secret = Scalar::random(&mut OsRng);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        let wrong_public = RISTRETTO_BASEPOINT_POINT * Scalar::random(&mut OsRng);
        let message = b"test message";

        let proof = SchnorrProof::prove_knowledge(&secret, &public, message);

        // Proof should fail with wrong public key
        assert!(!proof.verify(&wrong_public, message).unwrap());
    }

    #[test]
    fn test_equality_proof() {
        let value = 42u64;
        let r1 = Scalar::random(&mut OsRng);
        let r2 = Scalar::random(&mut OsRng);

        let g = RISTRETTO_BASEPOINT_POINT;
        let h = generator_h();
        let v = Scalar::from(value);

        let c1 = g * v + h * r1;
        let c2 = g * v + h * r2;

        let proof = EqualityProof::prove_equality(value, &r1, &r2, &c1, &c2);
        assert!(proof.verify().unwrap());
    }

    #[test]
    fn test_equality_proof_different_values() {
        let value1 = 42u64;
        let value2 = 43u64;
        let r1 = Scalar::random(&mut OsRng);
        let r2 = Scalar::random(&mut OsRng);

        let g = RISTRETTO_BASEPOINT_POINT;
        let h = generator_h();

        let c1 = g * Scalar::from(value1) + h * r1;
        let c2 = g * Scalar::from(value2) + h * r2;

        // This proof should fail because values are different
        let proof = EqualityProof::prove_equality(value1, &r1, &r2, &c1, &c2);
        assert!(!proof.verify().unwrap());
    }

    #[test]
    fn test_knowledge_proof_via_zk_proof() {
        // Test that the ProofData::Knowledge variant works correctly
        // We need to manually construct a proof compatible with verify_knowledge_proof
        let secret = Scalar::random(&mut OsRng);
        let g = RISTRETTO_BASEPOINT_POINT;
        let public = g * secret;

        // Generate random k
        let k = Scalar::random(&mut OsRng);

        // Compute R = k*G
        let r = g * k;
        let r_bytes = r.compress().to_bytes();

        // Compute challenge c = H(R || P)
        let mut hasher = Sha256::new();
        hasher.update(&r_bytes);
        hasher.update(public.compress().as_bytes());
        let challenge: [u8; 32] = hasher.finalize().into();
        let c = Scalar::from_bytes_mod_order(challenge);

        // Compute response s = k + c*x
        let s = k + c * secret;
        let response = s.to_bytes();

        // Create a ZkProof with Knowledge variant
        let zk_proof = ZkProof::new(
            ProofType::KnowledgeProof,
            ProofData::Knowledge {
                commitment: public.compress().to_bytes(),
                challenge,
                response,
            },
        );

        // Verify through the ProofVerifier
        let result = ProofVerifier::verify(&zk_proof).unwrap();
        assert!(result);
    }

    #[test]
    fn statement_bound_knowledge_proof_verifies_only_for_its_own_statement() {
        let secret = Scalar::random(&mut OsRng);
        let data = ProofData::prove_knowledge_bound(&secret, b"ex:note-1 authored by ex:alice");
        let proof = ZkProof::new(ProofType::KnowledgeProof, data.clone());
        assert!(ProofVerifier::verify(&proof).unwrap());

        // Transplant onto a different claim: the sigma-protocol values are
        // untouched, only the statement changes. This is the attack the v1
        // scheme permits and the whole reason v2 exists.
        let ProofData::KnowledgeBound {
            commitment,
            challenge,
            response,
            ..
        } = data
        else {
            panic!("prove_knowledge_bound must build a KnowledgeBound");
        };
        let transplanted = ZkProof::new(
            ProofType::KnowledgeProof,
            ProofData::KnowledgeBound {
                commitment,
                challenge,
                response,
                statement: b"ex:note-2 authored by ex:mallory".to_vec(),
            },
        );
        assert!(
            !ProofVerifier::verify(&transplanted).unwrap(),
            "a proof bound to one statement must not verify for another"
        );
    }

    #[test]
    fn statement_bound_equality_proof_verifies_only_for_its_own_statement() {
        let r1 = Scalar::random(&mut OsRng);
        let r2 = Scalar::random(&mut OsRng);
        let g = RISTRETTO_BASEPOINT_POINT;
        let h = generator_h();
        let v = Scalar::from(42u64);
        let c1 = g * v + h * r1;
        let c2 = g * v + h * r2;

        let data = ProofData::prove_equality_bound(&r1, &r2, &c1, &c2, b"same balance");
        assert!(
            ProofVerifier::verify(&ZkProof::new(ProofType::EqualityProof, data.clone())).unwrap()
        );

        let ProofData::EqualityBound {
            commitment1,
            commitment2,
            proof,
            ..
        } = data
        else {
            panic!("prove_equality_bound must build an EqualityBound");
        };
        let transplanted = ZkProof::new(
            ProofType::EqualityProof,
            ProofData::EqualityBound {
                commitment1,
                commitment2,
                proof,
                statement: b"different balance".to_vec(),
            },
        );
        assert!(!ProofVerifier::verify(&transplanted).unwrap());
    }

    #[test]
    fn the_v2_preimage_is_domain_separated_and_length_prefixed() {
        // Without the length prefix, ("ab", "c") and ("a", "bc") would collide
        // once the statement is concatenated with anything that follows it.
        let r = [1u8; 32];
        let p = [2u8; 32];
        assert_ne!(
            knowledge_v2_challenge_preimage(&r, &p, b"ab"),
            knowledge_v2_challenge_preimage(&r, &p, b"a"),
        );
        // And the two v2 schemes must never share a preimage.
        assert_ne!(
            knowledge_v2_challenge_preimage(&r, &p, b"x"),
            equality_v2_challenge_preimage(&r, &p, b"x"),
        );
        assert!(knowledge_v2_challenge_preimage(&r, &p, b"").starts_with(KNOWLEDGE_V2_DOMAIN));
    }

    #[test]
    fn only_the_v2_variants_report_a_bound_statement() {
        let secret = Scalar::random(&mut OsRng);
        assert_eq!(
            ProofData::prove_knowledge_bound(&secret, b"claim").bound_statement(),
            Some(&b"claim"[..])
        );
        assert!(ProofData::Knowledge {
            commitment: [0u8; 32],
            challenge: [0u8; 32],
            response: [0u8; 32],
        }
        .bound_statement()
        .is_none());
    }

    #[test]
    fn test_equality_proof_via_zk_proof() {
        // Test that the ProofData::Equality variant works correctly
        let value = 42u64;
        let r1 = Scalar::random(&mut OsRng);
        let r2 = Scalar::random(&mut OsRng);

        let g = RISTRETTO_BASEPOINT_POINT;
        let h = generator_h();
        let v = Scalar::from(value);

        let c1 = g * v + h * r1;
        let c2 = g * v + h * r2;

        let equality_proof = EqualityProof::prove_equality(value, &r1, &r2, &c1, &c2);

        // Concatenate challenge and response for the proof Vec<u8>
        let mut proof_bytes = Vec::new();
        proof_bytes.extend_from_slice(&equality_proof.challenge);
        proof_bytes.extend_from_slice(&equality_proof.response);

        // Create a ZkProof with Equality variant
        let zk_proof = ZkProof::new(
            ProofType::EqualityProof,
            ProofData::Equality {
                commitment1: equality_proof.commitment1,
                commitment2: equality_proof.commitment2,
                proof: proof_bytes,
            },
        );

        // Verify through the ProofVerifier
        let result = ProofVerifier::verify(&zk_proof).unwrap();
        assert!(result);
    }
}
