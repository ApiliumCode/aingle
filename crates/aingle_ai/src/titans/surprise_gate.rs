//! Surprise Gate implementation
//!
//! Controls when to update long-term memory based on "surprise" metric.
//! Inspired by the Titans paper's surprise-gated memory updates.

use crate::titans::LongTermMemory;
use crate::types::Pattern;
use std::collections::VecDeque;

/// Surprise-based gating for memory updates
pub struct SurpriseGate {
    /// Threshold for triggering memory update
    threshold: f32,

    /// Recent surprise values for calibration
    recent_surprises: VecDeque<f32>,

    /// Window size for calibration
    calibration_window: usize,

    /// Running mean of surprises
    mean_surprise: f32,

    /// Running variance of surprises
    var_surprise: f32,

    /// Number of observations
    observation_count: usize,
}

impl SurpriseGate {
    /// Create a new surprise gate
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            recent_surprises: VecDeque::with_capacity(100),
            calibration_window: 100,
            mean_surprise: 0.5,
            var_surprise: 0.1,
            observation_count: 0,
        }
    }

    /// Compute surprise value for a pattern
    ///
    /// Surprise = 1 - max_similarity to known patterns
    /// High surprise means the pattern is novel
    pub fn compute_surprise(&self, pattern: &Pattern, ltm: &LongTermMemory) -> f32 {
        // If memory is empty, everything is surprising
        if ltm.is_empty() {
            return 1.0;
        }

        // Get prediction from long-term memory
        let prediction = ltm.predict(&pattern.embedding);

        // Compute similarity to most similar pattern
        let max_similarity = ltm.max_similarity(&pattern.embedding);

        // Surprise is inverse of both prediction accuracy and similarity
        let raw_surprise = (1.0 - prediction) * 0.5 + (1.0 - max_similarity) * 0.5;

        // Normalize by running statistics (adaptive threshold)
        self.normalize_surprise(raw_surprise)
    }

    /// Observe a pattern (update statistics)
    pub fn observe(&mut self, _pattern: &Pattern) {
        // This is called after a pattern is stored in long-term memory
        // We don't compute surprise here since we already did in compute_surprise
        self.observation_count += 1;
    }

    /// Record a surprise value for calibration
    pub fn record_surprise(&mut self, surprise: f32) {
        // Add to recent window
        self.recent_surprises.push_back(surprise);
        if self.recent_surprises.len() > self.calibration_window {
            self.recent_surprises.pop_front();
        }

        // Update running statistics using Welford's algorithm
        self.observation_count += 1;
        let n = self.observation_count as f32;
        let delta = surprise - self.mean_surprise;
        self.mean_surprise += delta / n;
        let delta2 = surprise - self.mean_surprise;
        self.var_surprise += delta * delta2;
    }

    /// Normalize surprise value using adaptive statistics
    fn normalize_surprise(&self, raw: f32) -> f32 {
        if self.observation_count < 10 {
            // Not enough data for normalization
            return raw;
        }

        let std = self.get_std().max(0.01);
        let z_score = (raw - self.mean_surprise) / std;

        // Convert z-score to 0-1 range using sigmoid
        1.0 / (1.0 + (-z_score).exp())
    }

    /// Get standard deviation
    fn get_std(&self) -> f32 {
        if self.observation_count > 1 {
            (self.var_surprise / (self.observation_count as f32 - 1.0)).sqrt()
        } else {
            0.1
        }
    }

    /// Check if pattern should trigger memory update
    pub fn should_update(&self, surprise: f32) -> bool {
        surprise > self.threshold
    }

    /// Get current threshold
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Set threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }

    /// Get adaptive threshold based on recent statistics
    pub fn adaptive_threshold(&self) -> f32 {
        // Use mean + 1 std as adaptive threshold
        let std = self.get_std();
        (self.mean_surprise + std).min(1.0).max(0.1)
    }

    /// Get statistics
    pub fn stats(&self) -> SurpriseStats {
        SurpriseStats {
            mean: self.mean_surprise,
            std: self.get_std(),
            threshold: self.threshold,
            observation_count: self.observation_count,
        }
    }
}

/// Surprise gate statistics
#[derive(Debug, Clone)]
pub struct SurpriseStats {
    /// Mean surprise value
    pub mean: f32,
    /// Standard deviation
    pub std: f32,
    /// Current threshold
    pub threshold: f32,
    /// Number of observations
    pub observation_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{pattern_id, Embedding};
    use std::collections::HashMap;

    fn make_pattern(id: u8) -> Pattern {
        let embedding = Embedding::new(vec![id as f32 / 255.0; 16]);
        Pattern {
            id: pattern_id(&[id]),
            embedding,
            metadata: HashMap::new(),
            created_at: 1702656000000,
        }
    }

    #[test]
    fn test_surprise_gate_empty_memory() {
        let gate = SurpriseGate::new(0.5);
        let ltm = LongTermMemory::new(100, 16);
        let pattern = make_pattern(1);

        let surprise = gate.compute_surprise(&pattern, &ltm);
        assert_eq!(surprise, 1.0); // Empty memory = max surprise
    }

    #[test]
    fn test_surprise_gate_known_pattern() {
        let gate = SurpriseGate::new(0.5);
        let mut ltm = LongTermMemory::new(100, 16);

        // Add patterns to memory
        for i in 0..10 {
            ltm.update(make_pattern(i)).unwrap();
        }

        // Query with known pattern
        let pattern = make_pattern(5);
        let surprise = gate.compute_surprise(&pattern, &ltm);

        // Known pattern should have lower surprise
        assert!(surprise < 0.8);
    }

    #[test]
    fn test_surprise_gate_novel_pattern() {
        let gate = SurpriseGate::new(0.5);
        let mut ltm = LongTermMemory::new(100, 16);

        // Add low-value patterns
        for i in 0..10 {
            ltm.update(make_pattern(i)).unwrap();
        }

        // Query with very different pattern
        let novel = Pattern {
            id: pattern_id(&[255]),
            embedding: Embedding::new(vec![1.0; 16]),
            metadata: HashMap::new(),
            created_at: 1702656000000,
        };
        let surprise = gate.compute_surprise(&novel, &ltm);

        // Novel pattern should have higher surprise
        assert!(surprise > 0.3);
    }

    #[test]
    fn test_adaptive_threshold() {
        let mut gate = SurpriseGate::new(0.5);

        // Record some surprises
        for i in 0..50 {
            let surprise = (i as f32) / 100.0;
            gate.record_surprise(surprise);
        }

        let adaptive = gate.adaptive_threshold();
        assert!(adaptive > 0.0 && adaptive < 1.0);
    }
}
