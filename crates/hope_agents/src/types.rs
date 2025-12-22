//! Core, general-purpose data types for the HOPE Agents framework.

use serde::{Deserialize, Serialize};

/// A high-precision timestamp in microseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Returns the current timestamp.
    pub fn now() -> Self {
        let now = chrono::Utc::now();
        let micros = (now.timestamp() as u64) * 1_000_000 + (now.timestamp_subsec_micros() as u64);
        Self(micros)
    }

    /// Calculates the age of the timestamp in seconds from the present moment.
    pub fn age_secs(&self) -> u64 {
        let now = Self::now();
        (now.0.saturating_sub(self.0)) / 1_000_000
    }
}

/// A generic value that can be observed or used in actions and goals.
///
/// This enum allows agents to handle various data types in a uniform way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    /// A boolean value.
    Bool(bool),
    /// A 64-bit signed integer value.
    Int(i64),
    /// A 64-bit floating-point value.
    Float(f64),
    /// A string value.
    String(String),
    /// A vector of bytes.
    Bytes(Vec<u8>),
    /// A flexible JSON value.
    Json(serde_json::Value),
    /// Represents the absence of a value.
    None,
}

impl Value {
    /// Attempts to convert the `Value` to an `f64`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Attempts to convert the `Value` to an `i64`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Attempts to convert the `Value` to a `bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            _ => None,
        }
    }

    /// Converts the `Value` to a `String` representation.
    pub fn as_string(&self) -> String {
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => s.clone(),
            Value::Bytes(b) => format!("{:?}", b),
            Value::Json(j) => j.to_string(),
            Value::None => "none".to_string(),
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        Value::Json(v)
    }
}

/// A numeric range, used in `Condition`s and `Goal`s.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueRange {
    /// The minimum value of the range (inclusive), if bounded.
    pub min: Option<f64>,
    /// The maximum value of the range (inclusive), if bounded.
    pub max: Option<f64>,
}

impl ValueRange {
    /// Creates a new `ValueRange` with a defined minimum and maximum.
    pub fn new(min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }

    /// Creates an unbounded range.
    pub fn unbounded() -> Self {
        Self {
            min: None,
            max: None,
        }
    }

    /// Creates a range with only a minimum value.
    pub fn at_least(min: f64) -> Self {
        Self {
            min: Some(min),
            max: None,
        }
    }

    /// Creates a range with only a maximum value.
    pub fn at_most(max: f64) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }

    /// Returns `true` if the given value is within the range.
    pub fn contains(&self, value: f64) -> bool {
        if let Some(min) = self.min {
            if value < min {
                return false;
            }
        }
        if let Some(max) = self.max {
            if value > max {
                return false;
            }
        }
        true
    }

    /// Returns `true` if the value is below the range's minimum.
    pub fn is_below(&self, value: f64) -> bool {
        self.min.map(|min| value < min).unwrap_or(false)
    }

    /// Returns `true` if the value is above the range's maximum.
    pub fn is_above(&self, value: f64) -> bool {
        self.max.map(|max| value > max).unwrap_or(false)
    }
}

impl From<std::ops::Range<f64>> for ValueRange {
    fn from(r: std::ops::Range<f64>) -> Self {
        Self::new(r.start, r.end)
    }
}

/// Represents the priority level for goals and actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Priority {
    /// Lowest priority.
    Low = 0,
    /// Default priority.
    #[default]
    Normal = 1,
    /// High priority.
    High = 2,
    /// Highest priority, should be acted upon immediately.
    Critical = 3,
}

/// Represents a confidence level, from 0.0 to 1.0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Confidence(pub f32);

impl Confidence {
    /// Creates a new `Confidence` value, clamped between 0.0 and 1.0.
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Returns the raw confidence value as an `f32`.
    pub fn value(&self) -> f32 {
        self.0
    }

    /// Returns `true` if the confidence is high (>= 0.8).
    pub fn is_high(&self) -> bool {
        self.0 >= 0.8
    }

    /// Returns `true` if the confidence is low (< 0.3).
    pub fn is_low(&self) -> bool {
        self.0 < 0.3
    }
}

