//! # Nested Learning Layer
//!
//! Multi-level optimization system based on Nested Learning paper (OpenReview nbMeRvNb7A).
//!
//! ## Architecture
//!
//! - **Meta-Level**: Global network parameters (slow updates, ~1000 blocks)
//! - **Optimizer-Level**: Validation strategies (medium updates, ~100 transactions)
//! - **Transaction-Level**: Data processing (fast updates, per transaction)
//!
//! ## Example
//!
//! ```rust,no_run
//! use aingle_ai::nested_learning::{NestedLearning, NestedConfig};
//!
//! let config = NestedConfig::default();
//! let mut nested = NestedLearning::new(config);
//!
//! // Process transactions through multi-level optimization
//! // let result = nested.process(&tx);
//! ```

mod config;
mod meta_level;
mod optimizer_level;
mod transaction_level;

pub use config::NestedConfig;
pub use meta_level::MetaLevel;
pub use optimizer_level::{OptimizerLevel, ValidationPlan, ValidationStrategy};
pub use transaction_level::{ProcessedTransaction, TransactionLevel};

use crate::error::AiResult;
use crate::types::AiTransaction;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, trace};

/// Nested Learning System for multi-level optimization
pub struct NestedLearning {
    /// Meta-level: Global network parameters
    pub meta_level: Arc<RwLock<MetaLevel>>,

    /// Optimizer-level: Validation parameters
    pub optimizer_level: Arc<RwLock<OptimizerLevel>>,

    /// Transaction-level: Data processing
    pub transaction_level: Arc<RwLock<TransactionLevel>>,

    /// Configuration
    config: NestedConfig,

    /// Block counter for meta-level updates
    block_count: u64,

    /// Transaction counter for optimizer-level updates
    tx_count: u64,
}

impl NestedLearning {
    /// Create a new nested learning system
    pub fn new(config: NestedConfig) -> Self {
        Self {
            meta_level: Arc::new(RwLock::new(MetaLevel::new(&config))),
            optimizer_level: Arc::new(RwLock::new(OptimizerLevel::new(&config))),
            transaction_level: Arc::new(RwLock::new(TransactionLevel::new(&config))),
            config,
            block_count: 0,
            tx_count: 0,
        }
    }

    /// Process a transaction through all levels
    pub fn process(&mut self, tx: &AiTransaction) -> AiResult<NestedResult> {
        trace!(hash = ?tx.hash, "Processing transaction through nested learning");

        // 1. Transaction-level processing (always runs)
        let processed = {
            let mut tl = self.transaction_level.write();
            tl.process(tx.clone())
        };

        // 2. Get validation strategy from optimizer level
        let strategy = {
            let ol = self.optimizer_level.read();
            ol.get_strategy(&processed)
        };

        // 3. Increment transaction counter
        self.tx_count += 1;

        // 4. Check if optimizer-level update is needed
        if self.tx_count % self.config.optimizer_update_interval == 0 {
            debug!(
                tx_count = self.tx_count,
                "Triggering optimizer-level update"
            );
            let mut ol = self.optimizer_level.write();
            ol.periodic_update();
        }

        Ok(NestedResult {
            processed,
            strategy,
            meta_params: self.get_meta_params(),
        })
    }

    /// Notify of a new block (triggers meta-level update check)
    pub fn on_new_block(&mut self, block_stats: &BlockStats) {
        self.block_count += 1;

        // Check if meta-level update is needed
        if self.block_count % self.config.meta_update_interval == 0 {
            debug!(
                block_count = self.block_count,
                "Triggering meta-level update"
            );
            let mut ml = self.meta_level.write();
            ml.update(block_stats);
        }
    }

    /// Learn from validation outcome
    pub fn learn(&mut self, tx: &AiTransaction, outcome: &ValidationOutcome) {
        // Update optimizer level with outcome
        let mut ol = self.optimizer_level.write();
        ol.learn(tx, outcome);

        // Update transaction level features
        let mut tl = self.transaction_level.write();
        tl.update_features(tx, outcome);
    }

