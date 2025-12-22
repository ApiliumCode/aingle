//! Configuration for HOPE Agents.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Defines the configuration for a HOPE agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// The human-readable name of the agent.
    pub name: String,
    /// The maximum memory usage in bytes the agent should consume.
    pub max_memory_bytes: usize,
    /// The time interval between aget's decision-making loops.
    pub decision_interval: Duration,
    /// The maximum number of concurrent goals the agent can manage.
    pub max_goals: usize,
    /// A flag to enable or disable the agent's learning capabilities.
    pub learning_enabled: bool,
    /// The learning rate (alpha) for reinforcement learning algorithms (typically 0.0 to 1.0).
    pub learning_rate: f32,
    /// The exploration rate (epsilon) for epsilon-greedy policies, determining the balance
    /// between exploring new actions and exploiting known ones (0.0 to 1.0).
    pub exploration_rate: f32,
    /// The maximum number of rules the agent's policy engine can hold.
    pub max_rules: usize,
    /// The default interval for polling sensors for new observations.
    pub sensor_interval: Duration,
    /// The default timeout for actions executed by the agent.
    pub action_timeout: Duration,
}

impl Default for AgentConfig {
    /// Provides a default, balanced configuration for a standard agent.
    fn default() -> Self {
        Self {
            name: "agent".to_string(),
            max_memory_bytes: 512 * 1024, // 512KB
            decision_interval: Duration::from_millis(100),
            max_goals: 10,
            learning_enabled: true,
            learning_rate: 0.1,
            exploration_rate: 0.1,
            max_rules: 100,
            sensor_interval: Duration::from_millis(50),
            action_timeout: Duration::from_secs(5),
        }
    }
}

impl AgentConfig {
    /// Creates a new configuration with a specified name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Returns a configuration optimized for IoT and resource-constrained environments.
    /// This mode has a smaller memory footprint and disables learning by default.
    pub fn iot_mode() -> Self {
        Self {
            name: "iot_agent".to_string(),
            max_memory_bytes: 64 * 1024, // 64KB
            decision_interval: Duration::from_millis(50),
            max_goals: 5,
            learning_enabled: false, // Save resources
            learning_rate: 0.0,
            exploration_rate: 0.0,
            max_rules: 20,
            sensor_interval: Duration::from_millis(100),
            action_timeout: Duration::from_secs(2),
        }
    }

    /// Returns a configuration suitable for a more capable AI agent with learning enabled.
    /// This mode allocates more resources for memory and learning processes.
    pub fn ai_mode() -> Self {
        Self {
            name: "ai_agent".to_string(),
            max_memory_bytes: 2 * 1024 * 1024, // 2MB
            decision_interval: Duration::from_millis(200),
            max_goals: 20,
            learning_enabled: true,
            learning_rate: 0.15,
            exploration_rate: 0.2,
            max_rules: 500,
            sensor_interval: Duration::from_millis(100),
            action_timeout: Duration::from_secs(10),
        }
    }

    /// Sets the name of the agent in the configuration.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Enables or disables the learning capability in the configuration.
    pub fn with_learning(mut self, enabled: bool) -> Self {
        self.learning_enabled = enabled;
        self
    }

    /// Sets the maximum memory limit in bytes.
    pub fn with_memory_limit(mut self, bytes: usize) -> Self {
        self.max_memory_bytes = bytes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert_eq!(config.name, "agent");
        assert!(config.learning_enabled);
    }

    #[test]
    fn test_iot_config() {
        let config = AgentConfig::iot_mode();
        assert!(config.max_memory_bytes <= 128 * 1024);
        assert!(!config.learning_enabled);
    }

    #[test]
    fn test_ai_config() {
        let config = AgentConfig::ai_mode();
        assert!(config.learning_enabled);
        assert!(config.max_rules >= 100);
    }
}
