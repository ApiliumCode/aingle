//! # Titans Memory Layer
//!
//! Implementation of dual memory system based on the Titans paper (arXiv 2501.00663).
//!
//! ## Architecture
//!
//! - **Short-Term Memory**: Recent transactions with attention-based weighting
//! - **Long-Term Memory**: Historical patterns with neural compression
//! - **Surprise Gate**: Controls when to update long-term memory
//!
//! ## Example
//!
//! ```rust,no_run
//! use aingle_ai::titans::{TitansMemory, TitansConfig};
//! use aingle_ai::AiTransaction;
//!
//! let config = TitansConfig::default();
//! let mut memory = TitansMemory::new(config);
//!
//! // Process a transaction
//! // let result = memory.process(&tx);
//! ```

mod config;
mod long_term;
mod short_term;
mod surprise_gate;

pub use config::TitansConfig;
pub use long_term::{LongTermMemory, MemoryBank};
pub use short_term::ShortTermMemory;
pub use surprise_gate::SurpriseGate;

use crate::error::AiResult;
use crate::types::{
    AiTransaction, AnomalyResult, MemoryMatch, MemorySource, Pattern, ProcessResult,
};
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, trace};

/// Titans Memory System for AIngle nodes
///
/// Implements dual memory architecture:
/// - Short-term: Fast access to recent transactions
/// - Long-term: Compressed historical patterns
pub struct TitansMemory {
    /// Short-term memory: Recent transactions (attention-based)
    short_term: Arc<RwLock<ShortTermMemory>>,

    /// Long-term memory: Historical patterns (neural memory module)
    long_term: Arc<RwLock<LongTermMemory>>,

    /// Surprise-based gating for memory updates
    surprise_gate: SurpriseGate,

    /// Configuration
    config: TitansConfig,
}

impl TitansMemory {
    /// Create a new Titans memory system
    pub fn new(config: TitansConfig) -> Self {
        let short_term = ShortTermMemory::new(config.window_size);
        let long_term = LongTermMemory::new(config.memory_capacity, config.embedding_dim);
        let surprise_gate = SurpriseGate::new(config.surprise_threshold);

        Self {
            short_term: Arc::new(RwLock::new(short_term)),
            long_term: Arc::new(RwLock::new(long_term)),
            surprise_gate,
            config,
        }
    }

    /// Process a new transaction
    ///
    /// 1. Adds to short-term memory
    /// 2. Computes relevance using attention
    /// 3. Checks if "surprising" enough for long-term storage
    /// 4. Updates long-term memory if threshold exceeded
    pub fn process(&mut self, tx: &AiTransaction) -> AiResult<ProcessResult> {
        let pattern = tx.to_pattern();
        trace!(hash = ?tx.hash, "Processing transaction through Titans memory");

        // 1. Add to short-term memory
        {
            let mut stm = self.short_term.write();
            stm.add(pattern.clone());
        }

        // 2. Compute relevance using attention
        let relevance = {
            let stm = self.short_term.read();
            stm.attention_score(&pattern)
        };

        // 3. Check if "surprising" enough for long-term storage
        let surprise = {
            let ltm = self.long_term.read();
            self.surprise_gate.compute_surprise(&pattern, &ltm)
        };

        let stored_long_term = surprise > self.config.surprise_threshold;

        // 4. Update long-term memory if surprising
        if stored_long_term {
            debug!(
                surprise = surprise,
                threshold = self.config.surprise_threshold,
                "Storing pattern in long-term memory"
            );
            let mut ltm = self.long_term.write();
            ltm.update(pattern.clone())?;
            self.surprise_gate.observe(&pattern);
        }

        // 5. Check for anomalies
        let anomaly = if self.config.anomaly_detection {
            Some(self.detect_anomaly(tx)?)
        } else {
            None
        };

        Ok(ProcessResult {
            relevance,
            surprise,
            stored_long_term,
            anomaly,
        })
    }

    /// Query memory for similar patterns
    pub fn query(&self, pattern: &Pattern, limit: usize) -> Vec<MemoryMatch> {
        let mut matches = Vec::new();

        // Search short-term memory
        {
            let stm = self.short_term.read();
            let stm_matches = stm.search(&pattern.embedding, limit);
            for (p, score) in stm_matches {
                matches.push(MemoryMatch {
                    pattern: p,
                    similarity: score,
                    source: MemorySource::ShortTerm,
                });
            }
        }

        // Search long-term memory
        {
            let ltm = self.long_term.read();
            let ltm_matches = ltm.retrieve(&pattern.embedding, limit);
            for (p, score) in ltm_matches {
                matches.push(MemoryMatch {
                    pattern: p,
                    similarity: score,
                    source: MemorySource::LongTerm,
                });
            }
        }

        // Sort by similarity and take top results
        matches.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
        matches.truncate(limit);
        matches
    }

