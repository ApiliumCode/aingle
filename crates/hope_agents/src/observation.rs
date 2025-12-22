//! Observation types for HOPE Agents.
//!
//! Observations represent the data an agent perceives from its environment,
//! forming the basis for its state and decision-making processes.

use crate::types::{Confidence, Timestamp, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The type or category of an [`Observation`].
///
/// `ObservationType` classifies observations by their source or nature, helping agents
/// understand and prioritize different kinds of environmental input.
///
/// # Examples
///
/// ```
/// # use hope_agents::ObservationType;
/// let sensor_type = ObservationType::sensor("temperature");
/// let network_type = ObservationType::network("peer_connected");
/// let alert_type = ObservationType::alert("System overload");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObservationType {
    /// A reading from a physical or virtual sensor (e.g., temperature, stock price).
    Sensor(String),
    /// An event from the network (e.g., a message was received, a peer connected).
    Network(String),
    /// An input provided directly by a user.
    UserInput(String),
    /// A notification about a change in the agent's own internal state.
    StateChange(String),
    /// An event triggered by a timer or schedule.
    Timer(String),
    /// An alert or error condition.
    Alert(String),
    /// A user-defined custom observation type.
    Custom(String),
}

impl ObservationType {
    /// Creates a `Sensor` observation type.
    ///
    /// # Arguments
    ///
    /// * `name` - The name or identifier of the sensor
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ObservationType;
    /// let obs_type = ObservationType::sensor("temperature");
    /// ```
    pub fn sensor(name: &str) -> Self {
        ObservationType::Sensor(name.to_string())
    }

    /// Creates a `Network` observation type.
    ///
    /// # Arguments
    ///
    /// * `event` - The name or type of the network event
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ObservationType;
    /// let obs_type = ObservationType::network("peer_connected");
    /// ```
    pub fn network(event: &str) -> Self {
        ObservationType::Network(event.to_string())
    }

    /// Creates an `Alert` observation type.
    ///
    /// # Arguments
    ///
    /// * `msg` - The alert message
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::ObservationType;
    /// let obs_type = ObservationType::alert("Critical error");
    /// ```
    pub fn alert(msg: &str) -> Self {
        ObservationType::Alert(msg.to_string())
    }
}

/// Represents a single piece of information perceived by an agent from its environment.
///
/// An `Observation` is the primary input to an agent's decision-making process.
/// It encapsulates environmental data along with metadata like timestamps, confidence
/// scores, and additional context.
///
/// # Examples
///
/// ```
/// # use hope_agents::Observation;
/// // Simple sensor observation
/// let temp_obs = Observation::sensor("temperature", 23.5);
///
/// // Observation with confidence and metadata
/// let uncertain_obs = Observation::sensor("humidity", 65.0)
///     .with_confidence(0.8)
///     .with_metadata("location", "room_a");
/// ```
///
/// # See Also
///
/// - [`ObservationType`] for different observation categories
/// - [`Sensor`] trait for observation sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// The category of the observation.
    pub obs_type: ObservationType,
    /// The value or data associated with the observation.
    pub value: Value,
    /// The timestamp of when the observation was made.
    pub timestamp: Timestamp,
    /// A score from 0.0 to 1.0 indicating the reliability of the observation.
    pub confidence: Confidence,
    /// A map for any additional, unstructured metadata.
    pub metadata: HashMap<String, Value>,
}

impl Observation {
    /// Creates a new `Observation`.
    ///
    /// # Arguments
    ///
    /// * `obs_type` - The type/category of the observation
    /// * `value` - The observation's value
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::{Observation, ObservationType};
    /// let obs = Observation::new(ObservationType::sensor("pressure"), 1013.25);
    /// ```
    pub fn new(obs_type: ObservationType, value: impl Into<Value>) -> Self {
        Self {
            obs_type,
            value: value.into(),
            timestamp: Timestamp::now(),
            confidence: Confidence::default(),
            metadata: HashMap::new(),
        }
    }

    /// Creates a new `Sensor` observation.
    ///
    /// # Arguments
    ///
    /// * `name` - The sensor name or identifier
    /// * `value` - The sensor reading
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let temp = Observation::sensor("temperature", 23.5);
    /// let humidity = Observation::sensor("humidity", 65);
    /// ```
    pub fn sensor(name: &str, value: impl Into<Value>) -> Self {
        Self::new(ObservationType::sensor(name), value)
    }

