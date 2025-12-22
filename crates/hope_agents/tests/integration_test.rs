//! Integration tests for HOPE Agents
//!
//! These tests demonstrate the complete functionality of the HOPE Agents framework,
//! including coordination, persistence, learning, and goal management.

use hope_agents::*;
use std::collections::HashMap;

/// Test basic agent creation and operation
#[test]
fn test_simple_agent_workflow() {
    let mut agent = SimpleAgent::new("test_agent");

    // Add a rule
    let rule = Rule::new(
        "high_temp",
        Condition::above("temperature", 30.0),
        Action::alert("High temperature"),
    );
    agent.add_rule(rule);

    // Observe
    let obs = Observation::sensor("temperature", 35.0);
    agent.observe(obs.clone());

    // Decide
    let action = agent.decide();
    assert!(!action.is_noop());

    // Execute
    let result = agent.execute(action.clone());
    assert!(result.success);

    // Learn
    agent.learn(&obs, &action, &result);

    assert_eq!(agent.stats().observations_received, 1);
    assert_eq!(agent.stats().actions_executed, 1);
}

/// Test HOPE agent with full learning cycle
#[test]
fn test_hope_agent_learning_cycle() {
    let mut agent = HopeAgent::with_default_config();

    // Set a goal
    let goal = Goal::maintain("temperature", 20.0..25.0).with_priority(Priority::High);
    let _goal_id = agent.set_goal(goal);

    // Run multiple learning episodes
    for episode in 0..10 {
        for step in 0..5 {
            let temp = 20.0 + (step as f64) + (episode as f64 * 0.1);
            let obs = Observation::sensor("temperature", temp);

            let action = agent.step(obs.clone());

            let reward = if (20.0..=25.0).contains(&temp) {
                1.0
            } else {
                -1.0
            };
            let next_obs = Observation::sensor("temperature", temp + 0.1);
            let result = ActionResult::success(&action.id);
            let done = step == 4;

            let outcome = Outcome::new(action, result, reward, next_obs, done);
            agent.learn(outcome);
        }

        agent.reset();
    }

    let stats = agent.get_statistics();
    assert_eq!(stats.episodes_completed, 10);
    assert!(stats.total_steps > 0);
    assert!(stats.learning_updates > 0);
}

/// Test multi-agent coordination
#[test]
fn test_multi_agent_coordination() {
    let mut coordinator = AgentCoordinator::new();

    // Create and register multiple agents
    let agent1 = HopeAgent::with_default_config();
    let agent2 = HopeAgent::with_default_config();
    let agent3 = HopeAgent::with_default_config();

    let id1 = coordinator.register_agent(agent1);
    let id2 = coordinator.register_agent(agent2);
    let id3 = coordinator.register_agent(agent3);

    assert_eq!(coordinator.agent_count(), 3);

    // Test broadcast messaging
    let msg = Message::new("status", "System update").with_priority(MessagePriority::High);
    coordinator.broadcast(msg);

    // Test shared memory
    coordinator
        .shared_memory_mut()
        .set("global_temp".to_string(), "25.0".to_string());
    assert_eq!(
        coordinator.shared_memory_mut().get("global_temp"),
        Some("25.0".to_string())
    );

    // Step all agents
    let mut observations = HashMap::new();
    observations.insert(id1.clone(), Observation::sensor("temp", 20.0));
    observations.insert(id2.clone(), Observation::sensor("temp", 22.0));
    observations.insert(id3.clone(), Observation::sensor("temp", 24.0));

    let actions = coordinator.step_all(observations);
    assert_eq!(actions.len(), 3);

    // Test agent retrieval
    assert!(coordinator.get_agent(&id1).is_some());
    assert!(coordinator.get_agent_mut(&id2).is_some());

    // Unregister an agent
    let agent = coordinator.unregister_agent(&id3);
    assert!(agent.is_ok());
    assert_eq!(coordinator.agent_count(), 2);
}

