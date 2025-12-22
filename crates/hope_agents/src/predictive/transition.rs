//! A model for learning and predicting state transitions.

use crate::{Action, Observation};
use std::collections::HashMap;

/// A model that learns a probabilistic representation of state transitions.
///
/// It records `(state, action, next_state)` tuples and uses them to predict
/// the most likely next state or a probability distribution over possible next states.
pub struct TransitionModel {
    // Maps (state_key, action_key) to statistics about the resulting next states.
    transitions: HashMap<(StateKey, ActionKey), TransitionStats>,
    state_encoder: StateEncoder,
}

/// Stores statistics for transitions from a single state-action pair.
#[derive(Clone, Default, Debug)]
pub struct TransitionStats {
    /// A map from a `StateKey` of a next state to the number of times it has occurred.
    pub next_states: HashMap<StateKey, u64>,
    /// The total number of times this transition has been observed.
    pub total_count: u64,
}

/// A discrete, hashed representation of an agent's state, derived from an `Observation`.
pub type StateKey = u64;
/// A discrete, hashed representation of an agent's action.
pub type ActionKey = u64;

/// Encodes continuous or complex `Observation`s into discrete `StateKey`s.
///
/// This is a simple implementation that discretizes numeric values into bins.
#[allow(dead_code)]
pub struct StateEncoder {
    discretization_bins: usize,
}

impl TransitionModel {
    /// Creates a new `TransitionModel`.
    ///
    /// # Arguments
    ///
    /// * `bins` - The number of bins to use for discretizing continuous observation values.
    pub fn new(bins: usize) -> Self {
        Self {
            transitions: HashMap::new(),
            state_encoder: StateEncoder::new(bins),
        }
    }

    /// Records an observed state transition.
    pub fn record(&mut self, state: &Observation, action: &Action, next_state: &Observation) {
        let state_key = self.state_encoder.encode(state);
        let next_state_key = self.state_encoder.encode(next_state);
        let action_key = self.hash_action(action);

        let stats = self.transitions.entry((state_key, action_key)).or_default();

        *stats.next_states.entry(next_state_key).or_insert(0) += 1;
        stats.total_count += 1;
    }

    /// Returns the probability distribution of possible next states for a given state and action.
    pub fn get_transition_probs(
        &self,
        state: &Observation,
        action: &Action,
    ) -> HashMap<StateKey, f64> {
        let state_key = self.state_encoder.encode(state);
        let action_key = self.hash_action(action);

        self.transitions
            .get(&(state_key, action_key))
            .map(|stats| {
                stats
                    .next_states
                    .iter()
                    .map(|(k, count)| (*k, *count as f64 / stats.total_count as f64))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Predicts the most likely next state for a given state and action.
    pub fn predict(&self, state: &Observation, action: &Action) -> Option<StateKey> {
        let state_key = self.state_encoder.encode(state);
        let action_key = self.hash_action(action);

        self.transitions
            .get(&(state_key, action_key))
            .and_then(|stats| {
                stats
                    .next_states
                    .iter()
                    .max_by_key(|(_, count)| *count)
                    .map(|(key, _)| *key)
            })
    }

    /// Returns the total number of times a specific state-action transition has been observed.
    pub fn get_count(&self, state: &Observation, action: &Action) -> u64 {
        let state_key = self.state_encoder.encode(state);
        let action_key = self.hash_action(action);

        self.transitions
            .get(&(state_key, action_key))
            .map(|stats| stats.total_count)
            .unwrap_or(0)
    }

    /// Hashes an `Action` to get a discrete `ActionKey`.
    fn hash_action(&self, action: &Action) -> ActionKey {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        action.action_type.hash(&mut hasher);
        hasher.finish()
    }
}

impl StateEncoder {
    /// Creates a new `StateEncoder`.
    pub fn new(bins: usize) -> Self {
        Self {
            discretization_bins: bins,
        }
    }

    /// Encodes an `Observation` into a discrete `StateKey`.
    pub fn encode(&self, obs: &Observation) -> StateKey {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash the observation type
        obs.obs_type.hash(&mut hasher);

        // Discretize and hash numeric values
        if let Some(f) = obs.value.as_f64() {
            let discretized = (f / 10.0).floor() as i64; // Discretize to bins of 10
            discretized.hash(&mut hasher);
        } else if let Some(i) = obs.value.as_i64() {
            let discretized = i / 10; // Discretize to bins of 10
            discretized.hash(&mut hasher);
        } else {
            // For non-numeric values, hash the string representation
            obs.value.as_string().hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Decodes a `StateKey` back into a feature vector.
    ///
    /// **Note:** This is a simplified implementation. In practice, a reverse
    /// mapping or a different encoding scheme would be needed for a meaningful decode.
    pub fn decode(&self, _key: StateKey) -> Vec<f64> {
        // Simplified: return empty vector
        // A real implementation would need to maintain a reverse mapping
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActionType;

    #[test]
    fn test_state_encoder() {
        let encoder = StateEncoder::new(100);

        let obs1 = Observation::sensor("temp", 20.0);
        let obs2 = Observation::sensor("temp", 20.5);
        let obs3 = Observation::sensor("temp", 30.0);

        let key1 = encoder.encode(&obs1);
        let key2 = encoder.encode(&obs2);
        let key3 = encoder.encode(&obs3);

        // Same bin should produce same key
        assert_eq!(key1, key2);
        // Different bin should produce different key
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_transition_model_record() {
        let mut model = TransitionModel::new(100);

        let obs1 = Observation::sensor("temp", 20.0);
        let obs2 = Observation::sensor("temp", 21.0);
        let action = Action::new(ActionType::Custom("heat".to_string()));

        model.record(&obs1, &action, &obs2);

        let count = model.get_count(&obs1, &action);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_transition_model_predict() {
        let mut model = TransitionModel::new(100);

        let obs1 = Observation::sensor("temp", 20.0);
        let obs2 = Observation::sensor("temp", 21.0);
        let action = Action::new(ActionType::Custom("heat".to_string()));

        // Record multiple transitions
        model.record(&obs1, &action, &obs2);
        model.record(&obs1, &action, &obs2);

        let predicted = model.predict(&obs1, &action);
        assert!(predicted.is_some());
    }

    #[test]
    fn test_transition_probabilities() {
        let mut model = TransitionModel::new(100);

        let obs1 = Observation::sensor("temp", 20.0);
        let obs2 = Observation::sensor("temp", 21.0);
        let obs3 = Observation::sensor("temp", 22.0);
        let action = Action::new(ActionType::Custom("heat".to_string()));

        // Record transitions with different outcomes
        model.record(&obs1, &action, &obs2);
        model.record(&obs1, &action, &obs2);
        model.record(&obs1, &action, &obs3);

        let probs = model.get_transition_probs(&obs1, &action);
        assert!(!probs.is_empty());

        // Check that probabilities sum to approximately 1.0
        let sum: f64 = probs.values().sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_transition_stats() {
        let mut stats = TransitionStats::default();

        stats.next_states.insert(1, 5);
        stats.next_states.insert(2, 3);
        stats.total_count = 8;

        assert_eq!(stats.total_count, 8);
        assert_eq!(stats.next_states.len(), 2);
    }
}
