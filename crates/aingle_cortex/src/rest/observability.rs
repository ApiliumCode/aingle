//! Observability REST endpoints for semantic tracing.
//!
//! These endpoints provide a thin layer over the triple API, scoped to
//! the `{ns}:event:*` namespace for recording and querying trace events.

use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, Triple, Value};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

/// Request to batch-store trace events.
#[derive(Deserialize, Debug)]
pub struct BatchStoreEventsRequest {
    /// The namespace prefix (e.g., "mayros").
    pub namespace: String,
    /// The events to store.
    pub events: Vec<TraceEventInput>,
}

/// A single trace event to store.
#[derive(Deserialize, Debug)]
pub struct TraceEventInput {
    /// Unique event ID.
    pub id: String,
    /// Event type (e.g., "tool_call", "llm_call", "decision", "delegation", "error").
    pub event_type: String,
    /// Agent ID that produced this event.
    pub agent_id: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Session key.
    #[serde(default)]
    pub session: Option<String>,
    /// Parent event ID for causal chaining.
    #[serde(default)]
    pub parent_event: Option<String>,
    /// Duration in milliseconds.
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Additional key-value fields stored as triples.
    #[serde(default)]
    pub fields: std::collections::HashMap<String, String>,
}

/// Response from batch event storage.
#[derive(Serialize, Debug)]
pub struct BatchStoreEventsResponse {
    /// Number of events stored.
    pub stored: usize,
    /// Number of triples created.
    pub triples_created: usize,
}

/// Query parameters for listing events.
#[derive(Deserialize, Debug, Default)]
pub struct EventsQuery {
    /// Filter by agent ID.
    pub agent: Option<String>,
    /// Filter by event type.
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    /// Filter events after this timestamp (ISO 8601).
    pub from: Option<String>,
    /// Filter events before this timestamp (ISO 8601).
    pub to: Option<String>,
    /// Maximum number of events to return.
    pub limit: Option<usize>,
    /// Namespace prefix.
    pub namespace: Option<String>,
}

/// A trace event in the response.
#[derive(Serialize, Debug)]
pub struct TraceEventOutput {
    pub id: String,
    pub event_type: String,
    pub agent_id: String,
    pub timestamp: String,
    pub session: Option<String>,
    pub parent_event: Option<String>,
    pub duration_ms: Option<u64>,
    pub fields: std::collections::HashMap<String, String>,
}

/// Response from querying events.
#[derive(Serialize, Debug)]
pub struct QueryEventsResponse {
    pub events: Vec<TraceEventOutput>,
    pub total: usize,
}

/// A node in the causal chain.
#[derive(Serialize, Debug)]
pub struct CausalNode {
    pub event_id: String,
    pub event_type: String,
    pub agent_id: String,
    pub timestamp: String,
    pub summary: String,
}

/// Response from getting a causal chain.
#[derive(Serialize, Debug)]
pub struct CausalChainResponse {
    pub chain: Vec<CausalNode>,
}

/// Helper: extract a string value from a triple's object.
fn value_as_string(v: &Value) -> Option<&str> {
    v.as_string()
}

