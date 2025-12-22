//! Example of using the HOPE Agent Orchestrator
//!
//! This example demonstrates how to:
//! - Create and configure a HOPE agent
//! - Set goals
//! - Run the agent step/learn cycle
//! - Switch operation modes
//! - Track statistics
//! - Serialize/deserialize agent state

use hope_agents::{
    Action, ActionResult, ActionType, Goal, GoalSelectionStrategy, HopeAgent, HopeConfig,
    LearningAlgorithm, LearningConfig, Observation, OperationMode, Outcome, PredictiveConfig,
    Priority,
};

fn main() {
    println!("=== HOPE Agent Orchestrator Example ===\n");

    // Example 1: Basic agent creation and configuration
    basic_example();

    println!("\n");

    // Example 2: Goal-driven agent
    goal_driven_example();

    println!("\n");

    // Example 3: Multi-episode learning
    multi_episode_example();

    println!("\n");

    // Example 4: Operation mode switching
    mode_switching_example();

    println!("\n");

    // Example 5: State persistence
    persistence_example();
}

fn basic_example() {
    println!("--- Example 1: Basic Agent ---");

    // Create a HOPE agent with default configuration
    let mut agent = HopeAgent::with_default_config();

    println!("Created agent with mode: {:?}", agent.mode());

    // Simulate a simple interaction
    for step in 0..5 {
        // Agent observes environment
        let obs = Observation::sensor("temperature", 20.0 + step as f64);
        println!(
            "Step {}: Observed temperature = {}",
            step,
            20.0 + step as f64
        );

        // Agent decides action
        let action = agent.step(obs.clone());
        println!("  Action selected: {:?}", action.action_type);

        // Execute action and get outcome
        let result = ActionResult::success(&action.id);
        let reward = 1.0; // Simple positive reward
        let new_obs = Observation::sensor("temperature", 21.0 + step as f64);

        let outcome = Outcome::new(action, result, reward, new_obs, false);

        // Agent learns from outcome
        agent.learn(outcome);
    }

    // Check statistics
    let stats = agent.get_statistics();
    println!("\nAgent Statistics:");
    println!("  Total steps: {}", stats.total_steps);
    println!("  Learning updates: {}", stats.learning_updates);
    println!("  Average reward: {:.2}", stats.avg_reward);
}

fn goal_driven_example() {
    println!("--- Example 2: Goal-Driven Agent ---");

    // Create agent with custom configuration
    let config = HopeConfig {
        mode: OperationMode::GoalDriven,
        learning: LearningConfig {
            learning_rate: 0.15,
            discount_factor: 0.95,
            algorithm: LearningAlgorithm::QLearning,
            epsilon: 0.2,
            ..Default::default()
        },
        goal_strategy: GoalSelectionStrategy::Priority,
        auto_decompose_goals: true,
        ..Default::default()
    };

    let mut agent = HopeAgent::new(config);

    // Set multiple goals
    let goal1 = Goal::maintain("temperature", 18.0..22.0).with_priority(Priority::High);

    let goal2 = Goal::maximize("efficiency").with_priority(Priority::Normal);

    let goal_id1 = agent.set_goal(goal1);
    let goal_id2 = agent.set_goal(goal2);

    println!("Set goals: {} and {}", goal_id1, goal_id2);
    println!("Active goal: {:?}", agent.current_goal().map(|g| &g.name));

    // Run agent for several steps
    for step in 0..10 {
        let temp = 20.0 + (step as f64 * 0.5);
        let obs = Observation::sensor("temperature", temp);

        let action = agent.step(obs.clone());

        // Simulate action execution with varying success
        let success = step % 3 != 0; // Fail every 3rd step
        let result = if success {
            ActionResult::success(&action.id)
        } else {
            ActionResult::failure(&action.id, "Simulated failure")
        };

        let reward = if success { 2.0 } else { -1.0 };
        let new_obs = Observation::sensor("temperature", temp + 0.5);

        let outcome = Outcome::new(action, result, reward, new_obs, false);
        agent.learn(outcome);
    }

    // Check goal progress
    if let Some(goal) = agent.current_goal() {
        println!("\nCurrent goal progress: {:.1}%", goal.progress * 100.0);
        println!("Goal status: {:?}", goal.status);
    }

    let stats = agent.get_statistics();
    println!("Success rate: {:.1}%", stats.success_rate * 100.0);
}