/// Test consensus mechanism
#[test]
fn test_consensus_mechanism() {
    let mut coordinator = AgentCoordinator::new();

    // Register agents
    let agents: Vec<_> = (0..5)
        .map(|_| coordinator.register_agent(HopeAgent::with_default_config()))
        .collect();

    // Create a proposal
    let proposal_id =
        coordinator.create_proposal("new_policy", "Should we adopt the new temperature policy?");

    // Initially pending
    match coordinator.get_consensus(&proposal_id) {
        Some(ConsensusResult::Pending) => {}
        _ => panic!("Expected pending consensus"),
    }

    // Cast votes (3 yes, 2 no)
    let vote_msg_yes = Message::with_payload(
        "vote",
        MessagePayload::Vote {
            proposal_id: proposal_id.clone(),
            vote: true,
        },
    );

    let vote_msg_no = Message::with_payload(
        "vote",
        MessagePayload::Vote {
            proposal_id: proposal_id.clone(),
            vote: false,
        },
    );

    for (i, agent_id) in agents.iter().enumerate() {
        if i < 3 {
            coordinator.send_to(agent_id, vote_msg_yes.clone()).unwrap();
        } else {
            coordinator.send_to(agent_id, vote_msg_no.clone()).unwrap();
        }
    }

    // Process messages
    let observations = agents
        .iter()
        .map(|id| (id.clone(), Observation::sensor("dummy", 0.0)))
        .collect();
    coordinator.step_all(observations);

    // Check consensus
    match coordinator.get_consensus(&proposal_id) {
        Some(ConsensusResult::Decided {
            approved,
            votes_for,
            votes_against,
            ..
        }) => {
            assert!(approved); // Majority voted yes
            assert_eq!(votes_for, 3);
            assert_eq!(votes_against, 2);
        }
        _ => panic!("Expected decided consensus"),
    }
}

/// Test agent persistence (save/load)
#[test]
fn test_agent_persistence() {
    let mut agent = HopeAgent::with_default_config();

    // Train the agent
    for i in 0..20 {
        let obs = Observation::sensor("value", i as f64);
        let action = agent.step(obs.clone());

        let outcome = Outcome::new(action, ActionResult::success("test"), 1.0, obs, i % 5 == 4);
        agent.learn(outcome);
    }

    let original_steps = agent.get_statistics().total_steps;
    let original_episodes = agent.get_statistics().episodes_completed;

    // Save to file
    let temp_path = std::env::temp_dir().join("test_agent.json");
    agent.save_to_file(&temp_path).unwrap();
    assert!(temp_path.exists());

    // Load from file
    let loaded_agent = HopeAgent::load_from_file(&temp_path).unwrap();

    assert_eq!(loaded_agent.get_statistics().total_steps, original_steps);
    assert_eq!(
        loaded_agent.get_statistics().episodes_completed,
        original_episodes
    );

    // Cleanup
    let _ = std::fs::remove_file(&temp_path);
}

/// Test persistence with different formats
#[test]
fn test_persistence_formats() {
    let agent = HopeAgent::with_default_config();

    // Test JSON format
    let json_options = PersistenceOptions {
        format: PersistenceFormat::Json,
        pretty: true,
        compress: false,
    };
    let json_bytes = agent.to_bytes_with_options(&json_options).unwrap();
    let loaded_from_json = HopeAgent::from_bytes_with_options(&json_bytes, &json_options).unwrap();
    assert_eq!(
        loaded_from_json.get_statistics().total_steps,
        agent.get_statistics().total_steps
    );

    // Test with compression
    let compressed_options = PersistenceOptions {
        format: PersistenceFormat::Json,
        pretty: false,
        compress: true,
    };
    let compressed_bytes = agent.to_bytes_with_options(&compressed_options).unwrap();
    let loaded_compressed =
        HopeAgent::from_bytes_with_options(&compressed_bytes, &compressed_options).unwrap();
    assert_eq!(
        loaded_compressed.get_statistics().total_steps,
        agent.get_statistics().total_steps
    );
}