    /// Creates a new `Alert` observation.
    ///
    /// # Arguments
    ///
    /// * `message` - The alert message
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let alert = Observation::alert("Temperature threshold exceeded");
    /// ```
    pub fn alert(message: &str) -> Self {
        Self::new(
            ObservationType::alert(message),
            Value::String(message.to_string()),
        )
    }

    /// Creates a new `StateChange` observation.
    ///
    /// # Arguments
    ///
    /// * `state_name` - The name of the state that changed
    /// * `new_value` - The new value of the state
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::state_change("system_mode", "active");
    /// ```
    pub fn state_change(state_name: &str, new_value: impl Into<Value>) -> Self {
        Self::new(
            ObservationType::StateChange(state_name.to_string()),
            new_value,
        )
    }

    /// Creates a new custom event observation.
    ///
    /// # Arguments
    ///
    /// * `event_name` - The name of the event
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::event("user_login");
    /// ```
    pub fn event(event_name: &str) -> Self {
        Self::new(
            ObservationType::Custom(event_name.to_string()),
            Value::String(event_name.to_string()),
        )
    }

    /// Creates a new error observation.
    ///
    /// This is a specialized alert observation for error conditions.
    ///
    /// # Arguments
    ///
    /// * `error_type` - The category or type of error
    /// * `message` - A description of the error
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::error("network", "Connection timeout");
    /// ```
    pub fn error(error_type: &str, message: &str) -> Self {
        Self::new(
            ObservationType::Alert(format!("error:{}", error_type)),
            Value::String(message.to_string()),
        )
    }

    /// Creates a new `Network` observation.
    ///
    /// # Arguments
    ///
    /// * `event` - The type of network event
    /// * `data` - Associated data for the event
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::network("peer_connected", "peer_123");
    /// ```
    pub fn network(event: &str, data: impl Into<Value>) -> Self {
        Self::new(ObservationType::network(event), data)
    }

    /// Creates a new `Timer` observation.
    ///
    /// # Arguments
    ///
    /// * `timer_name` - The name or identifier of the timer
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::timer("hourly_check");
    /// ```
    pub fn timer(timer_name: &str) -> Self {
        Self::new(
            ObservationType::Timer(timer_name.to_string()),
            Value::String(timer_name.to_string()),
        )
    }

    /// Sets the confidence score for the observation.
    ///
    /// Confidence indicates the reliability or certainty of the observation,
    /// from 0.0 (no confidence) to 1.0 (full confidence).
    ///
    /// # Arguments
    ///
    /// * `confidence` - The confidence score (will be clamped to 0.0-1.0)
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::sensor("noisy_sensor", 42.0)
    ///     .with_confidence(0.7);
    /// ```
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Confidence::new(confidence);
        self
    }

    /// Adds a piece of metadata to the observation.
    ///
    /// Metadata allows attaching additional context or information to observations.
    ///
    /// # Arguments
    ///
    /// * `key` - The metadata key
    /// * `value` - The metadata value
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::sensor("temperature", 23.5)
    ///     .with_metadata("location", "room_a")
    ///     .with_metadata("sensor_id", "temp_001");
    /// ```
    pub fn with_metadata(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.metadata.insert(key.to_string(), value.into());
        self
    }

    /// Returns the age of the observation in seconds.
    ///
    /// This calculates the time elapsed since the observation was created.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// let obs = Observation::sensor("temp", 20.0);
    /// // Age will be 0 or very small immediately after creation
    /// assert!(obs.age_secs() < 1);
    /// ```
    pub fn age_secs(&self) -> u64 {
        self.timestamp.age_secs()
    }

    /// Returns `true` if the observation's age is within the given threshold.
    ///
    /// This is useful for filtering out stale observations.
    ///
    /// # Arguments
    ///
    /// * `threshold_secs` - The maximum age in seconds
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::Observation;
    /// let obs = Observation::sensor("temp", 20.0);
    /// assert!(obs.is_recent(10)); // Fresh observation is recent
    /// ```
    pub fn is_recent(&self, threshold_secs: u64) -> bool {
        self.age_secs() <= threshold_secs
    }
}

