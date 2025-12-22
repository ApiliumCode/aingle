#![doc = include_str!("../README.md")]
//! # HOPE Agents - Hierarchical Optimizing Policy Engine
//!
//! Autonomous AI agents framework for AIngle semantic networks.
//!
//! ## Overview
//!
//! HOPE Agents provides a complete framework for building autonomous AI agents that can:
//! - **Observe** their environment (IoT sensors, network events, user inputs)
//! - **Decide** based on learned policies and hierarchical goals
//! - **Execute** actions in the AIngle network
//! - **Learn** and adapt over time using reinforcement learning
//!
//! This crate is designed for use cases ranging from simple reactive agents to complex
//! multi-agent systems with learning capabilities
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                       HOPE Agent                            │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
//! │  │   Sensors    │  │   Policy     │  │    Actuators     │  │
//! │  │              │  │   Engine     │  │                  │  │
//! │  │ • IoT data   │─►│              │─►│ • Network calls  │  │
//! │  │ • Events     │  │ • Goals      │  │ • State changes  │  │
//! │  │ • Messages   │  │ • Rules      │  │ • Messages       │  │
//! │  └──────────────┘  │ • Learning   │  └──────────────────┘  │
//! │                    └──────┬───────┘                         │
//! │                           │                                 │
//! │                    ┌──────▼───────┐                         │
//! │                    │   Memory     │                         │
//! │                    │ (Titans)     │                         │
//! │                    │              │                         │
//! │                    │ STM ◄──► LTM │                         │
//! │                    └──────────────┘                         │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ### Simple Reactive Agent
//!
//! ```rust,ignore
//! use hope_agents::{Agent, SimpleAgent, Goal, Observation, Rule, Condition, Action};
//!
//! // Create a simple reactive agent
//! let mut agent = SimpleAgent::new("sensor_monitor");
//!
//! // Add a rule: if temperature > 30, alert
//! let rule = Rule::new(
//!     "high_temp",
//!     Condition::above("temperature", 30.0),
//!     Action::alert("Temperature too high!"),
//! );
//! agent.add_rule(rule);
//!
//! // Process observations
//! let obs = Observation::sensor("temperature", 35.0);
//! agent.observe(obs.clone());
//! let action = agent.decide();
//! let result = agent.execute(action.clone());
//! agent.learn(&obs, &action, &result);
//! ```
//!
//! ### HOPE Agent with Learning
//!
//! ```rust,ignore
//! use hope_agents::{HopeAgent, HopeConfig, Observation, Goal, Priority, Outcome};
//!
//! // Create a HOPE agent with learning, prediction, and hierarchical goals
//! let mut agent = HopeAgent::with_default_config();
//!
//! // Set a goal
//! let goal = Goal::maintain("temperature", 20.0..25.0)
//!     .with_priority(Priority::High);
//! agent.set_goal(goal);
//!
//! // Agent loop with reinforcement learning
//! for episode in 0..100 {
//!     let obs = Observation::sensor("temperature", 22.0);
//!     let action = agent.step(obs.clone());
//!
//!     // Execute action in environment and get reward
//!     let reward = 1.0; // Example reward
//!     let next_obs = Observation::sensor("temperature", 21.0);
//!
//!     let outcome = Outcome::new(action, result, reward, next_obs, false);
//!     agent.learn(outcome);
//! }
//! ```
//!
//! ### Multi-Agent Coordination
//!
//! ```rust,ignore
//! use hope_agents::{AgentCoordinator, HopeAgent, Message, Observation};
//! use std::collections::HashMap;
//!
//! // Create coordinator
//! let mut coordinator = AgentCoordinator::new();
//!
//! // Register agents
//! let agent1 = HopeAgent::with_default_config();
//! let agent2 = HopeAgent::with_default_config();
//!
//! let id1 = coordinator.register_agent(agent1);
//! let id2 = coordinator.register_agent(agent2);
//!
//! // Broadcast message
//! coordinator.broadcast(Message::new("update", "System status changed"));
//!
//! // Step all agents
//! let mut observations = HashMap::new();
//! observations.insert(id1, Observation::sensor("temp", 20.0));
//! observations.insert(id2, Observation::sensor("humidity", 60.0));
//!
//! let actions = coordinator.step_all(observations);
//! ```
//!
//! ### State Persistence
//!
//! ```rust,ignore
//! use hope_agents::{HopeAgent, AgentPersistence};
//! use std::path::Path;
//!
//! let mut agent = HopeAgent::with_default_config();
//!
//! // Train the agent...
//!
//! // Save agent state
//! agent.save_to_file(Path::new("agent_state.json")).unwrap();
//!
//! // Later, load agent state
//! let loaded_agent = HopeAgent::load_from_file(Path::new("agent_state.json")).unwrap();
//! ```
//!
//! ## Agent Types
//!
//! - **ReactiveAgent**: Simple stimulus-response behavior
//! - **GoalBasedAgent**: Works toward explicit goals
//! - **LearningAgent**: Adapts behavior over time
//! - **CooperativeAgent**: Coordinates with other agents

