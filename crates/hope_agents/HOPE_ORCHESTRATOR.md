# HOPE Agent Orchestrator

The HOPE Agent Orchestrator integrates all HOPE (Hierarchical, Optimistic, Predictive, Emergent) components into a unified intelligent agent system.

## Overview

The HOPE Agent is a complete autonomous agent that combines:

- **Learning Engine** (Q-Learning, SARSA, TD) - Learn from experience
- **Hierarchical Goal Solver** - Manage and decompose complex goals
- **Predictive Model** - Forecast future states and detect anomalies
- **Operation Modes** - Adapt behavior based on context

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    HOPE Agent                           │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Observation → State → Decision → Action → Learning    │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │  Predictive  │  │ Hierarchical │  │   Learning   │ │
│  │    Model     │  │ Goal Solver  │  │    Engine    │ │
│  │              │  │              │  │              │ │
│  │ • Anomaly    │  │ • Goals      │  │ • Q-Learning │ │
│  │ • Forecast   │  │ • Planning   │  │ • SARSA      │ │
│  │ • Patterns   │  │ • Conflicts  │  │ • Experience │ │
│  └──────────────┘  └──────────────┘  └──────────────┘ │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

## Core Components

### 1. HOPE Agent (`HopeAgent`)

Main orchestrator that integrates all modules.

```rust
use hope_agents::{HopeAgent, HopeConfig};

// Create with default config
let mut agent = HopeAgent::with_default_config();

// Or with custom config
let config = HopeConfig {
    mode: OperationMode::GoalDriven,
    learning: LearningConfig { /* ... */ },
    predictive: PredictiveConfig { /* ... */ },
    // ...
};
let mut agent = HopeAgent::new(config);
```

### 2. Operation Modes

The agent can operate in different modes:

- **Exploration**: High exploration rate for learning new behaviors
- **Exploitation**: Use learned knowledge, minimal exploration
- **GoalDriven**: Focus on completing goals with balanced exploration
- **Adaptive**: Automatically adjust based on performance

```rust
// Set operation mode
agent.set_mode(OperationMode::Exploration);

// Check current mode
let mode = agent.mode();
```

### 3. Goal Management

Set and track goals:

```rust
use hope_agents::{Goal, Priority};

// Create goals
let goal = Goal::maintain("temperature", 18.0..22.0)
    .with_priority(Priority::High);

// Set goal
let goal_id = agent.set_goal(goal);

// Check current goal
if let Some(goal) = agent.current_goal() {
    println!("Progress: {:.1}%", goal.progress * 100.0);
}
```

### 4. Step-Learn Cycle

Main interaction loop:

```rust
use hope_agents::{Observation, ActionResult, Outcome};

// 1. Agent observes environment
let obs = Observation::sensor("temperature", 22.5);

// 2. Agent decides action
let action = agent.step(obs.clone());

// 3. Execute action (in your environment)
let result = ActionResult::success(&action.id);

// 4. Calculate reward
let reward = 1.0;

// 5. Observe new state
let new_obs = Observation::sensor("temperature", 22.3);

// 6. Agent learns from outcome
let outcome = Outcome::new(action, result, reward, new_obs, false);
agent.learn(outcome);
```

## Key Features

### 1. Integrated Learning

The agent learns from every interaction:

```rust
// The learning engine updates Q-values
// Experience replay improves sample efficiency
// Epsilon-greedy balances exploration/exploitation
```

### 2. Anomaly Detection

Predictive model detects unusual observations:

```rust
// Anomalies are automatically detected during step()
// Agent increases exploration when anomalies occur
// Statistics track anomaly count
let stats = agent.get_statistics();
println!("Anomalies detected: {}", stats.anomalies_detected);
```

### 3. Hierarchical Goals

Complex goals can be decomposed:

```rust
// Set auto-decomposition in config
let config = HopeConfig {
    auto_decompose_goals: true,
    // ...
};

// Goals are automatically broken into subgoals
// Agent tracks progress hierarchically
// Conflicts are detected and resolved
```

### 4. State Persistence

Save and restore agent state:

```rust
// Save state
let state = agent.save_state();

// Serialize to JSON/binary
let json = serde_json::to_string(&state)?;

// Create new agent and restore
let mut new_agent = HopeAgent::with_default_config();
new_agent.load_state(state);
```

## Statistics and Monitoring

Track agent performance:

```rust
let stats = agent.get_statistics();

println!("Total steps: {}", stats.total_steps);
println!("Learning updates: {}", stats.learning_updates);
println!("Episodes completed: {}", stats.episodes_completed);
println!("Goals achieved: {}", stats.goals_achieved);
println!("Success rate: {:.1}%", stats.success_rate * 100.0);
println!("Average reward: {:.2}", stats.avg_reward);
println!("Current epsilon: {:.3}", stats.current_epsilon);
```

## Configuration Options

### HopeConfig

