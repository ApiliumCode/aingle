# Tutorial: AI-Powered Application using HOPE Agents

## Objective

Build an intelligent application that automatically learns and makes decisions using HOPE Agents (Hierarchical Optimized Policy Engine). Agents can learn from experience, handle hierarchical goals, detect anomalies, and execute autonomous actions.

## Prerequisites

- Complete the [getting started tutorial](./getting-started.md)
- Basic knowledge of reinforcement learning
- Familiarity with AI concepts (optional but helpful)

## Estimated time

90-120 minutes

---

## Step 1: Understanding HOPE Agents

HOPE (Hierarchical Optimized Policy Engine) combines:

- **Q-Learning**: Learns state-action values
- **SARSA**: Learns on-policy policies
- **TD-Learning**: Temporal difference learning
- **Hierarchical Goals**: Decomposes complex objectives
- **Predictive Model**: Predicts future states
- **Anomaly Detection**: Detects unusual behaviors

**Architecture:**

```
┌─────────────────────────────────────────┐
│           HOPE Agent                     │
├─────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────────────┐     │
│  │ Learning │  │ Goal Solver      │     │
│  │ Engine   │  │ (Hierarchical)   │     │
│  └──────────┘  └──────────────────┘     │
│  ┌──────────────────────────────────┐   │
│  │ Predictive Model                  │   │
│  │ - Forecasting                     │   │
│  │ - Anomaly Detection               │   │
│  └──────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

---

## Step 2: Set up basic HOPE Agent

Create a new project:

```bash
mkdir aingle-ai-app
cd aingle-ai-app
cargo init
```

Add dependencies to `Cargo.toml`:

```toml
[package]
name = "aingle-ai-app"
version = "0.1.0"
edition = "2021"

[dependencies]
hope_agents = { path = "../../crates/hope_agents" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
env_logger = "0.11"
```

Create your first agent:

```rust
// src/main.rs
use hope_agents::{HopeAgent, AgentConfig, Observation, ActionResult};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // Configure agent
    let config = AgentConfig {
        name: "smart_thermostat".to_string(),
        max_memory_bytes: 2 * 1024 * 1024, // 2 MB
        decision_interval: Duration::from_millis(200),
        max_goals: 20,
        learning_enabled: true,
        learning_rate: 0.15,       // Learning rate
        exploration_rate: 0.2,     // Exploration vs exploitation
        max_rules: 500,
        sensor_interval: Duration::from_millis(100),
        action_timeout: Duration::from_secs(10),
    };

    println!("🤖 Creating HOPE Agent: {}", config.name);
    println!("   Learning: {}", config.learning_enabled);
    println!("   Learning rate: {}", config.learning_rate);
    println!("   Exploration: {}%\n", config.exploration_rate * 100.0);

    // Create agent
    let mut agent = HopeAgent::new(config)?;

    println!("✓ Agent created and ready to learn\n");

    Ok(())
}
```

**Parameter explanation:**

- `learning_rate (0.15)`: How fast it learns (0=nothing, 1=instant)
- `exploration_rate (0.2)`: 20% explores new actions, 80% uses learned knowledge
- `max_goals (20)`: Can handle up to 20 simultaneous objectives
- `decision_interval`: Makes decisions every 200ms

---

## Step 3: Define observations and actions

Agents learn through environment observations and executing actions:

```rust
// src/thermostat.rs
use serde::{Deserialize, Serialize};
use hope_agents::{Observation, Action, ActionResult};

/// Thermostat state
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ThermostatState {
    pub current_temp: f64,      // Current temperature
    pub target_temp: f64,       // Desired temperature
    pub humidity: f64,          // Humidity %
    pub occupancy: bool,        // Is anyone present?
    pub time_of_day: u8,        // Hour 0-23
}

impl ThermostatState {
    /// Convert to observation for the agent
    pub fn to_observation(&self) -> Observation {
        // Discretize the state into categories
        let temp_diff = (self.target_temp - self.current_temp).round() as i32;
        let humidity_level = (self.humidity / 10.0).round() as u8; // 0-10
        let occupancy_flag = if self.occupancy { 1 } else { 0 };

        Observation {
            state_id: format!(
                "temp_{}_hum_{}_occ_{}_hour_{}",
                temp_diff, humidity_level, occupancy_flag, self.time_of_day
            ),
            features: vec![
                self.current_temp,
                self.target_temp,
                self.humidity,
                occupancy_flag as f64,
                self.time_of_day as f64,
            ],
            metadata: Some(serde_json::json!({
                "temp_diff": temp_diff,
                "comfort_level": self.calculate_comfort(),
            })),
        }
    }

