//! Value function approximation for HOPE Agents.
//!
//! Provides different methods for approximating state-value functions (V-functions),
//! which estimate how good it is for an agent to be in a given state.
//!
//! - **Tabular:** A simple lookup table, mapping each state to a value.
//! - **Linear:** A linear combination of features extracted from a state.
//!
//! These are typically used in value-based reinforcement learning algorithms.

use crate::learning::engine::StateId;
use crate::observation::Observation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A trait for state-value function approximators.
pub trait ValueFunction {
    /// Evaluates the estimated value of a given state.
    fn evaluate(&self, state: &StateId) -> f64;

    /// Updates the value function for a state based on a target value and learning rate.
    fn update(&mut self, state: &StateId, target: f64, learning_rate: f64);

    /// Resets the value function to its initial state.
    fn reset(&mut self);

    /// Returns the number of states or features tracked by the function.
    fn size(&self) -> usize;
}

/// A simple tabular value function that uses a `HashMap` as a lookup table.
///
/// This is suitable for environments with a small, discrete number of states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabularValueFunction {
    /// A map from a `StateId` to its estimated value.
    values: HashMap<StateId, f64>,
    /// The default value to return for states not yet present in the table.
    default_value: f64,
}

impl TabularValueFunction {
    /// Creates a new `TabularValueFunction` with a specified default value.
    pub fn new(default_value: f64) -> Self {
        Self {
            values: HashMap::new(),
            default_value,
        }
    }

    /// Returns a reference to the internal map of all state values.
    pub fn get_all_values(&self) -> &HashMap<StateId, f64> {
        &self.values
    }

    /// Directly sets the value for a specific state.
    pub fn set_value(&mut self, state: &StateId, value: f64) {
        self.values.insert(state.clone(), value);
    }
}

impl Default for TabularValueFunction {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl ValueFunction for TabularValueFunction {
    fn evaluate(&self, state: &StateId) -> f64 {
        self.values
            .get(state)
            .copied()
            .unwrap_or(self.default_value)
    }

    fn update(&mut self, state: &StateId, target: f64, learning_rate: f64) {
        let current = self.evaluate(state);
        let new_value = current + learning_rate * (target - current);
        self.values.insert(state.clone(), new_value);
    }

    fn reset(&mut self) {
        self.values.clear();
    }

    fn size(&self) -> usize {
        self.values.len()
    }
}

/// A function that extracts a feature vector (`Vec<f64>`) from an `Observation`.
pub type FeatureExtractor = Box<dyn Fn(&Observation) -> Vec<f64> + Send + Sync>;

/// A value function approximator that uses a linear combination of features.
///
/// The value of a state `s` is calculated as `V(s) = w · φ(s)`, where `w` is a weight
/// vector and `φ(s)` is a feature vector extracted from the state.
/// This is useful for environments with large or continuous state spaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearValueFunction {
    /// The vector of weights for the linear function.
    weights: Vec<f64>,
    /// The number of features expected in the feature vector.
    num_features: usize,
    /// The default value to return if features cannot be evaluated.
    default_value: f64,
    /// A count of updates performed for each feature, for statistical purposes.
    #[serde(skip)]
    update_counts: Vec<u64>,
}

impl LinearValueFunction {
    /// Creates a new `LinearValueFunction` with a specified number of features.
    pub fn new(num_features: usize, default_value: f64) -> Self {
        Self {
            weights: vec![0.0; num_features],
            num_features,
            default_value,
            update_counts: vec![0; num_features],
        }
    }

    /// Creates a new `LinearValueFunction` with a predefined set of initial weights.
    pub fn with_weights(weights: Vec<f64>) -> Self {
        let num_features = weights.len();
        Self {
            weights,
            num_features,
            default_value: 0.0,
            update_counts: vec![0; num_features],
        }
    }

    /// Evaluates the value of a state given its feature vector.
    /// This is the dot product of the weights and the features.
    pub fn evaluate_features(&self, features: &[f64]) -> f64 {
        if features.len() != self.num_features {
            return self.default_value;
        }

        // Dot product of weights and features
        self.weights
            .iter()
            .zip(features.iter())
            .map(|(w, f)| w * f)
            .sum()
    }

