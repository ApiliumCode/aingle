//! Complete HOPE Agents Demonstration
//!
//! This example demonstrates all major features of the HOPE Agents framework:
//! - Learning with different algorithms
//! - Hierarchical goal management
//! - Multi-agent coordination
//! - State persistence
//! - Anomaly detection
//!
//! Run with: cargo run --example complete_demo

use hope_agents::*;
use std::collections::HashMap;

fn main() {
    println!("=== HOPE Agents Complete Demo ===\n");

    demo_simple_agent();
    demo_learning_agent();
    demo_hierarchical_goals();
    demo_multi_agent_coordination();
    demo_persistence();
    demo_anomaly_detection();

    println!("\n=== Demo Complete ===");
}

/// Demonstrate simple reactive agent
fn demo_simple_agent() {
    println!("--- 1. Simple Reactive Agent ---");

    let mut agent = SimpleAgent::new("temperature_monitor");

    // Add rules for different temperature ranges
    agent.add_rule(Rule::new(
        "too_hot",
        Condition::above("temperature", 30.0),
        Action::alert("Temperature too high!"),
    ));

    agent.add_rule(Rule::new(
        "too_cold",
        Condition::below("temperature", 15.0),
        Action::alert("Temperature too low!"),
    ));

    // Simulate observations
    let observations = vec![
        ("normal", 22.0),
        ("hot", 35.0),
        ("cold", 10.0),
        ("normal", 20.0),
    ];

    for (label, temp) in observations {
        let obs = Observation::sensor("temperature", temp);
        agent.observe(obs.clone());

        let action = agent.decide();
        println!(
            "  {} ({}°C) -> Action: {:?}",
            label, temp, action.action_type
        );

        let result = agent.execute(action.clone());
        agent.learn(&obs, &action, &result);
    }

    println!(
        "  Stats: {} observations, {} actions\n",
        agent.stats().observations_received,
        agent.stats().actions_executed
    );
}

/// Demonstrate learning agent with Q-Learning
fn demo_learning_agent() {
    println!("--- 2. Learning Agent (Q-Learning) ---");

    let mut agent = HopeAgent::with_default_config();

    // Set a goal
    let goal = Goal::maintain("temperature", 20.0..25.0).with_priority(Priority::High);
    agent.set_goal(goal);

    println!("  Training for 10 episodes...");

    // Training loop
    for episode in 0..10 {
        let mut episode_reward = 0.0;

        for step in 0..20 {
            // Simulate temperature that drifts
            let temp = 20.0 + (step as f64 * 0.5) + (rand::random::<f64>() * 2.0 - 1.0);
            let obs = Observation::sensor("temperature", temp);

            let action = agent.step(obs.clone());

            // Calculate reward based on how close to target range
            let reward = if (20.0..=25.0).contains(&temp) {
                1.0
            } else {
                -((temp - 22.5).abs() * 0.1).min(1.0)
            };

            episode_reward += reward;

            let next_obs = Observation::sensor("temperature", temp + 0.1);
            let result = ActionResult::success(&action.id);
            let done = step == 19;

            let outcome = Outcome::new(action, result, reward, next_obs, done);
            agent.learn(outcome);
        }

        if episode % 3 == 0 {
            println!("  Episode {}: reward = {:.2}", episode, episode_reward);
        }
    }

    let stats = agent.get_statistics();
    println!("  Final stats:");
    println!("    Episodes: {}", stats.episodes_completed);
    println!("    Learning updates: {}", stats.learning_updates);
    println!("    Average reward: {:.2}", stats.avg_reward);
    println!("    Current epsilon: {:.3}\n", stats.current_epsilon);
}

/// Demonstrate hierarchical goal management
fn demo_hierarchical_goals() {
    println!("--- 3. Hierarchical Goal Management ---");

    let mut agent = HopeAgent::with_default_config();

    // Create multiple goals with different priorities
    let goal1 = Goal::maintain("temperature", 20.0..25.0).with_priority(Priority::High);
    let goal2 = Goal::maintain("humidity", 40.0..60.0).with_priority(Priority::Normal);
    let goal3 = Goal::avoid("pressure").with_priority(Priority::Critical);

    println!("  Adding goals:");
    println!("    - Maintain temperature (High priority)");
    agent.set_goal(goal1);

    println!("    - Maintain humidity (Normal priority)");
    agent.set_goal(goal2);

    println!("    - Avoid pressure threshold (Critical priority)");
    agent.set_goal(goal3);

    let active_goals = agent.active_goals();
    println!("  Active goals: {}", active_goals.len());

    for (i, goal) in active_goals.iter().enumerate() {
        println!(
            "    {}. {:?} - Priority: {:?}",
            i + 1,
            goal.goal_type,
            goal.priority
        );
    }

    println!();
}

