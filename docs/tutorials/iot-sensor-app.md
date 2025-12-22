# Building an IoT Sensor Application with AIngle

This tutorial guides you through creating a temperature monitoring application using AIngle's minimal node for IoT devices.

## Prerequisites

- Rust 1.70 or later
- Basic understanding of async Rust
- An IoT device or simulator

## Project Setup

Create a new Rust project:

```bash
cargo new temperature-monitor
cd temperature-monitor
```

Add dependencies to `Cargo.toml`:

```toml
[dependencies]
aingle_minimal = { version = "0.1", features = ["coap", "smart_agents"] }
hope_agents = "0.1"
smol = "2.0"
log = "0.4"
env_logger = "0.11"
```

## Basic Temperature Monitor

Create a simple node that records temperature readings:

```rust
use aingle_minimal::{Config, MinimalNode, Entry, EntryType, Result};

fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Create IoT-optimized node
    let config = Config::iot_mode();
    let mut node = MinimalNode::new(config)?;

    // Simulate temperature readings
    for temp in [22.5, 23.0, 22.8, 24.1, 23.5] {
        let entry = Entry {
            entry_type: EntryType::App,
            content: format!(r#"{{"temperature": {}, "unit": "celsius"}}"#, temp).into_bytes(),
        };

        let hash = node.create_entry(entry)?;
        println!("Recorded temperature: {}°C -> {}", temp, hash);
    }

    // Get statistics
    let stats = node.stats()?;
    println!("Total entries: {}", stats.entry_count);

    Ok(())
}
```

## Adding AI with SmartNode

Upgrade to a SmartNode for intelligent decision-making:

```rust
use aingle_minimal::{SmartNode, SmartNodeConfig, SensorAdapter, IoTPolicyBuilder};
use hope_agents::{Observation, Action, Goal};

fn main() -> Result<()> {
    env_logger::init();

    // Create smart node with AI capabilities
    let config = SmartNodeConfig::iot_mode();
    let mut node = SmartNode::new(config)?;

    // Create sensor adapter for raw ADC readings
    // Converts raw 10-bit ADC (0-1023) to temperature (-40 to 125°C)
    let temp_sensor = SensorAdapter::with_scaling(
        "temperature",
        0.161,  // scale: (125 - (-40)) / 1023
        -40.0   // offset
    );

    // Add high temperature alert policy
    let rule = IoTPolicyBuilder::threshold_alert(
        "temperature",
        30.0,  // threshold
        "High temperature detected!"
    );
    node.add_rule(rule);

    // Add goal: maintain temperature between 20-25°C
    node.add_goal(Goal::maintain("temperature", 20.0..25.0));

    // Simulate sensor readings (raw ADC values)
    let raw_readings = [434, 447, 509, 465, 478];  // ~30, ~32, ~42, ~35, ~37°C

    for raw in raw_readings {
        // Convert raw reading to observation
        let obs = temp_sensor.reading(raw as f64);
        println!("Raw ADC: {} -> Temp: {:.1}°C", raw, obs.value.as_f64().unwrap());

        // Process observation
        node.observe(obs)?;

        // Let agent decide and act
        if let Some(result) = node.step()? {
            if result.success {
                println!("  Action executed: {:?}", result.action_id);
            }
        }
    }

    // Print statistics
    let stats = node.stats()?;
    println!("\nStatistics:");
    println!("  Observations: {}", stats.agent_stats.observations_received);
    println!("  Actions: {}", stats.agent_stats.actions_executed);
    println!("  Entries: {}", stats.node_stats.entry_count);

    Ok(())
}
```

## Network Communication with CoAP

Enable CoAP for IoT-friendly networking:

```rust
use aingle_minimal::{Config, MinimalNode, network::Network};
use aingle_minimal::config::{TransportConfig, GossipConfig};

#[smol::main]
async fn main() -> Result<()> {
    // Configure with CoAP transport
    let mut config = Config::iot_mode();
    config.transport = TransportConfig::Coap {
        bind_addr: "0.0.0.0".to_string(),
        port: 5683,  // CoAP default port
    };

    let mut node = MinimalNode::new(config)?;

    // Start network (async)
    node.start().await?;

    // Add a peer
    let peer_addr = "192.168.1.100:5683".parse().unwrap();
    node.network_mut().add_peer(peer_addr);

    // Main loop
    loop {
        // Process incoming messages
        if let Some((addr, msg)) = node.network().recv().await? {
            println!("Received from {}: {:?}", addr, msg);
        }

        // Gossip sync
        if node.gossip().should_gossip() {
            node.sync().await?;
        }

        // Sleep to save power
        smol::Timer::after(Duration::from_millis(100)).await;
    }
}
```

## Multi-Sensor Setup

Monitor multiple sensors with policies:

