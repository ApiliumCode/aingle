//! Core data types for the Titans Memory system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A unique, content-based identifier for a `MemoryEntry`.
///
/// It is derived from a blake3 hash of the entry's content and creation timestamp,
/// ensuring that each memory entry has a stable and unique ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoryId([u8; 32]);

impl MemoryId {
    /// Creates a `MemoryId` from a 32-byte array.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Creates a `MemoryId` by hashing the given data.
    pub fn from_data(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Returns the raw byte representation of the ID.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns a hexadecimal string representation of the ID.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Creates a `MemoryId` from a hexadecimal string.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let s = std::str::from_utf8(chunk).ok()?;
            bytes[i] = u8::from_str_radix(s, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl Serialize for MemoryId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for MemoryId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).ok_or_else(|| serde::de::Error::custom("invalid memory id hex"))
    }
}

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

    /// Creates a `Timestamp` from a duration in seconds since the epoch.
    pub fn from_secs(secs: u64) -> Self {
        Self(secs * 1_000_000)
    }

    /// Calculates the age of the timestamp in seconds from the present moment.
    pub fn age_secs(&self) -> u64 {
        let now = Self::now();
        (now.0.saturating_sub(self.0)) / 1_000_000
    }
}

/// The fundamental unit of memory stored in the system.
///
/// A `MemoryEntry` represents a single piece of information, observation, or event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The unique identifier for this memory entry.
    pub id: MemoryId,
    /// A string that categorizes the entry (e.g., "observation", "chat_message", "error").
    pub entry_type: String,
    /// The actual data payload of the memory, stored as a flexible JSON value.
    pub data: serde_json::Value,
    /// Metadata associated with this memory, such as timestamps and importance scores.
    pub metadata: MemoryMetadata,
    /// A list of `SemanticTag`s for indexing and querying.
    pub tags: Vec<SemanticTag>,
    /// An optional embedding vector for semantic search.
    pub embedding: Option<Embedding>,
}

impl MemoryEntry {
    /// Creates a new `MemoryEntry` with a unique ID generated from its content and a timestamp.
    pub fn new(entry_type: &str, data: serde_json::Value) -> Self {
        // Include entry_type and timestamp in ID to ensure uniqueness
        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let mut to_hash = Vec::new();
        to_hash.extend_from_slice(entry_type.as_bytes());
        to_hash.extend_from_slice(&timestamp.to_le_bytes());
        to_hash.extend_from_slice(&serde_json::to_vec(&data).unwrap_or_default());
        let id = MemoryId::from_data(&to_hash);

        Self {
            id,
            entry_type: entry_type.to_string(),
            data,
            metadata: MemoryMetadata::default(),
            tags: Vec::new(),
            embedding: None,
        }
    }

    /// Associates a list of tags with the memory entry.
    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|t| SemanticTag::new(t)).collect();
        self
    }

    /// Sets the importance score for the memory entry.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.metadata.importance = importance;
        self
    }

    /// Attaches an embedding vector to the memory entry.
    pub fn with_embedding(mut self, embedding: Embedding) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Computes an estimate of the memory entry's size in bytes.
    pub fn size_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.entry_type.len()
            + serde_json::to_vec(&self.data).map(|v| v.len()).unwrap_or(0)
            + self.tags.iter().map(|t| t.0.len()).sum::<usize>()
            + self.embedding.as_ref().map(|e| e.0.len() * 4).unwrap_or(0)
    }
}

/// Metadata associated with a `MemoryEntry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    /// The timestamp of when this memory was created.
    pub created_at: Timestamp,
    /// The timestamp of the last time this memory was accessed.
    pub last_accessed: Timestamp,
    /// A counter for how many times this memory has been accessed.
    pub access_count: u32,
    /// A score from 0.0 to 1.0 indicating the memory's intrinsic importance.
    pub importance: f32,
    /// A score from 0.0 to 1.0 representing the memory's current relevance, which decays over time.
    pub attention: f32,
    /// A flag indicating whether this memory has been consolidated into Long-Term Memory.
    pub consolidated: bool,
    /// A string indicating the origin of this memory (e.g., "sensor", "user", "inference").
    pub source: String,
}

impl Default for MemoryMetadata {
    fn default() -> Self {
        let now = Timestamp::now();
        Self {
            created_at: now,
            last_accessed: now,
            access_count: 0,
            importance: 0.5,
            attention: 1.0,
            consolidated: false,
            source: "unknown".to_string(),
        }
    }
}

impl MemoryMetadata {
    /// Creates a new `MemoryMetadata` with a specified source.
    pub fn with_source(source: &str) -> Self {
        Self {
            source: source.to_string(),
            ..Default::default()
        }
    }

