// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Query endpoints for pattern matching

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::middleware::RequestNamespace;
use crate::rest::triples::{TripleDto, ValueDto};
use crate::state::AppState;

/// Pattern query request
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
    let namespace = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    Ok(Json(
        crate::service::query::query_pattern(&state, req, namespace).await?,
    ))
}

/// Query parameters for listing subjects
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
    let namespace = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    Ok(Json(
        crate::service::query::list_subjects(&state, query, namespace).await?,
    ))
}

/// Response for listing subjects
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Debug, Serialize)]
pub struct ListSubjectsResponse {
    pub subjects: Vec<String>,
    pub total: usize,
}

/// Query parameters for listing predicates
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
    let namespace = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    Ok(Json(
        crate::service::query::list_predicates(&state, query, namespace).await?,
    ))
}

/// Response for listing predicates
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Debug, Serialize)]
pub struct ListPredicatesResponse {
    pub predicates: Vec<String>,
    pub total: usize,
}
