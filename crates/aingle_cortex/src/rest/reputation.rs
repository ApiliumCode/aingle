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
///
/// The score is `verified / total` over PoL verdicts, so it inherits everything
/// those verdicts are and are not — see [`crate::rest::pol_evidence`]. It is
/// arithmetic over this node's assertions, not a reputation measurement, and it
/// is meaningless when `rule_set.vacuous` is true (every assertion passes, so
/// the score is 1.0 for anyone with assertions and 0.0 for anyone without).
#[derive(Serialize, Debug)]
pub struct ConsistencyResponse {
    /// Consistency score between 0.0 and 1.0: `verified / total`, or 0.0 when
    /// `total` is 0.
    ///
    /// **Derived from assertions by this server, so it is an assertion too.**
    /// 0.0 from an empty `assertions` list means "nothing found", which is not
    /// the same as "0% consistent" — check `total` before reporting a score.
    pub score: f64,
    /// Total number of assertion units scored — the denominator.
    pub total: usize,
    /// Number that passed PoL validation — the numerator.
    pub verified: usize,
    /// Every unit that went into the fraction, so the arithmetic is checkable
    /// and each verdict can be re-requested individually rather than taken as a
    /// summarized number.
    pub assertions: Vec<AgentAssertionOutcome>,
    /// The rule set every verdict above was evaluated against.
    pub rule_set: crate::rest::pol_evidence::RuleSetFingerprint,
    /// Steps a caller should run and report on instead of relaying the score.
    pub procedure: Vec<String>,
}

/// One unit that contributed to an agent's consistency score.
#[derive(Serialize, Debug)]
pub struct AgentAssertionOutcome {
    /// What the unit is: `subject` (an owned subject, verified when ANY of its
    /// triples validates) or `triple` (a single agent-prefixed assertion).
    /// The two are counted alike in the score even though they are not
    /// equivalent evidence; that is stated rather than hidden.
    pub unit: String,
    /// Subject of the assertion.
    pub subject: String,
    /// Predicate, when the unit is a single triple.
    pub predicate: Option<String>,
    /// Whether this unit counted towards `verified`.
    pub verified: bool,
    /// Identity of the triple evaluated, when the unit is a single triple, so a
    /// client can confirm which triple the verdict is about.
    pub triple: Option<crate::rest::pol_evidence::TripleIdentity>,
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
    /// Whether the assertion passed PoL validation on this node.
    ///
    /// **An assertion by this server, not proof** — and a lossy one: `false`
    /// covers both "no such triple exists here" and "a rule rejected it", which
    /// are completely different claims. `evidence.outcome` separates them.
    pub verified: bool,
    /// What the verdict was actually computed from.
    pub evidence: AssertionEvidence,
}

/// The material behind one assertion verdict.
#[derive(Serialize, Debug)]
pub struct AssertionEvidence {
    /// Whether a matching triple was found in the graph at all.
    pub found: bool,
    /// Precise outcome, never to be collapsed into the boolean:
    ///
    /// - `"accepted"` — a triple was found and no enabled rule rejected it.
    /// - `"rejected"` — a triple was found and an enabled rule rejected it; see
    ///   `rejected_by`.
    /// - `"not_found"` — no such triple here. Nothing was evaluated. This is not
    ///   evidence that the assertion is false, only that this node does not hold
    ///   it (it may also be filtered out of scope for this caller).
    pub outcome: String,
    /// Identity of the evaluated triple, so a client can confirm the verdict is
    /// about the triple it meant. `None` when nothing was found.
    pub triple: Option<crate::rest::pol_evidence::TripleIdentity>,
    /// Ids of the enabled rules that matched and accepted.
    pub matched_rule_ids: Vec<String>,
    /// Ids of the enabled rules that rejected, with their reasons.
    pub rejected_by: Vec<String>,
}

/// Response from batch assertion verification.
#[derive(Serialize, Debug)]
pub struct BatchVerifyAssertionsResponse {
    /// Results for each assertion.
    pub results: Vec<AssertionVerifyResult>,
    /// The rule set every verdict above was evaluated against — including
    /// whether it was empty, in which case nothing was examined.
    pub rule_set: crate::rest::pol_evidence::RuleSetFingerprint,
    /// Steps a caller should run and report on instead of relaying `verified`.
    pub procedure: Vec<String>,
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
