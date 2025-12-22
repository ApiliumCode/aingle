//! Smart Node - IoT+AI Pipeline Integration
//!
//! Combines `MinimalNode` with HOPE Agents to create intelligent IoT nodes
//! that can observe, decide, act, and learn.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      SmartNode                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌──────────────┐     ┌──────────────┐     ┌─────────────┐ │
//! │  │   Sensors    │────>│   HOPE       │────>│  Network    │ │
//! │  │   (IoT)      │     │   Agent      │     │  (CoAP)     │ │
//! │  └──────────────┘     └──────────────┘     └─────────────┘ │
//! │         │                    │                    │         │
//! │         v                    v                    v         │
//! │  ┌──────────────┐     ┌──────────────┐     ┌─────────────┐ │
//! │  │ Observations │     │   Entries    │     │   Records   │ │
//! │  │              │────>│   (DAG)      │<────│   (Gossip)  │ │
//! │  └──────────────┘     └──────────────┘     └─────────────┘ │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use aingle_minimal::{SmartNode, SmartNodeConfig};
//! use hope_agents::{Observation, Goal};
//!
//! // Create smart node with AI capabilities
//! let config = SmartNodeConfig::iot_mode();
//! let mut node = SmartNode::new(config).await?;
//!
//! // Add a goal
//! node.add_goal(Goal::maintain("temperature", 20.0..25.0));
//!
//! // Process sensor data
//! node.observe(Observation::sensor("temperature", 22.5));
//!
//! // Let the agent decide and act
//! if let Some(action) = node.step() {
//!     println!("Action taken: {:?}", action);
//! }
//! ```

use crate::config::Config;
use crate::error::Result;
use crate::node::MinimalNode;
use crate::types::{Entry, EntryType, Hash, NodeStats};