    /// Updates metadata to record an access event.
    /// This boosts the attention score and increments the access count.
    pub fn record_access(&mut self) {
        self.last_accessed = Timestamp::now();
        self.access_count += 1;
        // Boost attention on access
        self.attention = (self.attention + 0.2).min(1.0);
    }

    /// Applies a decay factor to the attention score.
    pub fn decay(&mut self, factor: f32) {
        self.attention *= factor;
    }
}

/// A tag used for indexing and querying memories.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemanticTag(pub String);

impl SemanticTag {
    /// Creates a new `SemanticTag` from a string slice.
    /// The tag is normalized to lowercase.
    pub fn new(tag: &str) -> Self {
        Self(tag.to_lowercase().trim().to_string())
    }
}

/// A vector of floating-point numbers representing a semantic embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    /// Creates a new `Embedding` from a vector of f32 values.
    pub fn new(values: Vec<f32>) -> Self {
        Self(values)
    }

    /// Computes the cosine similarity between this embedding and another.
    /// Returns a value between -1.0 and 1.0.
    pub fn cosine_similarity(&self, other: &Embedding) -> f32 {
        if self.0.len() != other.0.len() || self.0.is_empty() {
            return 0.0;
        }

        let dot: f32 = self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum();
        let mag_a: f32 = self.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = other.0.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            0.0
        } else {
            dot / (mag_a * mag_b)
        }
    }

    /// A simple placeholder for text-to-embedding conversion.
    /// In a production environment, this should be replaced with a proper model (e.g., from a transformer).
    pub fn from_text_simple(text: &str) -> Self {
        const DIM: usize = 64;
        let mut values = vec![0.0f32; DIM];

        for word in text.to_lowercase().split_whitespace() {
            let hash = blake3::hash(word.as_bytes());
            let bytes = hash.as_bytes();
            for (i, &b) in bytes.iter().take(DIM).enumerate() {
                values[i] += (b as f32 / 255.0) - 0.5;
            }
        }

        // Normalize the vector
        let magnitude: f32 = values.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for v in &mut values {
                *v /= magnitude;
            }
        }

        Self(values)
    }
}

/// Defines a query for searching and retrieving memories.
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    /// A text string to be used for semantic or keyword search.
    pub text: Option<String>,
    /// A list of `SemanticTag`s to filter by.
    pub tags: Vec<SemanticTag>,
    /// A filter for the `entry_type` of a memory.
    pub entry_type: Option<String>,
    /// A filter for the minimum importance score.
    pub min_importance: Option<f32>,
    /// A filter for memories created after this timestamp.
    pub after: Option<Timestamp>,
    /// A filter for memories created before this timestamp.
    pub before: Option<Timestamp>,
    /// The maximum number of results to return.
    pub limit: Option<usize>,
    /// An embedding vector to be used for similarity search.
    pub embedding: Option<Embedding>,
}

impl MemoryQuery {
    /// Creates a new query based on a text string.
    pub fn text(query: &str) -> Self {
        Self {
            text: Some(query.to_string()),
            embedding: Some(Embedding::from_text_simple(query)),
            ..Default::default()
        }
    }

    /// Creates a new query based on a list of tags.
    pub fn tags(tags: &[&str]) -> Self {
        Self {
            tags: tags.iter().map(|t| SemanticTag::new(t)).collect(),
            ..Default::default()
        }
    }

    /// Creates a new query that filters by entry type.
    pub fn entry_type(entry_type: &str) -> Self {
        Self {
            entry_type: Some(entry_type.to_string()),
            ..Default::default()
        }
    }

    /// Sets the maximum number of results for the query to return.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Sets the minimum importance score for memories to be included in the results.
    pub fn with_min_importance(mut self, importance: f32) -> Self {
        self.min_importance = Some(importance);
        self
    }
}

/// A single result returned from a memory query.
#[derive(Debug, Clone)]
pub struct MemoryResult {
    /// The `MemoryEntry` that matched the query.
    pub entry: MemoryEntry,
    /// A score from 0.0 to 1.0 indicating the relevance of this result to the query.
    pub relevance: f32,
    /// The source of the memory (STM or LTM).
    pub source: MemorySource,
}

/// Indicates whether a memory result came from Short-Term or Long-Term memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySource {
    /// The memory came from the Short-Term Memory store.
    ShortTerm,
    /// The memory came from the Long-Term Memory store.
    LongTerm,
}

// ============ Knowledge Graph Types for LTM ============

/// A node in the Long-Term Memory's knowledge graph.
///
/// An `Entity` represents a person, place, object, or concept extracted from a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// The unique identifier for this entity.
    pub id: EntityId,
    /// The type of the entity (e.g., "person", "sensor", "concept").
    pub entity_type: String,
    /// A human-readable name or label for the entity.
    pub name: String,
    /// A map of additional properties associated with the entity.
    pub properties: HashMap<String, serde_json::Value>,
    /// An optional embedding vector for semantic search.
    pub embedding: Option<Embedding>,
    /// Metadata associated with the entity's lifecycle.
    pub metadata: MemoryMetadata,
}

