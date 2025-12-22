//! Proof verification using aingle_zk
//!
//! This module integrates with aingle_zk to verify different types of
//! zero-knowledge proofs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::store::{ProofType, StoredProof};

/// Verification error types
#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum VerificationError {
    /// Proof not found in store
    #[error("Proof not found: {0}")]
    ProofNotFound(String),

    /// Invalid proof data format
    #[error("Invalid proof data: {0}")]
    InvalidProofData(String),

    /// Proof verification failed
    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    /// Unsupported proof type
    #[error("Unsupported proof type: {0}")]
    UnsupportedProofType(String),

    /// Missing required data for verification
    #[error("Missing verification data: {0}")]
    MissingData(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    /// ZK library error
    #[error("ZK error: {0}")]
    ZkError(String),
}

/// Result of proof verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the proof is valid
    pub valid: bool,
    /// Type of proof verified
    pub proof_type: ProofType,
    /// Timestamp of verification
    pub verified_at: DateTime<Utc>,
    /// Verification details/messages
    pub details: Vec<String>,
    /// Time taken to verify (microseconds)
    pub verification_time_us: u64,
}

impl VerificationResult {
    /// Create a successful verification result
    pub fn success(proof_type: ProofType, verification_time_us: u64) -> Self {
        Self {
            valid: true,
            proof_type,
            verified_at: Utc::now(),
            details: vec!["Proof verification succeeded".to_string()],
            verification_time_us,
        }
    }

    /// Create a failed verification result
    pub fn failure(proof_type: ProofType, reason: String, verification_time_us: u64) -> Self {
        Self {
            valid: false,
            proof_type,
            verified_at: Utc::now(),
            details: vec![format!("Verification failed: {}", reason)],
            verification_time_us,
        }
    }

    /// Add a detail message
    pub fn with_detail(mut self, detail: String) -> Self {
        self.details.push(detail);
        self
    }
}

/// Proof verifier that integrates with aingle_zk
pub struct ProofVerifier {
    /// Configuration for verification
    config: VerifierConfig,
}

/// Configuration for proof verification
#[derive(Debug, Clone)]
pub struct VerifierConfig {
    /// Maximum proof size in bytes
    pub max_proof_size: usize,
    /// Timeout for verification in seconds
    pub timeout_seconds: u64,
    /// Enable strict verification mode
    pub strict_mode: bool,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            max_proof_size: 10 * 1024 * 1024, // 10 MB
            timeout_seconds: 30,
            strict_mode: false,
        }
    }
}

impl ProofVerifier {
    /// Create a new proof verifier
    pub fn new() -> Self {
        Self::with_config(VerifierConfig::default())
    }

    /// Create a verifier with custom configuration
    pub fn with_config(config: VerifierConfig) -> Self {
        Self { config }
    }

    /// Verify a stored proof
    pub async fn verify(
        &self,
        proof: &StoredProof,
    ) -> Result<VerificationResult, VerificationError> {
        let start = std::time::Instant::now();

        // Check proof size
        if proof.data.len() > self.config.max_proof_size {
            return Err(VerificationError::InvalidProofData(format!(
                "Proof size {} exceeds maximum {}",
                proof.data.len(),
                self.config.max_proof_size
            )));
        }

        // Deserialize the proof data into aingle_zk::ZkProof
        let zk_proof: aingle_zk::ZkProof = serde_json::from_slice(&proof.data)
            .map_err(|e| VerificationError::DeserializationError(e.to_string()))?;

        // Verify based on proof type
        let valid = match proof.proof_type {
            ProofType::Schnorr => self.verify_schnorr(&zk_proof).await?,
            ProofType::Equality => self.verify_equality(&zk_proof).await?,
            ProofType::Membership => self.verify_membership(&zk_proof).await?,
            ProofType::NonMembership => self.verify_non_membership(&zk_proof).await?,
            ProofType::Range => self.verify_range(&zk_proof).await?,
            ProofType::HashOpening => self.verify_hash_opening(&zk_proof).await?,
            ProofType::Knowledge => self.verify_knowledge(&zk_proof).await?,
        };

        let elapsed = start.elapsed();
        let verification_time_us = elapsed.as_micros() as u64;

        if valid {
            Ok(VerificationResult::success(
                proof.proof_type.clone(),
                verification_time_us,
            ))
        } else {
            Ok(VerificationResult::failure(
                proof.proof_type.clone(),
                "Proof verification returned false".to_string(),
                verification_time_us,
            ))
        }
    }