fn multi_episode_example() {
    println!("--- Example 3: Multi-Episode Learning ---");

    let mut agent = HopeAgent::with_default_config();

    // Run 5 episodes
    for episode in 0..5 {
        println!("\nEpisode {}", episode + 1);

        // Each episode runs for 10 steps
        for step in 0..10 {
            let obs = Observation::sensor("sensor", step as f64);
            let action = agent.step(obs.clone());

            // Simulate action
            let result = ActionResult::success(&action.id);
            let reward = if step > 7 { 10.0 } else { 0.5 };
            let done = step == 9; // Episode ends at step 10

            let new_obs = Observation::sensor("sensor", (step + 1) as f64);
            let outcome = Outcome::new(action, result, reward, new_obs, done);

            agent.learn(outcome);
        }

        let stats = agent.get_statistics();
        println!("  Episode reward: {:.2}", stats.avg_reward);
        println!("  Epsilon: {:.3}", stats.current_epsilon);

        // Reset for next episode
        if episode < 4 {
            agent.reset();
        }
    }

    let final_stats = agent.get_statistics();
    println!("\nFinal Statistics:");
    println!("  Episodes completed: {}", final_stats.episodes_completed);
    println!("  Average reward: {:.2}", final_stats.avg_reward);
    println!(
        "  Learned state-action pairs: {}",
        agent.learning_engine().state_action_count()
    );
}

fn mode_switching_example() {
    println!("--- Example 4: Operation Mode Switching ---");

    let mut agent = HopeAgent::with_default_config();

    // Start with exploration
    agent.set_mode(OperationMode::Exploration);
    println!(
        "Mode: Exploration (epsilon = {:.3})",
        agent.learning_engine().epsilon()
    );

    // Run a few steps
    for i in 0..3 {
        let obs = Observation::sensor("test", i as f64);
        let action = agent.step(obs.clone());
        let result = ActionResult::success(&action.id);
        let outcome = Outcome::new(action, result, 1.0, obs, false);
        agent.learn(outcome);
    }

    // Switch to exploitation
    agent.set_mode(OperationMode::Exploitation);
    println!(
        "Mode: Exploitation (epsilon = {:.3})",
        agent.learning_engine().epsilon()
    );

    // Run more steps
    for i in 3..6 {
        let obs = Observation::sensor("test", i as f64);
        let action = agent.step(obs.clone());
        let result = ActionResult::success(&action.id);
        let outcome = Outcome::new(action, result, 1.0, obs, false);
        agent.learn(outcome);
    }

    // Switch to adaptive
    agent.set_mode(OperationMode::Adaptive);
    println!(
        "Mode: Adaptive (epsilon = {:.3})",
        agent.learning_engine().epsilon()
    );

    println!("Total steps: {}", agent.get_statistics().total_steps);
}

fn persistence_example() {
    println!("--- Example 5: State Persistence ---");

    // Create and train an agent
    let mut agent1 = HopeAgent::with_default_config();

    for i in 0..5 {
        let obs = Observation::sensor("data", i as f64);
        let action = agent1.step(obs.clone());
        let result = ActionResult::success(&action.id);
        let outcome = Outcome::new(action, result, 2.0, obs, false);
        agent1.learn(outcome);
    }

    println!("Original agent:");
    println!("  Steps: {}", agent1.get_statistics().total_steps);
    println!(
        "  Learning updates: {}",
        agent1.get_statistics().learning_updates
    );

    // Save state
    let saved_state = agent1.save_state();
    println!("\nSaved agent state");

    // Create new agent and load state
    let mut agent2 = HopeAgent::with_default_config();
    agent2.load_state(saved_state);

    println!("\nRestored agent:");
    println!("  Steps: {}", agent2.get_statistics().total_steps);
    println!(
        "  Learning updates: {}",
        agent2.get_statistics().learning_updates
    );

    // Continue with restored agent
    for i in 5..8 {
        let obs = Observation::sensor("data", i as f64);
        let action = agent2.step(obs.clone());
        let result = ActionResult::success(&action.id);
        let outcome = Outcome::new(action, result, 2.0, obs, false);
        agent2.learn(outcome);
    }

    println!("\nAfter continuing:");
    println!("  Steps: {}", agent2.get_statistics().total_steps);
    println!(
        "  Learning updates: {}",
        agent2.get_statistics().learning_updates
    );
}

// Example: Integration with AIngle minimal
#[allow(dead_code)]
fn aingle_integration_example() {
    println!("--- AIngle Integration Example ---");

    let mut agent = HopeAgent::with_default_config();

    // Set a goal for network operation
    let goal = Goal::perform("maintain_network_health").with_priority(Priority::Critical);
    agent.set_goal(goal);

    // Simulate network observations
    let network_obs = Observation::network("peer_connected", "peer_123");
    let action = agent.step(network_obs);

    match action.action_type {
        ActionType::SendMessage(ref target) => {
            println!("Agent decided to send message to: {}", target);
            // In real integration: execute network operation
        }
        ActionType::StoreData(ref key) => {
            println!("Agent decided to store data with key: {}", key);
            // In real integration: store to DHT
        }
        ActionType::Alert(ref msg) => {
            println!("Agent raised alert: {}", msg);
            // In real integration: trigger alert system
        }
        _ => {
            println!("Agent selected action: {:?}", action.action_type);
        }
    }
}
