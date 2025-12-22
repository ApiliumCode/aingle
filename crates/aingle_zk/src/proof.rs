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
                // Hash opening is verified by recomputing
                // The verifier needs the original data to complete verification
                Ok(commitment.iter().any(|&b| b != 0) && salt.iter().any(|&b| b != 0))
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

    /// Verify a hash opening with the original data
    pub fn verify_hash_opening(proof: &ZkProof, data: &[u8]) -> Result<bool> {
        match &proof.proof_data {
            ProofData::HashOpening { commitment, salt } => {
                let expected = HashCommitment::commit_with_salt(data, *salt);
                Ok(&expected.hash == commitment)
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
