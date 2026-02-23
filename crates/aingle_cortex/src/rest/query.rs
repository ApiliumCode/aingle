//! Query endpoints for pattern matching

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::middleware::{is_in_namespace, RequestNamespace};
use crate::rest::triples::{TripleDto, ValueDto};
use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, Triple, TriplePattern, Value};

/// Pattern query request
#[derive(Debug, Deserialize)]
pub struct PatternQueryRequest {
    /// Subject pattern (None = wildcard)
    pub subject: Option<String>,
    /// Predicate pattern (None = wildcard)
    pub predicate: Option<String>,
    /// Object pattern (None = wildcard)
    pub object: Option<ValueDto>,
    /// Maximum results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// Pattern query response
#[derive(Debug, Serialize)]
pub struct PatternQueryResponse {
    /// Matching triples
    pub matches: Vec<TripleDto>,
    /// Total matches found
    pub total: usize,
    /// Query pattern used
    pub pattern: PatternDescription,
}

/// Description of the query pattern
#[derive(Debug, Serialize)]
pub struct PatternDescription {
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub object: Option<serde_json::Value>,
}

/// Execute pattern matching query
///
/// POST /api/v1/query
pub async fn query_pattern(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Json(req): Json<PatternQueryRequest>,
) -> Result<Json<PatternQueryResponse>> {
    let graph = state.graph.read().await;

    // Build pattern from request
    let mut pattern = TriplePattern::any();

    if let Some(ref subject) = req.subject {
        pattern = pattern.with_subject(NodeId::named(subject));
    }
    if let Some(ref predicate) = req.predicate {
        pattern = pattern.with_predicate(Predicate::named(predicate));
    }
    if let Some(ref object) = req.object {
        let obj: Value = object.clone().into();
        pattern = pattern.with_object(obj);
    }

    let triples = graph.find(pattern)?;

    // Filter by namespace if present
    let ns_filter = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    let triples: Vec<Triple> = if let Some(ref ns) = ns_filter {
        triples.into_iter().filter(|t| is_in_namespace(&t.subject.to_string(), ns)).collect()
    } else {
        triples
    };

    let total = triples.len();
    let matches: Vec<TripleDto> = triples
        .into_iter()
        .take(req.limit)
        .map(|t| t.into())
        .collect();

    let pattern_desc = PatternDescription {
        subject: req.subject,
        predicate: req.predicate,
        object: req
            .object
            .map(|o| serde_json::to_value(o).unwrap_or_default()),
    };

    Ok(Json(PatternQueryResponse {
        matches,
        total,
        pattern: pattern_desc,
    }))
}

/// Query parameters for listing subjects
#[derive(Debug, Deserialize)]
pub struct ListSubjectsQuery {
    /// Filter by predicate
    pub predicate: Option<String>,
    /// Limit results
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// List all unique subjects
///
/// GET /api/v1/query/subjects
pub async fn list_subjects(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Query(query): Query<ListSubjectsQuery>,
) -> Result<Json<ListSubjectsResponse>> {
    let graph = state.graph.read().await;

    let pattern = if let Some(ref predicate) = query.predicate {
        TriplePattern::predicate(Predicate::named(predicate))
    } else {
        TriplePattern::any()
    };

    let triples = graph.find(pattern)?;
    let ns_filter = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    let mut subjects: Vec<String> = triples
        .into_iter()
        .map(|t| t.subject.to_string())
        .filter(|s| ns_filter.as_ref().map_or(true, |ns| is_in_namespace(s, ns)))
        .collect();
    subjects.sort();
    subjects.dedup();

    let total = subjects.len();
    let subjects: Vec<String> = subjects.into_iter().take(query.limit).collect();

    Ok(Json(ListSubjectsResponse { subjects, total }))
}

/// Response for listing subjects
#[derive(Debug, Serialize)]
pub struct ListSubjectsResponse {
    pub subjects: Vec<String>,
    pub total: usize,
}

/// Query parameters for listing predicates
#[derive(Debug, Deserialize)]
pub struct ListPredicatesQuery {
    /// Filter by subject
    pub subject: Option<String>,
    /// Limit results
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// List all unique predicates
///
/// GET /api/v1/query/predicates
pub async fn list_predicates(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Query(query): Query<ListPredicatesQuery>,
) -> Result<Json<ListPredicatesResponse>> {
    let graph = state.graph.read().await;

    let pattern = if let Some(ref subject) = query.subject {
        TriplePattern::subject(NodeId::named(subject))
    } else {
        TriplePattern::any()
    };

    let triples = graph.find(pattern)?;
    let ns_filter = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    let mut predicates: Vec<String> = triples
        .into_iter()
        .filter(|t| ns_filter.as_ref().map_or(true, |ns| is_in_namespace(&t.subject.to_string(), ns)))
        .map(|t| t.predicate.to_string())
        .collect();
    predicates.sort();
    predicates.dedup();

    let total = predicates.len();
    let predicates: Vec<String> = predicates.into_iter().take(query.limit).collect();

    Ok(Json(ListPredicatesResponse { predicates, total }))
}

/// Response for listing predicates
#[derive(Debug, Serialize)]
pub struct ListPredicatesResponse {
    pub predicates: Vec<String>,
    pub total: usize,
}