/// POST /api/v1/events — Batch store trace events as RDF triples.
pub async fn batch_store_events(
    State(state): State<AppState>,
    Json(req): Json<BatchStoreEventsRequest>,
) -> impl IntoResponse {
    let ns = &req.namespace;
    let graph = state.graph.write().await;
    let mut triples_created: usize = 0;

    for event in &req.events {
        let subj = NodeId::named(format!("{}:event:{}", ns, event.id));

        // Core event triples
        let core = vec![
            Triple::new(
                subj.clone(),
                Predicate::named(format!("{}:event:type", ns)),
                Value::literal(&event.event_type),
            ),
            Triple::new(
                subj.clone(),
                Predicate::named(format!("{}:event:agent", ns)),
                Value::node(NodeId::named(format!("{}:agent:{}", ns, event.agent_id))),
            ),
            Triple::new(
                subj.clone(),
                Predicate::named(format!("{}:event:timestamp", ns)),
                Value::literal(&event.timestamp),
            ),
        ];

        for t in core {
            let _ = graph.insert(t);
            triples_created += 1;
        }

        if let Some(ref session) = event.session {
            let _ = graph.insert(Triple::new(
                subj.clone(),
                Predicate::named(format!("{}:event:session", ns)),
                Value::node(NodeId::named(format!("{}:session:{}", ns, session))),
            ));
            triples_created += 1;
        }

        if let Some(ref parent) = event.parent_event {
            let _ = graph.insert(Triple::new(
                subj.clone(),
                Predicate::named(format!("{}:event:parent_event", ns)),
                Value::node(NodeId::named(format!("{}:event:{}", ns, parent))),
            ));
            triples_created += 1;
        }

        if let Some(duration) = event.duration_ms {
            let _ = graph.insert(Triple::new(
                subj.clone(),
                Predicate::named(format!("{}:event:duration_ms", ns)),
                Value::literal(duration.to_string()),
            ));
            triples_created += 1;
        }

        // Store additional fields
        for (key, value) in &event.fields {
            let _ = graph.insert(Triple::new(
                subj.clone(),
                Predicate::named(format!("{}:event:{}", ns, key)),
                Value::literal(value),
            ));
            triples_created += 1;
        }
    }

    (
        StatusCode::CREATED,
        Json(BatchStoreEventsResponse {
            stored: req.events.len(),
            triples_created,
        }),
    )
}

/// GET /api/v1/events — Query trace events.
pub async fn query_events(
    State(state): State<AppState>,
    Query(params): Query<EventsQuery>,
) -> impl IntoResponse {
    let ns = params.namespace.as_deref().unwrap_or("mayros");
    let graph = state.graph.read().await;
    let limit = params.limit.unwrap_or(100);

    // Query all event:type triples to find event subjects
    let type_pred = Predicate::named(format!("{}:event:type", ns));
    let type_triples = graph
        .get_predicate(&type_pred)
        .unwrap_or_default();

    let mut events: Vec<TraceEventOutput> = Vec::new();

    for triple in &type_triples {
        let subj = &triple.subject;
        // Extract event ID from subject name
        let subj_name = subj.as_name().unwrap_or("");
        let prefix = format!("{}:event:", ns);
        let event_id = subj_name.strip_prefix(&prefix).unwrap_or(subj_name);

        let event_type = value_as_string(&triple.object)
            .unwrap_or("unknown")
            .to_string();

        // Apply type filter
        if let Some(ref filter_type) = params.event_type {
            if &event_type != filter_type {
                continue;
            }
        }

        // Fetch all triples for this event subject
        let all_triples = graph.get_subject(subj).unwrap_or_default();

        // Extract agent
        let agent_pred_str = format!("{}:event:agent", ns);
        let agent_id = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == agent_pred_str)
            .and_then(|t| t.object.as_node())
            .and_then(|n| n.as_name())
            .map(|s| {
                let agent_prefix = format!("{}:agent:", ns);
                s.strip_prefix(&agent_prefix).unwrap_or(s).to_string()
            })
            .unwrap_or_default();

        // Apply agent filter
        if let Some(ref filter_agent) = params.agent {
            if &agent_id != filter_agent {
                continue;
            }
        }

        // Extract timestamp
        let ts_pred_str = format!("{}:event:timestamp", ns);
        let timestamp = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == ts_pred_str)
            .and_then(|t| value_as_string(&t.object))
            .unwrap_or("")
            .to_string();

        // Apply time range filters
        if let Some(ref from) = params.from {
            if timestamp < *from {
                continue;
            }
        }
        if let Some(ref to) = params.to {
            if timestamp > *to {
                continue;
            }
        }

        // Extract session
        let session_pred_str = format!("{}:event:session", ns);
        let session = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == session_pred_str)
            .and_then(|t| t.object.as_node())
            .and_then(|n| n.as_name())
            .map(|s| {
                let session_prefix = format!("{}:session:", ns);
                s.strip_prefix(&session_prefix).unwrap_or(s).to_string()
            });

        // Extract parent event
        let parent_pred_str = format!("{}:event:parent_event", ns);
        let parent_event = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == parent_pred_str)
            .and_then(|t| t.object.as_node())
            .and_then(|n| n.as_name())
            .map(|s| {
                let event_prefix = format!("{}:event:", ns);
                s.strip_prefix(&event_prefix).unwrap_or(s).to_string()
            });

        // Extract duration
        let dur_pred_str = format!("{}:event:duration_ms", ns);
        let duration_ms = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == dur_pred_str)
            .and_then(|t| value_as_string(&t.object))
            .and_then(|s| s.parse::<u64>().ok());

        // Collect additional fields (predicates starting with {ns}:event: that aren't core)
        let core_preds: std::collections::HashSet<&str> = [
            agent_pred_str.as_str(),
            ts_pred_str.as_str(),
            session_pred_str.as_str(),
            parent_pred_str.as_str(),
            dur_pred_str.as_str(),
            type_pred.as_str(),
        ]
        .into_iter()
        .collect();

        let event_pred_prefix = format!("{}:event:", ns);
        let mut fields = std::collections::HashMap::new();
        for t in &all_triples {
            let pred_str = t.predicate.as_str();
            if !core_preds.contains(pred_str) {
                if let Some(key) = pred_str.strip_prefix(&event_pred_prefix) {
                    if let Some(val) = value_as_string(&t.object) {
                        fields.insert(key.to_string(), val.to_string());
                    }
                }
            }
        }

        events.push(TraceEventOutput {
            id: event_id.to_string(),
            event_type,
            agent_id,
            timestamp,
            session,
            parent_event,
            duration_ms,
            fields,
        });

        if events.len() >= limit {
            break;
        }
    }

    let total = events.len();
    Json(QueryEventsResponse { events, total })
}