    /// Updates the weights using gradient descent.
    /// The update rule is: `w = w + α * error * features`.
    pub fn update_features(&mut self, features: &[f64], target: f64, learning_rate: f64) {
        if features.len() != self.num_features {
            return;
        }

        let prediction = self.evaluate_features(features);
        let error = target - prediction;

        // Gradient descent update
        for (i, &feature) in features.iter().enumerate() {
            self.weights[i] += learning_rate * error * feature;
            self.update_counts[i] += 1;
        }
    }

    /// Returns a slice of the current weights.
    pub fn get_weights(&self) -> &[f64] {
        &self.weights
    }

    /// Returns a slice of the update counts for each feature.
    pub fn get_update_counts(&self) -> &[u64] {
        &self.update_counts
    }

    /// Returns the number of features the function is configured for.
    pub fn num_features(&self) -> usize {
        self.num_features
    }
}

impl Default for LinearValueFunction {
    fn default() -> Self {
        Self::new(10, 0.0)
    }
}

impl ValueFunction for LinearValueFunction {
    fn evaluate(&self, _state: &StateId) -> f64 {
        // Linear value function needs features, which we don't have from StateId alone.
        // Users should call `evaluate_features` directly with a feature vector.
        self.default_value
    }

    fn update(&mut self, _state: &StateId, _target: f64, _learning_rate: f64) {
        // This method is here to satisfy the trait but is a no-op.
        // Users should call `update_features` directly.
    }

    fn reset(&mut self) {
        self.weights.fill(0.0);
        self.update_counts.fill(0);
    }

    fn size(&self) -> usize {
        self.num_features
    }
}

/// A collection of simple, common feature extractor functions.
pub mod feature_extractors {
    use super::*;

    /// Extracts the numeric value from an observation as a single feature.
    pub fn numeric_value(obs: &Observation) -> Vec<f64> {
        vec![obs.value.as_f64().unwrap_or(0.0)]
    }

    /// Extracts the numeric value and confidence score as two features.
    pub fn value_and_confidence(obs: &Observation) -> Vec<f64> {
        vec![
            obs.value.as_f64().unwrap_or(0.0),
            obs.confidence.value() as f64,
        ]
    }

    /// Extracts the numeric value, confidence, and age as three features.
    pub fn value_confidence_age(obs: &Observation) -> Vec<f64> {
        vec![
            obs.value.as_f64().unwrap_or(0.0),
            obs.confidence.value() as f64,
            obs.age_secs() as f64,
        ]
    }

    /// Creates polynomial features (1, x, x^2, x^3) for a numeric observation value.
    /// Useful for approximating non-linear functions.
    pub fn polynomial_features(obs: &Observation) -> Vec<f64> {
        let x = obs.value.as_f64().unwrap_or(0.0);
        vec![1.0, x, x * x, x * x * x]
    }