/// Demonstrate multi-agent coordination
fn demo_multi_agent_coordination() {
    println!("--- 4. Multi-Agent Coordination ---");

    let mut coordinator = AgentCoordinator::new();

    // Create and register agents
    println!("  Registering 3 agents...");
    let agent1 = HopeAgent::with_default_config();
    let agent2 = HopeAgent::with_default_config();
    let agent3 = HopeAgent::with_default_config();

    let id1 = coordinator.register_agent(agent1);
    let id2 = coordinator.register_agent(agent2);
    let id3 = coordinator.register_agent(agent3);

    println!("  Agents registered: {}", coordinator.agent_count());

    // Shared memory
    println!("  Using shared memory...");
    coordinator
        .shared_memory_mut()
        .set("global_temp".to_string(), "22.5".to_string());
    coordinator
        .shared_memory_mut()
        .set("system_mode".to_string(), "active".to_string());

    println!("    Stored: global_temp = 22.5");
    println!("    Stored: system_mode = active");

    // Broadcast message
    println!("  Broadcasting message to all agents...");
    let msg =
        Message::new("status", "System update available").with_priority(MessagePriority::High);
    coordinator.broadcast(msg);

    // Create proposal for consensus
    println!("  Creating consensus proposal...");
    let proposal_id = coordinator.create_proposal(
        "policy_update",
        "Should we adjust the temperature threshold?",
    );

    // Simulate voting
    let vote_yes = Message::with_payload(
        "vote",
        MessagePayload::Vote {
            proposal_id: proposal_id.clone(),
            vote: true,
        },
    );

    let vote_no = Message::with_payload(
        "vote",
        MessagePayload::Vote {
            proposal_id: proposal_id.clone(),
            vote: false,
        },
    );

    coordinator.send_to(&id1, vote_yes.clone()).unwrap();
    coordinator.send_to(&id2, vote_yes.clone()).unwrap();
    coordinator.send_to(&id3, vote_no).unwrap();

    // Process votes
    let mut observations = HashMap::new();
    observations.insert(id1, Observation::sensor("dummy", 0.0));
    observations.insert(id2, Observation::sensor("dummy", 0.0));
    observations.insert(id3, Observation::sensor("dummy", 0.0));
    coordinator.step_all(observations);

    // Check consensus
    match coordinator.get_consensus(&proposal_id) {
        Some(ConsensusResult::Decided {
            approved,
            votes_for,
            votes_against,
            approval_rate,
        }) => {
            println!("  Consensus reached:");
            println!(
                "    Decision: {}",
                if approved { "APPROVED" } else { "REJECTED" }
            );
            println!("    Votes: {} for, {} against", votes_for, votes_against);
            println!("    Approval rate: {:.1}%\n", approval_rate * 100.0);
        }
        _ => println!("  Voting still in progress\n"),
    }
}

/// Demonstrate state persistence
fn demo_persistence() {
    println!("--- 5. State Persistence ---");

    let mut agent = HopeAgent::with_default_config();

    // Train the agent briefly
    println!("  Training agent...");
    for i in 0..50 {
        let obs = Observation::sensor("value", (i % 10) as f64);
        let action = agent.step(obs.clone());

        let outcome = Outcome::new(action, ActionResult::success("test"), 1.0, obs, i % 10 == 9);
        agent.learn(outcome);
    }

    let original_steps = agent.get_statistics().total_steps;
    println!("    Completed {} steps", original_steps);

    // Save to bytes (in-memory)
    println!("  Serializing agent state...");
    let bytes = agent.to_bytes();
    println!("    Serialized to {} bytes", bytes.len());

    // Load from bytes
    println!("  Deserializing agent state...");
    let loaded_agent = HopeAgent::from_bytes(&bytes).unwrap();
    println!(
        "    Loaded agent with {} steps",
        loaded_agent.get_statistics().total_steps
    );

    assert_eq!(original_steps, loaded_agent.get_statistics().total_steps);
    println!("    ✓ State preserved correctly\n");
}

/// Demonstrate anomaly detection
fn demo_anomaly_detection() {
    println!("--- 6. Anomaly Detection ---");

    let mut agent = HopeAgent::with_default_config();

    // Establish normal pattern
    println!("  Establishing normal pattern (20-25°C)...");
    for i in 0..30 {
        let temp = 20.0 + (i % 6) as f64;
        let obs = Observation::sensor("temperature", temp);
        agent.step(obs);
    }

    let initial_anomalies = agent.get_statistics().anomalies_detected;
    println!("    Baseline anomalies: {}", initial_anomalies);

    // Introduce anomalies
    println!("  Introducing anomalous values...");
    let anomalies = vec![
        ("spike", 100.0),
        ("drop", -10.0),
        ("normal", 22.0),
        ("spike", 150.0),
    ];

    for (label, temp) in anomalies {
        let obs = Observation::sensor("temperature", temp);
        agent.step(obs);

        let current_anomalies = agent.get_statistics().anomalies_detected;
        let detected = if current_anomalies > initial_anomalies {
            "ANOMALY"
        } else {
            "normal"
        };

        println!("    {} ({}°C) -> {}", label, temp, detected);
    }

    let final_anomalies = agent.get_statistics().anomalies_detected;
    println!("  Total anomalies detected: {}\n", final_anomalies);
}