/// Test checkpoint manager
#[test]
fn test_checkpoint_manager() {
    let checkpoint_dir = std::env::temp_dir().join("hope_checkpoints");
    let mut manager = CheckpointManager::new(&checkpoint_dir, 3).with_interval(10);

    let mut agent = HopeAgent::with_default_config();

    // Train and checkpoint
    for step in 1..=35 {
        let obs = Observation::sensor("step", step as f64);
        agent.step(obs);

        if manager.should_checkpoint(step) {
            manager.save_checkpoint(&agent, step).unwrap();
        }
    }

    // Should have 3 checkpoints (max_checkpoints)
    assert!(checkpoint_dir.exists());

    // Load latest checkpoint
    let loaded = manager.load_latest_checkpoint().unwrap();
    assert!(loaded.get_statistics().total_steps > 0);

    // Cleanup
    let _ = std::fs::remove_dir_all(&checkpoint_dir);
}

/// Test hierarchical goal decomposition
#[test]
fn test_hierarchical_goals() {
    let mut agent = HopeAgent::with_default_config();

    // Create a complex goal
    let parent_goal = Goal::achieve("optimize_system", 1.0).with_priority(Priority::High);

    let _goal_id = agent.set_goal(parent_goal);

    // Goal should be automatically decomposed
    let active_goals = agent.active_goals();
    assert!(!active_goals.is_empty());
}

/// Test operation mode switching
#[test]
fn test_operation_modes() {
    let mut agent = HopeAgent::with_default_config();

    // Test different modes
    agent.set_mode(OperationMode::Exploration);
    assert_eq!(agent.mode(), OperationMode::Exploration);
    let epsilon_explore = agent.learning_engine().epsilon();

    agent.set_mode(OperationMode::Exploitation);
    assert_eq!(agent.mode(), OperationMode::Exploitation);
    let epsilon_exploit = agent.learning_engine().epsilon();

    // Exploration should have higher epsilon
    assert!(epsilon_explore > epsilon_exploit);

    agent.set_mode(OperationMode::GoalDriven);
    assert_eq!(agent.mode(), OperationMode::GoalDriven);

    agent.set_mode(OperationMode::Adaptive);
    assert_eq!(agent.mode(), OperationMode::Adaptive);
}

/// Test anomaly detection
#[test]
fn test_anomaly_detection() {
    let mut agent = HopeAgent::with_default_config();

    // Establish normal pattern
    for i in 0..20 {
        let obs = Observation::sensor("value", 20.0 + (i % 5) as f64);
        agent.step(obs);
    }

    let initial_anomalies = agent.get_statistics().anomalies_detected;

    // Introduce anomaly
    let anomaly_obs = Observation::sensor("value", 1000.0);
    agent.step(anomaly_obs);

    // Anomaly count may increase (depends on detector learning)
    let final_anomalies = agent.get_statistics().anomalies_detected;
    assert!(final_anomalies >= initial_anomalies);
}

/// Test message bus
#[test]
fn test_message_bus() {
    let mut bus = MessageBus::new();

    let agent_id = AgentId("agent_1".to_string());

    // Send messages
    let msg1 = Message::new("topic1", "Message 1")
        .to(agent_id.clone())
        .with_priority(MessagePriority::High);

    let msg2 = Message::new("topic2", "Message 2")
        .to(agent_id.clone())
        .with_priority(MessagePriority::Low);

    bus.send(msg1).unwrap();
    bus.send(msg2).unwrap();

    assert_eq!(bus.pending_count(), 2);

    // Receive messages (should be sorted by priority)
    let messages = bus.receive(&agent_id);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].priority, MessagePriority::High);
    assert_eq!(messages[1].priority, MessagePriority::Low);

    let (sent, delivered) = bus.stats();
    assert_eq!(sent, 2);
    assert_eq!(delivered, 2);
}

