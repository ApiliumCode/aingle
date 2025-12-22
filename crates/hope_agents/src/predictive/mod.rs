//! Predictive modeling for state and reward prediction in HOPE agents.
//!
//! This module provides capabilities for:
//! - Predicting next states given current state and action
//! - Predicting rewards for state-action pairs
//! - Trajectory prediction for sequences of actions
//! - Anomaly detection using statistical methods
//!
//! ## Overview
//!
//! The predictive model learns from observed transitions and can predict
//! future states and rewards. This enables agents to:
//! - Plan ahead by simulating action sequences
//! - Detect unusual observations
//! - Estimate uncertainty in predictions
//! - Make better informed decisions
//!
//! ## Example
//!
//! ```rust,ignore
//! use hope_agents::predictive::{PredictiveModel, PredictiveConfig};
//! use hope_agents::{Observation, Action, ActionType};
//!
//! // Create predictive model
//! let mut model = PredictiveModel::with_default_config();
//!
//! // Record transitions
//! let obs1 = Observation::sensor("temp", 20.0);
//! let obs2 = Observation::sensor("temp", 21.0);
//! let action = Action::new(ActionType::Custom("heat".to_string()));
//!
//! model.record_transition(&obs1, &action, 1.0, &obs2);
//! model.learn();
//!
//! // Make predictions
//! let pred = model.predict_next(&obs1, &action);
//! let reward = model.predict_reward(&obs1, &action);
//!
//! // Check for anomalies
//! let anomaly_obs = Observation::sensor("temp", 100.0);
//! if model.is_anomaly(&anomaly_obs) {
//!     println!("Anomalous observation detected!");
//! }
//! ```

mod anomaly;
mod model;
mod transition;

pub use anomaly::*;
pub use model::*;
pub use transition::*;