/// A trait for any source that can produce observations for an agent.
///
/// The `Sensor` trait abstracts over different sources of environmental data,
/// allowing agents to work with IoT sensors, simulated data sources, or any
/// other observation provider in a uniform way.
///
/// # Examples
///
/// ```
/// # use hope_agents::{Observation, observation::Sensor};
/// struct TemperatureSensor {
///     name: String,
///     current_temp: f64,
/// }
///
/// impl Sensor for TemperatureSensor {
///     fn name(&self) -> &str {
///         &self.name
///     }
///
///     fn read(&self) -> Option<Observation> {
///         Some(Observation::sensor(&self.name, self.current_temp))
///     }
/// }
/// ```
pub trait Sensor {
    /// Returns the unique name or identifier of the sensor.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::observation::{Sensor, ValueSensor};
    /// let sensor = ValueSensor::new("temp_sensor");
    /// assert_eq!(sensor.name(), "temp_sensor");
    /// ```
    fn name(&self) -> &str;

    /// Reads the current value from the sensor and returns it as an [`Observation`].
    ///
    /// # Returns
    ///
    /// `Some(Observation)` if a reading is available, `None` if the sensor
    /// cannot provide a reading at this time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::observation::{Sensor, ValueSensor};
    /// let mut sensor = ValueSensor::new("temp");
    /// sensor.set_value(23.5);
    /// let obs = sensor.read().unwrap();
    /// ```
    fn read(&self) -> Option<Observation>;

    /// Returns `true` if the sensor is available and can be read.
    ///
    /// The default implementation returns `true`. Override this to implement
    /// sensor availability checking.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::observation::{Sensor, ValueSensor};
    /// let sensor = ValueSensor::new("temp");
    /// // Sensor with no value is not available
    /// assert!(!sensor.is_available());
    /// ```
    fn is_available(&self) -> bool {
        true
    }
}

/// A simple implementation of [`Sensor`] that holds a single, updatable value.
///
/// `ValueSensor` is useful for testing, simulations, or representing simple state
/// variables as sensors. It allows manual updates to the sensor value.
///
/// # Examples
///
/// ```
/// # use hope_agents::observation::{Sensor, ValueSensor};
/// let mut sensor = ValueSensor::new("temperature");
///
/// // Initially, sensor has no value
/// assert!(!sensor.is_available());
/// assert!(sensor.read().is_none());
///
/// // Set a value
/// sensor.set_value(23.5);
/// assert!(sensor.is_available());
///
/// // Read the observation
/// let obs = sensor.read().unwrap();
/// assert_eq!(obs.value.as_f64().unwrap(), 23.5);
/// ```
pub struct ValueSensor {
    name: String,
    current_value: Option<Value>,
}

impl ValueSensor {
    /// Creates a new `ValueSensor` with no initial value.
    ///
    /// # Arguments
    ///
    /// * `name` - The sensor's name/identifier
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::observation::ValueSensor;
    /// let sensor = ValueSensor::new("my_sensor");
    /// ```
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            current_value: None,
        }
    }

    /// Sets the current value of the sensor.
    ///
    /// # Arguments
    ///
    /// * `value` - The new sensor value
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::observation::ValueSensor;
    /// let mut sensor = ValueSensor::new("temp");
    /// sensor.set_value(25.0);
    /// ```
    pub fn set_value(&mut self, value: impl Into<Value>) {
        self.current_value = Some(value.into());
    }

    /// Clears the current value of the sensor.
    ///
    /// After calling this, the sensor will report as unavailable.
    ///
    /// # Examples
    ///
    /// ```
    /// # use hope_agents::observation::{Sensor, ValueSensor};
    /// let mut sensor = ValueSensor::new("temp");
    /// sensor.set_value(25.0);
    /// sensor.clear();
    /// assert!(!sensor.is_available());
    /// ```
    pub fn clear(&mut self) {
        self.current_value = None;
    }
}

impl Sensor for ValueSensor {
    fn name(&self) -> &str {
        &self.name
    }

