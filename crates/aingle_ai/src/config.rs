//! Global AI configuration

use crate::hope::HopeConfig;
use crate::nested_learning::NestedConfig;
use crate::titans::TitansConfig;
use serde::{Deserialize, Serialize};

/// Global AI configuration for AIngle nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Titans Memory configuration
    pub titans: TitansConfig,

    /// Nested Learning configuration
    pub nested_learning: NestedConfig,

    /// HOPE Agent configuration
    pub hope: HopeConfig,

    /// Enable predictive validation
    pub predictive_validation: bool,

    /// Enable adaptive consensus
    pub adaptive_consensus: bool,

    /// IoT mode (reduced resource usage)
    pub iot_mode: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            titans: TitansConfig::default(),
            nested_learning: NestedConfig::default(),
            hope: HopeConfig::default(),
            predictive_validation: true,
            adaptive_consensus: true,
            iot_mode: false,
        }
    }
}

impl AiConfig {
    /// Create IoT-optimized configuration
    pub fn iot() -> Self {
        Self {
            titans: TitansConfig::iot(),
            nested_learning: NestedConfig::iot(),
            hope: HopeConfig::iot(),
            predictive_validation: false, // Too expensive for IoT
            adaptive_consensus: true,
            iot_mode: true,
        }
    }

    /// Create full-power configuration for servers
    pub fn full_power() -> Self {
        Self {
            titans: TitansConfig::full_power(),
            nested_learning: NestedConfig::full_power(),
            hope: HopeConfig::full_power(),
            predictive_validation: true,
            adaptive_consensus: true,
            iot_mode: false,
        }
    }

    /// Load configuration from TOML file
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize to TOML
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        self.titans.validate()?;
        self.nested_learning.validate()?;
        self.hope.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AiConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_iot_config() {
        let config = AiConfig::iot();
        assert!(config.iot_mode);
        assert!(!config.predictive_validation);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = AiConfig::default();
        let toml = config.to_toml().unwrap();
        let parsed = AiConfig::from_toml(&toml).unwrap();
        assert_eq!(config.iot_mode, parsed.iot_mode);
    }
}
