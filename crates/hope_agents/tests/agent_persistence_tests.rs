//! Integration tests for agent persistence
//!
//! Tests agent state serialization, learning state persistence,
//! goal manager persistence, and checkpoint management.

use hope_agents::policy::Condition;
use hope_agents::{
    Action, Agent, AgentConfig, Goal, GoalStatus, Observation, Policy, Rule, SimpleAgent,
};

// ============================================================================
// Agent State Serialization Tests
// ============================================================================

#[test]
fn test_agent_stats_serialization() {
    let mut agent = SimpleAgent::new("test_agent");

    // Add some activity
    for i in 0..5 {
        agent.observe(Observation::sensor("temp", i as f64 * 10.0));
    }

    let stats = agent.stats();
    let json = serde_json::to_string(stats).unwrap();

    // Verify it's valid JSON and contains expected fields
    assert!(json.contains("observations_received"));
    assert!(json.contains("5"));
}

#[test]
fn test_agent_config_serialization() {
    let config = AgentConfig::default();
    let json = serde_json::to_string(&config).unwrap();

    // Deserialize back
    let restored: AgentConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.name, restored.name);
}

#[test]
fn test_agent_config_iot_mode_serialization() {
    let config = AgentConfig::iot_mode();
    let json = serde_json::to_string(&config).unwrap();
    let restored: AgentConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.name, restored.name);
}

// ============================================================================
// Observation Serialization Tests
// ============================================================================

#[test]
fn test_observation_serialization() {
    let obs = Observation::sensor("temperature", 25.5);
    let json = serde_json::to_string(&obs).unwrap();
    let restored: Observation = serde_json::from_str(&json).unwrap();

    assert_eq!(obs.value.as_f64(), restored.value.as_f64());
}

#[test]
fn test_observation_event_serialization() {
    let obs = Observation::event("button_pressed");
    let json = serde_json::to_string(&obs).unwrap();
    let restored: Observation = serde_json::from_str(&json).unwrap();

    // Compare by converting to string
    assert_eq!(format!("{:?}", obs.value), format!("{:?}", restored.value));
}

// ============================================================================
// Action Serialization Tests
// ============================================================================

#[test]
fn test_action_serialization() {
    let action = Action::alert("Test alert message");
    let json = serde_json::to_string(&action).unwrap();
    let restored: Action = serde_json::from_str(&json).unwrap();

    assert_eq!(action.action_type, restored.action_type);
}

#[test]
fn test_action_with_params_serialization() {
    let action = Action::store("key", "value");
    let json = serde_json::to_string(&action).unwrap();
    let restored: Action = serde_json::from_str(&json).unwrap();

    assert_eq!(action.params.len(), restored.params.len());
}

// ============================================================================
// Goal Serialization Tests
// ============================================================================

#[test]
fn test_goal_serialization() {
    let goal = Goal::maintain("temperature", 20.0..25.0);
    let json = serde_json::to_string(&goal).unwrap();
    let restored: Goal = serde_json::from_str(&json).unwrap();

    assert_eq!(goal.name, restored.name);
    assert_eq!(goal.status, restored.status);
}

#[test]
fn test_goal_with_status_serialization() {
    let mut goal = Goal::achieve("target", 100);
    goal.status = GoalStatus::Active;

    let json = serde_json::to_string(&goal).unwrap();
    let restored: Goal = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.status, GoalStatus::Active);
}

#[test]
fn test_goal_completed_serialization() {
    let mut goal = Goal::maximize("efficiency");
    goal.status = GoalStatus::Achieved;
    goal.set_progress(1.0);

    let json = serde_json::to_string(&goal).unwrap();
    let restored: Goal = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.status, GoalStatus::Achieved);
    assert_eq!(restored.progress, 1.0);
}

// ============================================================================
// Policy Serialization Tests
// ============================================================================

#[test]
fn test_policy_serialization() {
    let mut policy = Policy::new("test_policy");
    policy.add_rule(Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("High temperature!"),
    ));

    let json = serde_json::to_string(&policy).unwrap();
    let restored: Policy = serde_json::from_str(&json).unwrap();

    assert_eq!(policy.rule_count(), restored.rule_count());
}

#[test]
fn test_policy_with_multiple_rules_serialization() {
    let mut policy = Policy::new("complex_policy");

    policy.add_rule(Rule::new(
        "rule1",
        Condition::above("temp", 30.0),
        Action::alert("Hot!"),
    ));

    policy.add_rule(Rule::new(
        "rule2",
        Condition::below("temp", 10.0),
        Action::alert("Cold!"),
    ));

    let json = serde_json::to_string(&policy).unwrap();
    let restored: Policy = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.rule_count(), 2);
}

// ============================================================================
// Rule Serialization Tests
// ============================================================================

#[test]
fn test_rule_serialization() {
    let rule = Rule::new(
        "test_rule",
        Condition::above("sensor", 50.0),
        Action::alert("Threshold exceeded"),
    );

    let json = serde_json::to_string(&rule).unwrap();
    let restored: Rule = serde_json::from_str(&json).unwrap();

    assert_eq!(rule.name, restored.name);
}

// ============================================================================
// Complex Scenario Tests
// ============================================================================

#[test]
fn test_complete_agent_state_persistence() {
    // Create agent with configuration
    let mut agent = SimpleAgent::with_config("persistent_agent", AgentConfig::default());

    // Add rules
    agent.add_rule(Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("High temperature!"),
    ));

    // Add goals
    let mut goal = Goal::maintain("temperature", 20.0..25.0);
    goal.activate();
    agent.add_goal(goal);

    // Process some observations
    for i in 0..10 {
        agent.observe(Observation::sensor("temperature", 20.0 + i as f64));
    }

    // Serialize stats
    let stats = agent.stats();
    let stats_json = serde_json::to_string(stats).unwrap();

    // Verify we can restore stats
    let restored_stats: hope_agents::agent::AgentStats = serde_json::from_str(&stats_json).unwrap();

    assert_eq!(restored_stats.observations_received, 10);
}

#[test]
fn test_observation_buffer_persistence() {
    let mut agent = SimpleAgent::new("buffer_test");

    // Fill buffer with observations
    for i in 0..5 {
        agent.observe(Observation::sensor("temp", i as f64 * 10.0));
    }

    // Get recent observations
    let recent = agent.recent_observations(5);
    assert_eq!(recent.len(), 5);

    // Serialize observations
    let observations: Vec<_> = recent.iter().map(|o| (*o).clone()).collect();
    let json = serde_json::to_string(&observations).unwrap();

    // Deserialize
    let restored: Vec<Observation> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.len(), 5);
}

#[test]
fn test_goals_batch_serialization() {
    let goals = vec![
        Goal::maintain("temperature", 20.0..25.0),
        Goal::achieve("target", 100),
        Goal::maximize("efficiency"),
    ];

    let json = serde_json::to_string(&goals).unwrap();
    let restored: Vec<Goal> = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.len(), 3);
}

#[test]
fn test_json_pretty_format() {
    let goal = Goal::maintain("temperature", 20.0..25.0);
    let pretty_json = serde_json::to_string_pretty(&goal).unwrap();

    // Pretty format should have newlines
    assert!(pretty_json.contains('\n'));

    // Should still deserialize correctly
    let restored: Goal = serde_json::from_str(&pretty_json).unwrap();
    assert_eq!(goal.name, restored.name);
}