use hope_agents::agent::AgentStats;
use hope_agents::{
    Action, ActionResult, ActionType, Agent, AgentConfig, AgentState, Goal, Observation, Policy,
    Rule, SimpleAgent,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for SmartNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartNodeConfig {
    /// Base node configuration
    pub node_config: Config,
    /// Agent configuration
    pub agent_config: AgentConfig,
    /// Auto-publish sensor observations
    pub auto_publish_observations: bool,
    /// Auto-publish action results
    pub auto_publish_actions: bool,
    /// Observation retention period in seconds
    pub observation_retention_secs: u64,
    /// Maximum pending actions
    pub max_pending_actions: usize,
}

impl Default for SmartNodeConfig {
    fn default() -> Self {
        Self {
            node_config: Config::default(),
            agent_config: AgentConfig::default(),
            auto_publish_observations: true,
            auto_publish_actions: true,
            observation_retention_secs: 300,
            max_pending_actions: 100,
        }
    }
}

impl SmartNodeConfig {
    /// Create IoT-optimized configuration
    pub fn iot_mode() -> Self {
        Self {
            node_config: Config::iot_mode(),
            agent_config: AgentConfig::iot_mode(),
            auto_publish_observations: true,
            auto_publish_actions: true,
            observation_retention_secs: 60,
            max_pending_actions: 20,
        }
    }

    /// Create low-power configuration
    pub fn low_power() -> Self {
        Self {
            node_config: Config::low_power(),
            agent_config: AgentConfig::iot_mode(), // Use iot_mode as base
            auto_publish_observations: false,      // Save energy
            auto_publish_actions: false,
            observation_retention_secs: 30,
            max_pending_actions: 10,
        }
    }
}

/// Smart Node combining MinimalNode with HOPE Agent
pub struct SmartNode {
    /// Base AIngle node
    node: MinimalNode,
    /// HOPE Agent
    agent: SimpleAgent,
    /// Configuration
    config: SmartNodeConfig,
    /// Pending actions to execute
    pending_actions: Vec<Action>,
    /// Action history
    action_history: Vec<(Action, ActionResult)>,
    /// Entry hash to observation mapping
    observation_entries: HashMap<Hash, Observation>,
}

impl SmartNode {
    /// Create a new smart node
    pub fn new(config: SmartNodeConfig) -> Result<Self> {
        let node = MinimalNode::new(config.node_config.clone())?;
        let agent =
            SimpleAgent::with_config(&config.agent_config.name, config.agent_config.clone());

        Ok(Self {
            node,
            agent,
            config,
            pending_actions: Vec::new(),
            action_history: Vec::new(),
            observation_entries: HashMap::new(),
        })
    }

    /// Create with a pre-configured agent
    pub fn with_agent(config: SmartNodeConfig, agent: SimpleAgent) -> Result<Self> {
        let node = MinimalNode::new(config.node_config.clone())?;

        Ok(Self {
            node,
            agent,
            config,
            pending_actions: Vec::new(),
            action_history: Vec::new(),
            observation_entries: HashMap::new(),
        })
    }

    /// Get the underlying node
    pub fn node(&self) -> &MinimalNode {
        &self.node
    }

    /// Get mutable node reference
    pub fn node_mut(&mut self) -> &mut MinimalNode {
        &mut self.node
    }

    /// Get the agent
    pub fn agent(&self) -> &SimpleAgent {
        &self.agent
    }

    /// Get mutable agent reference
    pub fn agent_mut(&mut self) -> &mut SimpleAgent {
        &mut self.agent
    }

    /// Process an observation from a sensor
    pub fn observe(&mut self, observation: Observation) -> Result<Option<Hash>> {
        // Feed observation to agent
        self.agent.observe(observation.clone());

        // Optionally publish to DAG
        if self.config.auto_publish_observations {
            let entry = self.observation_to_entry(&observation);
            let hash = self.node.create_entry(entry)?;
            self.observation_entries.insert(hash.clone(), observation);
            return Ok(Some(hash));
        }

        Ok(None)
    }

    /// Process a batch of observations
    pub fn observe_batch(&mut self, observations: Vec<Observation>) -> Result<Vec<Hash>> {
        let mut hashes = Vec::new();
        for obs in observations {
            if let Some(hash) = self.observe(obs)? {
                hashes.push(hash);
            }
        }
        Ok(hashes)
    }

    /// Let the agent decide on an action
    pub fn decide(&self) -> Action {
        self.agent.decide()
    }

    /// Execute a single step: decide and execute action
    pub fn step(&mut self) -> Result<Option<ActionResult>> {
        let action = self.decide();

        if action.is_noop() {
            return Ok(None);
        }

        let result = self.execute_action(action)?;
        Ok(Some(result))
    }

    /// Execute an action
    pub fn execute_action(&mut self, action: Action) -> Result<ActionResult> {
        let start = std::time::Instant::now();

        // Execute based on action type
        let result = match &action.action_type {
            ActionType::Publish(topic) => self.execute_publish(&action, topic)?,
            ActionType::StoreData(key) => self.execute_store(&action, key)?,
            ActionType::SendMessage(target) => self.execute_send(&action, target)?,
            ActionType::Alert(message) => self.execute_alert(&action, message)?,
            ActionType::UpdateState(state_name) => {
                self.execute_state_update(&action, state_name)?
            }
            ActionType::Query(_key) => {
                // Query not implemented yet
                ActionResult::failure(&action.id, "Query not implemented")
            }
            ActionType::RemoteCall(_target) => {
                // Remote call not implemented yet
                ActionResult::failure(&action.id, "RemoteCall not implemented")
            }
            ActionType::Wait => ActionResult::success(&action.id),
            ActionType::NoOp => ActionResult::success(&action.id),
            ActionType::Custom(name) => {
                log::debug!("Custom action '{}' executed", name);
                ActionResult::success(&action.id)
            }
        };

        let result = result.with_duration(start.elapsed().as_micros() as u64);

        // Clone observation for learning (to avoid borrow issues)
        let obs_clone = self
            .agent
            .recent_observations(1)
            .first()
            .map(|o| (*o).clone());

        // Learn from the result
        if let Some(obs) = obs_clone {
            self.agent.learn(&obs, &action, &result);
        }

        // Store in history
        if self.action_history.len() >= self.config.max_pending_actions {
            self.action_history.remove(0);
        }
        self.action_history.push((action.clone(), result.clone()));

        // Optionally publish action result
        if self.config.auto_publish_actions {
            let entry = self.action_to_entry(&action, &result);
            let _ = self.node.create_entry(entry);
        }

        Ok(result)
    }

    /// Execute publish action
    fn execute_publish(&mut self, action: &Action, topic: &str) -> Result<ActionResult> {
        // Get value from params
        let value = action.params.get("value").cloned();
        let value_str = match &value {
            Some(v) => serde_json::to_string(v).unwrap_or_default(),
            None => "null".to_string(),
        };

        // Create entry
        let content = format!("{{\"topic\":\"{}\",\"value\":{}}}", topic, value_str);
        let entry = Entry {
            entry_type: EntryType::App,
            content: content.into_bytes(),
        };

        match self.node.create_entry(entry) {
            Ok(hash) => Ok(ActionResult::success_with_value(
                &action.id,
                hash.to_string(),
            )),
            Err(e) => Ok(ActionResult::failure(&action.id, &e.to_string())),
        }
    }

    /// Execute store action
    fn execute_store(&mut self, action: &Action, key: &str) -> Result<ActionResult> {
        let value = action.params.get("value").cloned();
        let value_str = match &value {
            Some(v) => serde_json::to_string(v).unwrap_or_default(),
            None => "null".to_string(),
        };

        let content = format!("{{\"key\":\"{}\",\"value\":{}}}", key, value_str);
        let entry = Entry {
            entry_type: EntryType::App,
            content: content.into_bytes(),
        };

        match self.node.create_entry(entry) {
            Ok(hash) => Ok(ActionResult::success_with_value(
                &action.id,
                hash.to_string(),
            )),
            Err(e) => Ok(ActionResult::failure(&action.id, &e.to_string())),
        }
    }

    /// Execute send message action
    fn execute_send(&mut self, action: &Action, target: &str) -> Result<ActionResult> {
        log::info!(
            "Sending message to {}: {:?}",
            target,
            action.params.get("content")
        );
        // Network send would go here (via CoAP or gossip)
        Ok(ActionResult::success(&action.id))
    }

    /// Execute alert action
    fn execute_alert(&mut self, action: &Action, message: &str) -> Result<ActionResult> {
        log::warn!("ALERT: {}", message);

        // Create alert entry
        let content = format!(
            "{{\"alert\":\"{}\",\"timestamp\":{}}}",
            message,
            chrono::Utc::now().timestamp()
        );
        let entry = Entry {
            entry_type: EntryType::App,
            content: content.into_bytes(),
        };

        let _ = self.node.create_entry(entry);
        Ok(ActionResult::success(&action.id))
    }

    /// Execute state update action
    fn execute_state_update(&mut self, action: &Action, state_name: &str) -> Result<ActionResult> {
        let value = action.params.get("value").cloned();
        log::debug!("State update: {} = {:?}", state_name, value);
        Ok(ActionResult::success(&action.id))
    }

    /// Convert observation to DAG entry
    fn observation_to_entry(&self, obs: &Observation) -> Entry {
        let content = serde_json::json!({
            "type": "observation",
            "obs_type": format!("{:?}", obs.obs_type),
            "value": obs.value,
            "timestamp": obs.timestamp.0,
            "confidence": obs.confidence.0,
            "metadata": obs.metadata,
        });

        Entry {
            entry_type: EntryType::App,
            content: content.to_string().into_bytes(),
        }
    }

    /// Convert action result to DAG entry
    fn action_to_entry(&self, action: &Action, result: &ActionResult) -> Entry {
        let content = serde_json::json!({
            "type": "action_result",
            "action_type": format!("{:?}", action.action_type),
            "success": result.success,
            "value": result.value,
            "error": result.error,
            "executed_at": result.executed_at.0,
            "duration_us": result.duration_us,
        });

        Entry {
            entry_type: EntryType::App,
            content: content.to_string().into_bytes(),
        }
    }

    /// Add a policy to the agent
    pub fn add_policy(&mut self, policy: Policy) {
        self.agent.add_policy(policy);
    }

    /// Add a rule to the agent
    pub fn add_rule(&mut self, rule: Rule) {
        self.agent.add_rule(rule);
    }

    /// Add a goal to the agent
    pub fn add_goal(&mut self, goal: Goal) {
        self.agent.add_goal(goal);
    }

    /// Get agent state
    pub fn agent_state(&self) -> AgentState {
        self.agent.state()
    }

    /// Get agent statistics
    pub fn agent_stats(&self) -> &AgentStats {
        self.agent.stats()
    }

    /// Get node statistics
    pub fn node_stats(&self) -> Result<NodeStats> {
        self.node.stats()
    }

    /// Get combined statistics
    pub fn stats(&self) -> Result<SmartNodeStats> {
        Ok(SmartNodeStats {
            node_stats: self.node.stats()?,
            agent_stats: self.agent.stats().clone(),
            pending_actions: self.pending_actions.len(),
            action_history_len: self.action_history.len(),
            observation_entries: self.observation_entries.len(),
        })
    }

    /// Pause the agent
    pub fn pause(&mut self) {
        self.agent.pause();
    }

    /// Resume the agent
    pub fn resume(&mut self) {
        self.agent.resume();
    }

    /// Stop the smart node
    pub fn stop(&mut self) {
        self.agent.stop();
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.agent.is_running()
    }

    /// Get active goals
    pub fn active_goals(&self) -> Vec<&Goal> {
        self.agent.active_goals()
    }

    /// Get recent observations
    pub fn recent_observations(&self, count: usize) -> Vec<&Observation> {
        self.agent.recent_observations(count)
    }

    /// Get action history
    pub fn action_history(&self) -> &[(Action, ActionResult)] {
        &self.action_history
    }

    /// Clear action history
    pub fn clear_history(&mut self) {
        self.action_history.clear();
    }
}

/// Combined statistics for SmartNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartNodeStats {
    /// Node statistics
    pub node_stats: NodeStats,
    /// Agent statistics
    pub agent_stats: AgentStats,
    /// Pending actions count
    pub pending_actions: usize,
    /// Action history length
    pub action_history_len: usize,
    /// Observation entries count
    pub observation_entries: usize,
}

