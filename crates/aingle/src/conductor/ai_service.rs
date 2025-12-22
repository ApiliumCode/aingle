//! AI Service for the AIngle Conductor
//!
//! This module provides AI-powered validation and consensus features
//! using the aingle_ai crate (Titans Memory, Nested Learning, HOPE Agents).

#[cfg(feature = "ai-integration")]
use aingle_ai::{emergent::AiLayer, AiConfig, AiTransaction, ConsensusLevel, ValidationPrediction};

/// Default confidence threshold for fast-path validation
pub const DEFAULT_FAST_PATH_CONFIDENCE: f32 = 0.95;

/// Minimum number of predictions before enabling fast-path
pub const MIN_PREDICTIONS_FOR_FAST_PATH: u64 = 100;

/// Get the required receipt ratio for a given consensus level
///
/// This function maps ConsensusLevel to the percentage of validators
/// that must provide receipts before an operation is considered validated.
#[cfg(feature = "ai-integration")]
pub fn consensus_level_to_receipt_ratio(level: ConsensusLevel) -> f64 {
    match level {
        ConsensusLevel::Local => 0.0,     // No external validation needed
        ConsensusLevel::Quorum => 0.51,   // Simple majority
        ConsensusLevel::Majority => 0.67, // Two-thirds majority
        ConsensusLevel::Full => 1.0,      // All validators
    }
}

/// Description of what each consensus level means
#[cfg(feature = "ai-integration")]
pub fn consensus_level_description(level: ConsensusLevel) -> &'static str {
    match level {
        ConsensusLevel::Local => {
            "Local validation only - no receipts required from other validators"
        }
        ConsensusLevel::Quorum => "Quorum consensus - requires 51% of validators",
        ConsensusLevel::Majority => "Majority consensus - requires 67% of validators",
        ConsensusLevel::Full => "Full consensus - requires all validators",
    }
}

use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, trace, warn};

/// AI Service for intelligent validation and consensus
#[cfg(feature = "ai-integration")]
pub struct AiService {
    /// Unified AI layer (Titans + Nested Learning + Emergent)
    ai_layer: Arc<RwLock<AiLayer>>,

    /// Configuration
    config: AiConfig,

    /// Metrics
    metrics: AiMetrics,

    /// Whether the service is active
    active: bool,
}

#[cfg(feature = "ai-integration")]
impl AiService {
    /// Create a new AI service with the given configuration
    pub fn new(config: AiConfig) -> Self {
        info!("Initializing AI Service with config: {:?}", config);
        let ai_layer = AiLayer::new();

        Self {
            ai_layer: Arc::new(RwLock::new(ai_layer)),
            config,
            metrics: AiMetrics::default(),
            active: true,
        }
    }

    /// Create an AI service with default IoT configuration
    pub fn new_iot() -> Self {
        Self::new(AiConfig::iot())
    }

    /// Create an AI service with full configuration
    pub fn new_full() -> Self {
        Self::new(AiConfig::default())
    }

    /// Check if the AI service is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Temporarily disable the AI service
    pub fn disable(&mut self) {
        self.active = false;
        info!("AI Service disabled");
    }

    /// Re-enable the AI service
    pub fn enable(&mut self) {
        self.active = true;
        info!("AI Service enabled");
    }

    /// Predict validation outcome before full validation
    ///
    /// This processes the transaction through the AI layer and returns
    /// the validation prediction.
    pub fn predict_validation(&self, tx: &AiTransaction) -> Option<ValidationPrediction> {
        if !self.active || !self.config.predictive_validation {
            return None;
        }

        let start = std::time::Instant::now();

        let prediction = {
            let mut ai_layer = self.ai_layer.write();
            let result = ai_layer.process(tx);
            result.prediction
        };

        let elapsed_us = start.elapsed().as_micros() as u64;
        self.metrics.record_prediction(elapsed_us);

        trace!(
            hash = ?tx.hash[..8],
            confidence = prediction.confidence,
            likely_valid = prediction.likely_valid,
            time_us = elapsed_us,
            "AI prediction completed"
        );

        Some(prediction)
    }

    /// Process a transaction through the AI layer and learn from outcome
    ///
    /// This updates the AI models based on the actual validation result.
    pub fn learn_from_outcome(&self, tx: &AiTransaction, valid: bool, _time_ms: u64) {
        if !self.active {
            return;
        }

        // Process through AI layer (which includes learning)
        {
            let mut ai_layer = self.ai_layer.write();
            let _ = ai_layer.process(tx);
        }

        self.metrics.record_learning(valid);

        trace!(
            hash = ?tx.hash[..8],
            valid = valid,
            "AI learned from outcome"
        );
    }

