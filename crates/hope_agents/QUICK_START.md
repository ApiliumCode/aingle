# HOPE Agent Quick Start Guide

Get started with HOPE Agents in 5 minutes.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hope_agents = { path = "../hope_agents" }
```

## Basic Usage

```rust
use hope_agents::{HopeAgent, Observation, ActionResult, Outcome};

fn main() {
    // 1. Create agent
    let mut agent = HopeAgent::with_default_config();

    // 2. Main loop
    loop {
        // Observe environment
        let obs = Observation::sensor("temperature", 22.5);

        // Agent decides action
        let action = agent.step(obs.clone());

        // Execute action (your code here)
        println!("Executing: {:?}", action.action_type);

        // Create outcome
        let result = ActionResult::success(&action.id);
        let reward = 1.0;
        let new_obs = Observation::sensor("temperature", 22.3);
        let done = false;

        let outcome = Outcome::new(action, result, reward, new_obs, done);

        // Agent learns
        agent.learn(outcome);

        // Check progress
        let stats = agent.get_statistics();
        println!("Steps: {}, Avg Reward: {:.2}",
                 stats.total_steps, stats.avg_reward);

        if stats.total_steps >= 100 {
            break;
        }
    }
}
```

## With Goals

```rust
use hope_agents::{Goal, Priority};

// Set a goal
let goal = Goal::maintain("temperature", 18.0..22.0)
    .with_priority(Priority::High);

agent.set_goal(goal);

// Check progress
if let Some(current) = agent.current_goal() {
    println!("Goal: {} - Progress: {:.1}%",
             current.name, current.progress * 100.0);
}
```

## Operation Modes

```rust
use hope_agents::OperationMode;

// Exploration: Learn new behaviors
agent.set_mode(OperationMode::Exploration);

// Exploitation: Use learned knowledge
agent.set_mode(OperationMode::Exploitation);

// Goal-Driven: Focus on goals
agent.set_mode(OperationMode::GoalDriven);

// Adaptive: Auto-adjust (default)
agent.set_mode(OperationMode::Adaptive);
```

## Save/Load State

```rust
// Save
let state = agent.save_state();
let json = serde_json::to_string(&state)?;
std::fs::write("agent_state.json", json)?;

// Load
let json = std::fs::read_to_string("agent_state.json")?;
let state = serde_json::from_str(&json)?;
agent.load_state(state);
```

## Key Types

### Observations

```rust
// Sensor reading
Observation::sensor("temp", 22.5)

// Network event
Observation::network("peer_connected", "peer_123")

// Alert
Observation::alert("high_temperature")

// State change
Observation::state_change("mode", "active")
```

### Actions

```rust
// Send message
Action::send_message("peer_id", "hello")

// Store data
Action::store("key", "value")

// Alert
Action::alert("warning message")

// Wait
Action::wait()

// No operation
Action::noop()
```

### Goals

```rust
// Maintain value in range
Goal::maintain("temperature", 18.0..22.0)

// Achieve target value
Goal::achieve("count", 100)

// Maximize value
Goal::maximize("efficiency")

// Minimize value
Goal::minimize("latency")

// Avoid condition
Goal::avoid("overheating")

// Perform action
Goal::perform("calibrate_sensors")
```

## Configuration

```rust
use hope_agents::{HopeConfig, LearningConfig, LearningAlgorithm};

let config = HopeConfig {
    mode: OperationMode::GoalDriven,
    learning: LearningConfig {
        learning_rate: 0.15,
        discount_factor: 0.95,
        algorithm: LearningAlgorithm::QLearning,
        epsilon: 0.2,
        ..Default::default()
    },
    auto_decompose_goals: true,
    ..Default::default()
};

let agent = HopeAgent::new(config);
```

## Reward Design

Good reward functions are crucial:

```rust
fn calculate_reward(obs: &Observation, result: &ActionResult) -> f64 {
    // Penalty for failure
    if !result.success {
        return -5.0;
    }

    // Reward based on observation
    if let Some(temp) = obs.value.as_f64() {
        if temp >= 20.0 && temp <= 24.0 {
            10.0  // Perfect range
        } else if temp >= 18.0 && temp <= 26.0 {
            2.0   // Acceptable range
        } else {
            -2.0  // Outside range
        }
    } else {
        0.0  // Neutral for non-numeric
    }
}
```

## Statistics

```rust
let stats = agent.get_statistics();

println!("Total steps: {}", stats.total_steps);
println!("Learning updates: {}", stats.learning_updates);
println!("Episodes: {}", stats.episodes_completed);
println!("Goals achieved: {}", stats.goals_achieved);
println!("Success rate: {:.1}%", stats.success_rate * 100.0);
println!("Avg reward: {:.2}", stats.avg_reward);
println!("Epsilon: {:.3}", stats.current_epsilon);
println!("Anomalies: {}", stats.anomalies_detected);
```

## Common Patterns

### Multi-Episode Training

```rust
for episode in 0..100 {
    for step in 0..50 {
        // Step and learn
        let obs = get_observation();
        let action = agent.step(obs.clone());
        let outcome = execute_and_get_outcome(action);
        agent.learn(outcome);
    }

    // Reset for next episode
    agent.reset();
}
```

### Periodic State Saving

```rust
let mut step_count = 0;

loop {
    // ... agent interaction ...

    step_count += 1;
    if step_count % 1000 == 0 {
        let state = agent.save_state();
        save_to_disk(&state)?;
    }
}
```

### Goal Progress Monitoring

```rust
if let Some(goal) = agent.current_goal() {
    if goal.progress >= 0.9 {
        println!("Goal almost complete!");
    }

    if goal.is_overdue() {
        println!("Goal past deadline!");
        // Handle overdue goal
    }
}
```

### Adaptive Exploration

```rust
// Start with high exploration
agent.set_mode(OperationMode::Exploration);

// After learning phase, switch to exploitation
if agent.get_statistics().total_steps > 10000 {
    agent.set_mode(OperationMode::Exploitation);
}
```

## Debugging Tips

1. **Check statistics regularly**: Monitor learning progress
2. **Log actions**: Print action types to understand behavior
3. **Verify rewards**: Ensure reward function aligns with goals
4. **Monitor epsilon**: Should decrease over time
5. **Track Q-table size**: `agent.learning_engine().state_action_count()`
6. **Check goal status**: Monitor progress and conflicts

## Performance Tips

1. **Limit history**: Set `max_observations` and `max_actions` appropriately
2. **Batch replay**: Automatic, but can adjust batch size in learning config
3. **Episode length**: Shorter episodes = faster learning
4. **Epsilon decay**: Default decay works well, but can tune
5. **Goal decomposition**: Enables tackling complex tasks

## Next Steps

- Read [HOPE_ORCHESTRATOR.md](HOPE_ORCHESTRATOR.md) for detailed documentation
- Check [examples/hope_orchestrator.rs](examples/hope_orchestrator.rs) for complete examples
- Explore individual modules: learning, hierarchical, predictive
- Integrate with AIngle network operations

## Common Issues

**Agent not learning**: Check that rewards are being provided and are meaningful

**High exploration**: Epsilon might be too high, consider Exploitation mode

**Goals not completing**: Verify goal conditions and reward function alignment

**Memory usage**: Reduce `max_observations` and `max_actions` in config

**Slow convergence**: Increase learning rate or use different algorithm

## Resources

- [Learning Module Documentation](src/learning/mod.rs)
- [Hierarchical Goals Documentation](src/hierarchical/mod.rs)
- [Predictive Model Documentation](src/predictive/mod.rs)
- Run tests: `cargo test`
- Run examples: `cargo run --example hope_orchestrator`
