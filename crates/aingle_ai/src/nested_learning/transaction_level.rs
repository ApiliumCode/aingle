//! Transaction-Level processing
//!
//! Fast feature extraction and classification per transaction.

use super::{NestedConfig, ValidationOutcome};
use crate::types::AiTransaction;
use std::collections::{HashMap, VecDeque};

/// Transaction-level processor for fast data processing
pub struct TransactionLevel {
    /// Feature extractor
    feature_extractor: FeatureExtractor,

    /// Transaction classifier
    classifier: TransactionClassifier,

    /// Recent transaction buffer
    buffer: VecDeque<ProcessedTransaction>,

    /// Buffer size limit
    buffer_size: usize,
}

impl TransactionLevel {
    /// Create new transaction-level processor
    pub fn new(config: &NestedConfig) -> Self {
        Self {
            feature_extractor: FeatureExtractor::new(config.feature_dim),
            classifier: TransactionClassifier::new(),
            buffer: VecDeque::with_capacity(100),
            buffer_size: 100,
        }
    }

    /// Process a transaction
    pub fn process(&mut self, tx: AiTransaction) -> ProcessedTransaction {
        // Extract features
        let features = self.feature_extractor.extract(&tx);

        // Classify transaction type
        let tx_type = self.classifier.classify(&features);

        // Compute confidence based on classifier certainty
        let confidence = self.classifier.confidence(&features);

        let processed = ProcessedTransaction {
            hash: tx.hash,
            features,
            tx_type,
            confidence,
        };

        // Add to buffer
        self.buffer.push_back(processed.clone());
        if self.buffer.len() > self.buffer_size {
            self.buffer.pop_front();
        }

        processed
    }

    /// Update feature extractor based on validation outcome
    pub fn update_features(&mut self, tx: &AiTransaction, outcome: &ValidationOutcome) {
        let features = self.feature_extractor.extract(tx);
        self.classifier.update(&features, outcome.valid);
    }

    /// Get recent transaction patterns
    pub fn get_recent_patterns(&self) -> Vec<&ProcessedTransaction> {
        self.buffer.iter().collect()
    }

    /// Get buffer statistics
    pub fn buffer_stats(&self) -> BufferStats {
        let type_counts: HashMap<String, usize> =
            self.buffer.iter().fold(HashMap::new(), |mut acc, tx| {
                *acc.entry(tx.tx_type.clone()).or_insert(0) += 1;
                acc
            });

        let avg_confidence = if self.buffer.is_empty() {
            0.0
        } else {
            self.buffer.iter().map(|tx| tx.confidence).sum::<f32>() / self.buffer.len() as f32
        };

        BufferStats {
            size: self.buffer.len(),
            type_distribution: type_counts,
            avg_confidence,
        }
    }
}

/// Processed transaction with extracted features
#[derive(Debug, Clone)]
pub struct ProcessedTransaction {
    /// Original transaction hash
    pub hash: [u8; 32],
    /// Extracted features
    pub features: Vec<f32>,
    /// Classified transaction type
    pub tx_type: String,
    /// Classification confidence
    pub confidence: f32,
}

/// Feature extractor
struct FeatureExtractor {
    /// Feature dimension
    dim: usize,
}

impl FeatureExtractor {
    fn new(dim: usize) -> Self {
        Self { dim }
    }

    fn extract(&self, tx: &AiTransaction) -> Vec<f32> {
        let mut features = tx.extract_features();

        // Ensure correct dimension
        features.resize(self.dim, 0.0);

        // Normalize features
        let max = features.iter().cloned().fold(0.0_f32, f32::max);
        if max > 0.0 {
            for f in features.iter_mut() {
                *f /= max;
            }
        }

        features
    }
}

/// Transaction classifier
struct TransactionClassifier {
    /// Type centroids
    centroids: HashMap<String, Vec<f32>>,
    /// Type counts for weighting
    type_counts: HashMap<String, usize>,
    /// Learning rate
    learning_rate: f32,
}

impl TransactionClassifier {
    fn new() -> Self {
        let mut centroids = HashMap::new();
        let mut type_counts = HashMap::new();

        // Initialize with common transaction types
        let types = ["data", "link", "agent", "cap", "init"];
        for t in types {
            centroids.insert(t.to_string(), vec![0.0; 16]);
            type_counts.insert(t.to_string(), 1);
        }

        Self {
            centroids,
            type_counts,
            learning_rate: 0.1,
        }
    }

    fn classify(&self, features: &[f32]) -> String {
        // Find closest centroid
        self.centroids
            .iter()
            .map(|(name, centroid)| {
                let distance = Self::l2_distance(features, centroid);
                (name.clone(), distance)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(name, _)| name)
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn confidence(&self, features: &[f32]) -> f32 {
        // Compute distances to all centroids
        let distances: Vec<_> = self
            .centroids
            .values()
            .map(|c| Self::l2_distance(features, c))
            .collect();

        if distances.is_empty() {
            return 0.5;
        }

        // Find min and second min distance
        let mut sorted = distances.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let min_dist = sorted.first().copied().unwrap_or(1.0);
        let second_dist = sorted.get(1).copied().unwrap_or(min_dist);

        // Confidence based on separation
        if second_dist > 0.0 {
            (1.0 - min_dist / second_dist).max(0.0).min(1.0)
        } else {
            1.0
        }
    }

    fn update(&mut self, features: &[f32], was_valid: bool) {
        if !was_valid {
            return; // Only learn from valid transactions
        }

        let tx_type = self.classify(features);

        // Update centroid with exponential moving average
        if let Some(centroid) = self.centroids.get_mut(&tx_type) {
            for (i, &f) in features.iter().enumerate() {
                if i < centroid.len() {
                    centroid[i] = centroid[i] * (1.0 - self.learning_rate) + f * self.learning_rate;
                }
            }
        }

        // Update count
        *self.type_counts.entry(tx_type).or_insert(0) += 1;
    }

    fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

/// Buffer statistics
#[derive(Debug, Clone)]
pub struct BufferStats {
    /// Current buffer size
    pub size: usize,
    /// Distribution of transaction types
    pub type_distribution: HashMap<String, usize>,
    /// Average confidence
    pub avg_confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tx(id: u8) -> AiTransaction {
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
    fn test_transaction_processing() {
        let config = NestedConfig::default();
        let mut tl = TransactionLevel::new(&config);

        let tx = make_tx(1);
        let processed = tl.process(tx);

        assert_eq!(processed.features.len(), config.feature_dim);
        assert!(!processed.tx_type.is_empty());
        assert!(processed.confidence >= 0.0 && processed.confidence <= 1.0);
    }

    #[test]
    fn test_buffer() {
        let config = NestedConfig::default();
        let mut tl = TransactionLevel::new(&config);

        for i in 0..10 {
            tl.process(make_tx(i));
        }

        let stats = tl.buffer_stats();
        assert_eq!(stats.size, 10);
    }

    #[test]
    fn test_classifier_update() {
        let config = NestedConfig::default();
        let mut tl = TransactionLevel::new(&config);

        // Process and update
        let tx = make_tx(1);
        let processed = tl.process(tx.clone());

        let outcome = ValidationOutcome {
            valid: true,
            time_ms: 10,
            error: None,
        };
        tl.update_features(&tx, &outcome);

        // Subsequent processing should have updated classifier
        let processed2 = tl.process(tx);
        assert!(processed2.confidence > 0.0);
    }
}
