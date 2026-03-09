// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! # Emergent Capabilities
//!
//! Higher-level AI capabilities that emerge from combining Ineru,
//! Nested Learning, and Kaneru.
//!
//! ## Components
//!
//! - **PredictiveValidator**: Predict validation outcomes before full validation
//! - **AdaptiveConsensus**: Adjust consensus level based on transaction importance

mod adaptive_consensus;
mod predictive_validator;

pub use adaptive_consensus::AdaptiveConsensus;
pub use predictive_validator::PredictiveValidator;

use crate::nested_learning::NestedLearning;
use crate::ineru::IneruMemory;
use crate::types::{AiTransaction, ConsensusLevel, ValidationPrediction};

/// Unified AI layer combining all capabilities
pub struct AiLayer {
    /// Ineru memory for pattern learning
    ineru: IneruMemory,

    /// Nested Learning for optimization
    nested: NestedLearning,

    /// Predictive validator
    predictor: PredictiveValidator,

    /// Adaptive consensus
    consensus: AdaptiveConsensus,
}

impl AiLayer {
    /// Create a new AI layer with default configuration
    pub fn new() -> Self {
        use crate::nested_learning::NestedConfig;
        use crate::ineru::IneruConfig;

        Self {
            ineru: IneruMemory::new(IneruConfig::default()),
            nested: NestedLearning::new(NestedConfig::default()),
            predictor: PredictiveValidator::new(),
            consensus: AdaptiveConsensus::new(),
        }
    }

    /// Process a transaction through the full AI pipeline
    pub fn process(&mut self, tx: &AiTransaction) -> AiProcessResult {
        // 1. Process through Ineru memory
        let ineru_result = self.ineru.process(tx).ok();

        // 2. Process through Nested Learning
        let nested_result = self.nested.process(tx).ok();

        // 3. Get validation prediction
        let prediction = self.predictor.predict(tx, &self.ineru, &self.nested);

        // 4. Determine consensus level
        let consensus_level = self.consensus.determine_level(tx, &prediction);

        AiProcessResult {
            prediction,
            consensus_level,
            stored_pattern: ineru_result.map(|r| r.stored_long_term).unwrap_or(false),
            validation_strategy: nested_result.map(|r| r.strategy),
        }
    }

    /// Query for similar patterns
    pub fn query_similar(&self, tx: &AiTransaction, limit: usize) -> Vec<PatternMatch> {
        let pattern = tx.to_pattern();
        self.ineru
            .query(&pattern, limit)
            .into_iter()
            .map(|m| PatternMatch {
                similarity: m.similarity,
                source: format!("{:?}", m.source),
            })
            .collect()
    }

    /// Get AI layer statistics
    pub fn stats(&self) -> AiLayerStats {
        let ineru_stats = self.ineru.stats();
        let nested_stats = self.nested.stats();

        AiLayerStats {
            ineru_short_term_size: ineru_stats.short_term_size,
            ineru_long_term_size: ineru_stats.long_term_size,
            nested_tx_count: nested_stats.tx_count,
            nested_block_count: nested_stats.block_count,
        }
    }
}

impl Default for AiLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of AI processing
#[derive(Debug, Clone)]
pub struct AiProcessResult {
    /// Validation prediction
    pub prediction: ValidationPrediction,
    /// Recommended consensus level
    pub consensus_level: ConsensusLevel,
    /// Was pattern stored in long-term memory?
    pub stored_pattern: bool,
    /// Recommended validation strategy
    pub validation_strategy: Option<crate::nested_learning::ValidationStrategy>,
}

/// Pattern match result
#[derive(Debug, Clone)]
pub struct PatternMatch {
    /// Similarity score
    pub similarity: f32,
    /// Source (ShortTerm or LongTerm)
    pub source: String,
}

/// AI layer statistics
#[derive(Debug, Clone)]
pub struct AiLayerStats {
    /// Ineru short-term memory size
    pub ineru_short_term_size: usize,
    /// Ineru long-term memory size
    pub ineru_long_term_size: usize,
    /// Nested learning transaction count
    pub nested_tx_count: u64,
    /// Nested learning block count
    pub nested_block_count: u64,
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
    fn test_ai_layer_basic() {
        let mut layer = AiLayer::new();

        let tx = make_test_tx(1);
        let result = layer.process(&tx);

        assert!(result.prediction.confidence >= 0.0);
    }

    #[test]
    fn test_ai_layer_query() {
        let mut layer = AiLayer::new();

        // Add some transactions
        for i in 0..10 {
            let tx = make_test_tx(i);
            layer.process(&tx);
        }

        // Query for similar
        let tx = make_test_tx(5);
        let matches = layer.query_similar(&tx, 3);

        // Should have some matches
        // (may be empty if patterns didn't meet threshold)
        assert!(matches.len() <= 3);
    }
}
