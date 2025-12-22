//! Common types for the AI module

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Feature vector for ML operations
pub type FeatureVector = Vec<f32>;

/// Embedding representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// Vector representation
    pub vector: FeatureVector,
    /// Dimensionality
    pub dim: usize,
}

impl Embedding {
    /// Create a new embedding
    pub fn new(vector: FeatureVector) -> Self {
        let dim = vector.len();
        Self { vector, dim }
    }

    /// Create a zero embedding of given dimension
    pub fn zeros(dim: usize) -> Self {
        Self {
            vector: vec![0.0; dim],
            dim,
        }
    }

    /// Compute cosine similarity with another embedding
    pub fn cosine_similarity(&self, other: &Embedding) -> f32 {
        if self.dim != other.dim {
            return 0.0;
        }

        let dot: f32 = self
            .vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum();

        let norm_a: f32 = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = other.vector.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    /// Compute L2 distance
    pub fn l2_distance(&self, other: &Embedding) -> f32 {
        if self.dim != other.dim {
            return f32::MAX;
        }

        self.vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

/// Pattern extracted from transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern identifier
    pub id: PatternId,
    /// Feature embedding
    pub embedding: Embedding,
    /// Metadata
    pub metadata: HashMap<String, String>,
    /// Timestamp when pattern was created
    pub created_at: u64,
}

/// Pattern identifier
pub type PatternId = [u8; 32];

/// Generate a pattern ID from bytes
pub fn pattern_id(data: &[u8]) -> PatternId {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&result[..32]);
    id
}

/// Memory match result
#[derive(Debug, Clone)]
pub struct MemoryMatch {
    /// Matched pattern
    pub pattern: Pattern,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// Source (short-term or long-term)
    pub source: MemorySource,
}

/// Source of memory match
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySource {
    /// From short-term memory
    ShortTerm,
    /// From long-term memory
    LongTerm,
}

/// Result of processing a transaction through Titans memory
#[derive(Debug, Clone)]
pub struct ProcessResult {
    /// Relevance score (0.0 - 1.0)
    pub relevance: f32,
    /// Surprise score (0.0 - 1.0)
    pub surprise: f32,
    /// Whether stored in long-term memory
    pub stored_long_term: bool,
    /// Anomaly detection result
    pub anomaly: Option<AnomalyResult>,
}

/// Anomaly detection result
#[derive(Debug, Clone)]
pub struct AnomalyResult {
    /// Is this an anomaly?
    pub is_anomaly: bool,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f32,
    /// Explanation
    pub reason: String,
}

/// Transaction type for AI processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTransaction {
    /// Transaction hash
    pub hash: [u8; 32],
    /// Timestamp
    pub timestamp: u64,
    /// Agent who created the transaction
    pub agent: [u8; 32],
    /// Entry type
    pub entry_type: String,
    /// Serialized entry data
    pub data: Vec<u8>,
    /// Entry size in bytes
    pub size: usize,
}

impl AiTransaction {
    /// Extract features from transaction
    pub fn extract_features(&self) -> FeatureVector {
        let mut features = Vec::with_capacity(16);

        // Size feature (normalized)
        features.push((self.size as f32).ln().max(0.0) / 20.0);

        // Time features (hour of day, day of week)
        let timestamp_secs = self.timestamp / 1000;
        let hour = ((timestamp_secs / 3600) % 24) as f32 / 24.0;
        features.push(hour);

        // Entry type hash features (first 4 bytes as float)
        let type_hash = pattern_id(self.entry_type.as_bytes());
        for i in 0..4 {
            features.push(type_hash[i] as f32 / 255.0);
        }

        // Agent features (first 4 bytes)
        for i in 0..4 {
            features.push(self.agent[i] as f32 / 255.0);
        }

        // Data entropy approximation (byte distribution)
        if !self.data.is_empty() {
            let mut byte_counts = [0u32; 256];
            for &byte in &self.data {
                byte_counts[byte as usize] += 1;
            }
            let len = self.data.len() as f32;
            let entropy: f32 = byte_counts
                .iter()
                .filter(|&&c| c > 0)
                .map(|&c| {
                    let p = c as f32 / len;
                    -p * p.ln()
                })
                .sum();
            features.push(entropy / 8.0); // Normalize to ~1.0 for random data
        } else {
            features.push(0.0);
        }

        // Pad to 16 features
        while features.len() < 16 {
            features.push(0.0);
        }

        features
    }

    /// Convert to pattern
    pub fn to_pattern(&self) -> Pattern {
        let embedding = Embedding::new(self.extract_features());
        Pattern {
            id: self.hash,
            embedding,
            metadata: HashMap::new(),
            created_at: self.timestamp,
        }
    }
}

/// Validation prediction result
#[derive(Debug, Clone)]
pub struct ValidationPrediction {
    /// Likely to be valid?
    pub likely_valid: bool,
    /// Confidence (0.0 - 1.0)
    pub confidence: f32,
    /// Estimated validation time in milliseconds
    pub estimated_time_ms: u64,
}

/// Consensus level for adaptive consensus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusLevel {
    /// Full validation by all validators
    Full,
    /// 67% of validators (supermajority)
    Majority,
    /// 51% of validators (simple majority)
    Quorum,
    /// Local validation only
    Local,
}

/// Resource category for auto-reconfiguration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceCategory {
    /// Abundant resources (server/cloud)
    Abundant,
    /// Normal resources (desktop)
    Normal,
    /// Limited resources (edge device)
    Limited,
    /// Critical resources (IoT sensor)
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_cosine_similarity() {
        let a = Embedding::new(vec![1.0, 0.0, 0.0]);
        let b = Embedding::new(vec![1.0, 0.0, 0.0]);
        assert!((a.cosine_similarity(&b) - 1.0).abs() < 0.001);

        let c = Embedding::new(vec![0.0, 1.0, 0.0]);
        assert!(a.cosine_similarity(&c).abs() < 0.001);
    }

    #[test]
    fn test_transaction_features() {
        let tx = AiTransaction {
            hash: [1u8; 32],
            timestamp: 1702656000000, // Example timestamp
            agent: [2u8; 32],
            entry_type: "test_entry".to_string(),
            data: vec![0, 1, 2, 3, 4, 5],
            size: 6,
        };

        let features = tx.extract_features();
        assert_eq!(features.len(), 16);
        assert!(features.iter().all(|&f| f >= 0.0 && f <= 2.0));
    }
}