    /// Calculate comfort level (0-100)
    fn calculate_comfort(&self) -> f64 {
        let temp_comfort = if (self.current_temp - self.target_temp).abs() < 1.0 {
            100.0
        } else {
            let diff = (self.current_temp - self.target_temp).abs();
            (100.0 - diff * 10.0).max(0.0)
        };

        let humidity_comfort = if (40.0..=60.0).contains(&self.humidity) {
            100.0
        } else {
            let diff = if self.humidity < 40.0 {
                40.0 - self.humidity
            } else {
                self.humidity - 60.0
            };
            (100.0 - diff * 2.0).max(0.0)
        };

        (temp_comfort + humidity_comfort) / 2.0
    }
}

/// Actions the thermostat can take
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ThermostatAction {
    HeatHigh,       // Intense heating (+2°C/min)
    HeatLow,        // Gentle heating (+0.5°C/min)
    Cool,           // Cooling (-1°C/min)
    Fan,            // Fan only (improve circulation)
    Off,            // Turn off
}

impl ThermostatAction {
    /// Convert to Action for the agent
    pub fn to_action(&self) -> Action {
        Action {
            action_id: self.action_name().to_string(),
            parameters: serde_json::to_value(self).unwrap(),
        }
    }

    pub fn action_name(&self) -> &str {
        match self {
            Self::HeatHigh => "heat_high",
            Self::HeatLow => "heat_low",
            Self::Cool => "cool",
            Self::Fan => "fan",
            Self::Off => "off",
        }
    }

    /// Execute action (simulated)
    pub fn execute(&self, state: &mut ThermostatState) -> ActionResult {
        let initial_comfort = state.calculate_comfort();

        match self {
            Self::HeatHigh => {
                state.current_temp += 2.0;
            }
            Self::HeatLow => {
                state.current_temp += 0.5;
            }
            Self::Cool => {
                state.current_temp -= 1.0;
            }
            Self::Fan => {
                // Improves circulation, slightly reduces humidity
                state.humidity = (state.humidity - 2.0).max(0.0);
            }
            Self::Off => {
                // Drifts toward ambient temperature (20°C)
                let ambient = 20.0;
                let drift = (ambient - state.current_temp) * 0.1;
                state.current_temp += drift;
            }
        }

        let final_comfort = state.calculate_comfort();
        let comfort_improvement = final_comfort - initial_comfort;

        // Reward based on improving comfort
        let reward = comfort_improvement / 10.0; // Scale to [-10, 10]

        ActionResult {
            success: true,
            reward,
            new_observation: state.to_observation(),
            done: false,
            metadata: Some(serde_json::json!({
                "comfort_before": initial_comfort,
                "comfort_after": final_comfort,
                "improvement": comfort_improvement,
            })),
        }
    }
}
```

**Explanation:**

- **Observation**: Represents the current state of the environment
- **Action**: Represents an action the agent can execute
- **Reward**: Learning signal (+reward = good, -reward = bad)
- **calculate_comfort()**: Objective function that the agent maximizes

---

## Step 4: Hierarchical goals

HOPE Agents support hierarchical goals that decompose into subgoals:

```rust
// src/goals.rs
use hope_agents::{Goal, GoalPriority};

