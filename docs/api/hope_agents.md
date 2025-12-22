# HOPE Agents API Reference

Hierarchical Optimizing Policy Engine for AIngle AI agents.

## Overview

```rust
use hope_agents::{
    Agent, SimpleAgent, AgentConfig,
    Observation, ObservationType,
    Action, ActionResult, ActionType,
    Goal, GoalPriority,
    Policy, Rule, Condition,
};
```

## Agent Trait

The core trait for all agents.

```rust
pub trait Agent {
    fn name(&self) -> &str;
    fn id(&self) -> &AgentId;
    fn state(&self) -> AgentState;
    fn observe(&mut self, observation: Observation);
    fn decide(&mut self) -> Option<Action>;
    fn learn(&mut self, observation: &Observation, success: bool);
}
```

## SimpleAgent

A lightweight reactive agent suitable for IoT devices.

### Creation

```rust
// Basic creation
let agent = SimpleAgent::new("my_agent");

// With custom config
let config = AgentConfig::iot_mode();
let agent = SimpleAgent::with_config("my_agent", config);
```

### Core Methods

```rust
// Add an observation
agent.observe(Observation::sensor("temp", 25.0));

// Get next action
if let Some(action) = agent.decide() {
    // Execute action...
    agent.learn(&observation, success);
}

// Add policy rules
agent.add_rule(rule);

// Set a goal
agent.add_goal(goal);
```

### Accessors

```rust
agent.name()           // -> &str
agent.id()             // -> &AgentId
agent.state()          // -> AgentState
agent.config()         // -> &AgentConfig
agent.stats()          // -> AgentStats
```

## AgentConfig

Configuration for agent behavior.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_memory_bytes` | `usize` | 1MB | Memory budget |
| `decision_interval_ms` | `u64` | 100 | Min time between decisions |
| `learning_rate` | `f64` | 0.1 | How fast agent adapts |
| `exploration_rate` | `f64` | 0.1 | Random action probability |

### Constructors

```rust
AgentConfig::default()      // Standard config
AgentConfig::iot_mode()     // 128KB memory, slower decisions
```

## Observation

Represents sensor data or events.

### Creation

```rust
// Sensor reading
Observation::sensor("temperature", 25.5)

// Event
Observation::event("motion_detected")

// With metadata
Observation::new(
    ObservationType::Sensor,
    "temperature",
    Value::Float(25.5),
)
```

### Fields

```rust
pub struct Observation {
    pub obs_type: ObservationType,
    pub source: String,
    pub value: Value,
    pub timestamp: u64,
}
```

## Action

Represents an action to be executed.

### Creation

```rust
// Send a message
Action::send_message("device", "command")

// Store data
Action::store("key", "value")

// Custom action
Action::new(ActionType::Custom, "my_action".to_string())
```

### Types

```rust
pub enum ActionType {
    SendMessage,  // Network message
    Store,        // Store to DAG
    Alert,        // Trigger alert
    Custom,       // User-defined
}
```

## ActionResult

Result of executing an action.

```rust
pub struct ActionResult {
    pub action_id: String,
    pub action_type: ActionType,
    pub success: bool,
    pub message: Option<String>,
    pub timestamp: u64,
}
```

## Goal

Agent objectives.

### Creation

```rust
// Maintain value in range
Goal::maintain("temperature", 20.0..25.0)

// Achieve target value
Goal::achieve("battery", 100.0)

// Minimize value
Goal::minimize("energy_usage")

// Maximize value
Goal::maximize("throughput")
```

### Fields

```rust
pub struct Goal {
    pub name: String,
    pub target: GoalTarget,
    pub priority: GoalPriority,
    pub status: GoalStatus,
}
```

### Priority Levels

```rust
pub enum GoalPriority {
    Critical,  // Must be achieved
    High,      // Very important
    Medium,    // Normal importance
    Low,       // Nice to have
}
```

## Policy

Collection of rules for decision making.

```rust
pub struct Policy {
    pub name: String,
    pub rules: Vec<Rule>,
    pub enabled: bool,
}
```

## Rule

Individual policy rule.

### Creation

```rust
Rule::new(
    "high_temp_alert",                          // name
    Condition::above("temperature", 30.0),      // condition
    Action::alert("High temperature detected!") // action
)
```

### Fields

```rust
pub struct Rule {
    pub name: String,
    pub condition: Condition,
    pub action: Action,
    pub priority: u8,
    pub enabled: bool,
}
```

## Condition

Triggers for rules.

```rust
// Value comparisons
Condition::above("sensor", 30.0)
Condition::below("sensor", 10.0)
Condition::between("sensor", 20.0, 25.0)
Condition::equals("sensor", 1.0)

// Logical combinations
Condition::and(vec![cond1, cond2])
Condition::or(vec![cond1, cond2])
Condition::not(condition)

// Always trigger
Condition::always()
```

## PolicyEngine

Evaluates policies and selects actions.

```rust
let engine = PolicyEngine::new();
engine.add_policy(policy);

// Evaluate all policies against observations
let actions = engine.evaluate(&observations);
```

## AgentStats

Runtime statistics.

```rust
pub struct AgentStats {
    pub observations_received: u64,
    pub decisions_made: u64,
    pub actions_executed: u64,
    pub actions_succeeded: u64,
    pub actions_failed: u64,
    pub learning_updates: u64,
}
```

## Complete Example

```rust
use hope_agents::*;

fn main() {
    // Create agent
    let mut agent = SimpleAgent::with_config(
        "thermostat",
        AgentConfig::iot_mode()
    );

    // Set goals
    agent.add_goal(Goal::maintain("temperature", 20.0..24.0));

    // Add rules
    agent.add_rule(Rule::new(
        "heating_on",
        Condition::below("temperature", 20.0),
        Action::send_message("heater", "ON")
    ));

    agent.add_rule(Rule::new(
        "heating_off",
        Condition::above("temperature", 24.0),
        Action::send_message("heater", "OFF")
    ));

    // Agent loop
    loop {
        let temp = read_temperature();
        let obs = Observation::sensor("temperature", temp);

        agent.observe(obs.clone());

        if let Some(action) = agent.decide() {
            let success = execute_action(&action);
            agent.learn(&obs, success);
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

## See Also

- [SmartNode API](./smart_node.md) - Integration with AIngle
- [IoT Tutorial](../tutorials/iot-sensor-app.md)
- [Architecture Overview](../architecture/overview.md)
