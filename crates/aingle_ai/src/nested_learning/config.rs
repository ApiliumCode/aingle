//! Nested Learning configuration

use serde::{Deserialize, Serialize};

/// Configuration for Nested Learning system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NestedConfig {
    /// Meta-level update interval (blocks)
    pub meta_update_interval: u64,

    /// Optimizer-level update interval (transactions)
    pub optimizer_update_interval: u64,

    /// Feature dimension for transaction processing
    pub feature_dim: usize,

    /// Enable fast-path optimization
    pub fast_path_enabled: bool,

    /// Fast-path confidence threshold
    pub fast_path_threshold: f32,

    /// Learning rate for optimizer updates
    pub learning_rate: f32,

    /// Enable parallel validation groups
    pub parallel_validation: bool,

    /// Maximum parallel validation group size
    pub max_parallel_group: usize,
}

impl Default for NestedConfig {
    fn default() -> Self {
        Self {
            meta_update_interval: 1000,
            optimizer_update_interval: 100,
            feature_dim: 16,
            fast_path_enabled: true,
            fast_path_threshold: 0.9,
            learning_rate: 0.01,
            parallel_validation: true,
            max_parallel_group: 10,
        }
    }
}

impl NestedConfig {
    /// IoT-optimized configuration
    pub fn iot() -> Self {
        Self {
            meta_update_interval: 5000, // Less frequent updates
            optimizer_update_interval: 500,
            feature_dim: 8, // Smaller features
            fast_path_enabled: true,
            fast_path_threshold: 0.95, // Stricter threshold
            learning_rate: 0.001,
            parallel_validation: false, // Sequential for simplicity
            max_parallel_group: 1,
        }
    }

    /// Full-power configuration for servers
    pub fn full_power() -> Self {
        Self {
            meta_update_interval: 100, // More frequent updates
            optimizer_update_interval: 10,
            feature_dim: 32, // Larger features
            fast_path_enabled: true,
            fast_path_threshold: 0.8, // More permissive
            learning_rate: 0.05,
            parallel_validation: true,
            max_parallel_group: 100,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.meta_update_interval == 0 {
            return Err("meta_update_interval must be > 0".to_string());
        }
        if self.optimizer_update_interval == 0 {
            return Err("optimizer_update_interval must be > 0".to_string());
        }
        if self.feature_dim == 0 {
            return Err("feature_dim must be > 0".to_string());
        }
        if self.fast_path_threshold < 0.0 || self.fast_path_threshold > 1.0 {
            return Err("fast_path_threshold must be between 0.0 and 1.0".to_string());
        }
        if self.learning_rate < 0.0 || self.learning_rate > 1.0 {
            return Err("learning_rate must be between 0.0 and 1.0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NestedConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_iot_config() {
        let config = NestedConfig::iot();
        assert!(config.validate().is_ok());
        assert!(config.meta_update_interval > NestedConfig::default().meta_update_interval);
    }
}
