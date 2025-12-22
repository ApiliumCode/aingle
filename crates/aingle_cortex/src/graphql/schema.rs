//! GraphQL schema definitions

use async_graphql::*;
use chrono::{DateTime, Utc};

/// Triple type in GraphQL
#[derive(Debug, Clone, SimpleObject)]
pub struct Triple {
    /// Unique hash identifier
    pub id: ID,
    /// Subject node
    pub subject: String,
    /// Predicate (relationship)
    pub predicate: String,
    /// Object value
    pub object: TripleValue,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// Value union type for triple objects
#[derive(Debug, Clone, Union)]
pub enum TripleValue {
    String(StringValue),
    Integer(IntegerValue),
    Float(FloatValue),
    Boolean(BooleanValue),
    Node(NodeValue),
}

/// String value wrapper
#[derive(Debug, Clone, SimpleObject)]
pub struct StringValue {
    pub value: String,
}

/// Integer value wrapper
#[derive(Debug, Clone, SimpleObject)]
pub struct IntegerValue {
    pub value: i64,
}

/// Float value wrapper
#[derive(Debug, Clone, SimpleObject)]
pub struct FloatValue {
    pub value: f64,
}

/// Boolean value wrapper
#[derive(Debug, Clone, SimpleObject)]
pub struct BooleanValue {
    pub value: bool,
}

/// Node reference wrapper
#[derive(Debug, Clone, SimpleObject)]
pub struct NodeValue {
    pub iri: String,
}

impl From<aingle_graph::Value> for TripleValue {
    fn from(v: aingle_graph::Value) -> Self {
        match v {
            aingle_graph::Value::String(s) => TripleValue::String(StringValue { value: s }),
            aingle_graph::Value::Integer(i) => TripleValue::Integer(IntegerValue { value: i }),
            aingle_graph::Value::Float(f) => TripleValue::Float(FloatValue { value: f }),
            aingle_graph::Value::Boolean(b) => TripleValue::Boolean(BooleanValue { value: b }),
            aingle_graph::Value::Node(n) => TripleValue::Node(NodeValue { iri: n.to_string() }),
            aingle_graph::Value::DateTime(dt) => TripleValue::String(StringValue { value: dt }),
            aingle_graph::Value::Typed { value, .. } => TripleValue::String(StringValue { value }),
            aingle_graph::Value::LangString { value, .. } => {
                TripleValue::String(StringValue { value })
            }
            aingle_graph::Value::Bytes(_) => TripleValue::String(StringValue {
                value: "[binary]".to_string(),
            }),
            aingle_graph::Value::Json(v) => TripleValue::String(StringValue {
                value: v.to_string(),
            }),
            aingle_graph::Value::Null => TripleValue::String(StringValue {
                value: "null".to_string(),
            }),
        }
    }
}

impl From<aingle_graph::Triple> for Triple {
    fn from(t: aingle_graph::Triple) -> Self {
        Self {
            id: ID(t.id().to_hex()),
            subject: t.subject.to_string(),
            predicate: t.predicate.to_string(),
            object: t.object.into(),
            created_at: t.meta.created_at,
        }
    }
}

/// Filter input for querying triples
#[derive(Debug, Clone, InputObject)]
pub struct TripleFilter {
    /// Filter by subject (exact match)
    pub subject: Option<String>,
    /// Filter by predicate (exact match)
    pub predicate: Option<String>,
    /// Filter by subject prefix
    pub subject_prefix: Option<String>,
    /// Filter by predicate prefix
    pub predicate_prefix: Option<String>,
}

/// Input for creating a triple
#[derive(Debug, Clone, InputObject)]
pub struct TripleInput {
    /// Subject node
    pub subject: String,
    /// Predicate (relationship)
    pub predicate: String,
    /// Object value
    pub object: ValueInput,
}

/// Value input for mutations
#[derive(Debug, Clone, InputObject)]
pub struct ValueInput {
    /// String value
    pub string: Option<String>,
    /// Integer value
    pub integer: Option<i64>,
    /// Float value
    pub float: Option<f64>,
    /// Boolean value
    pub boolean: Option<bool>,
    /// Node reference (IRI)
    pub node: Option<String>,
}

impl From<ValueInput> for aingle_graph::Value {
    fn from(v: ValueInput) -> Self {
        if let Some(s) = v.string {
            aingle_graph::Value::String(s)
        } else if let Some(i) = v.integer {
            aingle_graph::Value::Integer(i)
        } else if let Some(f) = v.float {
            aingle_graph::Value::Float(f)
        } else if let Some(b) = v.boolean {
            aingle_graph::Value::Boolean(b)
        } else if let Some(n) = v.node {
            aingle_graph::Value::Node(aingle_graph::NodeId::named(&n))
        } else {
            aingle_graph::Value::Null
        }
    }
}

/// Pattern input for queries
#[derive(Debug, Clone, InputObject)]
pub struct PatternInput {
    /// Subject pattern (None = wildcard)
    pub subject: Option<String>,
    /// Predicate pattern (None = wildcard)
    pub predicate: Option<String>,
    /// Object pattern (None = wildcard)
    pub object: Option<ValueInput>,
}

/// Query result type
#[derive(Debug, Clone, SimpleObject)]
pub struct QueryResult {
    /// Matching triples
    pub matches: Vec<Triple>,
    /// Total count
    pub total: i32,
}

/// Graph statistics
#[derive(Debug, Clone, SimpleObject)]
pub struct GraphStats {
    /// Total triple count
    pub triple_count: i32,
    /// Unique subject count
    pub subject_count: i32,
    /// Unique predicate count
    pub predicate_count: i32,
    /// Unique object count
    pub object_count: i32,
}

/// Logic proof type
#[derive(Debug, Clone, SimpleObject)]
pub struct LogicProof {
    /// Proof hash
    pub hash: ID,
    /// Proof steps
    pub steps: Vec<ProofStep>,
    /// Whether proof is valid
    pub valid: bool,
    /// Verification timestamp
    pub verified_at: DateTime<Utc>,
}

/// Proof step
#[derive(Debug, Clone, SimpleObject)]
pub struct ProofStep {
    /// Step index
    pub index: i32,
    /// Rule applied
    pub rule: String,
    /// Premises used
    pub premises: Vec<String>,
    /// Conclusion derived
    pub conclusion: String,
}

/// Validation result (custom type to avoid conflict with async_graphql::ValidationResult)
#[derive(Debug, Clone, SimpleObject)]
pub struct TripleValidationResult {
    /// Overall validity
    pub valid: bool,
    /// Validation messages
    pub messages: Vec<ValidationMessage>,
    /// Generated proof hash
    pub proof_hash: Option<String>,
}

/// Validation message
#[derive(Debug, Clone, SimpleObject)]
pub struct ValidationMessage {
    /// Message level
    pub level: String,
    /// Message text
    pub message: String,
    /// Rule that generated this
    pub rule: Option<String>,
}

/// Validation event for subscriptions
#[derive(Debug, Clone, SimpleObject)]
pub struct ValidationEvent {
    /// Triple hash
    pub hash: String,
    /// Whether valid
    pub valid: bool,
    /// Proof hash (if generated)
    pub proof_hash: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// SPARQL result
#[derive(Debug, Clone, SimpleObject)]
pub struct SparqlResult {
    /// Result variables
    pub variables: Vec<String>,
    /// Result bindings
    pub bindings: Vec<SparqlBinding>,
    /// Execution time in ms
    pub execution_time_ms: i32,
}

/// SPARQL binding
#[derive(Debug, Clone, SimpleObject)]
pub struct SparqlBinding {
    /// Variable values as JSON
    pub values: String,
}