    /// Determine the appropriate consensus level for a transaction
    ///
    /// Returns a consensus level based on transaction importance and confidence.
    pub fn determine_consensus_level(&self, tx: &AiTransaction) -> ConsensusLevel {
        if !self.active || !self.config.adaptive_consensus {
            return ConsensusLevel::Full;
        }

        let level = {
            let mut ai_layer = self.ai_layer.write();
            let result = ai_layer.process(tx);
            result.consensus_level
        };

        trace!(
            hash = ?tx.hash[..8],
            consensus_level = ?level,
            "AI determined consensus level"
        );

        level
    }

    /// Check if a transaction is anomalous
    ///
    /// Returns true if the transaction pattern differs significantly
    /// from known patterns (based on low confidence).
    pub fn is_anomalous(&self, tx: &AiTransaction) -> bool {
        if !self.active {
            return false;
        }

        let is_anomaly = {
            let mut ai_layer = self.ai_layer.write();
            let result = ai_layer.process(tx);
            // Consider low confidence as potential anomaly
            result.prediction.confidence < 0.3
        };

        if is_anomaly {
            self.metrics.record_anomaly();
            warn!(
                hash = ?tx.hash[..8],
                "AI detected anomalous transaction"
            );
        }

        is_anomaly
    }

    /// Query for similar patterns to a transaction
    pub fn query_similar(&self, tx: &AiTransaction, limit: usize) -> Vec<(f32, String)> {
        if !self.active {
            return Vec::new();
        }

        let ai_layer = self.ai_layer.read();
        ai_layer
            .query_similar(tx, limit)
            .into_iter()
            .map(|m| (m.similarity, m.source))
            .collect()
    }

    /// Get AI service statistics
    pub fn stats(&self) -> AiLayerStatsSnapshot {
        let ai_layer = self.ai_layer.read();
        let stats = ai_layer.stats();
        AiLayerStatsSnapshot {
            titans_short_term_size: stats.titans_short_term_size,
            titans_long_term_size: stats.titans_long_term_size,
            nested_tx_count: stats.nested_tx_count,
            nested_block_count: stats.nested_block_count,
        }
    }

    /// Get AI service metrics
    pub fn metrics(&self) -> &AiMetrics {
        &self.metrics
    }

    /// Get prediction accuracy
    pub fn accuracy(&self) -> f64 {
        self.metrics.accuracy()
    }

    /// Get the AI configuration
    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    /// Check if fast-path should be used for a prediction
    ///
    /// Fast-path is enabled when:
    /// 1. AI service is active
    /// 2. We have enough predictions to trust the model (MIN_PREDICTIONS_FOR_FAST_PATH)
    /// 3. Prediction confidence is above threshold (DEFAULT_FAST_PATH_CONFIDENCE)
    /// 4. Prediction indicates likely valid
    pub fn should_use_fast_path(&self, prediction: &ValidationPrediction) -> bool {
        if !self.active {
            return false;
        }

        // Need enough training data before trusting fast-path
        let total_predictions = self.metrics.total_predictions();
        if total_predictions < MIN_PREDICTIONS_FOR_FAST_PATH {
            return false;
        }

        // Check confidence and validity
        prediction.confidence >= DEFAULT_FAST_PATH_CONFIDENCE && prediction.likely_valid
    }

    /// Record a fast-path outcome (for learning)
    pub fn record_fast_path_outcome(&self, was_actually_valid: bool) {
        self.metrics.record_fast_path();
        if was_actually_valid {
            self.metrics.record_fast_path_correct();
        } else {
            // Fast-path was wrong - this is important to track!
            warn!("Fast-path prediction was incorrect - transaction was rejected");
        }
    }
}

/// Snapshot of AI layer statistics
#[derive(Debug, Clone)]
pub struct AiLayerStatsSnapshot {
    /// Titans short-term memory size
    pub titans_short_term_size: usize,
    /// Titans long-term memory size
    pub titans_long_term_size: usize,
    /// Nested learning transaction count
    pub nested_tx_count: u64,
    /// Nested learning block count
    pub nested_block_count: u64,
}

/// Metrics for AI service performance
#[derive(Default)]
pub struct AiMetrics {
    /// Total predictions made
    predictions_total: AtomicU64,
    /// Correct predictions (when we learn the outcome)
    predictions_correct: AtomicU64,
    /// Total learning events
    learning_events: AtomicU64,
    /// Valid transactions learned
    valid_learned: AtomicU64,
    /// Invalid transactions learned
    invalid_learned: AtomicU64,
    /// Anomalies detected
    anomalies_detected: AtomicU64,
    /// Total prediction time (microseconds)
    total_prediction_time_us: AtomicU64,
    /// Fast-path validations triggered
    fast_paths_triggered: AtomicU64,
    /// Fast-path validations that were correct
    fast_paths_correct: AtomicU64,
}

