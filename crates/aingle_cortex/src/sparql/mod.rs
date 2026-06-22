// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! SPARQL query engine for Córtex
//!
//! Provides SPARQL 1.1 query support for the AIngle graph.

mod executor;
mod parser;

pub use executor::*;
pub use parser::*;

use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::state::AppState;

/// Create SPARQL router
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sparql", post(execute_sparql))
        .route("/api/v1/sparql", post(execute_sparql))
}

/// SPARQL query request
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct SparqlRequest {
    /// SPARQL query string
    pub query: String,
    /// Default graph URI (optional)
    pub default_graph: Option<String>,
    /// Named graph URIs (optional)
    pub named_graphs: Option<Vec<String>>,
}

/// SPARQL query response
#[derive(Debug, Serialize)]
pub struct SparqlResponse {
    /// Result type: "bindings", "boolean", "graph"
    pub result_type: String,
    /// Variable names (for SELECT queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<String>>,
    /// Result bindings (for SELECT queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bindings: Option<Vec<serde_json::Value>>,
    /// Boolean result (for ASK queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boolean: Option<bool>,
    /// Triple count (for CONSTRUCT queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triple_count: Option<usize>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Execute SPARQL query
///
/// POST /sparql
pub async fn execute_sparql(
    State(state): State<AppState>,
    Json(req): Json<SparqlRequest>,
) -> Result<Json<SparqlResponse>> {
    let resp = crate::service::sparql::execute(&state, req).await?;
    Ok(Json(resp))
}

/// SPARQL result
#[derive(Debug)]
pub struct SparqlResult {
    pub result_type: String,
    pub variables: Option<Vec<String>>,
    pub bindings: Option<Vec<serde_json::Value>>,
    pub boolean: Option<bool>,
    pub triple_count: Option<usize>,
}
