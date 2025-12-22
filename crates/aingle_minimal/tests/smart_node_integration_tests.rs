//! Integration tests for SmartNode pipeline
//!
//! Tests the complete flow: Sensor → Observation → Agent → Action → DAG
//! These tests verify the integration between MinimalNode and HOPE Agents.
//!
//! Requires the `smart_agents` feature to be enabled.

#![cfg(feature = "smart_agents")]

use aingle_minimal::*;
use hope_agents::policy::Condition;
use hope_agents::{
    Action, ActionType, AgentConfig, Goal, Observation, ObservationType, Policy, Rule,
};

/// Helper to create test configuration
fn test_smart_config() -> SmartNodeConfig {
    SmartNodeConfig {
        node_config: Config::iot_mode(), // Use iot_mode for tests
        agent_config: AgentConfig::default(),
        auto_publish_observations: true,
        auto_publish_actions: true,
        observation_retention_secs: 60,
        max_pending_actions: 10,
    }
}

/// Helper to create low-power test configuration
fn low_power_config() -> SmartNodeConfig {
    SmartNodeConfig {
        node_config: Config::low_power(),
        agent_config: AgentConfig::iot_mode(),
        auto_publish_observations: false,
        auto_publish_actions: false,
        observation_retention_secs: 30,
        max_pending_actions: 5,
    }
}

// ============================================================================
// SmartNode Creation Tests
// ============================================================================

#[test]
fn test_smart_node_creation_default() {
    let config = test_smart_config();
    let node = SmartNode::new(config).unwrap();

    assert!(node.is_running());
    assert_eq!(node.action_history().len(), 0);
}

#[test]
fn test_smart_node_creation_iot_mode() {
    let config = SmartNodeConfig::iot_mode();
    let node = SmartNode::new(config).unwrap();

    assert!(node.is_running());
    let stats = node.stats().unwrap();
    assert_eq!(stats.pending_actions, 0);
    assert_eq!(stats.observation_entries, 0);
}

#[test]
fn test_smart_node_creation_low_power() {
    let config = SmartNodeConfig::low_power();
    let node = SmartNode::new(config).unwrap();

    assert!(node.is_running());
}

#[test]
fn test_smart_node_with_custom_agent() {
    use hope_agents::SimpleAgent;

    let mut agent = SimpleAgent::new("custom_agent");
    agent.add_rule(Rule::new(
        "test_rule",
        Condition::above("temperature", 30.0),
        Action::alert("High temp!"),
    ));

    let config = test_smart_config();
    let node = SmartNode::with_agent(config, agent).unwrap();

    assert!(node.is_running());
}

// ============================================================================
// Observation Flow Tests
// ============================================================================

#[test]
fn test_observation_to_agent() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let obs = Observation::sensor("temperature", 25.0);
    let result = node.observe(obs);

    assert!(result.is_ok());
    assert_eq!(node.agent_stats().observations_received, 1);
}

#[test]
fn test_observation_published_to_dag() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let obs = Observation::sensor("humidity", 65.0);
    let result = node.observe(obs).unwrap();

    // With auto_publish_observations = true, hash should be returned
    assert!(result.is_some());
    let hash = result.unwrap();
    assert!(!hash.to_string().is_empty());

    // Stats should reflect the observation entry
    let stats = node.stats().unwrap();
    assert_eq!(stats.observation_entries, 1);
}

#[test]
fn test_observation_not_published_in_low_power() {
    let config = low_power_config();
    let mut node = SmartNode::new(config).unwrap();

    let obs = Observation::sensor("temperature", 25.0);
    let result = node.observe(obs).unwrap();

    // With auto_publish_observations = false, no hash
    assert!(result.is_none());
    assert_eq!(node.agent_stats().observations_received, 1);
}

#[test]
fn test_batch_observations() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let observations = vec![
        Observation::sensor("temperature", 22.0),
        Observation::sensor("humidity", 55.0),
        Observation::sensor("pressure", 1013.25),
    ];

    let hashes = node.observe_batch(observations).unwrap();

    assert_eq!(hashes.len(), 3);
    assert_eq!(node.agent_stats().observations_received, 3);
    assert_eq!(node.stats().unwrap().observation_entries, 3);
}

#[test]
fn test_recent_observations_tracking() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    for i in 0..5 {
        let obs = Observation::sensor("temp", i as f64 * 10.0);
        node.observe(obs).unwrap();
    }

    let recent = node.recent_observations(3);
    assert_eq!(recent.len(), 3);
}

// ============================================================================
// Agent Decision Making Tests
// ============================================================================

#[test]
fn test_agent_decision_without_rules() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let obs = Observation::sensor("temperature", 25.0);
    node.observe(obs).unwrap();

    // Without rules, step should return None (noop)
    let result = node.step().unwrap();
    assert!(result.is_none());
}

