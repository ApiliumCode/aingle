//! The core predictive model for HOPE agents.

use crate::predictive::{AnomalyDetector, StateEncoder, TransitionModel};
use crate::{Action, Observation, Timestamp};
use std::collections::{HashMap, VecDeque};

/// A predictive model that learns state transitions and rewards from experience.
///
/// This model enables an agent to forecast future states, predict rewards,
/// and detect anomalies in its observations.
pub struct PredictiveModel {
    state_history: VecDeque<StateSnapshot>,
    transition_model: TransitionModel,
    reward_predictor: RewardPredictor,
    anomaly_detector: AnomalyDetector,
    config: PredictiveConfig,
}

/// Configuration for the `PredictiveModel`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PredictiveConfig {
    /// The maximum number of recent state snapshots to keep in history.
    pub history_size: usize,
    /// The number of steps ahead the model should predict in a trajectory.
    pub prediction_horizon: usize,
    /// The minimum confidence level for a prediction to be considered reliable.
    pub confidence_threshold: f64,
    /// The z-score threshold for the anomaly detector.
    pub anomaly_threshold: f64,
}

impl Default for PredictiveConfig {
    fn default() -> Self {
        Self {
            history_size: 1000,
            prediction_horizon: 10,
            confidence_threshold: 0.5,
            anomaly_threshold: 2.0, // 2 standard deviations
        }
    }
}

/// A snapshot of the agent's state at a particular time, including extracted features.
#[derive(Clone, Debug)]
pub struct StateSnapshot {
    pub observation: Observation,
    pub timestamp: Timestamp,
    pub features: Vec<f64>,
}

/// The predicted state of the environment at a future time.
#[derive(Clone, Debug)]
pub struct PredictedState {
    /// The observation representing the predicted state.
    pub observation: Observation,
    /// The model's confidence in this prediction (0.0 to 1.0).
    pub confidence: f64,
    /// The timestamp for which the state is predicted.
    pub timestamp: Timestamp,
}

/// A predicted sequence of future states and the total expected reward.
#[derive(Clone, Debug)]
pub struct Trajectory {
    /// The sequence of predicted future states.
    pub states: Vec<PredictedState>,
    /// The cumulative reward predicted over the trajectory.
    pub total_reward: f64,
    /// The overall confidence in the trajectory, typically the minimum confidence
    /// of any single state prediction in the sequence.
    pub confidence: f64,
}

impl PredictiveModel {
    /// Creates a new `PredictiveModel` with the given configuration.
    pub fn new(config: PredictiveConfig) -> Self {
        let discretization_bins = 100;
        Self {
            state_history: VecDeque::with_capacity(config.history_size),
            transition_model: TransitionModel::new(discretization_bins),
            reward_predictor: RewardPredictor::new(),
            anomaly_detector: AnomalyDetector::new(config.anomaly_threshold, config.history_size),
            config,
        }
    }

    /// Creates a `PredictiveModel` with a default configuration.
    pub fn with_default_config() -> Self {
        Self::new(PredictiveConfig::default())
    }

    /// Records an observation, updating the state history and anomaly detector.
    pub fn record(&mut self, obs: &Observation) {
        let features = self.extract_features(obs);
        let snapshot = StateSnapshot {
            observation: obs.clone(),
            timestamp: Timestamp::now(),
            features,
        };

        if self.state_history.len() >= self.config.history_size {
            self.state_history.pop_front();
        }
        self.state_history.push_back(snapshot);

        // Update anomaly detector
        self.anomaly_detector.update(obs);
    }

    /// Records a full state transition (`s, a, r, s'`).
    pub fn record_transition(
        &mut self,
        obs: &Observation,
        action: &Action,
        reward: f64,
        next_obs: &Observation,
    ) {
        // Record observations
        self.record(obs);
        self.record(next_obs);

        // Update transition model
        self.transition_model.record(obs, action, next_obs);

        // Update reward predictor
        self.reward_predictor.record(obs, action, reward);
    }

