//! The core reinforcement learning engine for HOPE Agents.
//!
//! Provides implementations of reinforcement learning algorithms including
//! Q-Learning, SARSA, and others, along with experience replay.

use crate::action::Action;
use crate::observation::Observation;
use crate::types::{Confidence, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// A unique identifier for a state, typically derived from an `Observation`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateId(String);

impl StateId {
    /// Creates a `StateId` from an `Observation`.
    /// This creates a simplified string representation to identify the state.
    pub fn from_observation(obs: &Observation) -> Self {
        // Create a simplified state representation
        let state_str = format!("{:?}_{}", obs.obs_type, obs.value.as_string());
        Self(state_str)
    }

    /// Creates a `StateId` from a raw string.
    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A unique identifier for an action, typically derived from an `Action`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionId(String);

impl ActionId {
    /// Creates an `ActionId` from an `Action`.
    pub fn from_action(action: &Action) -> Self {
        Self(format!("{:?}", action.action_type))
    }

    /// Creates an `ActionId` from a raw string.
    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A composite key representing a state-action pair, used for indexing Q-values.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateActionPair {
    pub state: StateId,
    pub action: ActionId,
}

impl StateActionPair {
    pub fn new(state: StateId, action: ActionId) -> Self {
        Self { state, action }
    }
}

/// Represents the learned value (Q-value) of a state-action pair, with associated statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QValue {
    /// The mean learned value for this state-action pair.
    pub mean: f64,
    /// The variance of the Q-value, indicating uncertainty.
    pub variance: f64,
    /// The number of times this Q-value has been updated.
    pub update_count: u64,
    /// The timestamp of the last update.
    pub last_updated: Timestamp,
}

impl QValue {
    /// Creates a new `QValue` with an initial value.
    pub fn new(initial_value: f64) -> Self {
        Self {
            mean: initial_value,
            variance: 0.0,
            update_count: 0,
            last_updated: Timestamp::now(),
        }
    }

    /// Updates the Q-value using a new sample and a learning rate.
    /// This uses Welford's algorithm for incremental variance calculation.
    pub fn update(&mut self, new_value: f64, learning_rate: f64) {
        let old_mean = self.mean;

        // Incremental mean update
        self.mean += learning_rate * (new_value - self.mean);

        // Incremental variance update (Welford's algorithm)
        if self.update_count > 0 {
            let delta = new_value - old_mean;
            let delta2 = new_value - self.mean;
            self.variance =
                self.variance + (delta * delta2 - self.variance) / (self.update_count as f64);
        }

        self.update_count += 1;
        self.last_updated = Timestamp::now();
    }

    /// Returns the confidence in this Q-value, based on update count and variance.
    pub fn confidence(&self) -> Confidence {
        if self.update_count == 0 {
            return Confidence::new(0.0);
        }

        // Confidence increases with more updates and decreases with higher variance
        let count_factor = (self.update_count as f32).min(100.0) / 100.0;
        let variance_factor = 1.0 / (1.0 + self.variance as f32);

        Confidence::new(count_factor * variance_factor)
    }
}

impl Default for QValue {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// The reinforcement learning algorithm to be used by the `LearningEngine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LearningAlgorithm {
    /// Q-Learning, an off-policy temporal difference algorithm.
    #[default]
    QLearning,
    /// SARSA, an on-policy temporal difference algorithm.
    SARSA,
    /// Expected SARSA, which uses the expected value of the next state-action pair.
    ExpectedSARSA,
    /// Basic Temporal Difference learning.
    TemporalDifference,
}

/// A single `(state, action, reward, next_state)` tuple, used for experience replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub state: StateId,
    pub action: ActionId,
    pub reward: f64,
    pub next_state: StateId,
    pub next_action: Option<ActionId>,
    pub done: bool,
    pub timestamp: Timestamp,
}

impl Experience {
    /// Creates a new `Experience` tuple.
    pub fn new(
        state: StateId,
        action: ActionId,
        reward: f64,
        next_state: StateId,
        done: bool,
    ) -> Self {
        Self {
            state,
            action,
            reward,
            next_state,
            next_action: None,
            done,
            timestamp: Timestamp::now(),
        }
    }

    /// Associates the next action with the experience (for SARSA).
    pub fn with_next_action(mut self, next_action: ActionId) -> Self {
        self.next_action = Some(next_action);
        self
    }
}

/// Configuration for the `LearningEngine`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConfig {
    /// The learning rate (alpha), determining how much new information overrides old information.
    pub learning_rate: f64,
    /// The discount factor (gamma), determining the importance of future rewards.
    pub discount_factor: f64,
    /// The learning algorithm to use.
    pub algorithm: LearningAlgorithm,
    /// The initial Q-value for new state-action pairs.
    pub initial_q_value: f64,
    /// The maximum number of experiences to store in the replay buffer.
    pub replay_buffer_size: usize,
    /// The minimum number of experiences required before starting experience replay.
    pub min_replay_size: usize,
    /// The exploration rate (epsilon) for the epsilon-greedy strategy.
    pub epsilon: f64,
    /// The rate at which epsilon decays after each episode.
    pub epsilon_decay: f64,
    /// The minimum value that epsilon can decay to.
    pub epsilon_min: f64,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.1,
            discount_factor: 0.99,
            algorithm: LearningAlgorithm::QLearning,
            initial_q_value: 0.0,
            replay_buffer_size: 10000,
            min_replay_size: 100,
            epsilon: 0.1,
            epsilon_decay: 0.995,
            epsilon_min: 0.01,
        }
    }
}

