//! Integration tests for HOPE Agents with Titans Memory
//!
//! These tests verify the complete workflow of memory-enabled agents.

#![cfg(feature = "memory")]

use hope_agents::{
    action::{Action, ActionType},
    agent::Agent,
    config::AgentConfig,
    goal::Goal,
    memory::MemoryAgent,
    observation::Observation,
    policy::{Condition, Rule},
};
use titans_memory::MemoryConfig;

/// Test: Create a memory agent and store observations
#[test]
fn test_memory_agent_observe_and_recall() {
    let mut agent = MemoryAgent::new("integration_test");

    // Store multiple observations
    for i in 0..5 {
        let obs = Observation::sensor("temperature", 20.0 + i as f64);
        agent.observe(obs);
    }

    // Verify memory stats
    let stats = agent.memory_stats();
    assert_eq!(stats.stm_count, 5);

    // Recall observations
    let dummy_obs = Observation::sensor("temperature", 22.0);
    let recalled = agent.recall_similar(&dummy_obs, 3);
    assert!(!recalled.is_empty());
    assert!(recalled.len() <= 3);
}

/// Test: Memory agent with custom configuration
#[test]
fn test_memory_agent_custom_config() {
    let agent_config = AgentConfig::iot_mode();
    let memory_config = MemoryConfig::iot_mode();

    let agent = MemoryAgent::with_config("custom_agent", agent_config, memory_config);

    assert_eq!(agent.name(), "custom_agent");
    assert_eq!(agent.memory_stats().stm_count, 0);
}

/// Test: Action execution and memory
#[test]
fn test_memory_agent_action_history() {
    let mut agent = MemoryAgent::new("action_test");

    // Execute some actions
    let action1 = Action::store("key1", "value1");
    let result1 = agent.execute(action1);
    assert!(result1.success);

    let action2 = Action::alert("Test alert");
    let result2 = agent.execute(action2);
    assert!(result2.success);

    // Recall past actions
    let past_actions = agent.recall_past_actions(5);
    assert_eq!(past_actions.len(), 2);
}

/// Test: Complete agent loop with memory
#[test]
fn test_complete_agent_loop() {
    let mut agent = MemoryAgent::new("loop_test");

    // Simulate multiple iterations of the agent loop
    for i in 0..10 {
        // 1. Observe
        let temp = 20.0 + (i % 5) as f64;
        let obs = Observation::sensor("temperature", temp);
        agent.observe(obs);

        // 2. Decide
        let action = agent.decide();

        // 3. Execute
        let result = agent.execute(action.clone());

        // 4. Learn
        let obs_for_learn = Observation::sensor("temperature", temp);
        agent.learn(&obs_for_learn, &action, &result);
    }

    // Verify statistics
    let stats = agent.memory_stats();
    assert!(stats.stm_count >= 10); // At least 10 observations stored
}

/// Test: Memory consolidation
#[test]
fn test_memory_consolidation() {
    let mut agent = MemoryAgent::new("consolidation_test");

    // Add many observations
    for i in 0..20 {
        let obs = Observation::sensor("humidity", 50.0 + i as f64);
        agent.observe(obs);
    }

    // Run maintenance (includes consolidation)
    let result = agent.maintenance();
    assert!(result.is_ok());

    // Memory should still function
    let stats = agent.memory_stats();
    assert!(stats.stm_count > 0);
}

/// Test: Agent with goals and memory
#[test]
fn test_memory_agent_with_goals() {
    let agent_config = AgentConfig::ai_mode();
    let memory_config = MemoryConfig::agent_mode();

    let mut agent = MemoryAgent::with_config("goal_agent", agent_config, memory_config);

    // This test verifies that MemoryAgent properly wraps SimpleAgent
    // Goals would be set on the inner agent (not directly accessible via trait)

    // Run observations
    agent.observe(Observation::sensor("energy", 75.0));
    let action = agent.decide();
    let result = agent.execute(action);

    assert!(result.success);
}

/// Test: Sensor data workflow
#[test]
fn test_iot_sensor_workflow() {
    let mut agent = MemoryAgent::new("iot_sensor");

    // Simulate IoT sensor readings
    let sensors = ["temp", "humidity", "pressure", "light"];
    let readings = [23.5, 65.0, 1013.25, 450.0];

    for _ in 0..5 {
        for (sensor, &reading) in sensors.iter().zip(readings.iter()) {
            let obs = Observation::sensor(sensor, reading);
            agent.observe(obs);
        }
    }

    // Verify all readings were stored
    let stats = agent.memory_stats();
    assert_eq!(stats.stm_count, 20); // 4 sensors * 5 iterations
}

/// Test: State change observations
#[test]
fn test_state_change_observations() {
    let mut agent = MemoryAgent::new("state_observer");

    // Observe state changes
    agent.observe(Observation::state_change("door", "open"));
    agent.observe(Observation::state_change("door", "closed"));
    agent.observe(Observation::state_change("light", "on"));

    let stats = agent.memory_stats();
    assert_eq!(stats.stm_count, 3);
}

/// Test: Event observations
#[test]
fn test_event_observations() {
    let mut agent = MemoryAgent::new("event_observer");

    agent.observe(Observation::event("button_press"));
    agent.observe(Observation::event("motion_detected"));
    agent.observe(Observation::error(
        "sensor_timeout",
        "Temperature sensor not responding",
    ));

    let stats = agent.memory_stats();
    assert_eq!(stats.stm_count, 3);
}

/// Test: Memory stats tracking
#[test]
fn test_memory_stats_accuracy() {
    let mut agent = MemoryAgent::new("stats_test");

    // Initial state
    assert_eq!(agent.memory_stats().stm_count, 0);

    // Add observations
    for i in 0..10 {
        agent.observe(Observation::sensor("test", i as f64));
    }
    assert_eq!(agent.memory_stats().stm_count, 10);

    // Add actions
    for _ in 0..5 {
        let action = Action::wait();
        agent.execute(action);
    }
    assert_eq!(agent.memory_stats().stm_count, 15); // 10 obs + 5 actions
}