#[test]
fn test_agent_decision_with_rule_triggered() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Add a rule: if temp > 30, alert
    let rule = Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("Temperature too high!"),
    );
    node.add_rule(rule);

    // Observe high temperature (above threshold)
    let obs = Observation::sensor("temperature", 35.0);
    node.observe(obs).unwrap();

    // Step should trigger the alert
    let result = node.step().unwrap();
    assert!(result.is_some());

    let action_result = result.unwrap();
    assert!(action_result.success);
}

#[test]
fn test_agent_decision_with_rule_not_triggered() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Add a rule: if temp > 30, alert
    let rule = Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("Temperature too high!"),
    );
    node.add_rule(rule);

    // Observe normal temperature (below threshold)
    let obs = Observation::sensor("temperature", 25.0);
    node.observe(obs).unwrap();

    // Step should return None (rule not triggered)
    let result = node.step().unwrap();
    assert!(result.is_none());
}

#[test]
fn test_agent_with_multiple_rules() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Rule 1: High temp alert
    node.add_rule(Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("High temperature!"),
    ));

    // Rule 2: Low temp alert
    node.add_rule(Rule::new(
        "low_temp",
        Condition::below("temperature", 10.0),
        Action::alert("Low temperature!"),
    ));

    // Test high temp
    node.observe(Observation::sensor("temperature", 35.0))
        .unwrap();
    let result1 = node.step().unwrap();
    assert!(result1.is_some());

    // Test low temp
    node.observe(Observation::sensor("temperature", 5.0))
        .unwrap();
    let result2 = node.step().unwrap();
    assert!(result2.is_some());
}

// ============================================================================
// Action Execution Tests
// ============================================================================

#[test]
fn test_execute_alert_action() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let action = Action::alert("Test alert message");
    let result = node.execute_action(action).unwrap();

    assert!(result.success);
    assert_eq!(node.action_history().len(), 1);
}

#[test]
fn test_execute_publish_action() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let action = Action::new(ActionType::Publish("sensor_data".to_string()));
    let result = node.execute_action(action).unwrap();

    assert!(result.success);
    // Value should contain the hash
    assert!(result.value.is_some());
}

#[test]
fn test_execute_store_action() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let action = Action::store("config_key", "config_value");
    let result = node.execute_action(action).unwrap();

    assert!(result.success);
}

#[test]
fn test_action_history_tracking() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Execute multiple actions
    for i in 0..5 {
        let action = Action::alert(&format!("Alert {}", i));
        node.execute_action(action).unwrap();
    }

    assert_eq!(node.action_history().len(), 5);

    // Verify history content
    for (action, result) in node.action_history() {
        assert!(matches!(action.action_type, ActionType::Alert(_)));
        assert!(result.success);
    }
}

#[test]
fn test_action_history_max_limit() {
    let mut config = test_smart_config();
    config.max_pending_actions = 3;
    let mut node = SmartNode::new(config).unwrap();

    // Execute more actions than max limit
    for i in 0..10 {
        let action = Action::alert(&format!("Alert {}", i));
        node.execute_action(action).unwrap();
    }

    // History should be capped at max_pending_actions
    assert_eq!(node.action_history().len(), 3);
}

#[test]
fn test_clear_action_history() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    node.execute_action(Action::alert("Test")).unwrap();
    assert_eq!(node.action_history().len(), 1);

    node.clear_history();
    assert_eq!(node.action_history().len(), 0);
}

// ============================================================================
// Complete Pipeline Tests
// ============================================================================

#[test]
fn test_complete_pipeline_sensor_to_dag() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Setup rule
    node.add_rule(Rule::new(
        "critical_temp",
        Condition::above("temperature", 40.0),
        Action::alert("CRITICAL: Temperature exceeds 40°C!"),
    ));

    // Step 1: Observe sensor data
    let obs = Observation::sensor("temperature", 45.0);
    let obs_hash = node.observe(obs).unwrap();
    assert!(obs_hash.is_some()); // Observation published to DAG

    // Step 2: Agent decides on action
    let result = node.step().unwrap();
    assert!(result.is_some()); // Alert action triggered

    // Step 3: Verify stats
    let stats = node.stats().unwrap();
    assert_eq!(stats.agent_stats.observations_received, 1);
    assert_eq!(stats.observation_entries, 1);
    assert!(stats.action_history_len > 0);
}

#[test]
fn test_multi_step_workflow() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Setup threshold rules
    node.add_rule(Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("High temperature!"),
    ));

    // Simulate 10 sensor readings
    for i in 0..10 {
        let temp = 20.0 + (i as f64 * 2.0); // 20, 22, 24, ..., 38
        let obs = Observation::sensor("temperature", temp);
        node.observe(obs).unwrap();

        // Process decision
        let _ = node.step();
    }

    // Verify all observations received
    assert_eq!(node.agent_stats().observations_received, 10);

    // Some actions should have been triggered (temps > 30)
    assert!(!node.action_history().is_empty());
}

// ============================================================================
// Policy Tests
// ============================================================================

#[test]
fn test_add_policy_to_smart_node() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let mut policy = Policy::new("temperature_policy");
    policy.add_rule(Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("Hot!"),
    ));
    policy.add_rule(Rule::new(
        "low_temp",
        Condition::below("temperature", 10.0),
        Action::alert("Cold!"),
    ));

    node.add_policy(policy);

    // Test the policy works
    node.observe(Observation::sensor("temperature", 35.0))
        .unwrap();
    let result = node.step().unwrap();
    assert!(result.is_some());
}

