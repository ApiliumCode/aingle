//! Core data types for the minimal AIngle node.
//!
//! This module defines the fundamental types used throughout the aingle_minimal crate,
//! including cryptographic primitives, data structures for the distributed hash table (DHT),
//! and node statistics.
//!
//! # Core Types
//!
//! - [`struct@Hash`] - Blake3 content-addressable identifier for data
//! - [`AgentPubKey`] - Public key identity for agents on the network
//! - [`Timestamp`] - High-precision microsecond timestamps
//! - [`Entry`] - Application data stored on the DHT
//! - [`Action`] - Signed operations on an agent's source chain
//! - [`Record`] - Complete unit combining an action with its entry
//! - [`Link`] - Directional relationships between entries
//! - [`NodeStats`] - Performance and state metrics for a node

use serde::{Deserialize, Serialize};

/// A 32-byte Blake3 hash used for content-addressable identification of data.
///
/// This type provides a cryptographically secure identifier for entries, actions,
/// and other data structures in the AIngle network. It uses the Blake3 hashing
/// algorithm for fast and secure content addressing.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::Hash;
/// // Create a hash from data
/// let data = b"Hello, AIngle!";
/// let hash = Hash::from_bytes(data);
///
/// // Convert to hexadecimal representation
/// let hex_string = hash.to_hex();
/// println!("Hash: {}", hex_string);
///
/// // Parse from hexadecimal
/// let parsed = Hash::from_hex(&hex_string).unwrap();
/// assert_eq!(hash, parsed);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    /// Creates a `Hash` by hashing the given byte slice with Blake3.
    ///
    /// This method computes a cryptographically secure hash of the input data,
    /// making it suitable for content-addressable storage.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Hash;
    /// let data = b"sensor data: temperature=23.5C";
    /// let hash = Hash::from_bytes(data);
    /// println!("Content hash: {}", hash.to_hex());
    /// ```
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let hash = blake3::hash(bytes);
        Self(*hash.as_bytes())
    }

    /// Creates a `Hash` from a raw 32-byte array without hashing.
    ///
    /// This method directly wraps existing hash bytes, which is useful when
    /// deserializing hashes or working with pre-computed values. If the input
    /// is shorter than 32 bytes, it will be zero-padded.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Hash;
    /// let raw_bytes = [0u8; 32]; // Pre-existing hash bytes
    /// let hash = Hash::from_raw(&raw_bytes);
    /// ```
    #[inline]
    pub fn from_raw(bytes: &[u8]) -> Self {
        let mut arr = [0u8; 32];
        let len = std::cmp::min(bytes.len(), 32);
        arr[..len].copy_from_slice(&bytes[..len]);
        Self(arr)
    }

    /// Returns the raw byte representation of the hash.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Hash;
    /// let hash = Hash::from_bytes(b"data");
    /// let bytes: &[u8; 32] = hash.as_bytes();
    /// assert_eq!(bytes.len(), 32);
    /// ```
    #[inline]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns a hexadecimal string representation of the hash.
    ///
    /// This is useful for logging, debugging, and human-readable display of hashes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Hash;
    /// let hash = Hash::from_bytes(b"test");
    /// let hex = hash.to_hex();
    /// assert_eq!(hex.len(), 64); // 32 bytes = 64 hex characters
    /// ```
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    /// Creates a `Hash` from a hexadecimal string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not valid hexadecimal or is not exactly
    /// 64 characters (32 bytes).
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Hash;
    /// let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    /// let hash = Hash::from_hex(hex).unwrap();
    /// assert_eq!(hash.to_hex(), hex);
    /// ```
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.to_hex()[..8])
    }
}

/// The 32-byte public key of an agent on the AIngle network.
///
/// This type represents the permanent identity of a node or agent. Each agent is
/// identified by their Ed25519 public key, which is used for signing actions and
/// verifying identity in peer-to-peer communications.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{MinimalNode, Config};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Get the public key for a node
/// let node = MinimalNode::new(Config::test_mode())?;
/// let pubkey = node.public_key();
/// println!("Agent identity: {}", pubkey.to_hex());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentPubKey(pub [u8; 32]);