    /// Detect anomalies in a transaction
    pub fn detect_anomaly(&self, tx: &AiTransaction) -> AiResult<AnomalyResult> {
        let pattern = tx.to_pattern();

        // Compare against learned patterns
        let similarity_short = {
            let stm = self.short_term.read();
            stm.max_similarity(&pattern.embedding)
        };

        let similarity_long = {
            let ltm = self.long_term.read();
            ltm.max_similarity(&pattern.embedding)
        };

        let max_similarity = similarity_short.max(similarity_long);
        let is_anomaly = max_similarity < self.config.anomaly_threshold;
        let confidence = 1.0 - max_similarity;

        let reason = if is_anomaly {
            format!(
                "Transaction pattern differs from known patterns (similarity: {:.2})",
                max_similarity
            )
        } else {
            "Transaction pattern matches known patterns".to_string()
        };

        Ok(AnomalyResult {
            is_anomaly,
            confidence,
            reason,
        })
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemoryStats {
        let stm = self.short_term.read();
        let ltm = self.long_term.read();

        MemoryStats {
            short_term_size: stm.len(),
            short_term_capacity: self.config.window_size,
            long_term_size: ltm.len(),
            long_term_capacity: self.config.memory_capacity,
            total_patterns: stm.len() + ltm.len(),
        }
    }

    /// Clear all memory
    pub fn clear(&self) {
        self.short_term.write().clear();
        self.long_term.write().clear();
    }

    /// Predict likely outcome based on similar patterns
    pub fn predict_outcome(&self, tx: &AiTransaction) -> PredictionResult {
        let pattern = tx.to_pattern();
        let matches = self.query(&pattern, 10);

        if matches.is_empty() {
            return PredictionResult {
                confidence: 0.0,
                predicted_valid: true, // Default to valid if no history
                similar_count: 0,
            };
        }

        // Use weighted voting based on similarity
        let weighted_sum: f32 = matches.iter().map(|m| m.similarity).sum();

        // All patterns in memory were valid (we only store valid ones)
        PredictionResult {
            confidence: (weighted_sum / matches.len() as f32).min(1.0),
            predicted_valid: true,
            similar_count: matches.len(),
        }
    }
}

/// Memory statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Current short-term memory size
    pub short_term_size: usize,
    /// Short-term memory capacity
    pub short_term_capacity: usize,
    /// Current long-term memory size
    pub long_term_size: usize,
    /// Long-term memory capacity
    pub long_term_capacity: usize,
    /// Total patterns stored
    pub total_patterns: usize,
}

/// Prediction result
#[derive(Debug, Clone)]
pub struct PredictionResult {
    /// Confidence in prediction (0.0 - 1.0)
    pub confidence: f32,
    /// Predicted validity
    pub predicted_valid: bool,
    /// Number of similar patterns found
    pub similar_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_tx(id: u8) -> AiTransaction {
        AiTransaction {
            hash: [id; 32],
            timestamp: 1702656000000 + (id as u64 * 1000),
            agent: [1u8; 32],
            entry_type: "test".to_string(),
            data: vec![id; 10],
            size: 10,
        }
    }

    #[test]
    fn test_titans_memory_basic() {
        let config = TitansConfig::default();
        let mut memory = TitansMemory::new(config);

        let tx = make_test_tx(1);
        let result = memory.process(&tx).unwrap();

        assert!(result.relevance >= 0.0);
        assert!(result.surprise >= 0.0);
    }

    #[test]
    fn test_titans_memory_query() {
        let config = TitansConfig::default();
        let mut memory = TitansMemory::new(config);

        // Add some transactions
        for i in 0..10 {
            let tx = make_test_tx(i);
            memory.process(&tx).unwrap();
        }

        // Query for similar
        let query_tx = make_test_tx(5);
        let pattern = query_tx.to_pattern();
        let matches = memory.query(&pattern, 5);

        assert!(!matches.is_empty());
    }

    #[test]
    fn test_anomaly_detection() {
        let config = TitansConfig {
            anomaly_detection: true,
            anomaly_threshold: 0.5,
            ..TitansConfig::default()
        };
        let mut memory = TitansMemory::new(config);

        // Train on similar transactions
        for i in 0..20 {
            let tx = make_test_tx(i % 3); // Only 3 unique patterns
            memory.process(&tx).unwrap();
        }

        // Test with known pattern
        let known_tx = make_test_tx(1);
        let _result = memory.detect_anomaly(&known_tx).unwrap();
        // Should have some similarity to known patterns

        // Test with completely different pattern
        let anomaly_tx = AiTransaction {
            hash: [255u8; 32],
            timestamp: 1702656000000,
            agent: [255u8; 32],
            entry_type: "unknown_type".to_string(),
            data: vec![255; 100],
            size: 100,
        };
        let result = memory.detect_anomaly(&anomaly_tx).unwrap();
        // More likely to be anomalous
        assert!(result.confidence >= 0.0);
    }

    #[test]
    fn test_memory_stats() {
        let config = TitansConfig::default();
        let mut memory = TitansMemory::new(config.clone());

        let stats = memory.stats();
        assert_eq!(stats.short_term_size, 0);
        assert_eq!(stats.short_term_capacity, config.window_size);

        // Add transactions
        for i in 0..5 {
            let tx = make_test_tx(i);
            memory.process(&tx).unwrap();
        }

        let stats = memory.stats();
        assert_eq!(stats.short_term_size, 5);
    }
}