/// Sensor adapter for converting sensor readings to HOPE observations
pub struct SensorAdapter {
    name: String,
    scale: f64,
    offset: f64,
}

impl SensorAdapter {
    /// Create a new sensor adapter
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            scale: 1.0,
            offset: 0.0,
        }
    }

    /// Create with scaling
    pub fn with_scaling(name: &str, scale: f64, offset: f64) -> Self {
        Self {
            name: name.to_string(),
            scale,
            offset,
        }
    }

    /// Convert raw reading to observation
    pub fn reading(&self, raw_value: f64) -> Observation {
        let scaled = raw_value * self.scale + self.offset;
        Observation::sensor(&self.name, scaled)
    }

    /// Create boolean observation (on/off, true/false)
    pub fn boolean(&self, value: bool) -> Observation {
        Observation::sensor(&self.name, if value { 1.0 } else { 0.0 })
    }

    /// Create event observation
    pub fn event(&self) -> Observation {
        Observation::event(&self.name)
    }
}

/// Policy builder for common IoT scenarios
pub struct IoTPolicyBuilder;

impl IoTPolicyBuilder {
    /// Create a threshold alert policy
    pub fn threshold_alert(sensor_name: &str, threshold: f64, alert_message: &str) -> Rule {
        use hope_agents::policy::Condition;

        Rule::new(
            &format!("{}_threshold", sensor_name),
            Condition::above(sensor_name, threshold),
            Action::alert(alert_message),
        )
    }