/// Define thermostat objectives
pub fn create_comfort_goals() -> Vec<Goal> {
    vec![
        // Main goal: Optimal comfort
        Goal {
            goal_id: "optimal_comfort".to_string(),
            description: "Maintain optimal comfort (temp + humidity)".to_string(),
            priority: GoalPriority::High,
            parent_goal: None,
            success_condition: Box::new(|obs| {
                // Success if comfort > 90%
                if let Some(metadata) = &obs.metadata {
                    if let Some(comfort) = metadata.get("comfort_level") {
                        return comfort.as_f64().unwrap_or(0.0) > 90.0;
                    }
                }
                false
            }),
            subgoals: vec!["maintain_temp".to_string(), "maintain_humidity".to_string()],
        },
        // Subgoal 1: Temperature
        Goal {
            goal_id: "maintain_temp".to_string(),
            description: "Maintain target temperature ±1°C".to_string(),
            priority: GoalPriority::Medium,
            parent_goal: Some("optimal_comfort".to_string()),
            success_condition: Box::new(|obs| {
                if let Some(metadata) = &obs.metadata {
                    if let Some(diff) = metadata.get("temp_diff") {
                        let diff_val = diff.as_i64().unwrap_or(10);
                        return diff_val.abs() <= 1;
                    }
                }
                false
            }),
            subgoals: vec![],
        },
        // Subgoal 2: Humidity
        Goal {
            goal_id: "maintain_humidity".to_string(),
            description: "Maintain humidity 40-60%".to_string(),
            priority: GoalPriority::Medium,
            parent_goal: Some("optimal_comfort".to_string()),
            success_condition: Box::new(|obs| {
                if obs.features.len() > 2 {
                    let humidity = obs.features[2];
                    return (40.0..=60.0).contains(&humidity);
                }
                false
            }),
            subgoals: vec![],
        },
        // Secondary goal: Energy efficiency
        Goal {
            goal_id: "energy_efficiency".to_string(),
            description: "Minimize energy usage".to_string(),
            priority: GoalPriority::Low,
            parent_goal: None,
            success_condition: Box::new(|_obs| {
                // Always active, low priority
                false
            }),
            subgoals: vec![],
        },
    ]
}
```

**Explanation:**

- **Hierarchy**: `optimal_comfort` → `maintain_temp` + `maintain_humidity`
- **Priorities**: High > Medium > Low
- **Success conditions**: Predicates that determine if the objective is achieved
- The agent works on multiple goals simultaneously

---

## Step 5: Automatic learning

Train the agent with learning episodes:

```rust
// src/training.rs
use hope_agents::HopeAgent;
use crate::thermostat::{ThermostatState, ThermostatAction};
use crate::goals::create_comfort_goals;

pub async fn train_agent(agent: &mut HopeAgent, episodes: usize) -> anyhow::Result<()> {
    println!("🎓 Starting training ({} episodes)...\n", episodes);

    // Load objectives
    let goals = create_comfort_goals();
    for goal in goals {
        agent.add_goal(goal).await?;
    }

    let actions = vec![
        ThermostatAction::HeatHigh,
        ThermostatAction::HeatLow,
        ThermostatAction::Cool,
        ThermostatAction::Fan,
        ThermostatAction::Off,
    ];

    for episode in 0..episodes {
        // Random initial state
        let mut state = ThermostatState {
            current_temp: 15.0 + (rand::random::<f64>() * 10.0), // 15-25°C
            target_temp: 21.0,
            humidity: 40.0 + (rand::random::<f64>() * 30.0), // 40-70%
            occupancy: rand::random(),
            time_of_day: (rand::random::<f64>() * 24.0) as u8,
        };

        let mut total_reward = 0.0;
        let max_steps = 50;

        for step in 0..max_steps {
            // Observe current state
            let observation = state.to_observation();

            // Decide action (explore or exploit)
            let action_name = agent.decide_action(&observation).await?;

            // Execute action
            let selected_action = actions
                .iter()
                .find(|a| a.action_name() == action_name)
                .unwrap_or(&ThermostatAction::Off);

            let result = selected_action.execute(&mut state);
            total_reward += result.reward;

            // Learn from experience
            agent.learn(
                &observation,
                &selected_action.to_action(),
                result.reward,
                &result.new_observation,
                result.done,
            ).await?;

            // Terminate if optimal comfort reached
            if state.calculate_comfort() > 95.0 {
                break;
            }
        }

        if (episode + 1) % 10 == 0 {
            println!("  Episode {}/{}: Reward = {:.2}", episode + 1, episodes, total_reward);
        }
    }

    println!("\n✓ Training completed\n");

    // Show learned policy
    agent.print_policy().await;

    Ok(())
}
```

**Process explanation:**

1. **Initialization**: Random state to explore diversity
2. **Observation**: The agent perceives the current state
3. **Decision**: Selects action (explores new or uses learned)
4. **Execution**: Action modifies the state
5. **Reward**: Signal of how good the action was
6. **Learning**: Updates Q(s,a) values with the reward
7. **Iteration**: Repeats until objective reached or timeout

---

## Step 6: Anomaly detection

The predictive model detects abnormal behaviors:

```rust
// src/anomaly.rs
use hope_agents::HopeAgent;
use crate::thermostat::ThermostatState;

pub async fn detect_anomalies(
    agent: &HopeAgent,
    state: &ThermostatState,
) -> anyhow::Result<bool> {
    let observation = state.to_observation();

    // Predict expected next state
    let prediction = agent.predict_next_state(&observation).await?;

    // Detect if current state is anomalous
    let is_anomaly = agent.is_anomaly(&observation).await?;

    if is_anomaly {
        println!("⚠️  ANOMALY DETECTED:");
        println!("   Temp: {:.1}°C (target: {:.1}°C)",
            state.current_temp, state.target_temp);
        println!("   Humidity: {:.1}%", state.humidity);
        println!("   Comfort: {:.1}%", state.calculate_comfort());

        // Get explanation
        if let Some(reason) = agent.explain_anomaly(&observation).await? {
            println!("   Reason: {}", reason);
        }
    }

    Ok(is_anomaly)
}

