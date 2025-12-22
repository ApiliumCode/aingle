# AIngle 2.0 Templates

Pre-built templates for common AIngle use cases, optimized for IoT and AI applications.

## Available Templates

| Template | Description | Use Case |
|----------|-------------|----------|
| **iot-sensor** | IoT sensor data collection | Smart devices, environmental monitoring |
| **ai-agent** | AI agents with Titans Memory | Machine learning, autonomous systems |
| **supply-chain** | Product tracking & provenance | Logistics, authenticity verification |

## Quick Start

```bash
# Copy a template
cp -r templates/iot-sensor my-sensor-zome
cd my-sensor-zome

# Build for WASM
cargo build --target wasm32-unknown-unknown --release

# Run tests
cargo test
```

## Integration with Titans Memory

All templates can leverage the Titans Memory system for AI-native memory management:

```rust
use aingle_minimal::{IoTMemory, Config};

// Create IoT-optimized memory
let mut memory = IoTMemory::new();

// Store sensor data with automatic importance scoring
memory.store_sensor_data("temp_001", SensorReading { value: 23.5 })?;

// Recall recent readings
let recent = memory.recall_recent(10)?;

// Run maintenance (decay + consolidation)
memory.maintenance()?;
```

### Memory Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Titans Memory System                      │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────────┐     ┌──────────────────────────────┐ │
│  │  Short-Term      │     │  Long-Term Memory (LTM)      │ │
│  │  Memory (STM)    │     │                              │ │
│  │  • Fast access   │ ──► │  • Knowledge Graph           │ │
│  │  • Attention     │     │  • Semantic Index            │ │
│  │  • Decay         │     │  • Embeddings                │ │
│  └──────────────────┘     └──────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Integration with HOPE Agents

Templates can use the HOPE (Hierarchical Optimizing Policy Engine) framework:

```rust
use hope_agents::{Agent, SimpleAgent, Goal, Observation, Rule, Condition, Action};

// Create an IoT-optimized agent
let mut agent = SimpleAgent::with_config("sensor_monitor", AgentConfig::iot_mode());

// Add goals
agent.set_goal(Goal::maintain("temperature", 20.0..25.0));

// Add reactive rules
agent.add_rule(Rule::new(
    "high_temp_alert",
    Condition::above("temperature", 30.0),
    Action::alert("Temperature too high!"),
));

// Agent loop
let obs = Observation::sensor("temperature", read_sensor());
agent.observe(obs);
let action = agent.decide();
agent.execute(action);
```

### Memory-Enabled Agents

```rust
use hope_agents::memory::MemoryAgent;

// Create memory-enabled agent
let mut agent = MemoryAgent::new("smart_controller");

// Observations are automatically remembered
agent.observe(Observation::sensor("temp", 25.0));

// Recall similar past observations for learning
let similar = agent.recall_similar(&current_obs, 5);
```

---

## IoT Sensor Template

Optimized for low-power IoT devices with sub-second confirmation.

**Features:**
- Lightweight sensor readings
- Batch upload for efficiency
- Time-range queries
- Device registration

**Entry Types:**
- `SensorReading` - Single measurement
- `SensorBatch` - Compressed batch of readings
- `SensorDevice` - Device registration

**Environment:**
```bash
# Enable sub-second confirmation
export AINGLE_PUBLISH_INTERVAL_MS=0
export AINGLE_IOT_MODE=1
```

---

## AI Agent Template

For AI agents using the Titans Memory architecture.

**Features:**
- Short-term memory (sliding window)
- Long-term memory checkpoints
- Learning event tracking
- Inference API

**Entry Types:**
- `ShortTermMemory` - Recent context
- `LongTermMemory` - Knowledge checkpoints
- `LearningEvent` - Training events

**Build with HOPE Agents:**
```bash
cargo build --features hope --target wasm32-unknown-unknown
```

---

## Supply Chain Template

Full product provenance tracking.

**Features:**
- Product registration
- Custody chain tracking
- IoT sensor integration
- Authenticity verification

**Entry Types:**
- `Product` - Product details
- `Location` - Checkpoints
- `CustodyEvent` - Transfers
- `InspectionRecord` - Quality checks

---

## Configuration Presets

| Mode | STM Size | LTM Size | Consolidation | Use Case |
|------|----------|----------|---------------|----------|
| IoT | 50 entries | 100 entities | Aggressive | Embedded devices |
| Agent | 500 entries | 10K entities | Balanced | AI applications |
| Server | 5000 entries | 1M entities | Conservative | Full nodes |

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `AINGLE_IOT_MODE` | Enable IoT optimizations | `false` |
| `AINGLE_PUBLISH_INTERVAL_MS` | Publish interval (0=immediate) | `5000` |
| `AINGLE_MEMORY_LIMIT_KB` | Memory limit for minimal node | `512` |

---

## Best Practices

1. **Keep entries small** - Under 1KB for IoT
2. **Use batch uploads** - Reduce network overhead
3. **Index with links** - Enable efficient queries
4. **Use Titans Memory** - For AI-enabled applications
5. **Configure for IoT** - Set `AINGLE_PUBLISH_INTERVAL_MS=0`

## Support

- Documentation: https://github.com/ApiliumCode/aingle
- Issues: https://github.com/ApiliumCode/aingle/issues
