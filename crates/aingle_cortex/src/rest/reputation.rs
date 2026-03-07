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
    // Determine namespace prefix for agent node
    let ns_prefix = ns_ext
        .as_ref()
        .and_then(|axum::Extension(RequestNamespace(ns))| ns.clone())
        .unwrap_or_else(|| "mayros".to_string());

    // Phase 1: collect all triples we need from the graph, then drop the lock.
    let (owned_subject_triples, prefixed_triples) = {
        let graph = state.graph.read().await;

        let agent_node = Value::node(NodeId::named(format!("{}:agent:{}", ns_prefix, agent_id)));

        // Collect owned triples (assertedBy / ownedBy) and their subject triples.
        let mut owned = Vec::new();
        if let Ok(triples) = graph.get_object(&agent_node) {
            for triple in &triples {
                let pred_str = triple.predicate.as_str();
                if pred_str.ends_with(":assertedBy") || pred_str.ends_with(":ownedBy") {
                    let subject_triples = graph.get_subject(&triple.subject).unwrap_or_default();
                    owned.push(subject_triples);
                }
            }
        }

        // Collect agent-prefixed assertion triples.
        let agent_prefix = format!("{}:agent:{}:", ns_prefix, agent_id);
        let mut prefixed = Vec::new();
        if let Ok(prefixed_subjects) = graph.subjects_with_prefix(&agent_prefix) {
            for subj in &prefixed_subjects {
                if let Ok(subj_triples) = graph.get_subject(subj) {
                    let filtered: Vec<_> = subj_triples
                        .into_iter()
                        .filter(|t| {
                            let p = t.predicate.as_str();
                            !p.ends_with(":assertedBy") && !p.ends_with(":ownedBy")
                        })
                        .collect();
                    prefixed.push(filtered);
                }
            }
        }

        (owned, prefixed)
        // graph lock dropped here
    };

    // Phase 2: validate with the logic engine (separate lock).
    let logic = state.logic.read().await;

    let mut total: usize = 0;
    let mut verified: usize = 0;

    for subject_triples in &owned_subject_triples {
        total += 1;
        let any_valid = subject_triples.iter().any(|t| logic.validate(t).is_valid);
        if any_valid {
            verified += 1;
        }
    }

    for triples in &prefixed_triples {
        for t in triples {
            total += 1;
            if logic.validate(t).is_valid {
                verified += 1;
            }
        }
    }

    drop(logic);

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
    // Extract namespace for filtering
    let ns_filter = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);

    // Phase 1: collect matching triples from the graph, then drop the lock.
    let assertion_triples: Vec<_> = {
        let graph = state.graph.read().await;

        req.assertions
            .iter()
            .map(|assertion| {
                if let Some(ref ns) = ns_filter {
                    if !is_in_namespace(&assertion.subject, ns) {
                        return None;
                    }
                }
                let subj = NodeId::named(&assertion.subject);
                let triples = graph.get_subject(&subj).unwrap_or_default();
                triples
                    .into_iter()
                    .find(|t| t.predicate.as_str() == assertion.predicate)
            })
            .collect()
        // graph lock dropped here
    };

    // Phase 2: validate with the logic engine (separate lock).
    let logic = state.logic.read().await;

    let results: Vec<AssertionVerifyResult> = req
        .assertions
        .iter()
        .zip(assertion_triples.iter())
        .map(|(assertion, maybe_triple)| {
            let verified = maybe_triple
                .as_ref()
                .map(|t| logic.validate(t).is_valid)
                .unwrap_or(false);
            AssertionVerifyResult {
                subject: assertion.subject.clone(),
                predicate: assertion.predicate.clone(),
                verified,
            }
        })
        .collect();

    drop(logic);

    Json(BatchVerifyAssertionsResponse { results })
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
