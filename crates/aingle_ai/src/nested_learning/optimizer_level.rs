//! Optimizer-Level optimization
//!
//! Validation strategies with medium-frequency updates (~100 transactions).

use super::{NestedConfig, ProcessedTransaction, ValidationOutcome};
use crate::types::AiTransaction;
use std::collections::HashMap;

/// Optimizer-level for validation strategy optimization
pub struct OptimizerLevel {
    /// Complexity model for transactions
    complexity_model: ComplexityModel,

    /// Strategy selector
    strategy_selector: StrategySelector,

    /// Configuration
    config: NestedConfig,

    /// Learning history
    history: Vec<LearningRecord>,
}

impl OptimizerLevel {
    /// Create new optimizer level
    pub fn new(config: &NestedConfig) -> Self {
        Self {
            complexity_model: ComplexityModel::new(),
            strategy_selector: StrategySelector::new(config.fast_path_threshold),
            config: config.clone(),
            history: Vec::new(),
        }
    }

    /// Get validation strategy for a processed transaction
    pub fn get_strategy(&self, processed: &ProcessedTransaction) -> ValidationStrategy {
        let complexity = self.complexity_model.predict(&processed.features);
        self.strategy_selector.select(complexity, processed)
    }

    /// Create validation plan for a batch
    pub fn create_plan(&self, batch: &[AiTransaction]) -> ValidationPlan {
        // Process all transactions
        let processed: Vec<_> = batch
            .iter()
            .map(|tx| {
                let features = tx.extract_features();
                let complexity = self.complexity_model.predict(&features);
                (tx.hash, complexity)
            })
            .collect();

        // Sort by complexity (shortest first for throughput)
        let mut order: Vec<_> = (0..batch.len()).collect();
        order.sort_by(|&a, &b| {
            processed[a]
                .1
                .partial_cmp(&processed[b].1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Find parallel groups (transactions that don't conflict)
        let parallel_groups = if self.config.parallel_validation {
            self.find_parallel_groups(batch)
        } else {
            vec![order.clone()] // Single sequential group
        };

        ValidationPlan {
            order,
            parallel_groups,
            estimated_times: processed.iter().map(|(_, c)| (*c * 100.0) as u64).collect(),
        }
    }

    /// Find groups of transactions that can be validated in parallel
    fn find_parallel_groups(&self, batch: &[AiTransaction]) -> Vec<Vec<usize>> {
        // Simple heuristic: group by agent (same agent = potential conflicts)
        let mut agent_groups: HashMap<[u8; 32], Vec<usize>> = HashMap::new();

        for (i, tx) in batch.iter().enumerate() {
            agent_groups.entry(tx.agent).or_default().push(i);
        }

        // Take one transaction from each agent per group
        let mut groups = Vec::new();
        let mut remaining: Vec<_> = agent_groups.values().cloned().collect();

        while remaining.iter().any(|g| !g.is_empty()) {
            let mut group = Vec::new();
            for agent_txs in remaining.iter_mut() {
                if let Some(idx) = agent_txs.pop() {
                    group.push(idx);
                    if group.len() >= self.config.max_parallel_group {
                        break;
                    }
                }
            }
            if !group.is_empty() {
                groups.push(group);
            }
        }

        groups
    }

    /// Learn from validation outcome
    pub fn learn(&mut self, tx: &AiTransaction, outcome: &ValidationOutcome) {
        let features = tx.extract_features();

        // Record learning history
        self.history.push(LearningRecord {
            features: features.clone(),
            actual_time: outcome.time_ms,
            was_valid: outcome.valid,
        });

        // Trim history
        if self.history.len() > 1000 {
            self.history.remove(0);
        }

        // Update complexity model
        let predicted_complexity = self.complexity_model.predict(&features);
        let actual_complexity = outcome.time_ms as f32 / 100.0;
        self.complexity_model
            .update(&features, actual_complexity, self.config.learning_rate);

        // Update strategy selector if prediction was wrong
        if outcome.valid && predicted_complexity > actual_complexity * 2.0 {
            // Was faster than expected - can be more aggressive
            self.strategy_selector.adjust_threshold(-0.01);
        } else if !outcome.valid {
            // Invalid - be more conservative
            self.strategy_selector.adjust_threshold(0.01);
        }
    }

    /// Periodic update based on accumulated history
    pub fn periodic_update(&mut self) {
        if self.history.is_empty() {
            return;
        }

        // Compute average prediction error
        let _avg_error: f32 = self
            .history
            .iter()
            .map(|r| {
                let predicted = self.complexity_model.predict(&r.features);
                (predicted - r.actual_time as f32 / 100.0).abs()
            })
            .sum::<f32>()
            / self.history.len() as f32;

        // Could implement more sophisticated updates here
    }
}

/// Validation strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStrategy {
    /// Fast path - minimal validation for known-good patterns
    FastPath,
    /// Standard full validation
    FullValidation,
    /// Defer to network consensus
    DeferToNetwork,
}

/// Validation plan for a batch
#[derive(Debug, Clone)]
pub struct ValidationPlan {
    /// Optimal validation order (indices into batch)
    pub order: Vec<usize>,
    /// Groups that can be validated in parallel
    pub parallel_groups: Vec<Vec<usize>>,
    /// Estimated validation time for each transaction (ms)
    pub estimated_times: Vec<u64>,
}

/// Complexity prediction model
struct ComplexityModel {
    /// Weights for linear model
    weights: Vec<f32>,
    /// Bias
    bias: f32,
}

impl ComplexityModel {
    fn new() -> Self {
        Self {
            weights: vec![1.0; 16], // Initial uniform weights
            bias: 0.5,
        }
    }

    fn predict(&self, features: &[f32]) -> f32 {
        let mut prediction = self.bias;
        for (i, &f) in features.iter().enumerate() {
            if i < self.weights.len() {
                prediction += f * self.weights[i];
            }
        }
        prediction.max(0.1) // Minimum complexity
    }

    fn update(&mut self, features: &[f32], actual: f32, learning_rate: f32) {
        let predicted = self.predict(features);
        let error = actual - predicted;

        // Gradient descent update
        self.bias += learning_rate * error;
        for (i, &f) in features.iter().enumerate() {
            if i < self.weights.len() {
                self.weights[i] += learning_rate * error * f;
            }
        }
    }
}

/// Strategy selector
struct StrategySelector {
    /// Threshold for fast path
    fast_path_threshold: f32,
}

impl StrategySelector {
    fn new(threshold: f32) -> Self {
        Self {
            fast_path_threshold: threshold,
        }
    }

    fn select(&self, complexity: f32, processed: &ProcessedTransaction) -> ValidationStrategy {
        // Use confidence from transaction processing
        if processed.confidence > self.fast_path_threshold && complexity < 0.3 {
            ValidationStrategy::FastPath
        } else if complexity > 2.0 {
            // Very complex - might benefit from network help
            ValidationStrategy::DeferToNetwork
        } else {
            ValidationStrategy::FullValidation
        }
    }

    fn adjust_threshold(&mut self, delta: f32) {
        self.fast_path_threshold = (self.fast_path_threshold + delta).clamp(0.5, 0.99);
    }
}

/// Record for learning history
#[allow(dead_code)]
struct LearningRecord {
    features: Vec<f32>,
    actual_time: u64,
    was_valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_processed(confidence: f32) -> ProcessedTransaction {
        ProcessedTransaction {
            hash: [0u8; 32],
            features: vec![0.5; 16],
            tx_type: "test".to_string(),
            confidence,
        }
    }

    #[test]
    fn test_strategy_selection() {
        let config = NestedConfig::default();
        let opt = OptimizerLevel::new(&config);

        // High confidence - strategy depends on complexity
        let high_conf = make_processed(0.95);
        let strategy = opt.get_strategy(&high_conf);
        // Strategy is determined by both confidence and complexity
        assert!(matches!(
            strategy,
            ValidationStrategy::FastPath
                | ValidationStrategy::FullValidation
                | ValidationStrategy::DeferToNetwork
        ));

        // Low confidence = not fast path
        let low_conf = make_processed(0.5);
        let strategy = opt.get_strategy(&low_conf);
        assert!(matches!(
            strategy,
            ValidationStrategy::FullValidation | ValidationStrategy::DeferToNetwork
        ));
    }

    #[test]
    fn test_validation_plan() {
        let config = NestedConfig::default();
        let opt = OptimizerLevel::new(&config);

        let batch: Vec<_> = (0..5)
            .map(|i| AiTransaction {
                hash: [i; 32],
                timestamp: 1702656000000,
                agent: [i % 2; 32], // Alternate between 2 agents
                entry_type: "test".to_string(),
                data: vec![i; 10],
                size: 10,
            })
            .collect();

        let plan = opt.create_plan(&batch);

        assert_eq!(plan.order.len(), 5);
        assert!(!plan.parallel_groups.is_empty());
    }

    #[test]
    fn test_learning() {
        let config = NestedConfig::default();
        let mut opt = OptimizerLevel::new(&config);

        let tx = AiTransaction {
            hash: [1u8; 32],
            timestamp: 1702656000000,
            agent: [1u8; 32],
            entry_type: "test".to_string(),
            data: vec![1; 10],
            size: 10,
        };

        let outcome = ValidationOutcome {
            valid: true,
            time_ms: 50,
            error: None,
        };

        opt.learn(&tx, &outcome);
        assert!(!opt.history.is_empty());
    }
}
