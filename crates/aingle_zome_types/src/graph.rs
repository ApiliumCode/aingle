//! Types for semantic graph operations across the WASM boundary.
//!
//! These types allow zome code to interact with the AIngle Cortex
//! semantic graph (RDF triples) and Titans memory system.

use serde::{Deserialize, Serialize};

/// A single RDF triple: subject-predicate-object.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Triple {
    /// The subject of the triple (e.g., "mayros:agent:alice").
    pub subject: String,
    /// The predicate/relationship (e.g., "mayros:memory:category").
    pub predicate: String,
    /// The object/value.
    pub object: ObjectValue,
}

/// The object component of a triple, which can be a node reference or a literal value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum ObjectValue {
    /// A reference to another node in the graph.
    Node(String),
    /// A string literal value.
    Literal(String),
    /// A numeric literal value.
    Number(f64),
    /// A boolean literal value.
    Boolean(bool),
}

/// A pattern for matching triples, where None means "match any".
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TriplePattern {
    /// Match triples with this subject (None = any).
    pub subject: Option<String>,
    /// Match triples with this predicate (None = any).
    pub predicate: Option<String>,
    /// Match triples with this object (None = any).
    pub object: Option<ObjectValue>,
}

// -- Graph Query --

/// Input for querying the semantic graph from a zome.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphQueryInput {
    /// Optional triple pattern to match against.
    pub pattern: Option<TriplePattern>,
    /// Filter by subject.
    pub subject: Option<String>,
    /// Filter by predicate.
    pub predicate: Option<String>,
    /// Maximum number of results to return.
    pub limit: Option<u32>,
}

/// Output from a graph query.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphQueryOutput {
    /// The matching triples.
    pub triples: Vec<Triple>,
    /// Total number of matching triples (may exceed limit).
    pub total: u64,
}

// -- Graph Store --

/// Input for storing a triple in the semantic graph.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphStoreInput {
    /// The subject of the triple.
    pub subject: String,
    /// The predicate of the triple.
    pub predicate: String,
    /// The object of the triple.
    pub object: ObjectValue,
}

/// Output after storing a triple.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphStoreOutput {
    /// The unique identifier for the stored triple.
    pub triple_id: String,
}

// -- Memory Recall --

/// Input for recalling memories from the Titans system.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryRecallInput {
    /// The query text to search for.
    pub query: String,
    /// Optional filter by entry type.
    pub entry_type: Option<String>,
    /// Maximum number of results.
    pub limit: Option<u32>,
}

/// A single memory result.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryResult {
    /// Unique identifier for this memory.
    pub id: String,
    /// The memory content.
    pub data: String,
    /// The type of memory entry.
    pub entry_type: String,
    /// Tags associated with this memory.
    pub tags: Vec<String>,
    /// Importance score (0.0 to 1.0).
    pub importance: f32,
    /// When this memory was created (ISO 8601).
    pub created_at: String,
}

/// Output from a memory recall operation.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryRecallOutput {
    /// The matching memory results.
    pub results: Vec<MemoryResult>,
}

// -- Memory Remember --

/// Input for storing a new memory in the Titans system.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryRememberInput {
    /// The data to remember.
    pub data: String,
    /// The type of entry (e.g., "fact", "preference", "decision").
    pub entry_type: String,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Importance score (0.0 to 1.0).
    pub importance: f32,
}

/// Output after storing a memory.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryRememberOutput {
    /// The unique identifier for the stored memory.
    pub id: String,
}