/// The main reinforcement learning engine for HOPE Agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEngine {
    /// The table of learned Q-values for state-action pairs.
    q_values: HashMap<StateActionPair, QValue>,
    /// The configuration for the learning process.
    config: LearningConfig,
    /// The experience replay buffer.
    replay_buffer: VecDeque<Experience>,
    /// Statistics on the total number of learning updates performed.
    total_updates: u64,
    /// Statistics on the total number of episodes completed.
    total_episodes: u64,
}

impl LearningEngine {
    /// Creates a new `LearningEngine` with the given configuration.
    pub fn new(config: LearningConfig) -> Self {
        Self {
            q_values: HashMap::new(),
            config,
            replay_buffer: VecDeque::new(),
            total_updates: 0,
            total_episodes: 0,
        }
    }

    /// Creates a `LearningEngine` with a default configuration.
    pub fn default_config() -> Self {
        Self::new(LearningConfig::default())
    }

    /// Gets the Q-value for a given state-action pair.
    pub fn get_q_value(&self, state: &StateId, action: &ActionId) -> f64 {
        let pair = StateActionPair::new(state.clone(), action.clone());
        self.q_values
            .get(&pair)
            .map(|qv| qv.mean)
            .unwrap_or(self.config.initial_q_value)
    }

    /// Gets the full `QValue` struct (including statistics) for a state-action pair.
    pub fn get_q_value_stats(&self, state: &StateId, action: &ActionId) -> QValue {
        let pair = StateActionPair::new(state.clone(), action.clone());
        self.q_values
            .get(&pair)
            .cloned()
            .unwrap_or_else(|| QValue::new(self.config.initial_q_value))
    }

    /// Sets the Q-value for a state-action pair.
    fn set_q_value(&mut self, state: &StateId, action: &ActionId, new_q: f64) {
        let pair = StateActionPair::new(state.clone(), action.clone());
        let qvalue = self
            .q_values
            .entry(pair)
            .or_insert_with(|| QValue::new(self.config.initial_q_value));

        qvalue.update(new_q, self.config.learning_rate);
        self.total_updates += 1;
    }

