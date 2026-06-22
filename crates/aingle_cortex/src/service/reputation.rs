// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Reputation business logic shared by REST and MCP.
//!
//! Agent assertion consistency scoring and batch assertion verification. Both
//! operations are read-only: they inspect the graph + logic engine and never
//! mutate state. Like the REST handlers, neither returns a hard error for empty
//! or unknown input — an unknown agent yields a well-formed default response
//! (score 0.0), and a batch of non-existent assertions yields `verified:false`
//! per entry.

use crate::middleware::is_in_namespace;
use crate::rest::{
    AssertionVerifyResult, BatchVerifyAssertionsRequest, BatchVerifyAssertionsResponse,
    ConsistencyResponse,
};
use crate::state::AppState;
use aingle_graph::{NodeId, Value};

/// Compute an agent's assertion consistency score.
///
/// Semantics preserved from the REST `GET /api/v1/agents/:id/consistency`
/// handler: collects every assertion owned by (or prefixed with) the agent node
/// and reports the fraction that pass PoL validation. `namespace` selects the
/// agent-node namespace prefix; REST passes the request namespace, MCP passes
/// `None` (defaulting to the `mayros` namespace, matching the handler default).
pub async fn agent_consistency(
    state: &AppState,
    agent_id: &str,
    namespace: Option<String>,
) -> ConsistencyResponse {
    let ns_prefix = namespace.unwrap_or_else(|| "mayros".to_string());

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

    ConsistencyResponse {
        score,
        total,
        verified,
    }
}

/// Batch-verify assertion proofs.
///
/// Semantics preserved from the REST `POST /api/v1/assertions/verify-batch`
/// handler: for each `(subject, predicate)` reference, locates the matching
/// triple and reports whether it passes PoL validation. Missing triples (and,
/// when `namespace` is `Some`, out-of-namespace subjects) report
/// `verified:false` rather than erroring. `namespace` is the request namespace
/// for REST and `None` for the MCP path.
pub async fn batch_verify_assertions(
    state: &AppState,
    req: BatchVerifyAssertionsRequest,
    namespace: Option<String>,
) -> BatchVerifyAssertionsResponse {
    let ns_filter = namespace;

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

    BatchVerifyAssertionsResponse { results }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn consistency_of_unknown_agent_is_zero() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let resp = agent_consistency(&state, "nobody", None).await;
        assert_eq!(resp.total, 0);
        assert_eq!(resp.verified, 0);
        assert_eq!(resp.score, 0.0);
    }

    #[tokio::test]
    async fn batch_verify_empty_returns_empty_results() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let req = BatchVerifyAssertionsRequest { assertions: vec![] };
        let resp = batch_verify_assertions(&state, req, None).await;
        assert!(resp.results.is_empty());
    }

    #[tokio::test]
    async fn batch_verify_unknown_assertion_is_unverified() {
        use crate::rest::AssertionRef;

        let state = AppState::with_db_path(":memory:", None).unwrap();

        // A reference to a triple that does not exist must come back as a
        // well-formed result with verified:false (not a hard error).
        let req = BatchVerifyAssertionsRequest {
            assertions: vec![AssertionRef {
                subject: "ex:thing".to_string(),
                predicate: "ex:claims".to_string(),
            }],
        };
        let resp = batch_verify_assertions(&state, req, None).await;
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].subject, "ex:thing");
        assert_eq!(resp.results[0].predicate, "ex:claims");
        assert!(!resp.results[0].verified);
    }
}