impl Default for Confidence {
    /// The default confidence is 0.5.
    fn default() -> Self {
        Self(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Timestamp Tests ====================

    #[test]
    fn test_timestamp_now() {
        let ts = Timestamp::now();
        assert!(ts.0 > 0);
    }

    #[test]
    fn test_timestamp_default() {
        let ts: Timestamp = Default::default();
        assert_eq!(ts.0, 0);
    }

    #[test]
    fn test_timestamp_age_secs() {
        // Create a timestamp from the past
        let past = Timestamp(0); // Unix epoch
        let age = past.age_secs();
        // Should be many years old
        assert!(age > 1_000_000);
    }

    #[test]
    fn test_timestamp_ordering() {
        let t1 = Timestamp(100);
        let t2 = Timestamp(200);
        assert!(t1 < t2);
        assert!(t2 > t1);
        assert!(t1 <= t1);
        assert_eq!(t1, Timestamp(100));
    }

    #[test]
    fn test_timestamp_serialize() {
        let ts = Timestamp(1234567890);
        let json = serde_json::to_string(&ts).unwrap();
        let parsed: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.0, 1234567890);
    }

    // ==================== Value Tests ====================

    #[test]
    fn test_value_conversions() {
        let v = Value::Float(3.14);
        assert!((v.as_f64().unwrap() - 3.14).abs() < 0.001);

        let v = Value::Int(42);
        assert_eq!(v.as_i64().unwrap(), 42);
    }

    #[test]
    fn test_value_bool() {
        let v = Value::Bool(true);
        assert_eq!(v.as_bool(), Some(true));

        let v = Value::Bool(false);
        assert_eq!(v.as_bool(), Some(false));
    }

    #[test]
    fn test_value_int_as_bool() {
        let v = Value::Int(1);
        assert_eq!(v.as_bool(), Some(true));

        let v = Value::Int(0);
        assert_eq!(v.as_bool(), Some(false));

        let v = Value::Int(-5);
        assert_eq!(v.as_bool(), Some(true));
    }

    #[test]
    fn test_value_int_as_f64() {
        let v = Value::Int(42);
        assert_eq!(v.as_f64(), Some(42.0));
    }

    #[test]
    fn test_value_float_as_i64() {
        let v = Value::Float(42.7);
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_value_string() {
        let v = Value::String("hello".to_string());
        assert_eq!(v.as_string(), "hello");
        assert!(v.as_f64().is_none());
        assert!(v.as_i64().is_none());
        assert!(v.as_bool().is_none());
    }

    #[test]
    fn test_value_bytes() {
        let v = Value::Bytes(vec![1, 2, 3]);
        let s = v.as_string();
        assert!(s.contains("1"));
        assert!(v.as_f64().is_none());
    }

    #[test]
    fn test_value_json() {
        let json_val = serde_json::json!({"key": "value"});
        let v = Value::Json(json_val);
        let s = v.as_string();
        assert!(s.contains("key"));
    }

    #[test]
    fn test_value_none() {
        let v = Value::None;
        assert_eq!(v.as_string(), "none");
        assert!(v.as_f64().is_none());
        assert!(v.as_i64().is_none());
        assert!(v.as_bool().is_none());
    }

    #[test]
    fn test_value_as_string_all_types() {
        assert_eq!(Value::Bool(true).as_string(), "true");
        assert_eq!(Value::Bool(false).as_string(), "false");
        assert_eq!(Value::Int(42).as_string(), "42");
        assert_eq!(Value::Float(3.14).as_string(), "3.14");
        assert_eq!(Value::String("test".to_string()).as_string(), "test");
    }

    #[test]
    fn test_value_from_bool() {
        let v: Value = true.into();
        assert!(matches!(v, Value::Bool(true)));

        let v: Value = false.into();
        assert!(matches!(v, Value::Bool(false)));
    }

    #[test]
    fn test_value_from_i64() {
        let v: Value = 42i64.into();
        assert!(matches!(v, Value::Int(42)));
    }

    #[test]
    fn test_value_from_f64() {
        let v: Value = 3.14f64.into();
        if let Value::Float(f) = v {
            assert!((f - 3.14).abs() < 0.001);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_value_from_string() {
        let v: Value = String::from("hello").into();
        assert!(matches!(v, Value::String(s) if s == "hello"));
    }

    #[test]
    fn test_value_from_str() {
        let v: Value = "hello".into();
        assert!(matches!(v, Value::String(s) if s == "hello"));
    }

    #[test]
    fn test_value_from_json() {
        let json_val = serde_json::json!({"a": 1});
        let v: Value = json_val.into();
        assert!(matches!(v, Value::Json(_)));
    }

    #[test]
    fn test_value_serialize() {
        let v = Value::Int(42);
        let json = serde_json::to_string(&v).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_i64(), Some(42));
    }

    // ==================== ValueRange Tests ====================

    #[test]
    fn test_value_range() {
        let range = ValueRange::new(20.0, 25.0);
        assert!(range.contains(22.0));
        assert!(!range.contains(18.0));
        assert!(range.is_below(18.0));
        assert!(range.is_above(30.0));
    }

    #[test]
    fn test_value_range_unbounded() {
        let range = ValueRange::unbounded();
        assert!(range.contains(0.0));
        assert!(range.contains(1000.0));
        assert!(range.contains(-1000.0));
        assert!(!range.is_below(0.0));
        assert!(!range.is_above(1000.0));
    }

    #[test]
    fn test_value_range_at_least() {
        let range = ValueRange::at_least(10.0);
        assert!(range.contains(10.0));
        assert!(range.contains(100.0));
        assert!(!range.contains(5.0));
        assert!(range.is_below(5.0));
        assert!(!range.is_above(1000.0)); // No max
    }

    #[test]
    fn test_value_range_at_most() {
        let range = ValueRange::at_most(100.0);
        assert!(range.contains(100.0));
        assert!(range.contains(0.0));
        assert!(!range.contains(200.0));
        assert!(!range.is_below(0.0)); // No min
        assert!(range.is_above(200.0));
    }

    #[test]
    fn test_value_range_from_std_range() {
        let range: ValueRange = (10.0..20.0).into();
        assert_eq!(range.min, Some(10.0));
        assert_eq!(range.max, Some(20.0));
    }

    #[test]
    fn test_value_range_serialize() {
        let range = ValueRange::new(10.0, 20.0);
        let json = serde_json::to_string(&range).unwrap();
        let parsed: ValueRange = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.min, Some(10.0));
        assert_eq!(parsed.max, Some(20.0));
    }

    // ==================== Priority Tests ====================

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Low < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::Critical);
    }