    /// Get current meta parameters
    pub fn get_meta_params(&self) -> MetaParams {
        let ml = self.meta_level.read();
        ml.get_params()
    }

    /// Get validation plan for a batch of transactions
    pub fn get_validation_plan(&self, batch: &[AiTransaction]) -> ValidationPlan {
        let ol = self.optimizer_level.read();
        ol.create_plan(batch)
    }

    /// Get statistics
    pub fn stats(&self) -> NestedStats {
        NestedStats {
            block_count: self.block_count,
            tx_count: self.tx_count,
            meta_update_count: self.block_count / self.config.meta_update_interval,
            optimizer_update_count: self.tx_count / self.config.optimizer_update_interval,
        }
    }
}

/// Result of nested learning processing
#[derive(Debug, Clone)]
pub struct NestedResult {
    /// Processed transaction with features
    pub processed: ProcessedTransaction,
    /// Recommended validation strategy
    pub strategy: ValidationStrategy,
    /// Current meta parameters
    pub meta_params: MetaParams,
}

/// Block statistics for meta-level updates
#[derive(Debug, Clone, Default)]
pub struct BlockStats {
    /// Number of transactions in block
    pub tx_count: usize,
    /// Block processing time in milliseconds
    pub processing_time_ms: u64,
    /// Number of validation failures
    pub failures: usize,
    /// Network latency 50th percentile (median)
    pub latency_p50: u64,
    /// Network latency 90th percentile
    pub latency_p90: u64,
    /// Network latency 99th percentile
    pub latency_p99: u64,
    /// Peer count
    pub peer_count: usize,
}

/// Validation outcome for learning
#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    /// Was the transaction valid?
    pub valid: bool,
    /// Actual validation time in milliseconds
    pub time_ms: u64,
    /// Error message if invalid
    pub error: Option<String>,
}

/// Meta-level parameters
#[derive(Debug, Clone)]
pub struct MetaParams {
    /// Target throughput (tx/s)
    pub target_throughput: f64,
    /// Target latency (ms)
    pub target_latency: u64,
    /// Validation strictness (0.0 - 1.0)
    pub validation_strictness: f32,
    /// Gossip frequency multiplier
    pub gossip_multiplier: f32,
}

impl Default for MetaParams {
    fn default() -> Self {
        Self {
            target_throughput: 1000.0,
            target_latency: 500,
            validation_strictness: 0.8,
            gossip_multiplier: 1.0,
        }
    }
}

/// Nested learning statistics
#[derive(Debug, Clone)]
pub struct NestedStats {
    /// Total blocks processed
    pub block_count: u64,
    /// Total transactions processed
    pub tx_count: u64,
    /// Number of meta-level updates
    pub meta_update_count: u64,
    /// Number of optimizer-level updates
    pub optimizer_update_count: u64,
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
    fn test_nested_learning_basic() {
        let config = NestedConfig::default();
        let mut nested = NestedLearning::new(config);

        let tx = make_test_tx(1);
        let result = nested.process(&tx).unwrap();

        assert!(result.processed.features.len() > 0);
    }

    #[test]
    fn test_nested_learning_batch() {
        let config = NestedConfig::default();
        let nested = NestedLearning::new(config);

        let batch: Vec<_> = (0..10).map(|i| make_test_tx(i)).collect();
        let plan = nested.get_validation_plan(&batch);

        assert_eq!(plan.order.len(), 10);
    }

    #[test]
    fn test_meta_level_update() {
        let mut config = NestedConfig::default();
        config.meta_update_interval = 5; // Update every 5 blocks
        let mut nested = NestedLearning::new(config);

        let stats = BlockStats::default();
        for _ in 0..10 {
            nested.on_new_block(&stats);
        }

        assert!(nested.stats().meta_update_count >= 2);
    }
}