    /// Gets the maximum Q-value for a given state across all available actions.
    fn get_max_q_value(&self, state: &StateId, available_actions: &[ActionId]) -> f64 {
        if available_actions.is_empty() {
            return self.config.initial_q_value;
        }

        available_actions
            .iter()
            .map(|action| self.get_q_value(state, action))
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Gets the average Q-value for a state, used for Expected SARSA.
    fn get_avg_q_value(&self, state: &StateId, available_actions: &[ActionId]) -> f64 {
        if available_actions.is_empty() {
            return self.config.initial_q_value;
        }

        let sum: f64 = available_actions
            .iter()
            .map(|action| self.get_q_value(state, action))
            .sum();

        sum / (available_actions.len() as f64)
    }

    /// Performs a Q-Learning update: `Q(s,a) += α * (r + γ * max_a' Q(s',a') - Q(s,a))`.
    pub fn update_q_learning(
        &mut self,
        state: &StateId,
        action: &ActionId,
        reward: f64,
        next_state: &StateId,
        available_actions: &[ActionId],
    ) {
        let current_q = self.get_q_value(state, action);
        let max_next_q = self.get_max_q_value(next_state, available_actions);

        // Bellman equation for Q-Learning
        let td_target = reward + self.config.discount_factor * max_next_q;
        let new_q = current_q + self.config.learning_rate * (td_target - current_q);

        self.set_q_value(state, action, new_q);
    }

    /// Performs a SARSA update: `Q(s,a) += α * (r + γ * Q(s',a') - Q(s,a))`.
    pub fn update_sarsa(
        &mut self,
        state: &StateId,
        action: &ActionId,
        reward: f64,
        next_state: &StateId,
        next_action: &ActionId,
    ) {
        let current_q = self.get_q_value(state, action);
        let next_q = self.get_q_value(next_state, next_action);

        // SARSA uses the actual next action (on-policy)
        let td_target = reward + self.config.discount_factor * next_q;
        let new_q = current_q + self.config.learning_rate * (td_target - current_q);

        self.set_q_value(state, action, new_q);
    }

    /// Performs an Expected SARSA update, using the average expected value of the next state.
    pub fn update_expected_sarsa(
        &mut self,
        state: &StateId,
        action: &ActionId,
        reward: f64,
        next_state: &StateId,
        available_actions: &[ActionId],
    ) {
        let current_q = self.get_q_value(state, action);
        let expected_next_q = self.get_avg_q_value(next_state, available_actions);

        let td_target = reward + self.config.discount_factor * expected_next_q;
        let new_q = current_q + self.config.learning_rate * (td_target - current_q);

        self.set_q_value(state, action, new_q);
    }

    /// Performs a Temporal Difference (TD) update.
    pub fn update_td(
        &mut self,
        state: &StateId,
        action: &ActionId,
        reward: f64,
        next_state: &StateId,
        available_actions: &[ActionId],
    ) {
        // For TD, we use the average Q-value as the state value estimate
        let current_q = self.get_q_value(state, action);
        let next_value = self.get_avg_q_value(next_state, available_actions);

        let td_target = reward + self.config.discount_factor * next_value;
        let new_q = current_q + self.config.learning_rate * (td_target - current_q);

        self.set_q_value(state, action, new_q);
    }

    /// Performs a learning update using the algorithm specified in the `LearningConfig`.
    pub fn update(
        &mut self,
        state: &StateId,
        action: &ActionId,
        reward: f64,
        next_state: &StateId,
        next_action: Option<&ActionId>,
        available_actions: &[ActionId],
    ) {
        match self.config.algorithm {
            LearningAlgorithm::QLearning => {
                self.update_q_learning(state, action, reward, next_state, available_actions);
            }
            LearningAlgorithm::SARSA => {
                if let Some(next_act) = next_action {
                    self.update_sarsa(state, action, reward, next_state, next_act);
                } else {
                    // Fallback to Q-learning if next action not provided
                    self.update_q_learning(state, action, reward, next_state, available_actions);
                }
            }
            LearningAlgorithm::ExpectedSARSA => {
                self.update_expected_sarsa(state, action, reward, next_state, available_actions);
            }
            LearningAlgorithm::TemporalDifference => {
                self.update_td(state, action, reward, next_state, available_actions);
            }
        }
    }

    /// Returns the best action for a given state (pure exploitation).
    pub fn get_best_action(
        &self,
        state: &StateId,
        available_actions: &[ActionId],
    ) -> Option<ActionId> {
        if available_actions.is_empty() {
            return None;
        }

        available_actions
            .iter()
            .max_by(|a, b| {
                let qa = self.get_q_value(state, a);
                let qb = self.get_q_value(state, b);
                qa.partial_cmp(&qb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    /// Returns an action for a given state using an epsilon-greedy strategy.
    pub fn get_action_epsilon_greedy(
        &self,
        state: &StateId,
        available_actions: &[ActionId],
    ) -> Option<ActionId> {
        use rand::Rng;

        if available_actions.is_empty() {
            return None;
        }

        let mut rng = rand::thread_rng();

        // Exploration vs exploitation
        if rng.gen::<f64>() < self.config.epsilon {
            // Explore: choose a random action
            let idx = rng.gen_range(0..available_actions.len());
            Some(available_actions[idx].clone())
        } else {
            // Exploit: choose the best known action
            self.get_best_action(state, available_actions)
        }
    }

    /// Decays the exploration rate (epsilon), typically called at the end of an episode.
    pub fn decay_epsilon(&mut self) {
        self.config.epsilon =
            (self.config.epsilon * self.config.epsilon_decay).max(self.config.epsilon_min);
    }

    /// Adds an `Experience` tuple to the replay buffer.
    pub fn add_experience(&mut self, experience: Experience) {
        if self.replay_buffer.len() >= self.config.replay_buffer_size {
            self.replay_buffer.pop_front();
        }
        self.replay_buffer.push_back(experience);
    }

    /// Performs experience replay by sampling a batch from the buffer and re-learning from it.
    pub fn replay_batch(&mut self, batch_size: usize, available_actions: &[ActionId]) {
        use rand::seq::SliceRandom;

        if self.replay_buffer.len() < self.config.min_replay_size {
            return;
        }

        let mut rng = rand::thread_rng();

        // Clone experiences to avoid borrowing issues
        let experiences: Vec<Experience> = self.replay_buffer.iter().cloned().collect();

        // Sample random batch
        let sample_size = batch_size.min(experiences.len());
        let batch: Vec<&Experience> = experiences.choose_multiple(&mut rng, sample_size).collect();

        // Update from batch
        for exp in batch {
            self.update(
                &exp.state,
                &exp.action,
                exp.reward,
                &exp.next_state,
                exp.next_action.as_ref(),
                available_actions,
            );
        }
    }

    /// Marks the end of a learning episode and decays epsilon.
    pub fn end_episode(&mut self) {
        self.total_episodes += 1;
        self.decay_epsilon();
    }

    /// Returns the total number of learning updates performed.
    pub fn total_updates(&self) -> u64 {
        self.total_updates
    }

    /// Returns the total number of episodes completed.
    pub fn total_episodes(&self) -> u64 {
        self.total_episodes
    }

    /// Returns the number of unique state-action pairs in the Q-table.
    pub fn state_action_count(&self) -> usize {
        self.q_values.len()
    }

    /// Returns the current exploration rate (epsilon).
    pub fn epsilon(&self) -> f64 {
        self.config.epsilon
    }

    /// Clears all learned Q-values and the experience replay buffer.
    pub fn reset(&mut self) {
        self.q_values.clear();
        self.replay_buffer.clear();
        self.total_updates = 0;
        self.total_episodes = 0;
    }

    /// Returns a reference to the learning configuration.
    pub fn config(&self) -> &LearningConfig {
        &self.config
    }

    /// Returns a mutable reference to the learning configuration.
    pub fn config_mut(&mut self) -> &mut LearningConfig {
        &mut self.config
    }

    /// Returns a reference to the entire Q-value table for inspection.
    pub fn get_all_q_values(&self) -> &HashMap<StateActionPair, QValue> {
        &self.q_values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_state_id(name: &str) -> StateId {
        StateId::from_string(name.to_string())
    }

    fn create_action_id(name: &str) -> ActionId {
        ActionId::from_string(name.to_string())
    }

    #[test]
    fn test_learning_engine_creation() {
        let engine = LearningEngine::default_config();
        assert_eq!(engine.total_updates(), 0);
        assert_eq!(engine.state_action_count(), 0);
    }

    #[test]
    fn test_q_value_update() {
        let mut qvalue = QValue::new(0.0);
        assert_eq!(qvalue.mean, 0.0);
        assert_eq!(qvalue.update_count, 0);

        qvalue.update(1.0, 0.1);
        assert!(qvalue.mean > 0.0);
        assert_eq!(qvalue.update_count, 1);
    }

    #[test]
    fn test_q_learning_update() {
        let mut engine = LearningEngine::default_config();

        let s0 = create_state_id("state0");
        let a0 = create_action_id("action0");
        let s1 = create_state_id("state1");
        let actions = vec![a0.clone()];

        // Initial Q-value should be 0
        assert_eq!(engine.get_q_value(&s0, &a0), 0.0);

        // Update with positive reward
        engine.update_q_learning(&s0, &a0, 1.0, &s1, &actions);

        // Q-value should have increased
        assert!(engine.get_q_value(&s0, &a0) > 0.0);
        assert_eq!(engine.total_updates(), 1);
    }

    #[test]
    fn test_sarsa_update() {
        let mut engine = LearningEngine::default_config();

        let s0 = create_state_id("state0");
        let a0 = create_action_id("action0");
        let s1 = create_state_id("state1");
        let a1 = create_action_id("action1");

        engine.update_sarsa(&s0, &a0, 1.0, &s1, &a1);

        assert!(engine.get_q_value(&s0, &a0) > 0.0);
    }

    #[test]
    fn test_get_best_action() {
        let mut engine = LearningEngine::default_config();

        let state = create_state_id("state");
        let action1 = create_action_id("action1");
        let action2 = create_action_id("action2");
        let actions = vec![action1.clone(), action2.clone()];

        // Set different Q-values
        engine.set_q_value(&state, &action1, 0.5);
        engine.set_q_value(&state, &action2, 1.0);

        let best = engine.get_best_action(&state, &actions);
        assert_eq!(best.unwrap(), action2);
    }

    #[test]
    fn test_experience_replay() {
        let mut engine = LearningEngine::default_config();

        let s0 = create_state_id("state0");
        let a0 = create_action_id("action0");
        let s1 = create_state_id("state1");
        let actions = vec![a0.clone()];

        // Add experiences
        for _ in 0..10 {
            let exp = Experience::new(s0.clone(), a0.clone(), 1.0, s1.clone(), false);
            engine.add_experience(exp);
        }

        assert_eq!(engine.replay_buffer.len(), 10);

        // Replay should not happen if min_replay_size not met
        engine.config.min_replay_size = 100;
        engine.replay_batch(5, &actions);
        assert_eq!(engine.total_updates(), 0);

        // Now allow replay
        engine.config.min_replay_size = 5;
        engine.replay_batch(5, &actions);
        assert!(engine.total_updates() > 0);
    }

    #[test]
    fn test_epsilon_decay() {
        let mut config = LearningConfig::default();
        config.epsilon = 1.0;
        config.epsilon_decay = 0.9;
        config.epsilon_min = 0.1;

        let mut engine = LearningEngine::new(config);

        let initial_epsilon = engine.epsilon();
        engine.decay_epsilon();

        assert!(engine.epsilon() < initial_epsilon);
        assert!(engine.epsilon() >= 0.1);
    }

    #[test]
    fn test_episode_management() {
        let mut engine = LearningEngine::default_config();

        assert_eq!(engine.total_episodes(), 0);

        engine.end_episode();
        assert_eq!(engine.total_episodes(), 1);
    }

    #[test]
    fn test_state_action_id_from_types() {
        let obs = Observation::sensor("temperature", 25.0);
        let state_id = StateId::from_observation(&obs);
        assert!(state_id.as_str().contains("temperature"));

        let action = Action::alert("test");
        let action_id = ActionId::from_action(&action);
        assert!(action_id.as_str().contains("Alert"));
    }
}
