//! Node identifiers for graph subjects and objects.
//!
//! A `NodeId` uniquely identifies a node (a subject or an object) in the graph.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A unique identifier for a node in the graph, which can be a subject or a named object.
///
/// Nodes are the entities in the graph that are connected by relationships (predicates).
/// A node can be identified in three ways: by a human-readable name, by a cryptographic
/// hash, or as an anonymous blank node.
///
/// # Examples
///
/// Creating a named node:
///
/// ```
/// use aingle_graph::NodeId;
///
/// let node = NodeId::named("user:alice");
/// assert!(node.is_named());
/// assert_eq!(node.as_name(), Some("user:alice"));
/// ```
///
/// Creating a hash-based node:
///
/// ```
/// use aingle_graph::NodeId;
///
/// let hash = [0u8; 32];
/// let node = NodeId::hash(hash);
/// assert!(node.is_hash());
/// ```
///
/// Creating a blank node:
///
/// ```
/// use aingle_graph::NodeId;
///
/// let node = NodeId::blank();
/// assert!(node.is_blank());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum NodeId {
    /// A named node, identified by a string, similar to a URI in RDF.
    ///
    /// Examples: `"user:alice"`, `"org:acme"`, `"aingle:action:abc123"`
    Named(String),

    /// A node identified by a 32-byte content hash, typically from an AIngle entry or action.
    Hash([u8; 32]),

    /// A blank (or anonymous) node, identified by a unique, auto-generated ID.
    Blank(u64),
}

impl NodeId {
    /// Creates a new `Named` node.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::NodeId;
    ///
    /// let node = NodeId::named("user:alice");
    /// assert_eq!(node.as_name(), Some("user:alice"));
    /// ```
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Creates a new `Hash`-based node.
    pub fn hash(hash: [u8; 32]) -> Self {
        Self::Hash(hash)
    }

    /// Creates a `Hash`-based node from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut arr = [0u8; 32];
        let len = bytes.len().min(32);
        arr[..len].copy_from_slice(&bytes[..len]);
        Self::Hash(arr)
    }

    /// Creates a new, unique `Blank` node with an auto-incrementing ID.
    ///
    /// Each call returns a globally unique blank node. Blank nodes are useful
    /// for representing anonymous entities in the graph.
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::NodeId;
    ///
    /// let node1 = NodeId::blank();
    /// let node2 = NodeId::blank();
    ///
    /// assert!(node1.is_blank());
    /// assert!(node2.is_blank());
    /// assert_ne!(node1, node2); // Each blank node is unique
    /// ```
    pub fn blank() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self::Blank(COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Creates a `Blank` node with a specific ID.
    pub fn blank_with_id(id: u64) -> Self {
        Self::Blank(id)
    }

    /// Returns `true` if this is a `Named` node.
    pub fn is_named(&self) -> bool {
        matches!(self, Self::Named(_))
    }

    /// Returns `true` if this is a `Hash` node.
    pub fn is_hash(&self) -> bool {
        matches!(self, Self::Hash(_))
    }

    /// Returns `true` if this is a `Blank` node.
    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Blank(_))
    }

    /// Returns the name if this is a `Named` node.
    pub fn as_name(&self) -> Option<&str> {
        match self {
            Self::Named(name) => Some(name),
            _ => None,
        }
    }

    /// Returns the hash bytes if this is a `Hash` node.
    pub fn as_hash(&self) -> Option<&[u8; 32]> {
        match self {
            Self::Hash(hash) => Some(hash),
            _ => None,
        }
    }

    /// For `Named` nodes, returns the namespace prefix (the part before the first colon).
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::NodeId;
    ///
    /// let node = NodeId::named("user:alice");
    /// assert_eq!(node.namespace(), Some("user"));
    ///
    /// let no_ns = NodeId::named("alice");
    /// assert_eq!(no_ns.namespace(), Some("alice"));
    /// ```
    pub fn namespace(&self) -> Option<&str> {
        match self {
            Self::Named(name) => name.split(':').next(),
            _ => None,
        }
    }

    /// For `Named` nodes, returns the local name (the part after the last colon).
    ///
    /// # Examples
    ///
    /// ```
    /// use aingle_graph::NodeId;
    ///
    /// let node = NodeId::named("user:alice");
    /// assert_eq!(node.local_name(), Some("alice"));
    ///
    /// let nested = NodeId::named("org:dept:engineering");
    /// assert_eq!(nested.local_name(), Some("engineering"));
    /// ```
    pub fn local_name(&self) -> Option<&str> {
        match self {
            Self::Named(name) => name.rsplit(':').next(),
            _ => None,
        }
    }

    /// Serializes the `NodeId` to a byte vector for storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    /// Deserializes a `NodeId` from a byte slice.
    pub fn from_storage_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => write!(f, "<{}>", name),
            Self::Hash(hash) => write!(f, "_:hash:{}", hex::encode(&hash[..8])),
            Self::Blank(id) => write!(f, "_:b{}", id),
        }
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self::Named(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self::Named(s.to_string())
    }
}

impl From<[u8; 32]> for NodeId {
    fn from(hash: [u8; 32]) -> Self {
        Self::Hash(hash)
    }
}

// Helper for hex encoding (minimal implementation)
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX_CHARS[(b >> 4) as usize] as char);
            s.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_node() {
        let node = NodeId::named("user:alice");
        assert!(node.is_named());
        assert_eq!(node.as_name(), Some("user:alice"));
        assert_eq!(node.namespace(), Some("user"));
        assert_eq!(node.local_name(), Some("alice"));
    }

    #[test]
    fn test_hash_node() {
        let hash = [1u8; 32];
        let node = NodeId::hash(hash);
        assert!(node.is_hash());
        assert_eq!(node.as_hash(), Some(&hash));
    }

    #[test]
    fn test_blank_node() {
        let node1 = NodeId::blank();
        let node2 = NodeId::blank();
        assert!(node1.is_blank());
        assert_ne!(node1, node2); // Each blank node is unique
    }

    #[test]
    fn test_display() {
        let named = NodeId::named("user:alice");
        assert_eq!(format!("{}", named), "<user:alice>");

        let blank = NodeId::blank_with_id(42);
        assert_eq!(format!("{}", blank), "_:b42");
    }

    #[test]
    fn test_serialization() {
        let node = NodeId::named("test:node");
        let bytes = node.to_bytes();
        let restored = NodeId::from_storage_bytes(&bytes).unwrap();
        assert_eq!(node, restored);
    }
}