/// Continuously monitor and alert anomalies
pub async fn monitor_anomalies(
    agent: &HopeAgent,
    states: Vec<ThermostatState>,
) -> anyhow::Result<()> {
    println!("🔍 Monitoring anomalies...\n");

    let mut anomaly_count = 0;

    for (i, state) in states.iter().enumerate() {
        let is_anomaly = detect_anomalies(agent, state).await?;

        if is_anomaly {
            anomaly_count += 1;
        }

        if (i + 1) % 20 == 0 {
            println!("  Processed {} readings, {} anomalies detected",
                i + 1, anomaly_count);
        }
    }

    println!("\n✓ Monitoring completed");
    println!("  Total anomalies: {} / {} ({:.1}%)",
        anomaly_count,
        states.len(),
        (anomaly_count as f64 / states.len() as f64) * 100.0
    );

    Ok(())
}
```

**Cases of detected anomalies:**

- Temperature rises/falls too quickly
- Humidity outside expected range
- State doesn't match model prediction
- Unusual usage pattern (e.g., heating in summer)

---

## Step 7: Autonomous decision-making

The agent executes decisions automatically:

```rust
// src/autonomous.rs
use hope_agents::HopeAgent;
use crate::thermostat::{ThermostatState, ThermostatAction};
use tokio::time::{interval, Duration};

pub async fn run_autonomous_mode(
    agent: &mut HopeAgent,
    initial_state: ThermostatState,
    duration_secs: u64,
) -> anyhow::Result<()> {
    println!("🤖 Autonomous mode activated for {} seconds\n", duration_secs);

    let mut state = initial_state;
    let mut tick_interval = interval(Duration::from_secs(1));
    let end_time = tokio::time::Instant::now() + Duration::from_secs(duration_secs);

    let actions = vec![
        ThermostatAction::HeatHigh,
        ThermostatAction::HeatLow,
        ThermostatAction::Cool,
        ThermostatAction::Fan,
        ThermostatAction::Off,
    ];

    while tokio::time::Instant::now() < end_time {
        tick_interval.tick().await;

        // Observe
        let observation = state.to_observation();

        // Decide (100% exploitation, 0% exploration)
        agent.set_exploration_rate(0.0);
        let action_name = agent.decide_action(&observation).await?;

        // Execute
        let selected_action = actions
            .iter()
            .find(|a| a.action_name() == action_name)
            .unwrap_or(&ThermostatAction::Off);

        let result = selected_action.execute(&mut state);

        // Log
        println!("│ Action: {:?}", selected_action);
        println!("│ Temp: {:.1}°C → {:.1}°C (target: {:.1}°C)",
            state.current_temp - result.new_observation.features[0],
            state.current_temp,
            state.target_temp
        );
        println!("│ Comfort: {:.1}%", state.calculate_comfort());
        println!("│ Reward: {:.2}", result.reward);
        println!("└─────────────────────────────────────");

        // Detect anomalies in real-time
        if agent.is_anomaly(&observation).await? {
            println!("⚠️  Anomalous behavior detected");
        }
    }

    println!("\n✓ Autonomous mode finished");
    println!("  Final temperature: {:.1}°C", state.current_temp);
    println!("  Final comfort: {:.1}%", state.calculate_comfort());

    Ok(())
}
```

**Explanation:**

- **Exploration rate = 0**: Only uses learned knowledge, doesn't explore
- **Continuous loop**: Makes decisions every second
- **Complete autonomy**: Requires no human intervention
- **Monitoring**: Detects anomalies in real-time

---

## Step 8: Complete program

Integrate all components:

```rust
// src/main.rs
mod thermostat;
mod goals;
mod training;
mod anomaly;
mod autonomous;

