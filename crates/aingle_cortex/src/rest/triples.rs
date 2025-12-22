//! Triple CRUD operations

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::state::{AppState, Event};
use aingle_graph::{NodeId, Predicate, Triple, TripleId, TriplePattern, Value};

/// Triple data transfer object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleDto {
    /// Triple hash (read-only)
    #[serde(skip_deserializing)]
    pub id: Option<String>,
    /// Subject
    pub subject: String,
    /// Predicate
    pub predicate: String,
    /// Object value
    pub object: ValueDto,
    /// Timestamp (read-only)
    #[serde(skip_deserializing)]
    pub created_at: Option<String>,
}

/// Value data transfer object
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueDto {
    /// String value
    String(String),
    /// Integer value
    Integer(i64),
    /// Float value
    Float(f64),
    /// Boolean value
    Boolean(bool),
    /// Node reference (IRI)
    Node { node: String },
}

impl From<Value> for ValueDto {
    fn from(v: Value) -> Self {
        match v {
            Value::String(s) => ValueDto::String(s),
            Value::Integer(i) => ValueDto::Integer(i),
            Value::Float(f) => ValueDto::Float(f),
            Value::Boolean(b) => ValueDto::Boolean(b),
            Value::Node(n) => ValueDto::Node {
                node: n.to_string(),
            },
            Value::DateTime(dt) => ValueDto::String(dt),
            Value::Typed { value, .. } => ValueDto::String(value),
            Value::LangString { value, .. } => ValueDto::String(value),
            Value::Bytes(_) => ValueDto::String("[binary]".to_string()),
            Value::Json(v) => ValueDto::String(v.to_string()),
            Value::Null => ValueDto::String("null".to_string()),
        }
    }
}

impl From<ValueDto> for Value {
    fn from(v: ValueDto) -> Self {
        match v {
            ValueDto::String(s) => Value::String(s),
            ValueDto::Integer(i) => Value::Integer(i),
            ValueDto::Float(f) => Value::Float(f),
            ValueDto::Boolean(b) => Value::Boolean(b),
            ValueDto::Node { node } => Value::Node(NodeId::named(&node)),
        }
    }
}

impl From<Triple> for TripleDto {
    fn from(t: Triple) -> Self {
        Self {
            id: Some(t.id().to_hex()),
            subject: t.subject.to_string(),
            predicate: t.predicate.to_string(),
            object: t.object.into(),
            created_at: Some(t.meta.created_at.to_rfc3339()),
        }
    }
}

/// Request to create a triple
#[derive(Debug, Deserialize)]
pub struct CreateTripleRequest {
    pub subject: String,
    pub predicate: String,
    pub object: ValueDto,
}

/// Query parameters for listing triples
#[derive(Debug, Deserialize)]
pub struct ListTriplesQuery {
    /// Filter by subject
    pub subject: Option<String>,
    /// Filter by predicate
    pub predicate: Option<String>,
    /// Filter by object (exact match)
    pub object: Option<String>,
    /// Limit results
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    100
}

/// Create a new triple
///
/// POST /api/v1/triples
pub async fn create_triple(
    State(state): State<AppState>,
    Json(req): Json<CreateTripleRequest>,
) -> Result<(StatusCode, Json<TripleDto>)> {
    // Validate input
    if req.subject.is_empty() {
        return Err(Error::InvalidInput("Subject cannot be empty".to_string()));
    }
    if req.predicate.is_empty() {
        return Err(Error::InvalidInput("Predicate cannot be empty".to_string()));
    }

    let object: Value = req.object.clone().into();

    // Create the triple
    let triple = Triple::new(
        NodeId::named(&req.subject),
        Predicate::named(&req.predicate),
        object,
    );

    // Add triple to graph
    let triple_id = {
        let graph = state.graph.read().await;
        graph.insert(triple.clone())?
    };

    // Broadcast event
    state.broadcaster.broadcast(Event::TripleAdded {
        hash: triple_id.to_hex(),
        subject: req.subject,
        predicate: req.predicate,
        object: serde_json::to_value(&req.object).unwrap_or_default(),
    });

    Ok((StatusCode::CREATED, Json(triple.into())))
}

/// Get a triple by hash
///
/// GET /api/v1/triples/:id
pub async fn get_triple(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TripleDto>> {
    let triple_id = TripleId::from_hex(&id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid triple ID: {}", id)))?;

    let graph = state.graph.read().await;
    let triple = graph
        .get(&triple_id)?
        .ok_or_else(|| Error::NotFound(format!("Triple {} not found", id)))?;

    Ok(Json(triple.into()))
}

/// Delete a triple
///
/// DELETE /api/v1/triples/:id
pub async fn delete_triple(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let triple_id = TripleId::from_hex(&id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid triple ID: {}", id)))?;

    let deleted = {
        let graph = state.graph.read().await;
        graph.delete(&triple_id)?
    };

    if deleted {
        state
            .broadcaster
            .broadcast(Event::TripleDeleted { hash: id });
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::NotFound(format!("Triple {} not found", id)))
    }
}

/// List triples with filters
///
/// GET /api/v1/triples
pub async fn list_triples(
    State(state): State<AppState>,
    Query(query): Query<ListTriplesQuery>,
) -> Result<Json<ListTriplesResponse>> {
    let graph = state.graph.read().await;

    // Build pattern based on provided filters
    let mut pattern = TriplePattern::any();

    if let Some(ref subject) = query.subject {
        pattern = pattern.with_subject(NodeId::named(subject));
    }
    if let Some(ref predicate) = query.predicate {
        pattern = pattern.with_predicate(Predicate::named(predicate));
    }

    let triples = graph.find(pattern)?;

    // Apply pagination
    let total = triples.len();
    let triples: Vec<TripleDto> = triples
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|t| t.into())
        .collect();

    Ok(Json(ListTriplesResponse {
        triples,
        total,
        limit: query.limit,
        offset: query.offset,
    }))
}

/// Response for listing triples
#[derive(Debug, Serialize)]
pub struct ListTriplesResponse {
    pub triples: Vec<TripleDto>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_dto_conversion() {
        let v = ValueDto::String("hello".to_string());
        let value: Value = v.into();
        assert!(matches!(value, Value::String(s) if s == "hello"));

        let v = ValueDto::Integer(42);
        let value: Value = v.into();
        assert!(matches!(value, Value::Integer(42)));
    }
}
