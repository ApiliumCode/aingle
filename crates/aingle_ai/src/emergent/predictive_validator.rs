//! Predictive Validator
//!
//! Predict validation outcome before full validation.

use crate::nested_learning::NestedLearning;
use crate::titans::TitansMemory;
use crate::types::{AiTransaction, ValidationPrediction};

/// Predict validation outcome before full validation
pub struct PredictiveValidator {
    /// Confidence boost for known patterns
    known_pattern_boost: f32,

    /// Minimum confidence threshold
    min_confidence: f32,

    /// History of predictions for accuracy tracking
    prediction_history: Vec<PredictionRecord>,
}

impl PredictiveValidator {
    /// Create new predictive validator
    pub fn new() -> Self {
        Self {
            known_pattern_boost: 0.2,
            min_confidence: 0.5,
            prediction_history: Vec::new(),
        }
    }

    /// Predict validation outcome
    pub fn predict(
        &self,
        tx: &AiTransaction,
        titans: &TitansMemory,
        nested: &NestedLearning,
    ) -> ValidationPrediction {
        // Use Titans memory for pattern matching
        let pattern = tx.to_pattern();
        let similar_patterns = titans.query(&pattern, 100);

        // Use Nested Learning for complexity estimation
        let complexity = {
            let _ol = nested.optimizer_level.read();
            let features = tx.extract_features();
            // Simple complexity estimate
            features.iter().sum::<f32>() / features.len() as f32
        };

        // Compute prediction confidence
        let pattern_count = similar_patterns.len();
        let confidence = if pattern_count > 100 {
            0.95 // High confidence for common patterns
        } else if pattern_count > 10 {
            0.75 + self.known_pattern_boost * (pattern_count as f32 / 100.0)
        } else if pattern_count > 0 {
            0.5 + self.known_pattern_boost * (pattern_count as f32 / 10.0)
        } else {
            self.min_confidence
        };

        // Check if similar patterns were valid
        let all_valid = similar_patterns.iter().all(|_| true); // Assume stored = valid

        // Estimate validation time based on complexity
        let estimated_time = (complexity * 100.0) as u64 + 10; // Base 10ms + complexity

        ValidationPrediction {
            likely_valid: all_valid,
            confidence: confidence.min(1.0),
            estimated_time_ms: estimated_time,
        }
    }

    /// Record actual outcome for accuracy tracking
    pub fn record_outcome(
        &mut self,
        prediction: &ValidationPrediction,
        actual_valid: bool,
        actual_time_ms: u64,
    ) {
        let record = PredictionRecord {
            predicted_valid: prediction.likely_valid,
            actual_valid,
            predicted_time: prediction.estimated_time_ms,
            actual_time: actual_time_ms,
            confidence: prediction.confidence,
        };

        self.prediction_history.push(record);

        // Trim history
        if self.prediction_history.len() > 1000 {
            self.prediction_history.remove(0);
        }
    }

    /// Get prediction accuracy
    pub fn accuracy(&self) -> PredictionAccuracy {
        if self.prediction_history.is_empty() {
            return PredictionAccuracy {
                validity_accuracy: 0.0,
                time_mae: 0.0,
                sample_count: 0,
            };
        }

        let correct_validity = self
            .prediction_history
            .iter()
            .filter(|r| r.predicted_valid == r.actual_valid)
            .count();

        let time_errors: Vec<f64> = self
            .prediction_history
            .iter()
            .map(|r| (r.predicted_time as f64 - r.actual_time as f64).abs())
            .collect();

        let time_mae = time_errors.iter().sum::<f64>() / time_errors.len() as f64;

        PredictionAccuracy {
            validity_accuracy: correct_validity as f32 / self.prediction_history.len() as f32,
            time_mae,
            sample_count: self.prediction_history.len(),
        }
    }
}

impl Default for PredictiveValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Record of a prediction
#[allow(dead_code)]
struct PredictionRecord {
    predicted_valid: bool,
    actual_valid: bool,
    predicted_time: u64,
    actual_time: u64,
    confidence: f32,
}

/// Prediction accuracy statistics
#[derive(Debug, Clone)]
pub struct PredictionAccuracy {
    /// Accuracy of validity predictions
    pub validity_accuracy: f32,
    /// Mean absolute error of time predictions (ms)
    pub time_mae: f64,
    /// Number of samples
    pub sample_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nested_learning::NestedConfig;
    use crate::titans::TitansConfig;

    fn make_test_tx(id: u8) -> AiTransaction {
        AiTransaction {
            hash: [id; 32],
            timestamp: 1702656000000,
            agent: [1u8; 32],
            entry_type: "test".to_string(),
            data: vec![id; 10],
            size: 10,
        }
    }

    #[test]
    fn test_predictive_validator() {
        let validator = PredictiveValidator::new();
        let titans = TitansMemory::new(TitansConfig::default());
        let nested = NestedLearning::new(NestedConfig::default());

        let tx = make_test_tx(1);
        let prediction = validator.predict(&tx, &titans, &nested);

        assert!(prediction.confidence >= 0.0 && prediction.confidence <= 1.0);
        assert!(prediction.estimated_time_ms > 0);
    }

    #[test]
    fn test_accuracy_tracking() {
        let mut validator = PredictiveValidator::new();

        // Record some outcomes
        for i in 0..10 {
            let prediction = ValidationPrediction {
                likely_valid: true,
                confidence: 0.8,
                estimated_time_ms: 50,
            };
            validator.record_outcome(&prediction, i % 2 == 0, 45 + i as u64);
        }

        let accuracy = validator.accuracy();
        assert_eq!(accuracy.sample_count, 10);
        assert!(accuracy.validity_accuracy >= 0.0 && accuracy.validity_accuracy <= 1.0);
    }
}
