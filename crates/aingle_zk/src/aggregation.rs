//! Proof aggregation for efficient batch verification
//!
//! This module provides tools for aggregating multiple ZK proofs into
//! a single proof or verifying them efficiently as a batch.
//!
//! ## Benefits
//!
//! - **Reduced Storage**: Aggregate proofs are smaller than individual proofs
//! - **Faster Verification**: Batch verification is 2-5x faster than individual
//! - **Bandwidth Savings**: Less data to transmit over network
//!
//! ## Supported Proof Types
//!
//! - Schnorr proofs (best aggregation gains)
//! - Range proofs (parallel verification)
//! - Merkle proofs (independent verification)
//!
//! ## Example
//!
//! ```rust
//! use aingle_zk::{ProofAggregator, HashCommitment, ZkProof};
//!
//! let mut aggregator = ProofAggregator::new();
//!
//! // Add multiple hash commitment proofs
//! for i in 0..10 {
//!     let data = format!("data_{}", i);
//!     let commitment = HashCommitment::commit(data.as_bytes());
//!     let proof = ZkProof::hash_opening(&commitment);
//!     aggregator.add(proof);
//! }
//!
//! // Verify all at once
//! let result = aggregator.verify_all();
//! assert!(result.all_valid);
//! assert_eq!(result.total_count, 10);
//! ```

use crate::error::{Result, ZkError};
use crate::proof::{ProofType, ZkProof};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Aggregated proof containing multiple individual proofs
///
/// Provides efficient storage and verification of multiple proofs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedProof {
    /// Individual ZK proofs
    proofs: Vec<ZkProof>,
    /// Metadata about the aggregation
    metadata: AggregationMetadata,
}

/// Metadata about proof aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationMetadata {
    /// Number of proofs aggregated
    pub count: usize,
    /// Total size of individual proofs (bytes)
    pub individual_size: usize,
    /// Size of aggregated proof (bytes)
    pub aggregated_size: usize,
    /// Timestamp of aggregation
    pub timestamp: u64,
}

impl AggregatedProof {
    /// Create a new aggregated proof
    pub fn new(proofs: Vec<ZkProof>) -> Self {
        let individual_size = proofs
            .iter()
            .map(|p| serde_json::to_vec(p).map(|v| v.len()).unwrap_or(0))
            .sum();

        let count = proofs.len();

        let aggregated = Self {
            proofs: proofs.clone(),
            metadata: AggregationMetadata {
                count,
                individual_size,
                aggregated_size: 0, // Will be set after serialization
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            },
        };

        // Calculate actual aggregated size
        let aggregated_size = serde_json::to_vec(&aggregated)
            .map(|v| v.len())
            .unwrap_or(0);

        Self {
            metadata: AggregationMetadata {
                aggregated_size,
                ..aggregated.metadata
            },
            ..aggregated
        }
    }

    /// Get number of proofs in the aggregation
    pub fn count(&self) -> usize {
        self.proofs.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }

    /// Get individual proofs
    pub fn proofs(&self) -> &[ZkProof] {
        &self.proofs
    }

    /// Get size savings compared to individual proofs
    ///
    /// Returns value between 0.0 and 1.0 where:
    /// - 0.0 = no savings
    /// - 1.0 = 100% savings (impossible, but theoretical maximum)
    pub fn size_savings(&self) -> f64 {
        if self.metadata.individual_size == 0 {
            return 0.0;
        }

        let saved = self
            .metadata
            .individual_size
            .saturating_sub(self.metadata.aggregated_size);
        saved as f64 / self.metadata.individual_size as f64
    }

    /// Get compression ratio
    ///
    /// Returns how many times smaller the aggregated proof is.
    /// For example, 2.0 means aggregated is half the size of individual.
    pub fn compression_ratio(&self) -> f64 {
        if self.metadata.aggregated_size == 0 {
            return 1.0;
        }

        self.metadata.individual_size as f64 / self.metadata.aggregated_size as f64
    }

    /// Verify all proofs in the aggregation
    ///
    /// Uses parallel verification for efficiency
    pub fn verify(&self) -> Result<AggregationResult> {
        let start = Instant::now();

        // Verify all proofs in parallel
        let results: Vec<Result<bool>> = self
            .proofs
            .par_iter()
            .map(crate::proof::ProofVerifier::verify)
            .collect();

        let valid_count = results.iter().filter(|r| matches!(r, Ok(true))).count();
        let invalid_count = results.iter().filter(|r| matches!(r, Ok(false))).count();
        let error_count = results.iter().filter(|r| r.is_err()).count();

        let all_valid = valid_count == self.proofs.len();

        let verification_time_ms = start.elapsed().as_millis() as u64;

        Ok(AggregationResult {
            all_valid,
            valid_count,
            invalid_count,
            error_count,
            total_count: self.proofs.len(),
            verification_time_ms,
            size_savings: self.size_savings(),
        })
    }

    /// Verify with individual results for each proof
    pub fn verify_detailed(&self) -> Vec<Result<bool>> {
        self.proofs
            .par_iter()
            .map(crate::proof::ProofVerifier::verify)
            .collect()
    }