impl AgentPubKey {
    /// Returns the raw byte representation of the public key.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = MinimalNode::new(Config::test_mode())?;
    /// let pubkey = node.public_key();
    /// let bytes = pubkey.as_bytes();
    /// assert_eq!(bytes.len(), 32);
    /// # Ok(())
    /// # }
    /// ```
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns a hexadecimal string representation of the public key.
    ///
    /// This is useful for displaying agent identities in logs, UIs, and network protocols.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{MinimalNode, Config};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let node = MinimalNode::new(Config::test_mode())?;
    /// let pubkey = node.public_key();
    /// let hex = pubkey.to_hex();
    /// assert_eq!(hex.len(), 64); // 32 bytes = 64 hex characters
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl Serialize for AgentPubKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for AgentPubKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut arr = [0u8; 32];
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Invalid public key length"));
        }
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

/// A high-precision timestamp in microseconds since the Unix epoch.
///
/// This type provides microsecond-level precision for ordering actions and entries
/// on the AIngle network. It's essential for maintaining causality in distributed
/// systems and resolving conflicts.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::Timestamp;
/// // Get current time
/// let now = Timestamp::now();
/// println!("Current time: {} microseconds", now.0);
///
/// // Create from milliseconds
/// let ts = Timestamp::from_millis(1000000);
/// assert_eq!(ts.as_millis(), 1000000);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Returns the current timestamp with microsecond precision.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Timestamp;
    /// let ts1 = Timestamp::now();
    /// std::thread::sleep(std::time::Duration::from_millis(1));
    /// let ts2 = Timestamp::now();
    /// assert!(ts2 > ts1);
    /// ```
    #[inline]
    pub fn now() -> Self {
        let now = chrono::Utc::now();
        // timestamp() returns seconds, multiply by 1_000_000 to get microseconds
        let micros = (now.timestamp() as u64) * 1_000_000 + (now.timestamp_subsec_micros() as u64);
        Self(micros)
    }

    /// Creates a `Timestamp` from milliseconds since the Unix epoch.
    ///
    /// This is useful when working with systems that use millisecond precision.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Timestamp;
    /// let ts = Timestamp::from_millis(1609459200000); // 2021-01-01 00:00:00 UTC
    /// assert_eq!(ts.as_millis(), 1609459200000);
    /// ```
    #[inline]
    pub fn from_millis(ms: u64) -> Self {
        Self(ms * 1000)
    }

    /// Returns the timestamp as milliseconds since the Unix epoch.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Timestamp;
    /// let ts = Timestamp::now();
    /// let millis = ts.as_millis();
    /// assert!(millis > 0);
    /// ```
    pub fn as_millis(&self) -> u64 {
        self.0 / 1000
    }
}

/// The type of data contained within an [`Entry`].
///
/// This enum classifies entries into different categories, each serving a specific
/// purpose in the AIngle network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntryType {
    /// A general-purpose application data entry.
    ///
    /// This is the most common entry type, used for storing arbitrary application
    /// data such as sensor readings, messages, or any other user-defined content.
    App,
    /// An entry representing an agent's public key.
    ///
    /// Used during agent initialization to publish their identity to the network.
    AgentKey,
    /// A capability grant entry.
    ///
    /// Grants specific capabilities or permissions to other agents.
    CapGrant,
    /// A capability claim entry.
    ///
    /// Claims a capability that was previously granted.
    CapClaim,
}

/// A fundamental unit of data stored on the distributed hash table (DHT).
///
/// Entries contain the actual application data in the AIngle network. Each entry
/// is content-addressable via its [`struct@Hash`] and is associated with an [`Action`]
/// that records metadata about when and by whom it was created.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::Entry;
/// use serde_json::json;
///
/// // Create an app entry with sensor data
/// let sensor_data = json!({
///     "temperature": 23.5,
///     "humidity": 65.2,
///     "timestamp": 1234567890
/// });
///
/// let entry = Entry::app(sensor_data).unwrap();
/// println!("Entry hash: {}", entry.hash().to_hex());
/// println!("Entry size: {} bytes", entry.size());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// The type of the entry.
    pub entry_type: EntryType,
    /// The content of the entry, stored as serialized bytes.
    pub content: Vec<u8>,
}