    #[test]
    fn test_priority_default() {
        let p: Priority = Default::default();
        assert_eq!(p, Priority::Normal);
    }

    #[test]
    fn test_priority_serialize() {
        for p in [
            Priority::Low,
            Priority::Normal,
            Priority::High,
            Priority::Critical,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            let parsed: Priority = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, p);
        }
    }

    // ==================== Confidence Tests ====================

    #[test]
    fn test_confidence() {
        let c = Confidence::new(0.9);
        assert!(c.is_high());
        assert!(!c.is_low());
    }

    #[test]
    fn test_confidence_clamping() {
        let c = Confidence::new(1.5);
        assert_eq!(c.value(), 1.0);

        let c = Confidence::new(-0.5);
        assert_eq!(c.value(), 0.0);
    }

    #[test]
    fn test_confidence_value() {
        let c = Confidence::new(0.75);
        assert_eq!(c.value(), 0.75);
    }

    #[test]
    fn test_confidence_is_high() {
        assert!(Confidence::new(0.8).is_high());
        assert!(Confidence::new(0.9).is_high());
        assert!(Confidence::new(1.0).is_high());
        assert!(!Confidence::new(0.79).is_high());
    }

    #[test]
    fn test_confidence_is_low() {
        assert!(Confidence::new(0.0).is_low());
        assert!(Confidence::new(0.1).is_low());
        assert!(Confidence::new(0.29).is_low());
        assert!(!Confidence::new(0.3).is_low());
    }

    #[test]
    fn test_confidence_default() {
        let c: Confidence = Default::default();
        assert_eq!(c.value(), 0.5);
    }

    #[test]
    fn test_confidence_serialize() {
        let c = Confidence::new(0.75);
        let json = serde_json::to_string(&c).unwrap();
        let parsed: Confidence = serde_json::from_str(&json).unwrap();
        assert!((parsed.value() - 0.75).abs() < 0.001);
    }
}
