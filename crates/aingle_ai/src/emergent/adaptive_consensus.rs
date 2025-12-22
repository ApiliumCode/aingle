//! Adaptive Consensus
//!
//! Adjust consensus level based on transaction importance.

use crate::types::{AiTransaction, ConsensusLevel, ValidationPrediction};

/// Adaptive consensus based on transaction importance
pub struct AdaptiveConsensus {
    /// Importance model
    importance_model: ImportanceModel,

    /// Override rules
    override_rules: Vec<OverrideRule>,
}

impl AdaptiveConsensus {
    /// Create new adaptive consensus
    pub fn new() -> Self {
        Self {
            importance_model: ImportanceModel::new(),
            override_rules: vec![
                // Always full consensus for large transactions
                OverrideRule {
                    condition: OverrideCondition::SizeAbove(1024 * 1024), // 1MB
                    level: ConsensusLevel::Full,
                },
                // Local consensus for tiny transactions with high confidence
                OverrideRule {
                    condition: OverrideCondition::SizeBelow(100),
                    level: ConsensusLevel::Local,
                },
            ],
        }
    }

    /// Determine consensus level for a transaction
    pub fn determine_level(
        &self,
        tx: &AiTransaction,
        prediction: &ValidationPrediction,
    ) -> ConsensusLevel {
        // Check override rules first
        for rule in &self.override_rules {
            if rule.matches(tx, prediction) {
                return rule.level;
            }
        }

        // Evaluate importance
        let importance = self.importance_model.evaluate(tx, prediction);

        match importance {
            Importance::Critical => ConsensusLevel::Full, // All validators
            Importance::High => ConsensusLevel::Majority, // 67% validators
            Importance::Normal => ConsensusLevel::Quorum, // 51% validators
            Importance::Low => ConsensusLevel::Local,     // Local validation only
        }
    }

    /// Add an override rule
    pub fn add_override(&mut self, rule: OverrideRule) {
        self.override_rules.push(rule);
    }

    /// Get current override rules
    pub fn get_overrides(&self) -> &[OverrideRule] {
        &self.override_rules
    }
}

impl Default for AdaptiveConsensus {
    fn default() -> Self {
        Self::new()
    }
}

/// Importance model for transactions
struct ImportanceModel {
    /// Weight for size factor
    size_weight: f32,
    /// Weight for confidence factor
    confidence_weight: f32,
    /// Weight for entry type factor
    type_weight: f32,
}

impl ImportanceModel {
    fn new() -> Self {
        Self {
            size_weight: 0.3,
            confidence_weight: 0.4,
            type_weight: 0.3,
        }
    }

    fn evaluate(&self, tx: &AiTransaction, prediction: &ValidationPrediction) -> Importance {
        // Size score (larger = more important)
        let size_score = (tx.size as f32).ln().max(0.0) / 20.0;

        // Confidence score (lower confidence = more important to validate fully)
        let confidence_score = 1.0 - prediction.confidence;

        // Type score based on entry type
        let type_score = match tx.entry_type.as_str() {
            "agent_validation" => 1.0, // Always critical
            "link" => 0.3,             // Usually low importance
            "cap_grant" => 0.8,        // Capability grants are important
            _ => 0.5,                  // Default medium
        };

        // Weighted sum
        let importance_score = size_score * self.size_weight
            + confidence_score * self.confidence_weight
            + type_score * self.type_weight;

        if importance_score > 0.8 {
            Importance::Critical
        } else if importance_score > 0.6 {
            Importance::High
        } else if importance_score > 0.3 {
            Importance::Normal
        } else {
            Importance::Low
        }
    }
}

/// Importance level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Importance {
    /// Critical - requires full consensus
    Critical,
    /// High - requires majority consensus
    High,
    /// Normal - requires quorum
    Normal,
    /// Low - local validation sufficient
    Low,
}

/// Override rule for consensus
#[derive(Debug, Clone)]
pub struct OverrideRule {
    /// Condition for override
    pub condition: OverrideCondition,
    /// Consensus level to use
    pub level: ConsensusLevel,
}

impl OverrideRule {
    fn matches(&self, tx: &AiTransaction, prediction: &ValidationPrediction) -> bool {
        match &self.condition {
            OverrideCondition::SizeAbove(threshold) => tx.size > *threshold,
            OverrideCondition::SizeBelow(threshold) => tx.size < *threshold,
            OverrideCondition::ConfidenceAbove(threshold) => prediction.confidence > *threshold,
            OverrideCondition::ConfidenceBelow(threshold) => prediction.confidence < *threshold,
            OverrideCondition::EntryType(entry_type) => &tx.entry_type == entry_type,
        }
    }
}

/// Condition for override
#[derive(Debug, Clone)]
pub enum OverrideCondition {
    /// Transaction size above threshold
    SizeAbove(usize),
    /// Transaction size below threshold
    SizeBelow(usize),
    /// Prediction confidence above threshold
    ConfidenceAbove(f32),
    /// Prediction confidence below threshold
    ConfidenceBelow(f32),
    /// Specific entry type
    EntryType(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_tx(id: u8, size: usize) -> AiTransaction {
        AiTransaction {
            hash: [id; 32],
            timestamp: 1702656000000,
            agent: [1u8; 32],
            entry_type: "test".to_string(),
            data: vec![0; size],
            size,
        }
    }

    fn make_prediction(confidence: f32) -> ValidationPrediction {
        ValidationPrediction {
            likely_valid: true,
            confidence,
            estimated_time_ms: 50,
        }
    }

    #[test]
    fn test_adaptive_consensus() {
        let consensus = AdaptiveConsensus::new();

        // Small transaction with high confidence = Local
        let small_tx = make_test_tx(1, 50);
        let high_conf = make_prediction(0.95);
        let level = consensus.determine_level(&small_tx, &high_conf);
        assert_eq!(level, ConsensusLevel::Local);

        // Large transaction = Full (override rule)
        let large_tx = make_test_tx(2, 2 * 1024 * 1024);
        let level = consensus.determine_level(&large_tx, &high_conf);
        assert_eq!(level, ConsensusLevel::Full);
    }

    #[test]
    fn test_importance_model() {
        let consensus = AdaptiveConsensus::new();

        // Low confidence = higher importance
        let tx = make_test_tx(1, 500);
        let low_conf = make_prediction(0.3);
        let level = consensus.determine_level(&tx, &low_conf);
        // Should be at least Quorum due to low confidence
        assert!(matches!(
            level,
            ConsensusLevel::Quorum | ConsensusLevel::Majority | ConsensusLevel::Full
        ));
    }

    #[test]
    fn test_override_rules() {
        let mut consensus = AdaptiveConsensus::new();

        // Add custom override with higher priority (insert at beginning)
        consensus.override_rules.insert(
            0,
            OverrideRule {
                condition: OverrideCondition::EntryType("critical_entry".to_string()),
                level: ConsensusLevel::Full,
            },
        );

        // Use size >= 100 to avoid matching the SizeBelow(100) default rule
        let tx = AiTransaction {
            hash: [1u8; 32],
            timestamp: 1702656000000,
            agent: [1u8; 32],
            entry_type: "critical_entry".to_string(),
            data: vec![0; 200],
            size: 200,
        };

        let prediction = make_prediction(0.99);
        let level = consensus.determine_level(&tx, &prediction);
        // The entry type override should match (inserted at position 0)
        assert_eq!(level, ConsensusLevel::Full);
    }
}
