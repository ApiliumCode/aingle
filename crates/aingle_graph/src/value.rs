//! Defines the `Value` type for the object of a semantic triple.
//!
//! A `Value` can be either a literal (like a string, number, or boolean) or a
//! reference to another node in the graph.

use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the object of a `(subject, predicate, object)` triple.
///
/// A `Value` can be either a reference to another node in the graph (creating a link)
/// or a literal value of various types (string, number, boolean, etc.).
///
/// # Examples
///
/// Creating a string literal:
///
/// ```
/// use aingle_graph::Value;
///
/// let val = Value::literal("Alice");
/// assert!(val.is_literal());
/// assert_eq!(val.as_string(), Some("Alice"));
/// ```
///
/// Creating a node reference:
///
/// ```
/// use aingle_graph::{Value, NodeId};
///
/// let val = Value::node(NodeId::named("user:bob"));
/// assert!(val.is_node());
/// ```
///
/// Creating numeric values:
///
/// ```
/// use aingle_graph::Value;
///
/// let age = Value::integer(30);
/// assert_eq!(age.as_integer(), Some(30));
///
/// let score = Value::float(98.5);
/// assert_eq!(score.as_float(), Some(98.5));
/// ```
///
/// Creating a typed literal:
///
/// ```
/// use aingle_graph::Value;
///
/// let val = Value::typed("2024-01-01", "xsd:date");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// A reference to another node in the graph, linking two subjects together.
    Node(NodeId),

    /// A UTF-8 string literal.
    String(String),

    /// A 64-bit signed integer literal.
    Integer(i64),

    /// A 64-bit floating-point literal.
    Float(f64),

    /// A boolean literal.
    Boolean(bool),

    /// A date-time literal, typically stored as an ISO 8601 string.
    DateTime(String),

    /// A literal with an explicit datatype URI, similar to RDF typed literals.
    Typed { value: String, datatype: String },

    /// A string literal with a language tag.
    LangString { value: String, lang: String },

    /// A blob of binary data.
    Bytes(Vec<u8>),

    /// A JSON value, allowing for complex, nested data structures as objects.
    Json(serde_json::Value),

    /// The null value.
    Null,
}

impl Value {
    /// Creates a new string literal [`Value`].
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::Value;
    ///
    /// let val = Value::literal("Hello, World!");
    /// assert_eq!(val.as_string(), Some("Hello, World!"));
    /// ```
    pub fn literal(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    /// Creates a [`Value`] that is a reference to another [`NodeId`].
    ///
    /// This creates a link between two nodes in the graph.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::{Value, NodeId};
    ///
    /// let val = Value::node(NodeId::named("user:bob"));
    /// assert!(val.is_node());
    /// assert_eq!(val.as_node(), Some(&NodeId::named("user:bob")));
    /// ```
    pub fn node(node: NodeId) -> Self {
        Self::Node(node)
    }

    /// Creates a new integer [`Value`].
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::Value;
    ///
    /// let val = Value::integer(42);
    /// assert_eq!(val.as_integer(), Some(42));
    /// ```
    pub fn integer(n: i64) -> Self {
        Self::Integer(n)
    }

    /// Creates a new float [`Value`].
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::Value;
    ///
    /// let val = Value::float(3.14);
    /// assert_eq!(val.as_float(), Some(3.14));
    /// ```
    pub fn float(f: f64) -> Self {
        Self::Float(f)
    }

    /// Creates a new boolean [`Value`].
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::Value;
    ///
    /// let val = Value::boolean(true);
    /// assert_eq!(val.as_boolean(), Some(true));
    /// ```
    pub fn boolean(b: bool) -> Self {
        Self::Boolean(b)
    }

    /// Creates a new date-time `Value`.
    pub fn datetime(dt: impl Into<String>) -> Self {
        Self::DateTime(dt.into())
    }

    /// Creates a new typed literal [`Value`].
    ///
    /// Typed literals have an associated datatype URI, similar to RDF typed literals.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::Value;
    ///
    /// let date = Value::typed("2024-01-01", "xsd:date");
    /// let custom = Value::typed("custom_value", "http://example.org/mytype");
    /// ```
    pub fn typed(value: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self::Typed {
            value: value.into(),
            datatype: datatype.into(),
        }
    }

    /// Creates a new language-tagged string [`Value`].
    ///
    /// Language-tagged strings are useful for internationalization, allowing you to
    /// store the same text in multiple languages.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::Value;
    ///
    /// let english = Value::lang_string("Hello", "en");
    /// let spanish = Value::lang_string("Hola", "es");
    /// let french = Value::lang_string("Bonjour", "fr");
    /// ```
    pub fn lang_string(value: impl Into<String>, lang: impl Into<String>) -> Self {
        Self::LangString {
            value: value.into(),
            lang: lang.into(),
        }
    }

    /// Creates a new bytes `Value`.
    pub fn bytes(data: Vec<u8>) -> Self {
        Self::Bytes(data)
    }

    /// Creates a new JSON `Value`.
    pub fn json(value: serde_json::Value) -> Self {
        Self::Json(value)
    }

    /// Creates a `Null` value.
    pub fn null() -> Self {
        Self::Null
    }

    /// Returns `true` if the `Value` is a `Node` reference.
    pub fn is_node(&self) -> bool {
        matches!(self, Self::Node(_))
    }

    /// Returns `true` if the `Value` is a literal (i.e., not a `Node` or `Null`).
    pub fn is_literal(&self) -> bool {
        !matches!(self, Self::Node(_) | Self::Null)
    }

    /// Returns `true` if the `Value` is `Null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns a reference to the `NodeId` if the `Value` is a `Node`.
    pub fn as_node(&self) -> Option<&NodeId> {
        match self {
            Self::Node(n) => Some(n),
            _ => None,
        }
    }

    /// Returns a string slice if the `Value` is a string-like literal.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            Self::LangString { value, .. } => Some(value),
            Self::Typed { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Returns the `i64` value if the `Value` is an `Integer`.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the `f64` value if the `Value` is a `Float` or can be cast from `Integer`.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Integer(n) => Some(*n as f64),
            _ => None,
        }
    }