/// GET /api/v1/events/:id/chain — Get causal chain for an event.
pub async fn get_causal_chain(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    Query(params): Query<EventsQuery>,
) -> impl IntoResponse {
    let ns = params.namespace.as_deref().unwrap_or("mayros");
    let graph = state.graph.read().await;
    let mut chain: Vec<CausalNode> = Vec::new();
    let mut current_id = event_id;

    let agent_prefix = format!("{}:agent:", ns);
    let event_prefix = format!("{}:event:", ns);

    // Walk up the parent chain (max 50 to prevent infinite loops)
    for _ in 0..50 {
        let subj = NodeId::named(format!("{}:event:{}", ns, current_id));
        let all_triples = graph.get_subject(&subj).unwrap_or_default();

        if all_triples.is_empty() {
            break; // Event not found
        }

        // Extract event type
        let type_pred_str = format!("{}:event:type", ns);
        let event_type = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == type_pred_str)
            .and_then(|t| value_as_string(&t.object))
            .unwrap_or("unknown")
            .to_string();

        // Extract agent
        let agent_pred_str = format!("{}:event:agent", ns);
        let agent_id = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == agent_pred_str)
            .and_then(|t| t.object.as_node())
            .and_then(|n| n.as_name())
            .map(|s| s.strip_prefix(&agent_prefix).unwrap_or(s).to_string())
            .unwrap_or_default();

        // Extract timestamp
        let ts_pred_str = format!("{}:event:timestamp", ns);
        let timestamp = all_triples
            .iter()
            .find(|t| t.predicate.as_str() == ts_pred_str)
            .and_then(|t| value_as_string(&t.object))
            .unwrap_or("")
            .to_string();

        chain.push(CausalNode {
            event_id: current_id.clone(),
            event_type: event_type.clone(),
            agent_id,
            timestamp,
            summary: format!("{} event", event_type),
        });

        // Look for parent event
        let parent_pred_str = format!("{}:event:parent_event", ns);
        match all_triples
            .iter()
            .find(|t| t.predicate.as_str() == parent_pred_str)
            .and_then(|t| t.object.as_node())
            .and_then(|n| n.as_name())
            .map(|s| s.strip_prefix(&event_prefix).unwrap_or(s).to_string())
        {
            Some(parent_id) => current_id = parent_id,
            None => break,
        }
    }

    // Reverse so oldest is first
    chain.reverse();
    Json(CausalChainResponse { chain })
}

/// Create the observability sub-router.
pub fn observability_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/v1/events", axum::routing::post(batch_store_events))
        .route("/api/v1/events", axum::routing::get(query_events))
        .route(
            "/api/v1/events/:id/chain",
            axum::routing::get(get_causal_chain),
        )
}