impl Entry {
    /// Creates a new application [`Entry`] with serialized content.
    ///
    /// This is the primary way to create entries containing application data.
    /// The content is automatically serialized to JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the content cannot be serialized to JSON.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Entry;
    /// use serde::{Serialize, Deserialize};
    ///
    /// #[derive(Serialize, Deserialize)]
    /// struct SensorReading {
    ///     sensor_id: String,
    ///     value: f64,
    /// }
    ///
    /// let reading = SensorReading {
    ///     sensor_id: "temp_01".to_string(),
    ///     value: 23.5,
    /// };
    ///
    /// let entry = Entry::app(reading).unwrap();
    /// ```
    pub fn app(content: impl Serialize) -> Result<Self, serde_json::Error> {
        Ok(Self {
            entry_type: EntryType::App,
            content: serde_json::to_vec(&content)?,
        })
    }

    /// Returns the content hash of the entry.
    ///
    /// This hash uniquely identifies the entry's content and is used for
    /// content-addressable storage in the DHT.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Entry;
    /// let entry = Entry::app("test data").unwrap();
    /// let hash = entry.hash();
    /// println!("Entry can be retrieved using hash: {}", hash.to_hex());
    /// ```
    #[inline]
    pub fn hash(&self) -> Hash {
        Hash::from_bytes(&self.content)
    }

    /// Returns the size of the entry's content in bytes.
    ///
    /// This is useful for monitoring storage usage and enforcing size limits.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::Entry;
    /// let entry = Entry::app("test").unwrap();
    /// println!("Entry size: {} bytes", entry.size());
    /// ```
    pub fn size(&self) -> usize {
        self.content.len()
    }
}

/// The type of an [`Action`] on an agent's source chain.
///
/// Each action type represents a different operation that can be performed
/// on the distributed hash table (DHT).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    /// The first action in an agent's source chain, establishing their identity.
    ///
    /// Every agent must begin their chain with a Genesis action.
    Genesis,
    /// An action that creates a new entry on the DHT.
    ///
    /// This is the most common action type, used whenever new data is published.
    Create,
    /// An action that updates an existing entry with new content.
    ///
    /// Updates reference the original entry being modified.
    Update,
    /// An action that marks an existing entry as deleted.
    ///
    /// Deletes don't remove data but signal that it should no longer be used.
    Delete,
    /// An action that creates a directional link between two entries.
    ///
    /// Links enable graph-like relationships between data.
    CreateLink,
    /// An action that removes a previously created link.
    DeleteLink,
}

/// A 64-byte Ed25519 cryptographic signature.
///
/// Signatures are used to authenticate actions and verify that they were created
/// by the claimed author. Each signature is computed over the action's content
/// using the author's private key.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{MinimalNode, Config};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Signatures are created automatically when creating entries
/// let mut node = MinimalNode::new(Config::test_mode())?;
/// let hash = node.create_entry("test data")?;
/// // The action is now signed with the node's private key
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Signature(pub [u8; 64]);

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut arr = [0u8; 64];
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("Invalid signature length"));
        }
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