```rust
pub struct HopeConfig {
    /// Learning configuration
    pub learning: LearningConfig,

    /// Predictive model configuration
    pub predictive: PredictiveConfig,

    /// Operation mode
    pub mode: OperationMode,

    /// Max observations to keep in history
    pub max_observations: usize,

    /// Max actions to keep in history
    pub max_actions: usize,

    /// Anomaly detection sensitivity (0.0 to 1.0)
    pub anomaly_sensitivity: f64,

    /// Goal selection strategy
    pub goal_strategy: GoalSelectionStrategy,

    /// Enable automatic goal decomposition
    pub auto_decompose_goals: bool,
}
```

### Goal Selection Strategies

```rust
pub enum GoalSelectionStrategy {
    Priority,      // Highest priority goal
    Deadline,      // Goal with nearest deadline
    Progress,      // Goal with highest progress
    RoundRobin,    // Round-robin between goals
}
```

## Advanced Usage

### Multi-Episode Training

```rust
for episode in 0..100 {
    for step in 0..50 {
        let obs = Observation::sensor("data", step as f64);
        let action = agent.step(obs.clone());

        // Execute and get outcome
        let outcome = /* ... */;
        agent.learn(outcome);
    }

    // Reset for next episode
    agent.reset();
}
```

### Custom Reward Function

```rust
fn calculate_reward(obs: &Observation, action: &Action, result: &ActionResult) -> f64 {
    if !result.success {
        return -5.0; // Penalty for failure
    }

    // Reward based on observation
    match obs.value.as_f64() {
        Some(v) if v > 20.0 && v < 25.0 => 10.0,  // Good range
        Some(v) if v > 15.0 && v < 30.0 => 2.0,   // Acceptable
        _ => -1.0,  // Outside desired range
    }
}
```

### Accessing Sub-components

```rust
// Access learning engine
let learning = agent.learning_engine();
println!("Q-table size: {}", learning.state_action_count());

// Access goal solver
let solver = agent.goal_solver();
let executable = solver.get_executable_goals();

// Access predictive model
let predictive = agent.predictive_model();
let history = predictive.history();
```

## Integration with AIngle

### Network Operations

```rust
use hope_agents::{ActionType, Observation};

// Observe network events
let obs = Observation::network("peer_connected", "peer_123");
let action = agent.step(obs);

// Execute based on action type
match action.action_type {
    ActionType::SendMessage(target) => {
        // Send message via AIngle network
        network.send_message(&target, data)?;
    }
    ActionType::StoreData(key) => {
        // Store to DHT
        dht.store(&key, value)?;
    }
    ActionType::Query(query) => {
        // Query network
        let results = network.query(&query)?;
    }
    _ => {}
}
```

### Creating Observations from Records

```rust
// Convert AIngle Record to Observation
fn record_to_observation(record: &Record) -> Observation {
    let value = serde_json::to_value(&record.entry).unwrap();
    Observation::new(
        ObservationType::Custom("record".to_string()),
        value
    )
    .with_confidence(0.9)
}
```

## Best Practices

1. **Start with Exploration**: Begin with high exploration to learn the environment
2. **Set Clear Goals**: Define specific, achievable goals with appropriate priorities
3. **Monitor Statistics**: Track success rate and adjust configuration accordingly
4. **Use Adaptive Mode**: Let the agent adjust its behavior based on performance
5. **Persist State**: Save agent state periodically for recovery
6. **Reward Design**: Design reward functions that align with desired behavior
7. **Episode Management**: Use episodes to structure learning tasks
8. **Anomaly Handling**: Monitor and respond to detected anomalies

## Examples

See `examples/hope_orchestrator.rs` for complete examples:

- Basic agent usage
- Goal-driven behavior
- Multi-episode learning
- Mode switching
- State persistence
- AIngle integration

Run examples:

```bash
cargo run --example hope_orchestrator
```

## Testing

Run tests:

```bash
cargo test hope_agent
```

All major features are tested:
- Agent creation and configuration
- Step-learn cycle
- Goal integration
- Mode switching
- Anomaly detection
- Statistics tracking
- Serialization/deserialization
- Multi-episode learning
- Goal completion
- Exploration vs exploitation

## Performance Considerations

- **Memory**: Configurable history limits prevent unbounded growth
- **Computation**: Q-learning is efficient, O(1) per update
- **Batch Replay**: Improves sample efficiency without significant overhead
- **Epsilon Decay**: Automatically reduces exploration over time
- **Anomaly Detection**: Statistical methods are lightweight

## Future Enhancements

Potential improvements:

1. Deep Q-Networks (DQN) for continuous state spaces
2. Actor-Critic methods for improved policy learning
3. Multi-agent coordination and communication
4. Hierarchical reinforcement learning
5. Transfer learning between tasks
6. Meta-learning for faster adaptation
7. Intrinsic motivation and curiosity-driven exploration

## References

- Q-Learning: Watkins & Dayan (1992)
- SARSA: Rummery & Niranjan (1994)
- Experience Replay: Lin (1992)
- Hierarchical RL: Barto & Mahadevan (2003)