use hope_agents::{HopeAgent, AgentConfig};
use thermostat::ThermostatState;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // 1. Create agent
    let config = AgentConfig::ai_mode();
    let mut agent = HopeAgent::new(config)?;
    println!("✓ HOPE Agent created\n");

    // 2. Train
    training::train_agent(&mut agent, 100).await?;

    // 3. Test anomaly detection
    let test_states = vec![
        ThermostatState {
            current_temp: 21.0,
            target_temp: 21.0,
            humidity: 50.0,
            occupancy: true,
            time_of_day: 14,
        },
        ThermostatState {
            current_temp: 35.0, // ANOMALY: too hot
            target_temp: 21.0,
            humidity: 90.0,     // ANOMALY: too humid
            occupancy: false,
            time_of_day: 3,
        },
    ];

    anomaly::monitor_anomalies(&agent, test_states).await?;

    // 4. Run autonomous mode
    let initial_state = ThermostatState {
        current_temp: 18.0, // Cold
        target_temp: 21.0,
        humidity: 55.0,
        occupancy: true,
        time_of_day: 10,
    };

    autonomous::run_autonomous_mode(&mut agent, initial_state, 60).await?;

    // 5. Save trained agent
    agent.save_to_file("thermostat_agent.json").await?;
    println!("\n✓ Agent saved to thermostat_agent.json");

    Ok(())
}
```

---

## Expected output

```
✓ HOPE Agent created

🎓 Starting training (100 episodes)...

  Episode 10/100: Reward = 23.45
  Episode 20/100: Reward = 31.20
  Episode 30/100: Reward = 38.67
  ...
  Episode 100/100: Reward = 45.89

✓ Training completed

📊 Learned policy:
  State: temp_-3_hum_5_occ_1_hour_10 → Action: heat_high (Q=8.5)
  State: temp_0_hum_5_occ_1_hour_10 → Action: off (Q=9.2)
  State: temp_+2_hum_5_occ_1_hour_10 → Action: cool (Q=7.8)

🔍 Monitoring anomalies...

⚠️  ANOMALY DETECTED:
   Temp: 35.0°C (target: 21.0°C)
   Humidity: 90.0%
   Comfort: 15.2%
   Reason: Temperature outside expected range (+3 σ)

✓ Monitoring completed
  Total anomalies: 1 / 2 (50.0%)

🤖 Autonomous mode activated for 60 seconds

│ Action: HeatLow
│ Temp: 18.0°C → 18.5°C (target: 21.0°C)
│ Comfort: 72.5%
│ Reward: 2.30
└─────────────────────────────────────
│ Action: HeatLow
│ Temp: 18.5°C → 19.0°C (target: 21.0°C)
│ Comfort: 80.0%
│ Reward: 0.75
└─────────────────────────────────────
...

✓ Autonomous mode finished
  Final temperature: 21.2°C
  Final comfort: 98.5%

✓ Agent saved to thermostat_agent.json
```

---

## Common troubleshooting

### Agent doesn't learn

**Problem:** Rewards always low, no improvement.

**Solution:**
```rust
config.learning_rate = 0.3;      // Increase (more aggressive)
config.exploration_rate = 0.3;   // More exploration
// Train more episodes
train_agent(&mut agent, 500).await?;
```

### Insufficient memory

**Problem:** "Out of memory" error.

**Solution:**
```rust
config.max_memory_bytes = 512 * 1024;  // Reduce to 512 KB
config.max_rules = 100;                // Fewer rules
```

### Anomalies not detected

**Problem:** Doesn't detect abnormal states.

**Solution:**
```rust
// Train more to improve predictive model
train_agent(&mut agent, 200).await?;

// Adjust anomaly threshold
agent.set_anomaly_threshold(2.5); // More sensitive (default: 3.0)
```

---

## Next steps

1. **[Integrate with IoT](./iot-sensor-network.md)**: Control real sensors
2. **[Semantic queries](./semantic-queries.md)**: Query agent decisions
3. **[Persistence](./getting-started.md)**: Save experiences in the DAG
4. **Production**: Deploy to edge devices with AIngle minimal

---

## Key concepts learned

- **HOPE Agent**: Hierarchical learning agent
- **Q-Learning**: Learns state-action values
- **Exploration vs Exploitation**: Balance between exploring and using learned knowledge
- **Hierarchical Goals**: Decompose complex objectives
- **Anomaly Detection**: Detect unusual behaviors
- **Autonomous Decision-making**: Makes decisions without human intervention

---

## References

- [HOPE Agents Implementation](../../crates/hope_agents/IMPLEMENTATION_SUMMARY.md)
- [Reinforcement Learning: An Introduction](http://incompleteideas.net/book/the-book-2nd.html)
- [Q-Learning Tutorial](https://en.wikipedia.org/wiki/Q-learning)
- [Hierarchical Reinforcement Learning](https://people.cs.umass.edu/~mahadeva/papers/hrl.pdf)