    fn read(&self) -> Option<Observation> {
        self.current_value
            .as_ref()
            .map(|v| Observation::sensor(&self.name, v.clone()))
    }

    fn is_available(&self) -> bool {
        self.current_value.is_some()
    }
}

/// A rolling buffer that stores recent observations.
///
/// `ObservationBuffer` maintains a fixed-size collection of recent observations,
/// automatically removing old observations based on age and capacity limits.
/// This is useful for agents that need to maintain context from recent history.
///
/// # Examples
///
/// ```
/// # use hope_agents::{Observation, observation::ObservationBuffer};
/// let mut buffer = ObservationBuffer::new(100);
///
/// buffer.push(Observation::sensor("temp", 20.0));
/// buffer.push(Observation::sensor("temp", 21.0));
/// buffer.push(Observation::sensor("temp", 22.0));
///
/// let recent = buffer.get_recent(2);
/// assert_eq!(recent.len(), 2);
/// ```
pub struct ObservationBuffer {
    observations: Vec<Observation>,
    max_size: usize,
    max_age_secs: u64,
}

impl ObservationBuffer {
    /// Creates a new `ObservationBuffer` with a given maximum size.
    pub fn new(max_size: usize) -> Self {
        Self {
            observations: Vec::with_capacity(max_size),
            max_size,
            max_age_secs: 300, // 5 minutes
        }
    }

    /// Sets the maximum age in seconds for observations to be retained in the buffer.
    pub fn with_max_age(mut self, secs: u64) -> Self {
        self.max_age_secs = secs;
        self
    }

    /// Adds a new observation to the buffer.
    ///
    /// This method will also prune any observations older than `max_age_secs` and,
    /// if the buffer is at `max_size`, remove the oldest observation.
    pub fn push(&mut self, obs: Observation) {
        // Remove old observations
        self.observations.retain(|o| o.is_recent(self.max_age_secs));

        // Add new observation
        if self.observations.len() >= self.max_size {
            self.observations.remove(0);
        }
        self.observations.push(obs);
    }

    /// Returns the `count` most recent observations.
    pub fn get_recent(&self, count: usize) -> Vec<&Observation> {
        self.observations.iter().rev().take(count).collect()
    }

    /// Returns all observations in the buffer that match a given `ObservationType`.
    pub fn get_by_type(&self, obs_type: &ObservationType) -> Vec<&Observation> {
        self.observations
            .iter()
            .filter(|o| &o.obs_type == obs_type)
            .collect()
    }

    /// Returns a reference to the most recent observation, if any.
    pub fn latest(&self) -> Option<&Observation> {
        self.observations.last()
    }

    /// Clears all observations from the buffer.
    pub fn clear(&mut self) {
        self.observations.clear();
    }