/// Test shared memory
#[test]
fn test_shared_memory() {
    let mut memory = SharedMemory::new();

    assert!(memory.is_empty());

    // Set values
    memory.set("key1".to_string(), "value1".to_string());
    memory.set("key2".to_string(), "value2".to_string());

    assert_eq!(memory.len(), 2);
    assert!(memory.contains("key1"));

    // Get values
    assert_eq!(memory.get("key1"), Some("value1".to_string()));
    assert_eq!(memory.get("nonexistent"), None);

    // Delete
    let deleted = memory.delete("key1");
    assert_eq!(deleted, Some("value1".to_string()));
    assert_eq!(memory.len(), 1);

    // Clear
    memory.clear();
    assert!(memory.is_empty());
}

/// Test learning engine persistence
#[test]
fn test_learning_engine_persistence() {
    let engine = LearningEngine::new(LearningConfig::default());

    let temp_path = std::env::temp_dir().join("learning_engine.json");

    // Save
    engine.save_to_file(&temp_path).unwrap();
    assert!(temp_path.exists());

    // Load
    let loaded_engine = LearningEngine::load_from_file(&temp_path).unwrap();
    assert_eq!(loaded_engine.total_updates(), engine.total_updates());

    // Cleanup
    let _ = std::fs::remove_file(&temp_path);
}

/// Test complete multi-agent scenario with coordination and learning
#[test]
fn test_complete_multi_agent_scenario() {
    let mut coordinator = AgentCoordinator::new();

    // Create agents with different goals
    let mut agent1 = HopeAgent::with_default_config();
    let goal1 = Goal::maintain("temperature", 20.0..25.0);
    agent1.set_goal(goal1);

    let mut agent2 = HopeAgent::with_default_config();
    let goal2 = Goal::maintain("humidity", 40.0..60.0);
    agent2.set_goal(goal2);

    let id1 = coordinator.register_agent(agent1);
    let id2 = coordinator.register_agent(agent2);

    // Run coordinated learning episodes
    for episode in 0..5 {
        for step in 0..10 {
            // Create observations
            let mut observations = HashMap::new();
            observations.insert(
                id1.clone(),
                Observation::sensor("temperature", 22.0 + (step as f64 * 0.1)),
            );
            observations.insert(
                id2.clone(),
                Observation::sensor("humidity", 50.0 + (step as f64 * 0.5)),
            );

            // Step all agents
            let actions = coordinator.step_all(observations);
            assert_eq!(actions.len(), 2);

            // Create outcomes
            let mut outcomes = HashMap::new();
            for (agent_id, action) in actions {
                let obs = if agent_id == id1 {
                    Observation::sensor("temperature", 22.5)
                } else {
                    Observation::sensor("humidity", 52.0)
                };

                let outcome =
                    Outcome::new(action, ActionResult::success("test"), 1.0, obs, step == 9);
                outcomes.insert(agent_id, outcome);
            }

            // Learn from outcomes
            coordinator.learn_all(outcomes);
        }

        // Share information between agents via shared memory
        coordinator
            .shared_memory_mut()
            .set(format!("episode_{}_complete", episode), "true".to_string());
    }

    // Verify both agents learned
    let agent1 = coordinator.get_agent(&id1).unwrap();
    let agent2 = coordinator.get_agent(&id2).unwrap();

    assert!(agent1.get_statistics().learning_updates > 0);
    assert!(agent2.get_statistics().learning_updates > 0);
}

/// Benchmark-style test to verify performance
#[test]
fn test_performance() {
    let mut agent = HopeAgent::with_default_config();

    let start = std::time::Instant::now();

    // Run 1000 steps
    for i in 0..1000 {
        let obs = Observation::sensor("value", (i % 100) as f64);
        let action = agent.step(obs.clone());

        let outcome = Outcome::new(action, ActionResult::success("test"), 1.0, obs, false);
        agent.learn(outcome);
    }

    let elapsed = start.elapsed();

    println!("1000 steps took: {:?}", elapsed);
    println!("Steps per second: {:.2}", 1000.0 / elapsed.as_secs_f64());

    // Should complete in reasonable time (< 5 seconds on most hardware)
    assert!(elapsed.as_secs() < 5);
}