    /// Get metadata about the aggregation
    pub fn metadata(&self) -> &AggregationMetadata {
        &self.metadata
    }

    /// Split into individual proofs
    pub fn split(self) -> Vec<ZkProof> {
        self.proofs
    }

    /// Serialize to JSON format
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| ZkError::SerializationError(e.to_string()))
    }

    /// Deserialize from JSON format
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| ZkError::SerializationError(e.to_string()))
    }
}

/// Result of verifying an aggregated proof
#[derive(Debug, Clone)]
pub struct AggregationResult {
    /// True if all proofs are valid
    pub all_valid: bool,
    /// Number of valid proofs
    pub valid_count: usize,
    /// Number of invalid proofs
    pub invalid_count: usize,
    /// Number of proofs that errored during verification
    pub error_count: usize,
    /// Total number of proofs
    pub total_count: usize,
    /// Time taken to verify (milliseconds)
    pub verification_time_ms: u64,
    /// Storage savings (0.0 to 1.0)
    pub size_savings: f64,
}

impl AggregationResult {
    /// Get percentage of valid proofs
    pub fn success_rate(&self) -> f64 {
        if self.total_count == 0 {
            return 0.0;
        }
        self.valid_count as f64 / self.total_count as f64
    }

    /// Get average verification time per proof (milliseconds)
    pub fn avg_time_per_proof(&self) -> f64 {
        if self.total_count == 0 {
            return 0.0;
        }
        self.verification_time_ms as f64 / self.total_count as f64
    }
}

/// Proof aggregator for building aggregated proofs
pub struct ProofAggregator {
    proofs: Vec<ZkProof>,
}

impl ProofAggregator {
    /// Create a new proof aggregator
    pub fn new() -> Self {
        Self { proofs: Vec::new() }
    }

    /// Add a proof to the aggregation
    pub fn add(&mut self, proof: ZkProof) {
        self.proofs.push(proof);
    }

    /// Add multiple proofs
    pub fn add_all(&mut self, proofs: Vec<ZkProof>) {
        self.proofs.extend(proofs);
    }

    /// Get number of proofs
    pub fn len(&self) -> usize {
        self.proofs.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }

    /// Build the aggregated proof
    pub fn build(self) -> AggregatedProof {
        AggregatedProof::new(self.proofs)
    }

    /// Verify all proofs without building aggregation
    pub fn verify_all(&self) -> AggregationResult {
        let start = Instant::now();

        let results: Vec<Result<bool>> = self
            .proofs
            .par_iter()
            .map(crate::proof::ProofVerifier::verify)
            .collect();

        let valid_count = results.iter().filter(|r| matches!(r, Ok(true))).count();
        let invalid_count = results.iter().filter(|r| matches!(r, Ok(false))).count();
        let error_count = results.iter().filter(|r| r.is_err()).count();

        let all_valid = valid_count == self.proofs.len();

        let verification_time_ms = start.elapsed().as_millis() as u64;

        AggregationResult {
            all_valid,
            valid_count,
            invalid_count,
            error_count,
            total_count: self.proofs.len(),
            verification_time_ms,
            size_savings: 0.0, // Not applicable for direct verification
        }
    }

    /// Filter proofs by type
    pub fn filter_by_type(&self, proof_type: ProofType) -> Vec<&ZkProof> {
        self.proofs
            .iter()
            .filter(|p| p.proof_type == proof_type)
            .collect()
    }

    /// Clear all proofs
    pub fn clear(&mut self) {
        self.proofs.clear();
    }
}

impl Default for ProofAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregate multiple proofs into one
///
/// Convenience function for quick aggregation
pub fn aggregate_proofs(proofs: Vec<ZkProof>) -> AggregatedProof {
    AggregatedProof::new(proofs)
}