#[test]
fn test_iot_policy_builder_threshold() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let rule = IoTPolicyBuilder::threshold_alert("humidity", 80.0, "High humidity warning!");
    node.add_rule(rule);

    node.observe(Observation::sensor("humidity", 85.0)).unwrap();
    let result = node.step().unwrap();
    assert!(result.is_some());
}

#[test]
fn test_iot_policy_builder_range() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    let rules = IoTPolicyBuilder::maintain_range(
        "temperature",
        18.0,
        26.0,
        Action::new(ActionType::Custom("heater_on".to_string())),
        Action::new(ActionType::Custom("cooler_on".to_string())),
    );

    for rule in rules {
        node.add_rule(rule);
    }

    // Test below minimum
    node.observe(Observation::sensor("temperature", 15.0))
        .unwrap();
    let result = node.step().unwrap();
    assert!(result.is_some());
}

// ============================================================================
// Goal Tests
// ============================================================================

#[test]
fn test_add_goal_to_smart_node() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Goals are added with Pending status, need to activate to become "active"
    let mut goal = Goal::maintain("temperature", 20.0..25.0);
    goal.activate(); // Make it active before adding
    node.add_goal(goal);

    let active_goals = node.active_goals();
    assert!(!active_goals.is_empty());
    assert_eq!(active_goals.len(), 1);
}

// ============================================================================
// State Management Tests
// ============================================================================

#[test]
fn test_smart_node_pause_resume() {
    use hope_agents::AgentState;

    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    assert!(node.is_running());
    // Agent starts in Idle state
    assert!(matches!(
        node.agent_state(),
        AgentState::Idle | AgentState::Initializing
    ));

    node.pause();
    // Paused is still considered "running" (not Stopped or Error)
    assert!(node.is_running());
    assert!(matches!(node.agent_state(), AgentState::Paused));

    node.resume();
    assert!(node.is_running());
    // After resume, goes back to Idle
    assert!(matches!(node.agent_state(), AgentState::Idle));
}

#[test]
fn test_smart_node_stop() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    assert!(node.is_running());

    node.stop();
    assert!(!node.is_running());
}

// ============================================================================
// SensorAdapter Tests
// ============================================================================

#[test]
fn test_sensor_adapter_basic() {
    let adapter = SensorAdapter::new("temperature");
    let obs = adapter.reading(25.0);

    assert_eq!(obs.value.as_f64().unwrap(), 25.0);
}

#[test]
fn test_sensor_adapter_with_scaling() {
    // Simulate ADC conversion: raw_value * 0.1 - 40 = actual_temp
    let adapter = SensorAdapter::with_scaling("temperature", 0.1, -40.0);

    // Raw ADC value 650 -> (650 * 0.1) - 40 = 25°C
    let obs = adapter.reading(650.0);
    assert_eq!(obs.value.as_f64().unwrap(), 25.0);

    // Raw ADC value 1000 -> (1000 * 0.1) - 40 = 60°C
    let obs2 = adapter.reading(1000.0);
    assert_eq!(obs2.value.as_f64().unwrap(), 60.0);
}

#[test]
fn test_sensor_adapter_boolean() {
    let adapter = SensorAdapter::new("motion");

    let on = adapter.boolean(true);
    assert_eq!(on.value.as_f64().unwrap(), 1.0);

    let off = adapter.boolean(false);
    assert_eq!(off.value.as_f64().unwrap(), 0.0);
}

#[test]
fn test_sensor_adapter_event() {
    let adapter = SensorAdapter::new("button_press");
    let obs = adapter.event();

    // Event observations have Custom type
    assert!(matches!(obs.obs_type, ObservationType::Custom(_)));
}

// ============================================================================
// Statistics Tests
// ============================================================================

#[test]
fn test_combined_statistics() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Add some observations and actions
    for _ in 0..5 {
        node.observe(Observation::sensor("temp", 25.0)).unwrap();
    }
    node.execute_action(Action::alert("Test")).unwrap();

    let stats = node.stats().unwrap();

    // Agent stats
    assert_eq!(stats.agent_stats.observations_received, 5);

    // Node stats should be valid
    assert!(stats.node_stats.uptime_secs >= 0);

    // Integration stats
    assert_eq!(stats.observation_entries, 5);
    assert_eq!(stats.action_history_len, 1);
}

#[test]
fn test_node_access() {
    let config = test_smart_config();
    let mut node = SmartNode::new(config).unwrap();

    // Access underlying node
    let node_stats = node.node_stats().unwrap();
    assert!(node_stats.uptime_secs >= 0);

    // Access underlying agent
    let agent_stats = node.agent_stats();
    assert_eq!(agent_stats.observations_received, 0);

    // Mutable access
    let _node_mut = node.node_mut();
    let _agent_mut = node.agent_mut();
}
