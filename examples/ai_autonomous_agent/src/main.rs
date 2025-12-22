//! Autonomous AI Agent Example
//!
//! Demonstrates how to use HOPE Agents for autonomous decision-making.
//!
//! # Features Demonstrated
//! - Creating simple and advanced agents
//! - Defining rules and conditions
//! - Hierarchical goals
//! - Agent coordination
//!
//! # Running
//! ```bash
//! cargo run --release -p ai_autonomous_agent
//! ```

use hope_agents::{
    create_iot_agent, Action, ActionType, Agent, Condition, Goal, Observation, Rule, SimpleAgent,
    ValueRange,
};
use rand::Rng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== HOPE Agents - Autonomous AI Example ===\n");

    // Example 1: Simple Reactive Agent
    simple_reactive_agent_demo()?;

    // Example 2: IoT Monitoring Agent
    iot_monitoring_agent_demo()?;

    // Example 3: Goal-Oriented Agent
    goal_oriented_agent_demo()?;

    println!("\nAll examples completed successfully!");
    Ok(())
}

/// Demonstrates a simple reactive agent with basic rules
fn simple_reactive_agent_demo() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 1: Simple Reactive Agent ---\n");

    let mut agent = SimpleAgent::new("temperature_monitor");

    // Disable exploration for deterministic behavior
    agent.set_exploration_rate(0.0);

    // Add rules for temperature monitoring
    let high_temp_rule = Rule::new(
        "high_temperature_alert",
        Condition::above("temperature", 30.0),
        Action::alert("Temperature is too high! Activating cooling."),
    );

    let low_temp_rule = Rule::new(
        "low_temperature_alert",
        Condition::below("temperature", 15.0),
        Action::alert("Temperature is too low! Activating heating."),
    );

    // For normal temperature, we use a store action to log it
    let normal_temp_rule = Rule::new(
        "normal_temperature",
        Condition::in_range("temperature", 15.0..30.0),
        Action::store("status", "Temperature is within normal range."),
    );

    agent.add_rule(high_temp_rule);
    agent.add_rule(low_temp_rule);
    agent.add_rule(normal_temp_rule);

    println!("Agent '{}' created with 3 rules", agent.name());

    // Simulate temperature readings
    let temperatures = [25.0, 32.0, 18.0, 12.0, 28.0];

    for temp in temperatures {
        let obs = Observation::sensor("temperature", temp);
        agent.observe(obs.clone());

        let action = agent.decide();
        println!(
            "  Temperature: {:.1}C -> Action: {:?}",
            temp, action.action_type
        );

        let result = agent.execute(action.clone());
        agent.learn(&obs, &action, &result);
    }

    let stats = agent.stats();
    println!(
        "\nAgent stats: {} observations, {} actions executed\n",
        stats.observations_received, stats.actions_executed
    );

    Ok(())
}

/// Demonstrates an IoT-optimized agent for sensor monitoring
fn iot_monitoring_agent_demo() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Example 2: IoT Monitoring Agent ---\n");

    // Use the helper function for IoT-optimized agents
    let mut agent = create_iot_agent("smart_sensor");

    // Disable exploration for deterministic behavior
    agent.set_exploration_rate(0.0);

    // Add multiple sensor monitoring rules
    agent.add_rule(Rule::new(
        "humidity_high",
        Condition::above("humidity", 80.0),
        Action::alert("High humidity detected!"),
    ));

    agent.add_rule(Rule::new(
        "motion_detected",
        Condition::above("motion", 0.5),
        Action::store("motion_log", "Motion detected in monitored area."),
    ));

    agent.add_rule(Rule::new(
        "light_low",
        Condition::below("light", 100.0),
        Action::new(ActionType::Custom("turn_on_lights".to_string()))
            .with_param("brightness", 80i64),
    ));

    println!("IoT Agent '{}' configured", agent.name());

    // Simulate IoT sensor data
    let mut rng = rand::rng();

    for cycle in 1..=5 {
        println!("\n  Cycle {}:", cycle);

        // Simulate multiple sensors
        let observations = vec![
            Observation::sensor("temperature", rng.random_range(18.0..35.0)),
            Observation::sensor("humidity", rng.random_range(40.0..90.0)),
            Observation::sensor("motion", rng.random_range(0.0..1.0)),
            Observation::sensor("light", rng.random_range(50.0..500.0)),
        ];

        for obs in observations {
            agent.observe(obs.clone());
            let action = agent.decide();

            if !action.is_noop() {
                println!(
                    "    {:?}: {:.2} -> {:?}",
                    obs.obs_type,
                    obs.value.as_f64().unwrap_or(0.0),
                    action.action_type
                );
            }

            let result = agent.execute(action.clone());
            agent.learn(&obs, &action, &result);
        }
    }

    println!("\nIoT agent demo completed");
    Ok(())
}

/// Demonstrates a goal-oriented agent with hierarchical goals
fn goal_oriented_agent_demo() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Example 3: Goal-Oriented Agent ---\n");

    let mut agent = SimpleAgent::new("climate_controller");

    // Disable exploration for deterministic behavior
    agent.set_exploration_rate(0.0);

    // Define hierarchical goals using the Goal API
    let comfort_goal = Goal::maintain("temperature", ValueRange::new(20.0, 24.0));
    let efficiency_goal = Goal::minimize("energy_consumption");

    agent.add_goal(comfort_goal);
    agent.add_goal(efficiency_goal);

    // Add rules that support the goals
    agent.add_rule(Rule::new(
        "cool_down",
        Condition::above("temperature", 24.0),
        Action::new(ActionType::Custom("set_ac".to_string()))
            .with_param("mode", "cool")
            .with_param("target", 22i64),
    ));

    agent.add_rule(Rule::new(
        "heat_up",
        Condition::below("temperature", 20.0),
        Action::new(ActionType::Custom("set_heater".to_string()))
            .with_param("mode", "heat")
            .with_param("target", 22i64),
    ));

    agent.add_rule(Rule::new(
        "optimal_range",
        Condition::in_range("temperature", 20.0..24.0),
        Action::new(ActionType::Custom("eco_mode".to_string())).with_param("mode", "standby"),
    ));

    println!("Goal-oriented agent configured with 2 goals");
    println!("Goals:");
    for goal in agent.active_goals() {
        println!("  - {} ({:?})", goal.name, goal.priority);
    }

    // Simulate temperature fluctuations
    let mut current_temp: f64 = 18.0;
    let mut rng = rand::rng();

    println!("\nSimulating climate control:");
    for hour in 1..=8 {
        // Temperature changes based on time of day
        current_temp += rng.random_range(-2.0..3.0);
        current_temp = current_temp.clamp(15.0, 35.0);

        let obs = Observation::sensor("temperature", current_temp);
        agent.observe(obs.clone());

        let action = agent.decide();
        let result = agent.execute(action.clone());

        println!(
            "  Hour {}: {:.1}C -> {:?}",
            hour, current_temp, action.action_type
        );

        agent.learn(&obs, &action, &result);
    }

    println!("\nFinal agent statistics:");
    let stats = agent.stats();
    println!("  Total observations: {}", stats.observations_received);
    println!("  Actions executed: {}", stats.actions_executed);
    println!("  Successful actions: {}", stats.successful_actions);

    Ok(())
}
