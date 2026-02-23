//! Reputation REST endpoints.
//!
//! Provides agent consistency scoring and batch assertion verification
//! for the skill reputation system.

use crate::middleware::{is_in_namespace, RequestNamespace};
use crate::state::AppState;
use aingle_graph::{NodeId, Value};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

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
#[derive(Deserialize, Debug)]
pub struct BatchVerifyAssertionsRequest {
    /// Assertions to verify.
    pub assertions: Vec<AssertionRef>,
}

/// Reference to an assertion to verify.
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
    let graph = state.graph.read().await;
    let logic = state.logic.read().await;

    // Determine namespace prefix for agent node
    let ns_prefix = ns_ext
        .as_ref()
        .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone())
        .unwrap_or_else(|| "mayros".to_string());

    let mut total: usize = 0;
    let mut verified: usize = 0;

    // Find all triples where the object references this agent node.
    // Convention: `{ns}:assertedBy` or `{ns}:ownedBy` predicates point
    // to agent nodes like `{ns}:agent:{id}` or `agent:{id}`.
    let agent_node = Value::node(NodeId::named(format!("{}:agent:{}", ns_prefix, agent_id)));

    if let Ok(triples) = graph.get_object(&agent_node) {
        for triple in &triples {
            let pred_str = triple.predicate.as_str();
            if pred_str.ends_with(":assertedBy") || pred_str.ends_with(":ownedBy") {
                total += 1;

                // For each owned triple, find the actual assertion triples
                // under that subject and validate them with the logic engine.
                if let Ok(subject_triples) = graph.get_subject(&triple.subject) {
                    let any_valid = subject_triples.iter().any(|t| {
                        let result = logic.validate(t);
                        result.is_valid
                    });
                    if any_valid {
                        verified += 1;
                    }
                }
            }
        }
    }

    // Secondary pass: catch assertions stored under agent-prefixed subjects
    // (e.g. "{ns}:agent:{id}:assertion:xyz")
    let agent_prefix = format!("{}:agent:{}:", ns_prefix, agent_id);
    if let Ok(prefixed_subjects) = graph.subjects_with_prefix(&agent_prefix) {
        for subj in &prefixed_subjects {
            if let Ok(subj_triples) = graph.get_subject(subj) {
                for t in &subj_triples {
                    let pred_str = t.predicate.as_str();
                    // Skip ownership predicates already counted above
                    if pred_str.ends_with(":assertedBy") || pred_str.ends_with(":ownedBy") {
                        continue;
                    }
                    total += 1;
                    if logic.validate(t).is_valid {
                        verified += 1;
                    }
                }
            }
        }
    }

    let score = if total > 0 {
        verified as f64 / total as f64
    } else {
        0.0
    };

    Json(ConsistencyResponse {
        score,
        total,
        verified,
    })
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
    let graph = state.graph.read().await;
    let logic = state.logic.read().await;

    // Extract namespace for filtering
    let ns_filter = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);

    let mut results: Vec<AssertionVerifyResult> = Vec::new();

    for assertion in &req.assertions {
        // Skip assertions whose subject is outside the namespace
        if let Some(ref ns) = ns_filter {
            if !is_in_namespace(&assertion.subject, ns) {
                results.push(AssertionVerifyResult {
                    subject: assertion.subject.clone(),
                    predicate: assertion.predicate.clone(),
                    verified: false,
                });
                continue;
            }
        }

        let subj = NodeId::named(&assertion.subject);

        // Find all triples for this subject
        let triples = graph.get_subject(&subj).unwrap_or_default();

        // Find the triple matching the declared predicate
        let matching = triples
            .iter()
            .find(|t| t.predicate.as_str() == assertion.predicate);

        let verified = if let Some(triple) = matching {
            // Triple exists — validate it against the logic engine
            logic.validate(triple).is_valid
        } else {
            false
        };

        results.push(AssertionVerifyResult {
            subject: assertion.subject.clone(),
            predicate: assertion.predicate.clone(),
            verified,
        });
    }

    Json(BatchVerifyAssertionsResponse { results })
}

/// Create the reputation sub-router.
pub fn reputation_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/api/v1/agents/:id/consistency",
            axum::routing::get(get_agent_consistency),
        )
        .route(
            "/api/v1/assertions/verify-batch",
            axum::routing::post(batch_verify_assertions),
        )
}
