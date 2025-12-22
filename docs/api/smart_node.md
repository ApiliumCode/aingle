# SmartNode API Reference

SmartNode combines MinimalNode with HOPE Agent capabilities for intelligent IoT devices.

## Overview

```rust
use aingle_minimal::{SmartNode, SmartNodeConfig, SensorAdapter, IoTPolicyBuilder};
use hope_agents::{Goal, Observation};
```

## SmartNode

The main struct that combines a MinimalNode with a HOPE Agent.

### Creation

```rust
// Default configuration
let node = SmartNode::new(SmartNodeConfig::default())?;

// IoT-optimized
let node = SmartNode::new(SmartNodeConfig::iot_mode())?;

// Low-power mode
let node = SmartNode::new(SmartNodeConfig::low_power())?;
```

### Core Methods

#### `observe(&mut self, observation: Observation) -> Result<Hash>`

Records an observation from a sensor or event.

```rust
let obs = Observation::sensor("temperature", 25.5);
let hash = node.observe(obs)?;
```

#### `step(&mut self) -> Result<Option<ActionResult>>`

Runs one agent decision cycle. Returns the result if an action was executed.

```rust
while let Some(result) = node.step()? {
    println!("Action: {} -> {}", result.action_id, result.success);
}
```

#### `add_rule(&mut self, rule: Rule)`

Adds a policy rule to the agent.

```rust
let rule = IoTPolicyBuilder::threshold_alert("temperature", 30.0, "High temp!");
node.add_rule(rule);
```

#### `add_goal(&mut self, goal: Goal)`

Sets a goal for the agent to pursue.

```rust
node.add_goal(Goal::maintain("temperature", 20.0..25.0));
```

### Accessors

```rust
// Access underlying MinimalNode
let node_ref = smart_node.node();
let node_mut = smart_node.node_mut();

// Get statistics
let stats = smart_node.stats()?;
println!("Observations: {}", stats.agent_stats.observations_received);
println!("Actions: {}", stats.agent_stats.actions_executed);
```

## SmartNodeConfig

Configuration for SmartNode.

### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `node_config` | `Config` | `Config::default()` | MinimalNode configuration |
| `agent_config` | `AgentConfig` | `AgentConfig::default()` | HOPE agent configuration |
| `auto_publish_observations` | `bool` | `true` | Auto-publish observations to DAG |

### Constructors

```rust
// Standard configuration
SmartNodeConfig::default()

// IoT-optimized (512KB memory budget)
SmartNodeConfig::iot_mode()

// Low-power mode (128KB, slower cycles)
SmartNodeConfig::low_power()
```

## SensorAdapter

Converts raw sensor readings to Observations.

### Basic Usage

```rust
// Simple sensor
let temp = SensorAdapter::new("temperature");
let obs = temp.reading(25.5);

// Sensor with scaling (raw ADC to physical units)
let temp = SensorAdapter::with_scaling(
    "temperature",
    0.161,  // scale factor
    -40.0   // offset
);
let obs = temp.reading(512.0);  // Raw ADC value -> ~42.4°C
```

### Methods

```rust
// Create observation from reading
fn reading(&self, value: f64) -> Observation

// Create observation with timestamp
fn reading_at(&self, value: f64, timestamp: u64) -> Observation
```

## IoTPolicyBuilder

Factory for common IoT policies.

### Threshold Alert

Triggers when value exceeds threshold.

```rust
let rule = IoTPolicyBuilder::threshold_alert(
    "temperature",  // sensor name
    30.0,           // threshold
    "High temp!"    // alert message
);
```

### Binary Control

On/off control based on threshold.

```rust
let rules = IoTPolicyBuilder::binary_control(
    "humidity",                              // sensor
    60.0,                                    // threshold
    Action::send_message("humidifier", "ON"), // above action
    Action::send_message("humidifier", "OFF") // below action
);
```

### Maintain Range

Keep value within range.

```rust
let rules = IoTPolicyBuilder::maintain_range(
    "light",                                    // sensor
    200.0, 800.0,                               // min, max lux
    Action::send_message("lights", "INCREASE"), // below min
    Action::send_message("lights", "DECREASE")  // above max
);
```

## SmartNodeStats

Statistics from the SmartNode.

```rust
pub struct SmartNodeStats {
    pub node_stats: NodeStats,
    pub agent_stats: AgentStats,
    pub pending_actions: usize,
    pub history_size: usize,
}
```

## Complete Example

```rust
use aingle_minimal::{SmartNode, SmartNodeConfig, SensorAdapter, IoTPolicyBuilder, Result};
use hope_agents::Goal;

fn main() -> Result<()> {
    // Create smart node
    let config = SmartNodeConfig::iot_mode();
    let mut node = SmartNode::new(config)?;

    // Setup sensors
    let temp = SensorAdapter::with_scaling("temperature", 0.161, -40.0);
    let humidity = SensorAdapter::new("humidity");

    // Add policies
    node.add_rule(IoTPolicyBuilder::threshold_alert("temperature", 35.0, "CRITICAL!"));

    // Add goals
    node.add_goal(Goal::maintain("temperature", 20.0..28.0));
    node.add_goal(Goal::maintain("humidity", 40.0..60.0));

    // Main loop
    loop {
        // Read sensors (simulated)
        let raw_temp = read_adc(0);
        let raw_humidity = read_adc(1);

        // Process observations
        node.observe(temp.reading(raw_temp))?;
        node.observe(humidity.reading(raw_humidity))?;

        // Execute agent decisions
        while let Some(result) = node.step()? {
            if result.success {
                println!("Executed: {}", result.action_id);
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

## See Also

- [MinimalNode API](./minimal_node.md)
- [HOPE Agents](./hope_agents.md)
- [IoT Tutorial](../tutorials/iot-sensor-app.md)