pub mod action;
pub mod agent;
pub mod config;
pub mod coordination;
pub mod error;
pub mod goal;
pub mod hierarchical;
pub mod hope_agent;
pub mod learning;
#[cfg(feature = "memory")]
pub mod memory;
pub mod observation;
pub mod persistence;
pub mod policy;
pub mod predictive;
pub mod types;

pub use action::{Action, ActionResult, ActionType};
pub use agent::{Agent, AgentId, AgentState, SimpleAgent};
pub use config::AgentConfig;
pub use coordination::{
    AgentCoordinator, ConsensusResult, CoordinationError, Message, MessageBus, MessageId,
    MessagePayload, MessagePriority, SharedMemory,
};
pub use error::{Error, Result};
pub use goal::{Goal, GoalPriority, GoalStatus, GoalType};
pub use hierarchical::{
    default_decomposition_rules, ConflictResolution, ConflictType, DecompositionResult,
    DecompositionRule, DecompositionStrategy, GoalConflict, GoalTree, GoalTypeFilter,
    HierarchicalGoalSolver, ParallelStrategy, SequentialStrategy,
};
pub use hope_agent::{
    AgentStats, GoalSelectionStrategy, HopeAgent, HopeConfig, OperationMode, Outcome,
    SerializedState,
};
pub use learning::{
    ActionId, Experience, LearningAlgorithm, LearningConfig, LearningEngine, QValue,
    StateActionPair, StateId,
};
pub use observation::{Observation, ObservationType, Sensor};
pub use persistence::{
    AgentPersistence, CheckpointManager, LearningSnapshot, PersistenceError, PersistenceFormat,
    PersistenceOptions,
};
pub use policy::{Condition, Policy, PolicyEngine, Rule};
pub use predictive::{
    AnomalyDetector, PredictedState, PredictiveConfig, PredictiveModel, StateEncoder,
    StateSnapshot, Trajectory, TransitionModel,
};
pub use types::*;

/// HOPE framework version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a simple agent with default configuration.
///
/// This is a convenience function that creates a [`SimpleAgent`] with standard settings
/// suitable for general-purpose use. The agent will have learning enabled, a maximum of
/// 10 goals, and default policy engine settings.
///
/// # Arguments
///
/// * `name` - A unique identifier for the agent. This will be used in logging and coordination.
///
/// # Examples
///
/// ```
/// use hope_agents::create_agent;
///
/// let agent = create_agent("my_agent");
/// assert_eq!(agent.name(), "my_agent");
/// ```
///
/// # See Also
///
/// - [`SimpleAgent::new`] for direct construction
/// - [`SimpleAgent::with_config`] for custom configuration
/// - [`create_iot_agent`] for IoT-optimized agents with reduced memory footprint
pub fn create_agent(name: &str) -> SimpleAgent {
    SimpleAgent::new(name)
}

/// Creates an IoT-optimized agent with reduced memory footprint.
///
/// This creates a [`SimpleAgent`] configured for resource-constrained environments
/// with memory limits suitable for embedded devices. IoT agents trade some capabilities
/// for reduced resource usage, making them ideal for edge computing scenarios.
///
/// # Arguments
///
/// * `name` - A unique identifier for the agent.
///
/// # Examples
///
/// ```
/// use hope_agents::create_iot_agent;
///
/// let agent = create_iot_agent("sensor_agent");
/// assert!(agent.config().max_memory_bytes <= 128 * 1024);
/// ```
///
/// # Configuration
///
/// IoT agents have:
/// - Maximum memory: 128KB
/// - Learning disabled by default (can be re-enabled)
/// - Reduced observation buffer size
/// - Maximum of 5 concurrent goals (vs. 10 for standard agents)
/// - Simplified policy engine with fewer rules
///
/// # See Also
///
/// - [`AgentConfig::iot_mode`] for manual configuration
/// - [`create_agent`] for standard agents with full capabilities
pub fn create_iot_agent(name: &str) -> SimpleAgent {
    SimpleAgent::with_config(name, AgentConfig::iot_mode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_agent() {
        let agent = create_agent("test_agent");
        assert_eq!(agent.name(), "test_agent");
    }

    #[test]
    fn test_create_iot_agent() {
        let agent = create_iot_agent("iot_agent");
        assert!(agent.config().max_memory_bytes <= 128 * 1024);
    }
}