    /// Returns the current number of observations in the buffer.
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    /// Returns `true` if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observation_creation() {
        let obs = Observation::sensor("temperature", 23.5);
        assert!(matches!(obs.obs_type, ObservationType::Sensor(_)));
    }

    #[test]
    fn test_observation_buffer() {
        let mut buffer = ObservationBuffer::new(10);
        buffer.push(Observation::sensor("temp", 20.0));
        buffer.push(Observation::sensor("temp", 21.0));

        assert_eq!(buffer.len(), 2);
        assert!(buffer.latest().is_some());
    }

    #[test]
    fn test_value_sensor() {
        let mut sensor = ValueSensor::new("test");
        assert!(!sensor.is_available());

        sensor.set_value(42.0);
        assert!(sensor.is_available());

        let obs = sensor.read().unwrap();
        assert_eq!(obs.value.as_f64().unwrap(), 42.0);
    }

    // ObservationType tests
    #[test]
    fn test_observation_type_sensor() {
        let obs_type = ObservationType::sensor("temperature");
        assert!(matches!(obs_type, ObservationType::Sensor(s) if s == "temperature"));
    }

    #[test]
    fn test_observation_type_network() {
        let obs_type = ObservationType::network("peer_connected");
        assert!(matches!(obs_type, ObservationType::Network(s) if s == "peer_connected"));
    }

    #[test]
    fn test_observation_type_alert() {
        let obs_type = ObservationType::alert("Critical error");
        assert!(matches!(obs_type, ObservationType::Alert(s) if s == "Critical error"));
    }

    #[test]
    fn test_observation_type_clone() {
        let obs_type = ObservationType::sensor("temp");
        let cloned = obs_type.clone();
        assert_eq!(obs_type, cloned);
    }

    #[test]
    fn test_observation_type_debug() {
        let obs_type = ObservationType::sensor("temp");
        let debug_str = format!("{:?}", obs_type);
        assert!(debug_str.contains("Sensor"));
        assert!(debug_str.contains("temp"));
    }

    #[test]
    fn test_observation_type_serialize() {
        let obs_type = ObservationType::sensor("temp");
        let json = serde_json::to_string(&obs_type).unwrap();
        assert!(json.contains("Sensor"));

        let parsed: ObservationType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, obs_type);
    }

    #[test]
    fn test_observation_type_eq_hash() {
        use std::collections::HashSet;

        let obs1 = ObservationType::sensor("temp");
        let obs2 = ObservationType::sensor("temp");
        let obs3 = ObservationType::network("peer");

        assert_eq!(obs1, obs2);
        assert_ne!(obs1, obs3);

        let mut set = HashSet::new();
        set.insert(obs1.clone());
        set.insert(obs2.clone());
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_observation_type_all_variants() {
        let types = vec![
            ObservationType::Sensor("s".into()),
            ObservationType::Network("n".into()),
            ObservationType::UserInput("u".into()),
            ObservationType::StateChange("sc".into()),
            ObservationType::Timer("t".into()),
            ObservationType::Alert("a".into()),
            ObservationType::Custom("c".into()),
        ];

        for t in types {
            let _ = format!("{:?}", t);
        }
    }

    // Observation tests
    #[test]
    fn test_observation_new() {
        let obs = Observation::new(ObservationType::sensor("pressure"), 1013.25);
        assert!(matches!(obs.obs_type, ObservationType::Sensor(_)));
        assert_eq!(obs.value.as_f64().unwrap(), 1013.25);
    }

    #[test]
    fn test_observation_alert() {
        let obs = Observation::alert("Temperature threshold exceeded");
        assert!(matches!(obs.obs_type, ObservationType::Alert(_)));
    }

    #[test]
    fn test_observation_state_change() {
        let obs = Observation::state_change("system_mode", "active");
        assert!(matches!(obs.obs_type, ObservationType::StateChange(_)));
    }

    #[test]
    fn test_observation_event() {
        let obs = Observation::event("user_login");
        assert!(matches!(obs.obs_type, ObservationType::Custom(_)));
    }

    #[test]
    fn test_observation_error() {
        let obs = Observation::error("network", "Connection timeout");
        assert!(matches!(obs.obs_type, ObservationType::Alert(_)));
    }

    #[test]
    fn test_observation_network() {
        let obs = Observation::network("peer_connected", "peer_123");
        assert!(matches!(obs.obs_type, ObservationType::Network(_)));
    }

    #[test]
    fn test_observation_timer() {
        let obs = Observation::timer("hourly_check");
        assert!(matches!(obs.obs_type, ObservationType::Timer(_)));
    }

    #[test]
    fn test_observation_with_confidence() {
        let obs = Observation::sensor("noisy_sensor", 42.0).with_confidence(0.7);
        assert!((obs.confidence.value() - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_observation_with_metadata() {
        let obs = Observation::sensor("temperature", 23.5)
            .with_metadata("location", "room_a")
            .with_metadata("sensor_id", "temp_001");

        assert_eq!(obs.metadata.len(), 2);
        assert!(obs.metadata.contains_key("location"));
        assert!(obs.metadata.contains_key("sensor_id"));
    }

    #[test]
    fn test_observation_age_secs() {
        let obs = Observation::sensor("temp", 20.0);
        assert!(obs.age_secs() < 1);
    }

    #[test]
    fn test_observation_is_recent() {
        let obs = Observation::sensor("temp", 20.0);
        assert!(obs.is_recent(10));
        assert!(obs.is_recent(1));
    }

    #[test]
    fn test_observation_clone() {
        let obs = Observation::sensor("temp", 25.0).with_confidence(0.9);
        let cloned = obs.clone();

        assert_eq!(obs.value.as_f64(), cloned.value.as_f64());
        assert!((obs.confidence.value() - cloned.confidence.value()).abs() < 0.001);
    }

    #[test]
    fn test_observation_debug() {
        let obs = Observation::sensor("temp", 25.0);
        let debug_str = format!("{:?}", obs);
        assert!(debug_str.contains("Observation"));
    }

    #[test]
    fn test_observation_serialize() {
        let obs = Observation::sensor("temp", 25.0);
        let json = serde_json::to_string(&obs).unwrap();
        assert!(json.contains("temp"));

        let parsed: Observation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.value.as_f64(), obs.value.as_f64());
    }

    // ValueSensor tests
    #[test]
    fn test_value_sensor_name() {
        let sensor = ValueSensor::new("my_sensor");
        assert_eq!(sensor.name(), "my_sensor");
    }

    #[test]
    fn test_value_sensor_clear() {
        let mut sensor = ValueSensor::new("temp");
        sensor.set_value(25.0);
        assert!(sensor.is_available());

        sensor.clear();
        assert!(!sensor.is_available());
        assert!(sensor.read().is_none());
    }

    #[test]
    fn test_value_sensor_multiple_values() {
        let mut sensor = ValueSensor::new("temp");

        sensor.set_value(20.0);
        assert_eq!(sensor.read().unwrap().value.as_f64().unwrap(), 20.0);

        sensor.set_value(25.0);
        assert_eq!(sensor.read().unwrap().value.as_f64().unwrap(), 25.0);

        sensor.set_value(30.0);
        assert_eq!(sensor.read().unwrap().value.as_f64().unwrap(), 30.0);
    }

    #[test]
    fn test_value_sensor_string_value() {
        let mut sensor = ValueSensor::new("status");
        sensor.set_value("active");
        let obs = sensor.read().unwrap();
        assert_eq!(obs.value.as_string(), "active");
    }

    // ObservationBuffer tests
    #[test]
    fn test_observation_buffer_new() {
        let buffer = ObservationBuffer::new(100);
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_observation_buffer_with_max_age() {
        let buffer = ObservationBuffer::new(100).with_max_age(60);
        assert_eq!(buffer.max_age_secs, 60);
    }

    #[test]
    fn test_observation_buffer_push_capacity() {
        let mut buffer = ObservationBuffer::new(3);

        buffer.push(Observation::sensor("temp", 1.0));
        buffer.push(Observation::sensor("temp", 2.0));
        buffer.push(Observation::sensor("temp", 3.0));
        assert_eq!(buffer.len(), 3);

        // Pushing beyond capacity removes oldest
        buffer.push(Observation::sensor("temp", 4.0));
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn test_observation_buffer_get_recent() {
        let mut buffer = ObservationBuffer::new(10);

        for i in 0..5 {
            buffer.push(Observation::sensor("temp", i as f64));
        }

        let recent = buffer.get_recent(2);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_observation_buffer_get_by_type() {
        let mut buffer = ObservationBuffer::new(10);

        buffer.push(Observation::sensor("temp", 20.0));
        buffer.push(Observation::alert("High temp!"));
        buffer.push(Observation::sensor("humidity", 60.0));

        let sensors = buffer.get_by_type(&ObservationType::sensor("temp"));
        assert_eq!(sensors.len(), 1);
    }

    #[test]
    fn test_observation_buffer_latest() {
        let mut buffer = ObservationBuffer::new(10);
        assert!(buffer.latest().is_none());

        buffer.push(Observation::sensor("temp", 1.0));
        buffer.push(Observation::sensor("temp", 2.0));

        let latest = buffer.latest().unwrap();
        assert_eq!(latest.value.as_f64().unwrap(), 2.0);
    }

    #[test]
    fn test_observation_buffer_clear() {
        let mut buffer = ObservationBuffer::new(10);

        buffer.push(Observation::sensor("temp", 20.0));
        buffer.push(Observation::sensor("temp", 21.0));
        assert_eq!(buffer.len(), 2);

        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_observation_buffer_is_empty() {
        let mut buffer = ObservationBuffer::new(10);
        assert!(buffer.is_empty());

        buffer.push(Observation::sensor("temp", 20.0));
        assert!(!buffer.is_empty());
    }
}
