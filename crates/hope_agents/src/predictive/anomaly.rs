//! Anomaly detection for HOPE Agents.

use crate::Observation;
use std::collections::VecDeque;

/// Detects anomalies in a stream of observations using a statistical method.
///
/// This detector maintains a sliding window of recent observations, calculates the
/// running mean and variance of features extracted from these observations, and
/// uses a z-score to determine if a new observation is anomalous.
pub struct AnomalyDetector {
    history: VecDeque<Vec<f64>>,
    mean: Vec<f64>,
    variance: Vec<f64>,
    threshold: f64,
    window_size: usize,
}

impl AnomalyDetector {
    /// Creates a new `AnomalyDetector`.
    ///
    /// # Arguments
    ///
    /// * `threshold` - The z-score threshold. An observation with a score above this
    ///   value is considered an anomaly. A common value is 2.0 or 3.0.
    /// * `window_size` - The number of recent observations to include in the sliding
    ///   window for calculating statistics.
    pub fn new(threshold: f64, window_size: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(window_size),
            mean: Vec::new(),
            variance: Vec::new(),
            threshold,
            window_size,
        }
    }

    /// Updates the detector's statistics with a new observation.
    pub fn update(&mut self, obs: &Observation) {
        let features = self.extract_features(obs);

        // Add to history
        if self.history.len() >= self.window_size {
            self.history.pop_front();
        }
        self.history.push_back(features.clone());

        // Recompute statistics if we have enough data
        if self.history.len() >= 10 {
            self.update_statistics();
        }
    }

    /// Returns `true` if the observation is considered anomalous.
    pub fn is_anomaly(&self, obs: &Observation) -> bool {
        if self.mean.is_empty() || self.history.len() < 10 {
            // Not enough data to determine
            return false;
        }

        let score = self.anomaly_score(obs);
        score > self.threshold
    }

    /// Calculates an anomaly score for the observation.
    ///
    /// The score is the average z-score across all extracted features, representing
    /// how many standard deviations the observation is from the mean. A higher score
    /// indicates a greater anomaly.
    pub fn anomaly_score(&self, obs: &Observation) -> f64 {
        if self.mean.is_empty() {
            return 0.0;
        }

        let features = self.extract_features(obs);
        self.compute_zscore(&features)
    }

    /// Computes the z-score for a given feature vector based on the detector's statistics.
    fn compute_zscore(&self, features: &[f64]) -> f64 {
        if self.mean.is_empty() || features.len() != self.mean.len() {
            return 0.0;
        }

        let mut total_zscore = 0.0;
        let mut count = 0;

        for (i, &feature) in features.iter().enumerate().take(self.mean.len()) {
            if self.variance[i] > 1e-10 {
                // Avoid division by zero
                let std_dev = self.variance[i].sqrt();
                let zscore = ((feature - self.mean[i]) / std_dev).abs();
                total_zscore += zscore;
                count += 1;
            }
        }

        if count > 0 {
            total_zscore / count as f64
        } else {
            0.0
        }
    }

    /// A simple feature extractor for an observation.
    fn extract_features(&self, obs: &Observation) -> Vec<f64> {
        let mut features = Vec::new();

        // Extract numeric features from the observation value
        if let Some(f) = obs.value.as_f64() {
            features.push(f);
        } else if let Some(i) = obs.value.as_i64() {
            features.push(i as f64);
        } else if let Some(b) = obs.value.as_bool() {
            features.push(if b { 1.0 } else { 0.0 });
        } else {
            // Default for non-numeric values
            features.push(0.0);
        }

        // Add confidence as feature
        features.push(obs.confidence.value() as f64);

        features
    }

    /// Updates the running mean and variance from the observation history.
    fn update_statistics(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let n = self.history.len();
        let feature_dim = self.history[0].len();

        // Initialize mean and variance vectors
        self.mean = vec![0.0; feature_dim];
        self.variance = vec![0.0; feature_dim];

        // Compute mean
        for features in &self.history {
            for (i, &value) in features.iter().enumerate() {
                if i < feature_dim {
                    self.mean[i] += value;
                }
            }
        }

        for mean_val in &mut self.mean {
            *mean_val /= n as f64;
        }

        // Compute variance
        for features in &self.history {
            for (i, &value) in features.iter().enumerate() {
                if i < feature_dim {
                    let diff = value - self.mean[i];
                    self.variance[i] += diff * diff;
                }
            }
        }

        for var_val in &mut self.variance {
            *var_val /= n as f64;
            // Add small epsilon to prevent zero variance
            *var_val = var_val.max(1e-10);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anomaly_detector_creation() {
        let detector = AnomalyDetector::new(2.0, 100);
        assert_eq!(detector.threshold, 2.0);
        assert_eq!(detector.window_size, 100);
    }

    #[test]
    fn test_anomaly_detection_insufficient_data() {
        let mut detector = AnomalyDetector::new(2.0, 100);

        // Add a few observations
        for i in 0..5 {
            detector.update(&Observation::sensor("temp", 20.0 + i as f64));
        }

        // Should not detect anomalies with insufficient data
        let obs = Observation::sensor("temp", 100.0);
        assert!(!detector.is_anomaly(&obs));
    }

    #[test]
    fn test_anomaly_detection() {
        let mut detector = AnomalyDetector::new(2.0, 100);

        // Train with normal values around 20-25
        for i in 0..100 {
            let value = 20.0 + (i % 5) as f64;
            detector.update(&Observation::sensor("temp", value));
        }

        // Test normal observation
        let normal = Observation::sensor("temp", 22.0);
        assert!(!detector.is_anomaly(&normal));

        // Test anomalous observation (far outside normal range)
        let anomaly = Observation::sensor("temp", 100.0);
        assert!(detector.is_anomaly(&anomaly));
    }

    #[test]
    fn test_anomaly_score() {
        let mut detector = AnomalyDetector::new(2.0, 100);

        // Train with values around 20 with some variance
        for i in 0..50 {
            detector.update(&Observation::sensor("temp", 20.0 + (i % 3) as f64));
        }

        let normal = Observation::sensor("temp", 20.0);
        let slightly_off = Observation::sensor("temp", 25.0);
        let very_off = Observation::sensor("temp", 100.0);

        let score_normal = detector.anomaly_score(&normal);
        let score_slightly = detector.anomaly_score(&slightly_off);
        let score_very = detector.anomaly_score(&very_off);

        // Scores should increase with deviation
        assert!(score_normal < score_slightly);
        assert!(score_slightly < score_very);
    }

    #[test]
    fn test_window_size_limit() {
        let mut detector = AnomalyDetector::new(2.0, 5);

        // Add more observations than window size
        for i in 0..10 {
            detector.update(&Observation::sensor("temp", i as f64));
        }

        assert_eq!(detector.history.len(), 5);
    }

    #[test]
    fn test_statistics_update() {
        let mut detector = AnomalyDetector::new(2.0, 100);

        // Add known values
        for _ in 0..20 {
            detector.update(&Observation::sensor("temp", 10.0));
        }

        // Mean should be close to 10.0
        assert!(!detector.mean.is_empty());
        assert!((detector.mean[0] - 10.0).abs() < 0.1);

        // Variance should be very small (all same values)
        assert!(detector.variance[0] < 0.1);
    }

    #[test]
    fn test_different_value_types() {
        let mut detector = AnomalyDetector::new(2.0, 100);

        // Test with integers
        for i in 0..20 {
            detector.update(&Observation::sensor("count", i));
        }

        // Test with booleans
        detector.update(&Observation::sensor("flag", true));
        detector.update(&Observation::sensor("flag", false));

        assert!(detector.history.len() > 0);
    }

    #[test]
    fn test_zscore_calculation() {
        let mut detector = AnomalyDetector::new(2.0, 100);

        // Create data with mean=50, std=10
        for i in 0..100 {
            let value = 50.0 + ((i % 20) as f64 - 10.0);
            detector.update(&Observation::sensor("value", value));
        }

        // Value at mean should have low z-score
        let at_mean = Observation::sensor("value", 50.0);
        let score_mean = detector.anomaly_score(&at_mean);
        assert!(score_mean < 1.0);

        // Value 3 standard deviations away should have high z-score
        let far_away = Observation::sensor("value", 80.0);
        let score_far = detector.anomaly_score(&far_away);
        assert!(score_far > 2.0);
    }
}