    /// Creates a feature extractor that normalizes a numeric value to a [0, 1] range.
    pub fn normalized_value(min: f64, max: f64) -> impl Fn(&Observation) -> Vec<f64> {
        move |obs: &Observation| {
            let value = obs.value.as_f64().unwrap_or(0.0);
            let normalized = if max > min {
                ((value - min) / (max - min)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            vec![normalized]
        }
    }

    /// Creates a feature extractor that returns a binary feature (1.0 or 0.0)
    /// based on whether a value is above a threshold.
    pub fn threshold_feature(threshold: f64) -> impl Fn(&Observation) -> Vec<f64> {
        move |obs: &Observation| {
            let value = obs.value.as_f64().unwrap_or(0.0);
            vec![if value > threshold { 1.0 } else { 0.0 }]
        }
    }

    /// Creates a feature extractor that generates a vector of binary features,
    /// one for each provided threshold.
    pub fn multi_threshold_features(thresholds: Vec<f64>) -> impl Fn(&Observation) -> Vec<f64> {
        move |obs: &Observation| {
            let value = obs.value.as_f64().unwrap_or(0.0);
            thresholds
                .iter()
                .map(|&t| if value > t { 1.0 } else { 0.0 })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::Observation;

    fn create_state_id(name: &str) -> StateId {
        StateId::from_string(name.to_string())
    }

    #[test]
    fn test_tabular_value_function() {
        let mut vf = TabularValueFunction::new(0.0);

        let state = create_state_id("state1");

        // Initial value should be default
        assert_eq!(vf.evaluate(&state), 0.0);

        // Update value
        vf.update(&state, 1.0, 0.1);
        assert!(vf.evaluate(&state) > 0.0);

        // Multiple updates
        vf.update(&state, 1.0, 0.1);
        assert!(vf.evaluate(&state) > 0.0);

        assert_eq!(vf.size(), 1);
    }

    #[test]
    fn test_tabular_value_function_reset() {
        let mut vf = TabularValueFunction::new(0.0);
        let state = create_state_id("state1");

        vf.update(&state, 1.0, 0.1);
        assert!(vf.size() > 0);

        vf.reset();
        assert_eq!(vf.size(), 0);
        assert_eq!(vf.evaluate(&state), 0.0);
    }

    #[test]
    fn test_linear_value_function() {
        let mut vf = LinearValueFunction::new(3, 0.0);

        let features = vec![1.0, 2.0, 3.0];

        // Initial evaluation should be 0
        assert_eq!(vf.evaluate_features(&features), 0.0);

        // Update
        vf.update_features(&features, 10.0, 0.1);

        // Value should change
        let value = vf.evaluate_features(&features);
        assert!(value > 0.0);

        assert_eq!(vf.num_features(), 3);
    }

    #[test]
    fn test_linear_value_function_with_weights() {
        let weights = vec![1.0, 2.0, 3.0];
        let vf = LinearValueFunction::with_weights(weights);

        let features = vec![1.0, 1.0, 1.0];
        // Should be 1*1 + 2*1 + 3*1 = 6
        assert_eq!(vf.evaluate_features(&features), 6.0);
    }

    #[test]
    fn test_feature_extractor_numeric() {
        let obs = Observation::sensor("temp", 25.5);
        let features = feature_extractors::numeric_value(&obs);

        assert_eq!(features.len(), 1);
        assert_eq!(features[0], 25.5);
    }

    #[test]
    fn test_feature_extractor_value_confidence() {
        let obs = Observation::sensor("temp", 25.0).with_confidence(0.9);
        let features = feature_extractors::value_and_confidence(&obs);

        assert_eq!(features.len(), 2);
        assert_eq!(features[0], 25.0);
        // Use approximate comparison for floating point
        assert!((features[1] - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_feature_extractor_polynomial() {
        let obs = Observation::sensor("temp", 2.0);
        let features = feature_extractors::polynomial_features(&obs);

        assert_eq!(features.len(), 4);
        assert_eq!(features[0], 1.0); // constant
        assert_eq!(features[1], 2.0); // x
        assert_eq!(features[2], 4.0); // x^2
        assert_eq!(features[3], 8.0); // x^3
    }

    #[test]
    fn test_feature_extractor_normalized() {
        let extractor = feature_extractors::normalized_value(0.0, 100.0);
        let obs = Observation::sensor("temp", 50.0);
        let features = extractor(&obs);

        assert_eq!(features.len(), 1);
        assert_eq!(features[0], 0.5);
    }

    #[test]
    fn test_feature_extractor_threshold() {
        let extractor = feature_extractors::threshold_feature(30.0);

        let obs_low = Observation::sensor("temp", 20.0);
        let features_low = extractor(&obs_low);
        assert_eq!(features_low[0], 0.0);

        let obs_high = Observation::sensor("temp", 40.0);
        let features_high = extractor(&obs_high);
        assert_eq!(features_high[0], 1.0);
    }

    #[test]
    fn test_feature_extractor_multi_threshold() {
        let extractor = feature_extractors::multi_threshold_features(vec![20.0, 30.0, 40.0]);
        let obs = Observation::sensor("temp", 35.0);
        let features = extractor(&obs);

        assert_eq!(features.len(), 3);
        assert_eq!(features[0], 1.0); // > 20
        assert_eq!(features[1], 1.0); // > 30
        assert_eq!(features[2], 0.0); // < 40
    }
}