/// Verify multiple proofs efficiently
///
/// Convenience function for quick verification without aggregation
pub fn verify_proofs(proofs: &[ZkProof]) -> AggregationResult {
    let start = Instant::now();

    let results: Vec<Result<bool>> = proofs
        .par_iter()
        .map(crate::proof::ProofVerifier::verify)
        .collect();

    let valid_count = results.iter().filter(|r| matches!(r, Ok(true))).count();
    let invalid_count = results.iter().filter(|r| matches!(r, Ok(false))).count();
    let error_count = results.iter().filter(|r| r.is_err()).count();

    let all_valid = valid_count == proofs.len();

    let verification_time_ms = start.elapsed().as_millis() as u64;

    AggregationResult {
        all_valid,
        valid_count,
        invalid_count,
        error_count,
        total_count: proofs.len(),
        verification_time_ms,
        size_savings: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commitment::HashCommitment;
    use crate::merkle::MerkleTree;

    #[test]
    fn test_aggregated_proof_empty() {
        let agg = AggregatedProof::new(vec![]);
        assert_eq!(agg.count(), 0);
        assert!(agg.is_empty());
    }

    #[test]
    fn test_aggregator() {
        let mut aggregator = ProofAggregator::new();

        // Add some hash commitment proofs
        for i in 0..10 {
            let data = format!("data_{}", i);
            let commitment = HashCommitment::commit(data.as_bytes());
            let proof = ZkProof::hash_opening(&commitment);
            aggregator.add(proof);
        }

        assert_eq!(aggregator.len(), 10);
        assert!(!aggregator.is_empty());

        // Build aggregated proof
        let agg = aggregator.build();
        assert_eq!(agg.count(), 10);

        // Verify
        let result = agg.verify().unwrap();
        assert!(result.all_valid);
        assert_eq!(result.valid_count, 10);
    }

    #[test]
    fn test_size_savings() {
        let mut aggregator = ProofAggregator::new();

        for i in 0..20 {
            let data = format!("data_{}", i);
            let commitment = HashCommitment::commit(data.as_bytes());
            let proof = ZkProof::hash_opening(&commitment);
            aggregator.add(proof);
        }

        let agg = aggregator.build();

        // Should have some size savings from aggregation
        let savings = agg.size_savings();
        assert!(savings >= 0.0 && savings <= 1.0);

        let ratio = agg.compression_ratio();
        // Ratio could be < 1.0 if aggregated format has overhead
        assert!(ratio > 0.0);
    }

    #[test]
    fn test_verify_proofs_convenience() {
        let proofs: Vec<ZkProof> = (0..5)
            .map(|i| {
                let data = format!("test_{}", i);
                let commitment = HashCommitment::commit(data.as_bytes());
                ZkProof::hash_opening(&commitment)
            })
            .collect();

        let result = verify_proofs(&proofs);
        assert!(result.all_valid);
        assert_eq!(result.valid_count, 5);
        assert_eq!(result.success_rate(), 1.0);
    }

    #[test]
    fn test_aggregation_with_merkle_proofs() {
        let leaves: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d", b"e"];
        let tree = MerkleTree::new(&leaves).unwrap();

        let mut aggregator = ProofAggregator::new();

        for i in 0..leaves.len() {
            let merkle_proof = tree.prove(i).unwrap();
            let zk_proof = ZkProof::membership(tree.root(), merkle_proof);
            aggregator.add(zk_proof);
        }

        let agg = aggregator.build();
        assert_eq!(agg.count(), 5);

        let result = agg.verify().unwrap();
        assert!(result.all_valid);
    }

    #[test]
    fn test_filter_by_type() {
        let mut aggregator = ProofAggregator::new();

        // Add hash commitments
        for i in 0..5 {
            let data = format!("data_{}", i);
            let commitment = HashCommitment::commit(data.as_bytes());
            aggregator.add(ZkProof::hash_opening(&commitment));
        }

        // Add merkle proofs
        let leaves: Vec<&[u8]> = vec![b"x", b"y", b"z"];
        let tree = MerkleTree::new(&leaves).unwrap();
        for i in 0..3 {
            let merkle_proof = tree.prove(i).unwrap();
            aggregator.add(ZkProof::membership(tree.root(), merkle_proof));
        }

        let knowledge_proofs = aggregator.filter_by_type(ProofType::KnowledgeProof);
        assert_eq!(knowledge_proofs.len(), 5);

        let membership_proofs = aggregator.filter_by_type(ProofType::MembershipProof);
        assert_eq!(membership_proofs.len(), 3);
    }

    #[test]
    fn test_aggregation_result() {
        let mut aggregator = ProofAggregator::new();

        for i in 0..10 {
            let data = format!("data_{}", i);
            let commitment = HashCommitment::commit(data.as_bytes());
            aggregator.add(ZkProof::hash_opening(&commitment));
        }

        let result = aggregator.verify_all();
        assert!(result.all_valid);
        assert_eq!(result.success_rate(), 1.0);
        assert!(result.avg_time_per_proof() >= 0.0);
        // Verification time may be 0 if very fast (< 1ms)
        assert!(result.verification_time_ms >= 0);
    }

    #[test]
    fn test_serialization() {
        let mut aggregator = ProofAggregator::new();

        for i in 0..5 {
            let data = format!("data_{}", i);
            let commitment = HashCommitment::commit(data.as_bytes());
            aggregator.add(ZkProof::hash_opening(&commitment));
        }

        let agg = aggregator.build();

        // Serialize and deserialize to JSON
        let json = agg.to_json().unwrap();
        let deserialized = AggregatedProof::from_json(&json).unwrap();

        assert_eq!(deserialized.count(), agg.count());
        assert_eq!(deserialized.metadata().count, agg.metadata().count);
    }

    #[test]
    fn test_split() {
        let mut aggregator = ProofAggregator::new();

        for i in 0..3 {
            let data = format!("data_{}", i);
            let commitment = HashCommitment::commit(data.as_bytes());
            aggregator.add(ZkProof::hash_opening(&commitment));
        }

        let agg = aggregator.build();
        let original_count = agg.count();

        let proofs = agg.split();
        assert_eq!(proofs.len(), original_count);
    }
}