/// A signed action on an agent's source chain.
///
/// Actions form an immutable, append-only chain for each agent, providing a
/// cryptographically verified history of all operations. Each action is linked
/// to the previous one, creating a tamper-evident audit trail.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{Action, ActionType, AgentPubKey, Timestamp, Signature};
/// // Actions are typically created by MinimalNode.create_entry()
/// // but can be constructed manually for advanced use cases
/// let action = Action {
///     action_type: ActionType::Create,
///     author: AgentPubKey([0u8; 32]),
///     timestamp: Timestamp::now(),
///     seq: 1,
///     prev_action: None,
///     entry_hash: None,
///     signature: Signature([0u8; 64]),
/// };
///
/// // Get the action's hash
/// let hash = action.hash();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// The type of the action.
    pub action_type: ActionType,
    /// The public key of the agent who authored the action.
    pub author: AgentPubKey,
    /// The timestamp of when the action was created.
    pub timestamp: Timestamp,
    /// The sequence number of this action in the agent's source chain.
    ///
    /// Sequence numbers start at 1 and increment for each new action,
    /// ensuring total ordering within an agent's chain.
    pub seq: u32,
    /// The hash of the previous action in the chain.
    ///
    /// This is `None` only for the `Genesis` action, which is the first
    /// action in every agent's source chain.
    pub prev_action: Option<Hash>,
    /// The hash of the [`Entry`] associated with this action, if any.
    ///
    /// This links the action to its content on the DHT. Not all action types
    /// have associated entries (e.g., `DeleteLink` doesn't).
    pub entry_hash: Option<Hash>,
    /// The cryptographic signature of the action's content.
    ///
    /// This signature proves that the action was created by the agent identified
    /// by the `author` field and that the action hasn't been tampered with.
    pub signature: Signature,
}

impl Action {
    /// Returns the content hash of the action.
    ///
    /// This hash uniquely identifies the action and is used as a reference
    /// when linking to it from subsequent actions or other data structures.
    ///
    /// # Examples
    ///
    /// ```
    /// # use aingle_minimal::{Action, ActionType, AgentPubKey, Timestamp, Signature};
    /// let action = Action {
    ///     action_type: ActionType::Create,
    ///     author: AgentPubKey([0u8; 32]),
    ///     timestamp: Timestamp::now(),
    ///     seq: 1,
    ///     prev_action: None,
    ///     entry_hash: None,
    ///     signature: Signature([0u8; 64]),
    /// };
    ///
    /// let hash = action.hash();
    /// println!("Action hash: {}", hash.to_hex());
    /// ```
    pub fn hash(&self) -> Hash {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        Hash::from_bytes(&bytes)
    }
}

/// A complete, stored record combining an [`Action`] with its corresponding [`Entry`].
///
/// Records are the fundamental unit of data gossiped and stored across the network.
/// They combine the metadata (action) with the actual content (entry), providing a
/// complete, verifiable package of information.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{MinimalNode, Config};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut node = MinimalNode::new(Config::test_mode())?;
///
/// // Create an entry, which internally creates a Record
/// let hash = node.create_entry("sensor data")?;
///
/// // Records are stored and can be retrieved
/// // (retrieval API not shown in this minimal example)
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// The signed action metadata.
    pub action: Action,
    /// The entry data associated with the action, if applicable.
    ///
    /// This is `Some` for action types that reference content (like `Create` and `Update`)
    /// and `None` for actions that don't have associated content (like `DeleteLink`).
    pub entry: Option<Entry>,
}

/// Represents a directional link between two entries on the DHT.
///
/// Links enable creating graph-like relationships between entries, such as
/// references, hierarchies, or associations. They're useful for modeling
/// relationships like "sensor A is located in room B" or "message X replies to message Y".
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{Link, Hash, Timestamp};
/// // Links are typically created through MinimalNode APIs
/// // This example shows the structure
/// let link = Link {
///     base: Hash::from_bytes(b"base entry"),
///     target: Hash::from_bytes(b"target entry"),
///     link_type: 1, // Custom type for categorization
///     tag: b"sensor_to_room".to_vec(),
///     timestamp: Timestamp::now(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// The hash of the source or "base" entry.
    ///
    /// This is the entry that the link originates from.
    pub base: Hash,
    /// The hash of the target entry.
    ///
    /// This is the entry that the link points to.
    pub target: Hash,
    /// A numeric type for categorizing the link.
    ///
    /// Applications can use this to distinguish different types of relationships.
    pub link_type: u8,
    /// An arbitrary tag for additional metadata.
    ///
    /// Tags are useful for indexing and querying links, allowing efficient
    /// lookups of specific link categories.
    pub tag: Vec<u8>,
    /// The timestamp of when the link was created.
    pub timestamp: Timestamp,
}

