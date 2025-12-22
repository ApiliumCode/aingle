//! Meta-Level optimization
//!
//! Global network parameters with slow updates (~1000 blocks).

use super::{BlockStats, MetaParams, NestedConfig};

/// Meta-level optimization for global network parameters
pub struct MetaLevel {
    /// Current parameters
    params: MetaParams,

    /// Target efficiency (throughput / latency)
    target_efficiency: f64,

    /// Running statistics
    throughput_history: Vec<f64>,
    latency_history: Vec<u64>,

    /// Learning rate for parameter updates
    learning_rate: f32,

    /// History window size
    history_window: usize,
}

impl MetaLevel {
    /// Create new meta-level optimizer
    pub fn new(config: &NestedConfig) -> Self {
        Self {
            params: MetaParams::default(),
            target_efficiency: 2.0, // Target: 2 tx/ms
            throughput_history: Vec::with_capacity(100),
            latency_history: Vec::with_capacity(100),
            learning_rate: config.learning_rate,
            history_window: 100,
        }
    }

    /// Update parameters based on block statistics
    pub fn update(&mut self, stats: &BlockStats) {
        // Record history
        let throughput = if stats.processing_time_ms > 0 {
            (stats.tx_count as f64) / (stats.processing_time_ms as f64)
        } else {
            0.0
        };
        self.throughput_history.push(throughput);
        self.latency_history.push(stats.latency_p50);

        // Trim history
        if self.throughput_history.len() > self.history_window {
            self.throughput_history.remove(0);
        }
        if self.latency_history.len() > self.history_window {
            self.latency_history.remove(0);
        }

        // Compute current efficiency
        let avg_throughput = self.throughput_history.iter().sum::<f64>()
            / self.throughput_history.len().max(1) as f64;
        let avg_latency =
            self.latency_history.iter().sum::<u64>() / self.latency_history.len().max(1) as u64;

        let efficiency = if avg_latency > 0 {
            avg_throughput / (avg_latency as f64)
        } else {
            avg_throughput
        };

        // Adjust parameters based on efficiency gap
        let efficiency_gap = self.target_efficiency - efficiency;

        if efficiency_gap > 0.1 {
            // Need to improve efficiency
            self.adjust_for_efficiency();
        } else if efficiency_gap < -0.1 {
            // Can afford more strictness
            self.adjust_for_strictness();
        }

        // Update target throughput based on recent performance
        self.params.target_throughput = avg_throughput * 1000.0; // Convert to tx/s
        self.params.target_latency = avg_latency;
    }

    /// Adjust parameters to improve efficiency
    fn adjust_for_efficiency(&mut self) {
        // Reduce strictness to speed up validation
        self.params.validation_strictness =
            (self.params.validation_strictness - self.learning_rate).max(0.5);

        // Increase gossip frequency for faster propagation
        self.params.gossip_multiplier =
            (self.params.gossip_multiplier + self.learning_rate).min(2.0);
    }

    /// Adjust parameters for more strictness
    fn adjust_for_strictness(&mut self) {
        // Increase strictness when we have headroom
        self.params.validation_strictness =
            (self.params.validation_strictness + self.learning_rate * 0.5).min(1.0);

        // Normalize gossip frequency
        self.params.gossip_multiplier =
            (self.params.gossip_multiplier - self.learning_rate * 0.5).max(0.5);
    }

    /// Get current parameters
    pub fn get_params(&self) -> MetaParams {
        self.params.clone()
    }

    /// Set target efficiency
    pub fn set_target_efficiency(&mut self, target: f64) {
        self.target_efficiency = target.max(0.1);
    }

    /// Get performance statistics
    pub fn get_performance(&self) -> MetaPerformance {
        let avg_throughput = if self.throughput_history.is_empty() {
            0.0
        } else {
            self.throughput_history.iter().sum::<f64>() / self.throughput_history.len() as f64
        };

        let avg_latency = if self.latency_history.is_empty() {
            0
        } else {
            self.latency_history.iter().sum::<u64>() / self.latency_history.len() as u64
        };

        MetaPerformance {
            avg_throughput,
            avg_latency,
            efficiency: if avg_latency > 0 {
                avg_throughput / (avg_latency as f64)
            } else {
                0.0
            },
            sample_count: self.throughput_history.len(),
        }
    }
}

/// Meta-level performance statistics
#[derive(Debug, Clone)]
pub struct MetaPerformance {
    /// Average throughput (tx/ms)
    pub avg_throughput: f64,
    /// Average latency (ms)
    pub avg_latency: u64,
    /// Efficiency (throughput/latency)
    pub efficiency: f64,
    /// Number of samples
    pub sample_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta_level_basic() {
        let config = NestedConfig::default();
        let mut meta = MetaLevel::new(&config);

        let stats = BlockStats {
            tx_count: 100,
            processing_time_ms: 50,
            failures: 0,
            latency_p50: 100,
            latency_p90: 200,
            latency_p99: 500,
            peer_count: 10,
        };

        meta.update(&stats);

        let params = meta.get_params();
        assert!(params.target_throughput > 0.0);
    }

    #[test]
    fn test_efficiency_adjustment() {
        let config = NestedConfig::default();
        let mut meta = MetaLevel::new(&config);
        let initial_strictness = meta.params.validation_strictness;

        // Simulate poor efficiency
        for _ in 0..5 {
            let stats = BlockStats {
                tx_count: 10,
                processing_time_ms: 1000, // Very slow
                failures: 0,
                latency_p50: 500,
                latency_p90: 800,
                latency_p99: 1000,
                peer_count: 10,
            };
            meta.update(&stats);
        }

        // Strictness should have decreased
        assert!(meta.params.validation_strictness <= initial_strictness);
    }
}