    /// Returns the `bool` value if the `Value` is a `Boolean`.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Serializes the `Value` to a byte vector for storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    /// Deserializes a `Value` from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }

    /// Returns a byte vector suitable for lexicographical sorting in the database indexes.
    ///
    /// This method generates a binary representation of the value that can be sorted
    /// lexicographically to maintain proper ordering in the database. Different value
    /// types are given different type tags to ensure correct cross-type ordering.
    pub fn sort_key(&self) -> Vec<u8> {
        match self {
            Self::Node(n) => {
                let mut key = vec![0u8]; // Type tag
                key.extend(n.to_bytes());
                key
            }
            Self::String(s) => {
                let mut key = vec![1u8];
                key.extend(s.as_bytes());
                key
            }
            Self::Integer(n) => {
                let mut key = vec![2u8];
                // Use big-endian for proper sorting, and XOR to handle signed integers correctly.
                key.extend(&((*n as u64) ^ (1u64 << 63)).to_be_bytes());
                key
            }
            Self::Float(f) => {
                let mut key = vec![3u8];
                let bits = f.to_bits();
                // A trick to make floating point numbers sortable as integers.
                let sortable = if *f >= 0.0 {
                    bits ^ (1u64 << 63)
                } else {
                    !bits
                };
                key.extend(&sortable.to_be_bytes());
                key
            }
            Self::Boolean(b) => {
                vec![4u8, if *b { 1 } else { 0 }]
            }
            Self::DateTime(dt) => {
                let mut key = vec![5u8];
                key.extend(dt.as_bytes());
                key
            }
            Self::Typed { value, datatype } => {
                let mut key = vec![6u8];
                key.extend(datatype.as_bytes());
                key.push(0); // separator
                key.extend(value.as_bytes());
                key
            }
            Self::LangString { value, lang } => {
                let mut key = vec![7u8];
                key.extend(lang.as_bytes());
                key.push(0);
                key.extend(value.as_bytes());
                key
            }
            Self::Bytes(data) => {
                let mut key = vec![8u8];
                key.extend(data);
                key
            }
            Self::Json(v) => {
                let mut key = vec![9u8];
                if let Ok(s) = serde_json::to_string(v) {
                    key.extend(s.as_bytes());
                }
                key
            }
            Self::Null => vec![255u8], // Sort nulls last
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(n) => write!(f, "{}", n),
            Self::String(s) => write!(f, "\"{}\"", s),
            Self::Integer(n) => write!(f, "{}", n),
            Self::Float(n) => write!(f, "{}", n),
            Self::Boolean(b) => write!(f, "{}", b),
            Self::DateTime(dt) => write!(f, "\"{}\"^^xsd:dateTime", dt),
            Self::Typed { value, datatype } => write!(f, "\"{}\"^^<{}>", value, datatype),
            Self::LangString { value, lang } => write!(f, "\"{}\"@{}", value, lang),
            Self::Bytes(data) => write!(f, "_:bytes[{}]", data.len()),
            Self::Json(v) => write!(f, "{}", v),
            Self::Null => write!(f, "null"),
        }
    }
}

impl Eq for Value {}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash based on the sort_key for consistent hashing across different value types.
        self.sort_key().hash(state);
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

// Convenient conversions from standard types into `Value`.
impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Self::Integer(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Self::Integer(n as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

impl From<NodeId> for Value {
    fn from(n: NodeId) -> Self {
        Self::Node(n)
    }
}

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        Self::Json(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_value() {
        let val = Value::literal("hello");
        assert!(val.is_literal());
        assert_eq!(val.as_string(), Some("hello"));
    }

    #[test]
    fn test_node_value() {
        let node = NodeId::named("user:alice");
        let val = Value::node(node.clone());
        assert!(val.is_node());
        assert_eq!(val.as_node(), Some(&node));
    }

    #[test]
    fn test_numeric_values() {
        let int_val = Value::integer(42);
        assert_eq!(int_val.as_integer(), Some(42));

        let float_val = Value::float(3.14);
        assert_eq!(float_val.as_float(), Some(3.14));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Value::literal("test")), "\"test\"");
        assert_eq!(format!("{}", Value::integer(42)), "42");
        assert_eq!(format!("{}", Value::boolean(true)), "true");
    }

    #[test]
    fn test_serialization() {
        let val = Value::literal("test");
        let bytes = val.to_bytes();
        let restored = Value::from_bytes(&bytes).unwrap();
        assert_eq!(val, restored);
    }

    #[test]
    fn test_sort_order() {
        let v1 = Value::integer(1);
        let v2 = Value::integer(2);
        let v3 = Value::integer(-1);

        assert!(v3 < v1);
        assert!(v1 < v2);
    }

    #[test]
    fn test_conversions() {
        let s: Value = "hello".into();
        assert_eq!(s.as_string(), Some("hello"));

        let n: Value = 42i64.into();
        assert_eq!(n.as_integer(), Some(42));

        let b: Value = true.into();
        assert_eq!(b.as_boolean(), Some(true));
    }
}