    /// Predicts the next state given the current state and an action.
    pub fn predict_next(&self, current: &Observation, action: &Action) -> PredictedState {
        let state_key = self.transition_model.predict(current, action);

        match state_key {
            Some(_key) => {
                // Get transition probabilities to estimate confidence
                let probs = self.transition_model.get_transition_probs(current, action);
                let confidence = probs
                    .values()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);

                // For now, return the current observation with updated confidence
                // In a more sophisticated implementation, we would decode the state_key
                PredictedState {
                    observation: current.clone(),
                    confidence,
                    timestamp: Timestamp::now(),
                }
            }
            None => {
                // No prediction available
                PredictedState {
                    observation: current.clone(),
                    confidence: 0.0,
                    timestamp: Timestamp::now(),
                }
            }
        }
    }

    /// Predicts a trajectory of future states for a given sequence of actions.
    pub fn predict_trajectory(&self, start: &Observation, actions: &[Action]) -> Trajectory {
        let mut states = Vec::with_capacity(actions.len() + 1);
        let mut total_reward = 0.0;
        let mut current = start.clone();
        let mut min_confidence: f64 = 1.0;

        for action in actions {
            let predicted = self.predict_next(&current, action);
            let reward = self.predict_reward(&current, action);

            total_reward += reward;
            min_confidence = min_confidence.min(predicted.confidence);

            states.push(predicted.clone());
            current = predicted.observation;
        }

        Trajectory {
            states,
            total_reward,
            confidence: min_confidence,
        }
    }

    /// Predicts the expected reward for taking a given action in a given state.
    pub fn predict_reward(&self, state: &Observation, action: &Action) -> f64 {
        self.reward_predictor.predict(state, action)
    }

    /// Returns `true` if the observation is considered anomalous based on historical data.
    pub fn is_anomaly(&self, obs: &Observation) -> bool {
        self.anomaly_detector.is_anomaly(obs)
    }

    /// Returns the confidence score of a `PredictedState`.
    pub fn get_confidence(&self, prediction: &PredictedState) -> f64 {
        prediction.confidence
    }

    /// Returns an estimate of the model's uncertainty about a given state.
    /// This uses the anomaly score as a proxy for uncertainty.
    pub fn get_uncertainty(&self, state: &Observation) -> f64 {
        // Use anomaly score as uncertainty measure
        let anomaly_score = self.anomaly_detector.anomaly_score(state);
        anomaly_score.min(1.0) // Cap at 1.0
    }

    /// Triggers the learning process for the model's internal components.
    pub fn learn(&mut self) {
        // In a more sophisticated implementation, this would:
        // - Update the transition model parameters
        // - Train neural network predictors
        // - Update confidence estimates
        // For now, the learning happens incrementally via record_transition
    }

    /// Returns a reference to the recent history of state snapshots.
    pub fn history(&self) -> &VecDeque<StateSnapshot> {
        &self.state_history
    }

    /// Extracts a feature vector from an observation for use in learning models.
    fn extract_features(&self, obs: &Observation) -> Vec<f64> {
        let mut features = Vec::new();

        // Extract numeric features from the observation value
        if let Some(f) = obs.value.as_f64() {
            features.push(f);
        } else if let Some(i) = obs.value.as_i64() {
            features.push(i as f64);
        } else if let Some(b) = obs.value.as_bool() {
            features.push(if b { 1.0 } else { 0.0 });
        }

        // Add timestamp feature (age in seconds)
        features.push(obs.age_secs() as f64);

        // Add confidence as feature
        features.push(obs.confidence.value() as f64);

        features
    }
}

/// A simple reward predictor that learns the average reward for a state-action pair.
pub struct RewardPredictor {
    // Maps (state_key, action_key) -> (total_reward, count)
    rewards: HashMap<(u64, u64), (f64, u64)>,
    encoder: StateEncoder,
}

impl RewardPredictor {
    /// Creates a new `RewardPredictor`.
    pub fn new() -> Self {
        Self {
            rewards: HashMap::new(),
            encoder: StateEncoder::new(100),
        }
    }

    /// Records a received reward for a given state-action pair.
    pub fn record(&mut self, state: &Observation, action: &Action, reward: f64) {
        let state_key = self.encoder.encode(state);
        let action_key = self.hash_action(action);

        let entry = self
            .rewards
            .entry((state_key, action_key))
            .or_insert((0.0, 0));
        entry.0 += reward;
        entry.1 += 1;
    }

    /// Predicts the average reward for a state-action pair.
    pub fn predict(&self, state: &Observation, action: &Action) -> f64 {
        let state_key = self.encoder.encode(state);
        let action_key = self.hash_action(action);

        self.rewards
            .get(&(state_key, action_key))
            .map(|(total, count)| total / (*count as f64))
            .unwrap_or(0.0)
    }

    fn hash_action(&self, action: &Action) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        action.action_type.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for RewardPredictor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActionType;

    #[test]
    fn test_predictive_model_creation() {
        let model = PredictiveModel::with_default_config();
        assert_eq!(model.history().len(), 0);
    }

    #[test]
    fn test_record_observation() {
        let mut model = PredictiveModel::with_default_config();
        let obs = Observation::sensor("temp", 20.0);

        model.record(&obs);
        assert_eq!(model.history().len(), 1);
    }

    #[test]
    fn test_predict_next_state() {
        let mut model = PredictiveModel::with_default_config();

        // Record some transitions
        let obs1 = Observation::sensor("temp", 20.0);
        let obs2 = Observation::sensor("temp", 21.0);
        let action = Action::new(ActionType::Custom("heat".to_string()));

        model.record_transition(&obs1, &action, 1.0, &obs2);
        model.learn();

        let pred = model.predict_next(&obs1, &action);
        assert!(pred.confidence >= 0.0);
    }

    #[test]
    fn test_predict_reward() {
        let mut model = PredictiveModel::with_default_config();

        let obs = Observation::sensor("temp", 20.0);
        let action = Action::new(ActionType::Custom("heat".to_string()));

        model.record_transition(&obs, &action, 5.0, &obs);

        let predicted_reward = model.predict_reward(&obs, &action);
        assert!((predicted_reward - 5.0).abs() < 0.1);
    }

    #[test]
    fn test_predict_trajectory() {
        let mut model = PredictiveModel::with_default_config();

        let obs = Observation::sensor("temp", 20.0);
        let action1 = Action::new(ActionType::Custom("heat".to_string()));
        let action2 = Action::new(ActionType::Wait);

        model.record_transition(&obs, &action1, 1.0, &obs);

        let trajectory = model.predict_trajectory(&obs, &[action1, action2]);
        assert_eq!(trajectory.states.len(), 2);
    }

    #[test]
    fn test_history_size_limit() {
        let config = PredictiveConfig {
            history_size: 5,
            ..Default::default()
        };
        let mut model = PredictiveModel::new(config);

        // Add more observations than history size
        for i in 0..10 {
            let obs = Observation::sensor("temp", i as f64);
            model.record(&obs);
        }

        assert_eq!(model.history().len(), 5);
    }
}