    /// Batch verify multiple proofs
    pub async fn batch_verify(
        &self,
        proofs: &[StoredProof],
    ) -> Vec<Result<VerificationResult, VerificationError>> {
        let mut results = Vec::new();
        for proof in proofs {
            results.push(self.verify(proof).await);
        }
        results
    }

    // Private verification methods for each proof type

    async fn verify_schnorr(
        &self,
        zk_proof: &aingle_zk::ZkProof,
    ) -> Result<bool, VerificationError> {
        // Use aingle_zk's ProofVerifier
        aingle_zk::ProofVerifier::verify(zk_proof)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    async fn verify_equality(
        &self,
        zk_proof: &aingle_zk::ZkProof,
    ) -> Result<bool, VerificationError> {
        aingle_zk::ProofVerifier::verify(zk_proof)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    async fn verify_membership(
        &self,
        zk_proof: &aingle_zk::ZkProof,
    ) -> Result<bool, VerificationError> {
        // For membership proofs, we need the actual data
        // This is a structural verification - the proof format is valid
        aingle_zk::ProofVerifier::verify(zk_proof)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    async fn verify_non_membership(
        &self,
        zk_proof: &aingle_zk::ZkProof,
    ) -> Result<bool, VerificationError> {
        // Non-membership proofs also require data
        // Here we verify the proof structure
        aingle_zk::ProofVerifier::verify(zk_proof)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    async fn verify_range(&self, zk_proof: &aingle_zk::ZkProof) -> Result<bool, VerificationError> {
        // Range proofs (bulletproofs) verification
        aingle_zk::ProofVerifier::verify(zk_proof)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    async fn verify_hash_opening(
        &self,
        zk_proof: &aingle_zk::ZkProof,
    ) -> Result<bool, VerificationError> {
        // Hash commitment opening verification
        aingle_zk::ProofVerifier::verify(zk_proof)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    async fn verify_knowledge(
        &self,
        zk_proof: &aingle_zk::ZkProof,
    ) -> Result<bool, VerificationError> {
        // Knowledge proof verification (Schnorr-like)
        aingle_zk::ProofVerifier::verify(zk_proof)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    /// Verify membership proof with actual data
    pub async fn verify_membership_with_data(
        &self,
        zk_proof: &aingle_zk::ZkProof,
        data: &[u8],
    ) -> Result<bool, VerificationError> {
        aingle_zk::ProofVerifier::verify_membership(zk_proof, data)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }

    /// Verify hash opening with actual data
    pub async fn verify_hash_opening_with_data(
        &self,
        zk_proof: &aingle_zk::ZkProof,
        data: &[u8],
    ) -> Result<bool, VerificationError> {
        aingle_zk::ProofVerifier::verify_hash_opening(zk_proof, data)
            .map_err(|e| VerificationError::ZkError(e.to_string()))
    }
}

impl Default for ProofVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch verification helper
pub struct BatchVerificationHelper {
    verifier: ProofVerifier,
    results: Vec<VerificationResult>,
}

impl BatchVerificationHelper {
    /// Create a new batch verification helper
    pub fn new(verifier: ProofVerifier) -> Self {
        Self {
            verifier,
            results: Vec::new(),
        }
    }

    /// Add a proof to verify
    pub async fn add(&mut self, proof: &StoredProof) -> Result<(), VerificationError> {
        let result = self.verifier.verify(proof).await?;
        self.results.push(result);
        Ok(())
    }

    /// Get all verification results
    pub fn results(&self) -> &[VerificationResult] {
        &self.results
    }

    /// Check if all proofs are valid
    pub fn all_valid(&self) -> bool {
        self.results.iter().all(|r| r.valid)
    }

    /// Get count of valid proofs
    pub fn valid_count(&self) -> usize {
        self.results.iter().filter(|r| r.valid).count()
    }

    /// Get count of invalid proofs
    pub fn invalid_count(&self) -> usize {
        self.results.iter().filter(|r| !r.valid).count()
    }

    /// Get total verification time
    pub fn total_time_us(&self) -> u64 {
        self.results.iter().map(|r| r.verification_time_us).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proofs::store::ProofMetadata;

    fn create_test_proof(proof_type: ProofType, valid: bool) -> StoredProof {
        // Create a valid ZkProof structure from aingle_zk
        let zk_proof = if valid {
            // Create a simple hash opening proof
            let commitment = aingle_zk::HashCommitment::commit(b"test data");
            aingle_zk::ZkProof::hash_opening(&commitment)
        } else {
            // Create invalid proof with wrong data
            aingle_zk::ZkProof::hash_opening(&aingle_zk::HashCommitment {
                hash: [0u8; 32],
                salt: [0u8; 32],
            })
        };

        let proof_json = serde_json::to_vec(&zk_proof).unwrap();
        StoredProof::new(proof_type, proof_json, ProofMetadata::default())
    }

    #[tokio::test]
    async fn test_verify_valid_proof() {
        let verifier = ProofVerifier::new();
        let proof = create_test_proof(ProofType::HashOpening, true);

        let result = verifier.verify(&proof).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.valid);
        assert_eq!(result.proof_type, ProofType::HashOpening);
        assert!(!result.details.is_empty());
    }

    #[tokio::test]
    async fn test_verify_invalid_proof() {
        let verifier = ProofVerifier::new();
        let proof = create_test_proof(ProofType::HashOpening, false);

        let result = verifier.verify(&proof).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(!result.valid);
    }

    #[tokio::test]
    async fn test_batch_verify() {
        let verifier = ProofVerifier::new();
        let proofs = vec![
            create_test_proof(ProofType::HashOpening, true),
            create_test_proof(ProofType::HashOpening, true),
        ];

        let results = verifier.batch_verify(&proofs).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
    }

    #[tokio::test]
    async fn test_batch_verification_helper() {
        let verifier = ProofVerifier::new();
        let mut helper = BatchVerificationHelper::new(verifier);

        let proof1 = create_test_proof(ProofType::Knowledge, true);
        let proof2 = create_test_proof(ProofType::Knowledge, true);

        helper.add(&proof1).await.unwrap();
        helper.add(&proof2).await.unwrap();

        assert_eq!(helper.results().len(), 2);
        assert!(helper.all_valid());
        assert_eq!(helper.valid_count(), 2);
        assert_eq!(helper.invalid_count(), 0);
        assert!(helper.total_time_us() > 0);
    }

    #[tokio::test]
    async fn test_verification_result_creation() {
        let success = VerificationResult::success(ProofType::Schnorr, 1000);
        assert!(success.valid);
        assert_eq!(success.verification_time_us, 1000);

        let failure =
            VerificationResult::failure(ProofType::Equality, "Test failure".to_string(), 2000);
        assert!(!failure.valid);
        assert_eq!(failure.verification_time_us, 2000);
        assert!(failure.details.iter().any(|d| d.contains("Test failure")));
    }

    #[tokio::test]
    async fn test_verifier_config() {
        let config = VerifierConfig {
            max_proof_size: 1024,
            timeout_seconds: 10,
            strict_mode: true,
        };

        let verifier = ProofVerifier::with_config(config.clone());
        assert_eq!(verifier.config.max_proof_size, 1024);
        assert_eq!(verifier.config.timeout_seconds, 10);
        assert!(verifier.config.strict_mode);
    }

    #[tokio::test]
    async fn test_proof_size_limit() {
        let config = VerifierConfig {
            max_proof_size: 10, // Very small limit
            timeout_seconds: 30,
            strict_mode: false,
        };
        let verifier = ProofVerifier::with_config(config);

        let proof = create_test_proof(ProofType::Knowledge, true);
        let result = verifier.verify(&proof).await;

        assert!(result.is_err());
        match result {
            Err(VerificationError::InvalidProofData(msg)) => {
                assert!(msg.contains("exceeds maximum"));
            }
            _ => panic!("Expected InvalidProofData error"),
        }
    }
}