/// Collects statistics about the node's performance and state.
///
/// These metrics are useful for monitoring node health, resource usage,
/// and network connectivity.
///
/// # Examples
///
/// ```
/// # use aingle_minimal::{MinimalNode, Config};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let node = MinimalNode::new(Config::test_mode())?;
///
/// // Get current node statistics
/// let stats = node.stats()?;
/// println!("Entries: {}", stats.entries_count);
/// println!("Actions: {}", stats.actions_count);
/// println!("Storage used: {} bytes", stats.storage_used);
/// println!("Connected peers: {}", stats.peer_count);
/// println!("Uptime: {} seconds", stats.uptime_secs);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeStats {
    /// The total number of entries stored by the node.
    pub entries_count: u64,
    /// The total number of actions in the node's source chain.
    pub actions_count: u64,
    /// The current memory usage of the node in bytes.
    ///
    /// This may be 0 if memory tracking is not enabled or not available
    /// on the current platform.
    pub memory_used: usize,
    /// The current storage usage on disk in bytes.
    pub storage_used: usize,
    /// The number of currently connected peers.
    pub peer_count: usize,
    /// The uptime of the node in seconds since it was started.
    pub uptime_secs: u64,
}

// A simple hex encoding/decoding implementation.
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, FromHexError> {
        if s.len() % 2 != 0 {
            return Err(FromHexError::OddLength);
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| FromHexError::InvalidChar))
            .collect()
    }

    #[derive(Debug)]
    pub enum FromHexError {
        OddLength,
        InvalidChar,
    }

    impl std::fmt::Display for FromHexError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::OddLength => write!(f, "odd length hex string"),
                Self::InvalidChar => write!(f, "invalid hex character"),
            }
        }
    }

    impl std::error::Error for FromHexError {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_from_bytes() {
        let data = b"test data";
        let hash = Hash::from_bytes(data);
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn test_hash_from_raw() {
        let raw = [1u8; 32];
        let hash = Hash::from_raw(&raw);
        assert_eq!(hash.0, raw);
    }

    #[test]
    fn test_hash_from_raw_short() {
        let raw = [1u8; 16];
        let hash = Hash::from_raw(&raw);
        assert_eq!(&hash.0[..16], &raw);
        assert_eq!(&hash.0[16..], &[0u8; 16]);
    }

    #[test]
    fn test_hash_to_hex() {
        let hash = Hash([0xab; 32]);
        let hex = hash.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_from_hex() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let hash = Hash::from_hex(hex).unwrap();
        assert_eq!(hash.to_hex(), hex);
    }

    #[test]
    fn test_hash_from_hex_invalid() {
        let result = Hash::from_hex("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_display() {
        let hash = Hash([0x12; 32]);
        let display = format!("{}", hash);
        assert!(!display.is_empty());
        assert!(display.len() <= 16);
    }

    #[test]
    fn test_hash_serialization() {
        let hash = Hash::from_bytes(b"test");
        let json = serde_json::to_string(&hash).unwrap();
        let parsed: Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(hash, parsed);
    }

    #[test]
    fn test_agent_pubkey_to_hex() {
        let key = AgentPubKey([0xcd; 32]);
        let hex = key.to_hex();
        assert_eq!(hex.len(), 64);
    }

    #[test]
    fn test_agent_pubkey_as_bytes() {
        let key = AgentPubKey([0x55; 32]);
        let bytes = key.as_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes[0], 0x55);
    }

    #[test]
    fn test_agent_pubkey_serialization() {
        let key = AgentPubKey([0x55; 32]);
        let json = serde_json::to_string(&key).unwrap();
        let parsed: AgentPubKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_timestamp_now() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let ts = Timestamp::now();

        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        assert!(ts.0 >= before);
        assert!(ts.0 <= after);
    }

    #[test]
    fn test_timestamp_as_millis() {
        let ts = Timestamp(12345678);
        // as_millis converts microseconds to milliseconds
        assert_eq!(ts.as_millis(), 12345);
    }

    #[test]
    fn test_timestamp_serialization() {
        let ts = Timestamp(987654321);
        let json = serde_json::to_string(&ts).unwrap();
        let parsed: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(ts.0, parsed.0);
    }

    #[test]
    fn test_signature_fields() {
        let sig = Signature([0xab; 64]);
        assert_eq!(sig.0.len(), 64);
        let hex = hex::encode(&sig.0);
        assert_eq!(hex.len(), 128);
    }

    #[test]
    fn test_signature_serialization() {
        let sig = Signature([0x12; 64]);
        let json = serde_json::to_string(&sig).unwrap();
        let parsed: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(sig.0, parsed.0);
    }

    #[test]
    fn test_action_hash() {
        let action = Action {
            action_type: ActionType::Create,
            author: AgentPubKey([0u8; 32]),
            timestamp: Timestamp(1234567890),
            seq: 1,
            prev_action: None,
            entry_hash: None,
            signature: Signature([0u8; 64]),
        };

        let hash = action.hash();
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn test_action_type_variants() {
        let types = vec![
            ActionType::Genesis,
            ActionType::Create,
            ActionType::Update,
            ActionType::Delete,
            ActionType::CreateLink,
            ActionType::DeleteLink,
        ];

        for action_type in types {
            let debug_str = format!("{:?}", action_type);
            assert!(!debug_str.is_empty());
        }
    }

    #[test]
    fn test_entry_type_variants() {
        let types = vec![
            EntryType::App,
            EntryType::AgentKey,
            EntryType::CapGrant,
            EntryType::CapClaim,
        ];

        for entry_type in types {
            let debug_str = format!("{:?}", entry_type);
            assert!(!debug_str.is_empty());
        }
    }

    #[test]
    fn test_entry_hash() {
        let entry = Entry {
            entry_type: EntryType::App,
            content: vec![1, 2, 3, 4, 5],
        };

        let hash = entry.hash();
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn test_link_fields() {
        let link = Link {
            base: Hash([1u8; 32]),
            target: Hash([2u8; 32]),
            link_type: 42,
            tag: vec![10, 20, 30],
            timestamp: Timestamp::now(),
        };

        assert_eq!(link.link_type, 42);
        assert_eq!(link.tag, vec![10, 20, 30]);
    }

    #[test]
    fn test_record_fields() {
        let record = Record {
            action: Action {
                action_type: ActionType::Create,
                author: AgentPubKey([0u8; 32]),
                timestamp: Timestamp(1234567890),
                seq: 1,
                prev_action: None,
                entry_hash: None,
                signature: Signature([0u8; 64]),
            },
            entry: Some(Entry {
                entry_type: EntryType::App,
                content: vec![1, 2, 3],
            }),
        };

        // Record uses action.hash() for its hash
        let hash = record.action.hash();
        assert_eq!(hash.0.len(), 32);
        assert!(record.entry.is_some());
    }

    #[test]
    fn test_node_stats_default() {
        let stats = NodeStats::default();
        assert_eq!(stats.entries_count, 0);
        assert_eq!(stats.actions_count, 0);
        assert_eq!(stats.memory_used, 0);
        assert_eq!(stats.storage_used, 0);
    }

    #[test]
    fn test_node_stats_fields() {
        let stats = NodeStats {
            entries_count: 80,
            actions_count: 100,
            memory_used: 1024,
            storage_used: 4096,
            peer_count: 5,
            uptime_secs: 3600,
        };

        assert_eq!(stats.entries_count, 80);
        assert_eq!(stats.peer_count, 5);
        assert_eq!(stats.uptime_secs, 3600);
    }

    #[test]
    fn test_hex_encode() {
        let result = hex::encode(&[0x12, 0xab]);
        assert_eq!(result, "12ab");
    }

    #[test]
    fn test_hex_decode() {
        let result = hex::decode("12ab").unwrap();
        assert_eq!(result, vec![0x12, 0xab]);
    }

    #[test]
    fn test_hex_decode_odd_length() {
        let result = hex::decode("12a");
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_decode_invalid_char() {
        let result = hex::decode("zzzz");
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_error_display() {
        let error = hex::FromHexError::OddLength;
        let display = format!("{}", error);
        assert!(display.contains("odd"));

        let error = hex::FromHexError::InvalidChar;
        let display = format!("{}", error);
        assert!(display.contains("invalid"));
    }
}
