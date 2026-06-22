// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Reputation REST endpoints.
//!
//! Provides agent consistency scoring and batch assertion verification
//! for the skill reputation system.

use crate::middleware::RequestNamespace;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// Request identifying an agent whose consistency score to compute.
///
/// Used as the MCP input for the agent-consistency tool. (REST extracts the
/// agent id from the path, so this struct is MCP-only.)
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Deserialize, Debug)]
pub struct AgentConsistencyRequest {
    /// The agent id whose assertion consistency to score.
    pub agent_id: String,
}

/// Agent consistency score response.
#[derive(Serialize, Debug)]
pub struct ConsistencyResponse {
    /// Consistency score between 0.0 and 1.0.
    pub score: f64,
    /// Total number of assertions by this agent.
    pub total: usize,
    /// Number of verified assertions.
    pub verified: usize,
}

/// Request to batch-verify assertions.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Deserialize, Debug)]
pub struct BatchVerifyAssertionsRequest {
    /// Assertions to verify.
    pub assertions: Vec<AssertionRef>,
}

/// Reference to an assertion to verify.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Deserialize, Debug)]
pub struct AssertionRef {
    /// Subject of the assertion.
    pub subject: String,
    /// Predicate of the assertion.
    pub predicate: String,
}

/// Result of verifying a single assertion.
#[derive(Serialize, Debug)]
pub struct AssertionVerifyResult {
    /// Subject of the assertion.
    pub subject: String,
    /// Predicate of the assertion.
    pub predicate: String,
    /// Whether the assertion is verified.
    pub verified: bool,
}

/// Response from batch assertion verification.
#[derive(Serialize, Debug)]
pub struct BatchVerifyAssertionsResponse {
    /// Results for each assertion.
    pub results: Vec<AssertionVerifyResult>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/agents/:id/consistency — Get agent assertion consistency score.
///
/// Queries all assertions owned by the agent and checks how many
/// have been verified through PoL validation.
pub async fn get_agent_consistency(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    // Determine namespace prefix for agent node.
    let namespace = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);

    // Delegate the shared scoring logic (graph + logic engine read-only).
    let resp = crate::service::reputation::agent_consistency(&state, &agent_id, namespace).await;
    Json(resp)
}

/// POST /api/v1/assertions/verify-batch — Batch verify assertion proofs.
///
/// For each assertion (subject + predicate), checks if the triple exists
/// and if it passes PoL validation.
pub async fn batch_verify_assertions(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Json(req): Json<BatchVerifyAssertionsRequest>,
) -> impl IntoResponse {
    // Extract namespace for filtering.
    let namespace = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);

    // Delegate the shared verification logic (graph + logic engine read-only).
    let resp = crate::service::reputation::batch_verify_assertions(&state, req, namespace).await;
    Json(resp)
}

/// Create the reputation sub-router.
pub fn reputation_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/api/v1/agents/{id}/consistency",
            axum::routing::get(get_agent_consistency),
        )
        .route(
            "/api/v1/assertions/verify-batch",
            axum::routing::post(batch_verify_assertions),
        )
}
