//! Titans Memory configuration

use serde::{Deserialize, Serialize};

/// Configuration for Titans Memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitansConfig {
    /// Short-term window size (number of transactions)
    pub window_size: usize,

    /// Long-term memory capacity (number of patterns)
    pub memory_capacity: usize,

    /// Embedding dimension for patterns
    pub embedding_dim: usize,

    /// Surprise threshold for long-term memory updates
    /// Transactions with surprise > threshold are stored in long-term memory
    pub surprise_threshold: f32,

    /// Enable anomaly detection
    pub anomaly_detection: bool,

    /// Threshold for anomaly detection (lower = more sensitive)
    pub anomaly_threshold: f32,

    /// Attention decay factor for short-term memory
    pub attention_decay: f32,

    /// Enable memory compression
    pub compression_enabled: bool,

    /// Compression ratio (1.0 = no compression)
    pub compression_ratio: f32,
}

impl Default for TitansConfig {
    fn default() -> Self {
        Self {
            window_size: 1000,
            memory_capacity: 10000,
            embedding_dim: 16,
            surprise_threshold: 0.5,
            anomaly_detection: true,
            anomaly_threshold: 0.3,
            attention_decay: 0.99,
            compression_enabled: false,
            compression_ratio: 1.0,
        }
    }
}

impl TitansConfig {
    /// IoT-optimized configuration (minimal memory usage)
    pub fn iot() -> Self {
        Self {
            window_size: 100,         // Much smaller window
            memory_capacity: 500,     // Limited long-term storage
            embedding_dim: 8,         // Reduced dimensionality
            surprise_threshold: 0.7,  // Higher threshold = fewer updates
            anomaly_detection: false, // Disabled for performance
            anomaly_threshold: 0.3,
            attention_decay: 0.95,
            compression_enabled: true,
            compression_ratio: 0.5,
        }
    }

    /// Full-power configuration for servers
    pub fn full_power() -> Self {
        Self {
            window_size: 10000,      // Large window
            memory_capacity: 100000, // Large long-term storage
            embedding_dim: 32,       // Higher dimensionality
            surprise_threshold: 0.3, // Lower threshold = more learning
            anomaly_detection: true,
            anomaly_threshold: 0.2,
            attention_decay: 0.995,
            compression_enabled: false,
            compression_ratio: 1.0,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.window_size == 0 {
            return Err("window_size must be > 0".to_string());
        }
        if self.memory_capacity == 0 {
            return Err("memory_capacity must be > 0".to_string());
        }
        if self.embedding_dim == 0 {
            return Err("embedding_dim must be > 0".to_string());
        }
        if self.surprise_threshold < 0.0 || self.surprise_threshold > 1.0 {
            return Err("surprise_threshold must be between 0.0 and 1.0".to_string());
        }
        if self.anomaly_threshold < 0.0 || self.anomaly_threshold > 1.0 {
            return Err("anomaly_threshold must be between 0.0 and 1.0".to_string());
        }
        if self.attention_decay < 0.0 || self.attention_decay > 1.0 {
            return Err("attention_decay must be between 0.0 and 1.0".to_string());
        }
        Ok(())
    }

    /// Estimate memory usage in bytes
    pub fn estimated_memory_bytes(&self) -> usize {
        let pattern_size = self.embedding_dim * 4 + 32 + 8 + 100; // embedding + id + timestamp + metadata
        let short_term = self.window_size * pattern_size;
        let long_term = self.memory_capacity * pattern_size;
        short_term + long_term
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TitansConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_iot_config() {
        let config = TitansConfig::iot();
        assert!(config.validate().is_ok());
        assert!(config.window_size < TitansConfig::default().window_size);
    }

    #[test]
    fn test_validation() {
        let mut config = TitansConfig::default();
        config.window_size = 0;
        assert!(config.validate().is_err());

        config = TitansConfig::default();
        config.surprise_threshold = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_memory_estimation() {
        let config = TitansConfig::default();
        let bytes = config.estimated_memory_bytes();
        assert!(bytes > 0);

        let iot_config = TitansConfig::iot();
        let iot_bytes = iot_config.estimated_memory_bytes();
        assert!(iot_bytes < bytes);
    }
}