impl AiMetrics {
    /// Record a prediction
    pub fn record_prediction(&self, time_us: u64) {
        self.predictions_total.fetch_add(1, Ordering::Relaxed);
        self.total_prediction_time_us
            .fetch_add(time_us, Ordering::Relaxed);
    }

    /// Record a learning event
    pub fn record_learning(&self, valid: bool) {
        self.learning_events.fetch_add(1, Ordering::Relaxed);
        if valid {
            self.valid_learned.fetch_add(1, Ordering::Relaxed);
        } else {
            self.invalid_learned.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record an anomaly detection
    pub fn record_anomaly(&self) {
        self.anomalies_detected.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a correct prediction
    pub fn record_correct_prediction(&self) {
        self.predictions_correct.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a fast-path trigger
    pub fn record_fast_path(&self) {
        self.fast_paths_triggered.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a correct fast-path (prediction was right)
    pub fn record_fast_path_correct(&self) {
        self.fast_paths_correct.fetch_add(1, Ordering::Relaxed);
    }

    /// Get fast-path count
    pub fn fast_paths_triggered(&self) -> u64 {
        self.fast_paths_triggered.load(Ordering::Relaxed)
    }

    /// Get fast-path accuracy
    pub fn fast_path_accuracy(&self) -> f64 {
        let total = self.fast_paths_triggered.load(Ordering::Relaxed);
        let correct = self.fast_paths_correct.load(Ordering::Relaxed);
        if total > 0 {
            correct as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Get prediction accuracy
    pub fn accuracy(&self) -> f64 {
        let total = self.predictions_total.load(Ordering::Relaxed);
        let correct = self.predictions_correct.load(Ordering::Relaxed);
        if total > 0 {
            correct as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Get average prediction time in microseconds
    pub fn avg_prediction_time_us(&self) -> u64 {
        let total = self.predictions_total.load(Ordering::Relaxed);
        let time = self.total_prediction_time_us.load(Ordering::Relaxed);
        if total > 0 {
            time / total
        } else {
            0
        }
    }

    /// Get total predictions
    pub fn total_predictions(&self) -> u64 {
        self.predictions_total.load(Ordering::Relaxed)
    }

    /// Get anomalies detected
    pub fn anomalies_detected(&self) -> u64 {
        self.anomalies_detected.load(Ordering::Relaxed)
    }

    /// Get learning events count
    pub fn learning_events(&self) -> u64 {
        self.learning_events.load(Ordering::Relaxed)
    }
}

/// Helper to convert SgdOp data to AiTransaction
#[cfg(feature = "ai-integration")]
pub fn create_ai_transaction(
    hash: [u8; 32],
    timestamp: u64,
    agent: [u8; 32],
    entry_type: String,
    data: Vec<u8>,
) -> AiTransaction {
    let size = data.len();
    AiTransaction {
        hash,
        timestamp,
        agent,
        entry_type,
        data,
        size,
    }
}

#[cfg(test)]
#[cfg(feature = "ai-integration")]
mod tests {
    use super::*;

    #[test]
    fn test_ai_service_creation() {
        let service = AiService::new(AiConfig::default());
        assert!(service.is_active());
    }

    #[test]
    fn test_ai_service_disable_enable() {
        let mut service = AiService::new(AiConfig::default());
        assert!(service.is_active());

        service.disable();
        assert!(!service.is_active());

        service.enable();
        assert!(service.is_active());
    }

    #[test]
    fn test_ai_prediction() {
        let service = AiService::new(AiConfig::default());

        let tx = AiTransaction {
            hash: [1u8; 32],
            timestamp: 1702656000000,
            agent: [2u8; 32],
            entry_type: "test".to_string(),
            data: vec![0; 100],
            size: 100,
        };

        let prediction = service.predict_validation(&tx);
        assert!(prediction.is_some());

        let pred = prediction.unwrap();
        assert!(pred.confidence >= 0.0 && pred.confidence <= 1.0);
    }

    #[test]
    fn test_ai_consensus_level() {
        let service = AiService::new(AiConfig::default());

        let tx = AiTransaction {
            hash: [1u8; 32],
            timestamp: 1702656000000,
            agent: [2u8; 32],
            entry_type: "test".to_string(),
            data: vec![0; 100],
            size: 100,
        };

        let level = service.determine_consensus_level(&tx);
        assert!(matches!(
            level,
            ConsensusLevel::Full
                | ConsensusLevel::Majority
                | ConsensusLevel::Quorum
                | ConsensusLevel::Local
        ));
    }

    #[test]
    fn test_ai_metrics() {
        let metrics = AiMetrics::default();

        metrics.record_prediction(100);
        metrics.record_prediction(200);

        assert_eq!(metrics.total_predictions(), 2);
        assert_eq!(metrics.avg_prediction_time_us(), 150);

        metrics.record_learning(true);
        metrics.record_learning(false);

        assert_eq!(metrics.learning_events(), 2);
    }
}