impl Entity {
    /// Creates a new `Entity` with a unique ID generated from its type and name.
    pub fn new(entity_type: &str, name: &str) -> Self {
        let id_data = format!("{}:{}", entity_type, name);
        let id = EntityId::from_data(id_data.as_bytes());

        Self {
            id,
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            properties: HashMap::new(),
            embedding: None,
            metadata: MemoryMetadata::default(),
        }
    }

    /// Adds a property to the entity.
    pub fn with_property(mut self, key: &str, value: serde_json::Value) -> Self {
        self.properties.insert(key.to_string(), value);
        self
    }
}

/// A unique, content-based identifier for an `Entity`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityId([u8; 32]);

impl EntityId {
    /// Creates an `EntityId` from a 32-byte array.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Creates an `EntityId` by hashing the given data.
    pub fn from_data(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Returns the raw byte representation of the ID.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns a hexadecimal string representation of the ID.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl Serialize for EntityId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            if i >= 32 {
                break;
            }
            let hex_str = std::str::from_utf8(chunk).map_err(serde::de::Error::custom)?;
            bytes[i] = u8::from_str_radix(hex_str, 16).map_err(serde::de::Error::custom)?;
        }
        Ok(Self(bytes))
    }
}

/// A directional, weighted connection between two entities in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// The `EntityId` of the source entity.
    pub source: EntityId,
    /// The `EntityId` of the target entity.
    pub target: EntityId,
    /// The type of relationship this link represents.
    pub relation: Relation,
    /// The weight or strength of the link, typically between 0.0 and 1.0.
    pub weight: f32,
    /// A map of additional properties associated with the link.
    pub properties: HashMap<String, serde_json::Value>,
    /// The timestamp of when this link was created.
    pub created_at: Timestamp,
}

impl Link {
    /// Creates a new `Link` between a source and target entity with a given relation.
    pub fn new(source: EntityId, relation: Relation, target: EntityId) -> Self {
        Self {
            source,
            target,
            relation,
            weight: 1.0,
            properties: HashMap::new(),
            created_at: Timestamp::now(),
        }
    }

    /// Sets the weight of the link.
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }
}

/// The type of a `Link` in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Relation(pub String);

impl Relation {
    /// Creates a new `Relation` from a string slice.
    /// The name is normalized to uppercase.
    pub fn new(name: &str) -> Self {
        Self(name.to_uppercase())
    }

    // Common, predefined relations
    /// Represents an "IS A" or "type of" relationship.
    pub fn is_a() -> Self {
        Self::new("IS_A")
    }
    /// Represents a "HAS" or "possesses" relationship.
    pub fn has() -> Self {
        Self::new("HAS")
    }
    /// Represents a generic "RELATED TO" relationship.
    pub fn related_to() -> Self {
        Self::new("RELATED_TO")
    }
    /// Represents a causal relationship.
    pub fn caused_by() -> Self {
        Self::new("CAUSED_BY")
    }
    /// Represents a spatial relationship.
    pub fn located_at() -> Self {
        Self::new("LOCATED_AT")
    }
    /// Represents an observation relationship.
    pub fn observed() -> Self {
        Self::new("OBSERVED")
    }
}

/// A type alias for `Relation` for semantic clarity.
pub type LinkType = Relation;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_id() {
        let data = b"test data";
        let id1 = MemoryId::from_data(data);
        let id2 = MemoryId::from_data(data);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_embedding_similarity() {
        let e1 = Embedding::new(vec![1.0, 0.0, 0.0]);
        let e2 = Embedding::new(vec![1.0, 0.0, 0.0]);
        let e3 = Embedding::new(vec![0.0, 1.0, 0.0]);

        assert!((e1.cosine_similarity(&e2) - 1.0).abs() < 0.001);
        assert!((e1.cosine_similarity(&e3)).abs() < 0.001);
    }

    #[test]
    fn test_memory_entry() {
        let entry = MemoryEntry::new("test", serde_json::json!({"key": "value"}))
            .with_tags(&["tag1", "tag2"])
            .with_importance(0.8);

        assert_eq!(entry.entry_type, "test");
        assert_eq!(entry.tags.len(), 2);
        assert_eq!(entry.metadata.importance, 0.8);
    }

    #[test]
    fn test_entity() {
        let entity = Entity::new("sensor", "temp_001")
            .with_property("location", serde_json::json!("room_a"));

        assert_eq!(entity.entity_type, "sensor");
        assert_eq!(entity.name, "temp_001");
        assert!(entity.properties.contains_key("location"));
    }
}
