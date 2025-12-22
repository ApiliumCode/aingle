//! Learning module for HOPE Agents
//!
//! This module provides reinforcement learning capabilities for agents including:
//! - Q-Learning
//! - SARSA
//! - Expected SARSA
//! - Temporal Difference learning
//!
//! ## Overview
//!
//! The learning engine allows agents to learn from experience and improve their
//! decision-making over time using various reinforcement learning algorithms.
//!
//! ## Basic Example
//!
//! ```rust
//! use hope_agents::{Agent, SimpleAgent, Observation, LearningConfig, LearningAlgorithm};
//!
//! // Create an agent with learning enabled
//! let mut agent = SimpleAgent::new("learning_agent");
//!
//! // Configure learning
//! let config = LearningConfig {
//!     learning_rate: 0.1,
//!     discount_factor: 0.99,
//!     algorithm: LearningAlgorithm::QLearning,
//!     ..Default::default()
//! };
//! agent.enable_learning(config);
//!
//! // Simulate learning loop
//! for _ in 0..10 {
//!     // Observe environment
//!     let obs = Observation::sensor("temperature", 25.0);
//!     agent.observe(obs.clone());
//!
//!     // Decide action
//!     let action = agent.decide();
//!
//!     // Execute action
//!     let result = agent.execute(action.clone());
//!
//!     // Learn from result
//!     agent.learn(&obs, &action, &result);
//! }
//!
//! // Check learning progress
//! let engine = agent.learning_engine().unwrap();
//! println!("Total updates: {}", engine.total_updates());
//! println!("State-action pairs learned: {}", engine.state_action_count());
//! ```
//!
//! ## Advanced Example: Direct Learning Engine Usage
//!
//! ```rust
//! use hope_agents::learning::{
//!     LearningEngine, LearningConfig, LearningAlgorithm,
//!     StateId, ActionId, Experience
//! };
//!
//! // Create learning engine
//! let mut engine = LearningEngine::new(LearningConfig {
//!     learning_rate: 0.1,
//!     discount_factor: 0.99,
//!     algorithm: LearningAlgorithm::QLearning,
//!     epsilon: 0.1,  // 10% exploration
//!     ..Default::default()
//! });
//!
//! // Define states and actions
//! let state1 = StateId::from_string("state_1".to_string());
//! let state2 = StateId::from_string("state_2".to_string());
//! let action_a = ActionId::from_string("action_a".to_string());
//! let action_b = ActionId::from_string("action_b".to_string());
//! let actions = vec![action_a.clone(), action_b.clone()];
//!
//! // Update Q-values
//! engine.update_q_learning(&state1, &action_a, 1.0, &state2, &actions);
//!
//! // Get best action
//! let best = engine.get_best_action(&state1, &actions);
//!
//! // Use epsilon-greedy exploration
//! let action = engine.get_action_epsilon_greedy(&state1, &actions);
//!
//! // Experience replay
//! let exp = Experience::new(state1.clone(), action_a.clone(), 1.0, state2.clone(), false);
//! engine.add_experience(exp);
//! engine.replay_batch(32, &actions);
//! ```

pub mod engine;
pub mod value_function;

pub use engine::{
    ActionId, Experience, LearningAlgorithm, LearningConfig, LearningEngine, QValue,
    StateActionPair, StateId,
};
pub use value_function::{LinearValueFunction, TabularValueFunction, ValueFunction};