```rust
use aingle_minimal::{SmartNode, SmartNodeConfig, SensorAdapter, IoTPolicyBuilder};
use hope_agents::{Action, Policy, Rule, Condition};

fn setup_sensors() -> Vec<SensorAdapter> {
    vec![
        SensorAdapter::new("temperature"),
        SensorAdapter::new("humidity"),
        SensorAdapter::new("motion"),
        SensorAdapter::new("light"),
    ]
}

fn setup_policies(node: &mut SmartNode) {
    // Temperature alerts
    node.add_rule(IoTPolicyBuilder::threshold_alert(
        "temperature", 35.0, "CRITICAL: High temperature!"
    ));

    // Humidity control (binary on/off)
    let humidity_rules = IoTPolicyBuilder::binary_control(
        "humidity",
        60.0,  // threshold
        Action::send_message("humidifier", "ON"),
        Action::send_message("humidifier", "OFF"),
    );
    for rule in humidity_rules {
        node.add_rule(rule);
    }

    // Motion detection -> Record event
    let motion_rule = Rule::new(
        "motion_detected",
        Condition::above("motion", 0.5),
        Action::store("motion_event", "detected"),
    );
    node.add_rule(motion_rule);

    // Light-based automation
    let light_rules = IoTPolicyBuilder::maintain_range(
        "light",
        200.0, 800.0,  // lux range
        Action::send_message("lights", "INCREASE"),
        Action::send_message("lights", "DECREASE"),
    );
    for rule in light_rules {
        node.add_rule(rule);
    }
}
```

## Power Management

Optimize for battery-powered devices:

```rust
use aingle_minimal::{Config, SmartNodeConfig};
use aingle_minimal::config::PowerMode;

fn create_low_power_node() -> Result<SmartNode> {
    let mut config = SmartNodeConfig::low_power();

    // Further reduce activity
    config.node_config.power_mode = PowerMode::Critical;
    config.node_config.gossip.loop_delay = Duration::from_secs(60);
    config.auto_publish_observations = false;

    SmartNode::new(config)
}

async fn low_power_loop(node: &mut SmartNode) {
    loop {
        // Wake up, process, sleep
        node.node_mut().start().await?;

        // Quick sensor read
        let temp = read_temperature();
        node.observe(Observation::sensor("temperature", temp))?;
        node.step()?;

        // Sync if needed
        if node.node().gossip().should_gossip() {
            node.node_mut().sync().await?;
        }

        // Deep sleep
        node.node_mut().stop().await?;
        deep_sleep(Duration::from_secs(300));  // 5 minutes
    }
}
```

## Complete Example

Here's a full working example combining all concepts:

```rust
use aingle_minimal::{
    SmartNode, SmartNodeConfig, SensorAdapter, IoTPolicyBuilder, Result
};
use hope_agents::{Observation, Goal};
use std::time::Duration;

#[smol::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Configuration
    let config = SmartNodeConfig::iot_mode();
    let mut node = SmartNode::new(config)?;

    // Sensors
    let temp = SensorAdapter::new("temperature");
    let humidity = SensorAdapter::new("humidity");

    // Policies
    node.add_rule(IoTPolicyBuilder::threshold_alert("temperature", 30.0, "High temp!"));
    node.add_rule(IoTPolicyBuilder::threshold_alert("humidity", 80.0, "High humidity!"));

    // Goals
    node.add_goal(Goal::maintain("temperature", 18.0..26.0));
    node.add_goal(Goal::maintain("humidity", 40.0..60.0));

    // Start networking
    node.node_mut().start().await?;

    // Main loop
    println!("Starting IoT monitoring...");
    loop {
        // Read sensors (simulated)
        let t = 22.0 + (rand::random::<f64>() * 10.0 - 5.0);
        let h = 50.0 + (rand::random::<f64>() * 20.0 - 10.0);

        // Process observations
        node.observe(temp.reading(t))?;
        node.observe(humidity.reading(h))?;

        // Execute agent step
        while let Some(result) = node.step()? {
            println!("Action: {} -> {}", result.action_id, if result.success { "OK" } else { "FAIL" });
        }

        // Network sync
        if node.node().gossip().should_gossip() {
            node.node_mut().sync().await?;
        }

        smol::Timer::after(Duration::from_secs(1)).await;
    }
}
```

## Next Steps

- Read the [Architecture Overview](../architecture/overview.md)
- Explore [SmartNode API](../api/smart_node.md)
- Learn about [CoAP Protocol](../architecture/coap.md)
- Check [Gossip Optimization](../architecture/gossip.md)

## Resources

- [RFC 7252 - CoAP](https://tools.ietf.org/html/rfc7252)
- [HOPE Agents Documentation](../api/hope_agents.md)
- [AIngle GitHub Repository](https://github.com/ApiliumCode/aingle)
