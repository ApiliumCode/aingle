//! Auto-Reconfigurator implementation
//!
//! Resource-aware automatic reconfiguration.

use super::{PowerMode, ReconfigResult};
use crate::types::ResourceCategory;

/// Auto-Reconfiguration based on resources
pub struct AutoReconfigurator {
    /// Current configuration
    current_config: NodeConfig,

    /// Configuration templates
    templates: ConfigTemplates,

    /// History of reconfigurations
    history: Vec<ReconfigEvent>,
}

impl AutoReconfigurator {
    /// Create new auto-reconfigurator
    pub fn new() -> Self {
        Self {
            current_config: NodeConfig::balanced(),
            templates: ConfigTemplates::new(),
            history: Vec::new(),
        }
    }

    /// Automatically reconfigure based on available resources
    pub fn reconfigure(&mut self, resources: ResourceCategory) -> ReconfigResult {
        let new_config = match resources {
            ResourceCategory::Abundant => self.templates.full_power(),
            ResourceCategory::Normal => self.templates.balanced(),
            ResourceCategory::Limited => self.templates.low_power(),
            ResourceCategory::Critical => self.templates.minimal(),
        };

        if new_config != self.current_config {
            let event = ReconfigEvent {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                from: self.current_config.mode,
                to: new_config.mode,
                reason: format!("Resource category: {:?}", resources),
            };
            self.history.push(event);

            self.current_config = new_config.clone();
            ReconfigResult::Changed(new_config)
        } else {
            ReconfigResult::NoChange
        }
    }

    /// Get current configuration
    pub fn current(&self) -> &NodeConfig {
        &self.current_config
    }

    /// Get reconfiguration history
    pub fn history(&self) -> &[ReconfigEvent] {
        &self.history
    }
}

impl Default for AutoReconfigurator {
    fn default() -> Self {
        Self::new()
    }
}

/// Node configuration
#[derive(Debug, Clone, PartialEq)]
pub struct NodeConfig {
    /// Power mode
    pub mode: PowerMode,

    /// Memory limit in bytes
    pub memory_limit: usize,

    /// Validation parallelism
    pub validation_parallelism: usize,

    /// Gossip frequency multiplier
    pub gossip_multiplier: f32,

    /// AI features enabled
    pub ai_enabled: bool,

    /// Memory compression enabled
    pub compression_enabled: bool,
}

impl NodeConfig {
    /// Full power configuration
    pub fn full_power() -> Self {
        Self {
            mode: PowerMode::Full,
            memory_limit: 1024 * 1024 * 1024, // 1GB
            validation_parallelism: 16,
            gossip_multiplier: 2.0,
            ai_enabled: true,
            compression_enabled: false,
        }
    }

    /// Balanced configuration
    pub fn balanced() -> Self {
        Self {
            mode: PowerMode::Balanced,
            memory_limit: 256 * 1024 * 1024, // 256MB
            validation_parallelism: 4,
            gossip_multiplier: 1.0,
            ai_enabled: true,
            compression_enabled: false,
        }
    }

    /// Low power configuration
    pub fn low_power() -> Self {
        Self {
            mode: PowerMode::Low,
            memory_limit: 64 * 1024 * 1024, // 64MB
            validation_parallelism: 2,
            gossip_multiplier: 0.5,
            ai_enabled: true,
            compression_enabled: true,
        }
    }

    /// Minimal configuration
    pub fn minimal() -> Self {
        Self {
            mode: PowerMode::Critical,
            memory_limit: 16 * 1024 * 1024, // 16MB
            validation_parallelism: 1,
            gossip_multiplier: 0.25,
            ai_enabled: false,
            compression_enabled: true,
        }
    }
}

/// Configuration templates
struct ConfigTemplates;

impl ConfigTemplates {
    fn new() -> Self {
        Self
    }

    fn full_power(&self) -> NodeConfig {
        NodeConfig::full_power()
    }

    fn balanced(&self) -> NodeConfig {
        NodeConfig::balanced()
    }

    fn low_power(&self) -> NodeConfig {
        NodeConfig::low_power()
    }

    fn minimal(&self) -> NodeConfig {
        NodeConfig::minimal()
    }
}

/// Reconfiguration event record
#[derive(Debug, Clone)]
pub struct ReconfigEvent {
    /// Timestamp
    pub timestamp: u64,
    /// Previous mode
    pub from: PowerMode,
    /// New mode
    pub to: PowerMode,
    /// Reason for change
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_reconfigurator() {
        let mut reconf = AutoReconfigurator::new();

        // Start at balanced
        assert_eq!(reconf.current().mode, PowerMode::Balanced);

        // Trigger low power
        let result = reconf.reconfigure(ResourceCategory::Limited);
        assert!(matches!(result, ReconfigResult::Changed(_)));
        assert_eq!(reconf.current().mode, PowerMode::Low);

        // Same category = no change
        let result = reconf.reconfigure(ResourceCategory::Limited);
        assert!(matches!(result, ReconfigResult::NoChange));
    }

    #[test]
    fn test_config_templates() {
        let full = NodeConfig::full_power();
        let minimal = NodeConfig::minimal();

        assert!(full.memory_limit > minimal.memory_limit);
        assert!(full.validation_parallelism > minimal.validation_parallelism);
        assert!(full.ai_enabled);
        assert!(!minimal.ai_enabled);
    }
}