    /// Create a range maintenance policy
    pub fn maintain_range(
        sensor_name: &str,
        min: f64,
        max: f64,
        action_low: Action,
        action_high: Action,
    ) -> Vec<Rule> {
        use hope_agents::policy::Condition;

        vec![
            Rule::new(
                &format!("{}_below_min", sensor_name),
                Condition::below(sensor_name, min),
                action_low,
            ),
            Rule::new(
                &format!("{}_above_max", sensor_name),
                Condition::above(sensor_name, max),
                action_high,
            ),
        ]
    }

    /// Create a binary control policy (on/off based on threshold)
    pub fn binary_control(
        sensor_name: &str,
        threshold: f64,
        on_action: Action,
        off_action: Action,
    ) -> Vec<Rule> {
        use hope_agents::policy::Condition;

        vec![
            Rule::new(
                &format!("{}_turn_on", sensor_name),
                Condition::below(sensor_name, threshold),
                on_action,
            ),
            Rule::new(
                &format!("{}_turn_off", sensor_name),
                Condition::above(sensor_name, threshold),
                off_action,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SmartNodeConfig {
        SmartNodeConfig {
            node_config: Config::test_mode(),
            agent_config: AgentConfig::default(),
            auto_publish_observations: false,
            auto_publish_actions: false,
            observation_retention_secs: 60,
            max_pending_actions: 10,
        }
    }

    #[test]
    fn test_smart_node_creation() {
        let config = test_config();
        let node = SmartNode::new(config).unwrap();
        assert!(node.is_running());
    }

    #[test]
    fn test_smart_node_observe() {
        let config = test_config();
        let mut node = SmartNode::new(config).unwrap();

        let obs = Observation::sensor("temperature", 25.0);
        let result = node.observe(obs);
        assert!(result.is_ok());

        assert_eq!(node.agent_stats().observations_received, 1);
    }

    #[test]
    fn test_smart_node_step() {
        let config = test_config();
        let mut node = SmartNode::new(config).unwrap();

        // Without any rules or observations, step should return None
        let result = node.step().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_sensor_adapter() {
        let adapter = SensorAdapter::with_scaling("temperature", 0.1, -40.0);
        let obs = adapter.reading(650.0); // Raw ADC value

        assert_eq!(obs.value.as_f64().unwrap(), 25.0); // (650 * 0.1) - 40 = 25
    }

    #[test]
    fn test_policy_builder() {
        let rule = IoTPolicyBuilder::threshold_alert("temperature", 30.0, "High temperature!");

        assert!(matches!(rule.action.action_type, ActionType::Alert(_)));
    }

    #[test]
    fn test_smart_node_with_rule() {
        use hope_agents::policy::Condition;

        let config = test_config();
        let mut node = SmartNode::new(config).unwrap();

        // Add a rule: if temp > 30, alert
        let rule = Rule::new(
            "high_temp",
            Condition::above("temperature", 30.0),
            Action::alert("Temperature too high!"),
        );
        node.add_rule(rule);

        // Observe high temperature
        let obs = Observation::sensor("temperature", 35.0);
        node.observe(obs).unwrap();

        // Step should trigger the alert
        let result = node.step().unwrap();
        assert!(result.is_some());

        let action_result = result.unwrap();
        assert!(action_result.success);
    }

    #[test]
    fn test_smart_node_stats() {
        let config = test_config();
        let node = SmartNode::new(config).unwrap();

        let stats = node.stats().unwrap();
        assert_eq!(stats.pending_actions, 0);
        assert_eq!(stats.observation_entries, 0);
    }
}
