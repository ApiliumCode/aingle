//! HOPE Agent configuration

use serde::{Deserialize, Serialize};

/// Configuration for HOPE Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HopeConfig {
    /// Enable self-modification
    pub self_modification_enabled: bool,

    /// Safety bounds strictness
    pub safety_level: SafetyLevel,

    /// Memory dimension for continuum memory
    pub memory_dim: usize,

    /// Memory capacity
    pub memory_capacity: usize,

    /// Context capacity (max historical contexts)
    pub context_capacity: usize,

    /// Decay factor for context relevance
    pub context_decay: f32,

    /// Maximum rules for self-modification
    pub max_rules: usize,

    /// Learning rate for behavior updates
    pub learning_rate: f32,
}

/// Safety level for self-modification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyLevel {
    /// Strict - very limited modifications allowed
    Strict,
    /// Moderate - most modifications allowed except critical paths
    Moderate,
    /// Permissive - nearly all modifications allowed (use with caution)
    Permissive,
}

impl Default for HopeConfig {
    fn default() -> Self {
        Self {
            self_modification_enabled: true,
            safety_level: SafetyLevel::Strict,
            memory_dim: 32,
            memory_capacity: 10000,
            context_capacity: 1000,
            context_decay: 0.99,
            max_rules: 100,
            learning_rate: 0.01,
        }
    }
}

impl HopeConfig {
    /// IoT-optimized configuration
    pub fn iot() -> Self {
        Self {
            self_modification_enabled: false, // Too risky for IoT
            safety_level: SafetyLevel::Strict,
            memory_dim: 16,
            memory_capacity: 500,
            context_capacity: 100,
            context_decay: 0.95,
            max_rules: 10,
            learning_rate: 0.001,
        }
    }

    /// Full-power configuration
    pub fn full_power() -> Self {
        Self {
            self_modification_enabled: true,
            safety_level: SafetyLevel::Moderate,
            memory_dim: 64,
            memory_capacity: 100000,
            context_capacity: 10000,
            context_decay: 0.995,
            max_rules: 1000,
            learning_rate: 0.05,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.memory_dim == 0 {
            return Err("memory_dim must be > 0".to_string());
        }
        if self.memory_capacity == 0 {
            return Err("memory_capacity must be > 0".to_string());
        }
        if self.context_capacity == 0 {
            return Err("context_capacity must be > 0".to_string());
        }
        if self.context_decay < 0.0 || self.context_decay > 1.0 {
            return Err("context_decay must be between 0.0 and 1.0".to_string());
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
        let config = HopeConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_iot_config() {
        let config = HopeConfig::iot();
        assert!(config.validate().is_ok());
        assert!(!config.self_modification_enabled);
    }
}
