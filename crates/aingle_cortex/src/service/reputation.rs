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
use crate::rest::pol_evidence::{pol_procedure, RuleSetFingerprint, TripleIdentity};
use crate::rest::{
    AgentAssertionOutcome, AssertionEvidence, AssertionVerifyResult, BatchVerifyAssertionsRequest,
    BatchVerifyAssertionsResponse, ConsistencyResponse,
};
use crate::state::AppState;
use aingle_graph::{NodeId, Value};

/// Outcome string for a triple that was evaluated and not rejected.
const OUTCOME_ACCEPTED: &str = "accepted";
/// Outcome string for a triple an enabled rule rejected.
const OUTCOME_REJECTED: &str = "rejected";
/// Outcome string for an assertion no triple was found for. Deliberately
/// distinct from `rejected`: "we do not hold this" and "a rule refused this" are
/// different claims, and `verified: false` collapses them.
const OUTCOME_NOT_FOUND: &str = "not_found";

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
    let rule_set = RuleSetFingerprint::of(&logic);

    // Every unit that contributes to the fraction is recorded, not just counted:
    // a bare score cannot be checked, questioned, or drilled into, so a consumer
    // has no option but to repeat it as if it were a measurement.
    let mut units: Vec<AgentAssertionOutcome> = Vec::new();

    for subject_triples in &owned_subject_triples {
        let any_valid = subject_triples.iter().any(|t| logic.validate(t).is_valid);
        units.push(AgentAssertionOutcome {
            unit: "subject".to_string(),
            subject: subject_triples
                .first()
                .map(|t| t.subject.to_string())
                .unwrap_or_default(),
            predicate: None,
            verified: any_valid,
            triple: None,
        });
    }

    for triples in &prefixed_triples {
        for t in triples {
            units.push(AgentAssertionOutcome {
                unit: "triple".to_string(),
                subject: t.subject.to_string(),
                predicate: Some(t.predicate.as_str().to_string()),
                verified: logic.validate(t).is_valid,
                triple: Some(TripleIdentity::of(t)),
            });
        }
    }

    drop(logic);

    let total = units.len();
    let verified = units.iter().filter(|u| u.verified).count();

    let score = if total > 0 {
        verified as f64 / total as f64
    } else {
        0.0
    };

    ConsistencyResponse {
        score,
        total,
        verified,
        assertions: units,
        rule_set,
        procedure: consistency_procedure(),
    }
}

/// What a caller should do with a consistency score, and what it may not say.
fn consistency_procedure() -> Vec<String> {
    let mut steps = vec![
        "1. Check the arithmetic: `total` must equal the length of `assertions`, \
         `verified` must equal the number of entries with `verified: true`, and `score` \
         must be their quotient. A score you cannot decompose is not a measurement."
            .to_string(),
        "2. Note the units. Entries with `unit: \"subject\"` count as verified when ANY \
         triple on that subject validates; entries with `unit: \"triple\"` are single \
         assertions. Both count as 1 in the fraction, so the score mixes two kinds of \
         evidence — do not present it as a uniform percentage."
            .to_string(),
        "3. `total: 0` means no assertions were found for this agent, and the resulting \
         0.0 means 'nothing to score', NOT '0% consistent'. Say which."
            .to_string(),
    ];
    // The per-verdict ceiling is identical to every other PoL surface, so it is
    // stated once, in one place, rather than paraphrased differently here.
    steps.extend(
        pol_procedure()
            .into_iter()
            .map(|s| format!("(per verdict) {s}")),
    );
    steps
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
    let rule_set = RuleSetFingerprint::of(&logic);

    let results: Vec<AssertionVerifyResult> = req
        .assertions
        .iter()
        .zip(assertion_triples.iter())
        .map(|(assertion, maybe_triple)| {
            let (verified, evidence) = match maybe_triple {
                Some(t) => {
                    let validation = logic.validate(t);
                    let matched_rule_ids = validation
                        .matches
                        .iter()
                        .map(|m| m.rule_id.clone())
                        .collect();
                    let rejected_by: Vec<String> = validation
                        .rejections
                        .iter()
                        .map(|r| format!("{}: {}", r.rule_id, r.reason))
                        .collect();
                    let verified = validation.is_valid;
                    (
                        verified,
                        AssertionEvidence {
                            found: true,
                            outcome: if verified {
                                OUTCOME_ACCEPTED.to_string()
                            } else {
                                OUTCOME_REJECTED.to_string()
                            },
                            triple: Some(TripleIdentity::of(t)),
                            matched_rule_ids,
                            rejected_by,
                        },
                    )
                }
                // Nothing was evaluated. Reporting this as `verified: false` and
                // stopping there would let "we do not hold this assertion" read
                // as "this assertion was checked and failed".
                None => (
                    false,
                    AssertionEvidence {
                        found: false,
                        outcome: OUTCOME_NOT_FOUND.to_string(),
                        triple: None,
                        matched_rule_ids: Vec::new(),
                        rejected_by: Vec::new(),
                    },
                ),
            };

            AssertionVerifyResult {
                subject: assertion.subject.clone(),
                predicate: assertion.predicate.clone(),
                verified,
                evidence,
            }
        })
        .collect();

    drop(logic);

    BatchVerifyAssertionsResponse {
        results,
        rule_set,
        procedure: pol_procedure(),
    }
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

    // ========================================================================
    // What is behind `verified` / `score`
    //
    // These verdicts are PoL evaluations against the rule set THIS NODE has
    // loaded. That is not a cryptographic proof and cannot be made into one by
    // publishing more fields — so what these tests pin is that the response
    // says what it evaluated, against which rule set, and what the result is
    // therefore worth.
    // ========================================================================

    use crate::rest::AssertionRef;
    use aingle_graph::{Predicate, Triple};

    async fn state_with_assertion(subject: &str, predicate: &str) -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let g = state.graph.write().await;
            g.insert(Triple::new(
                NodeId::named(subject),
                Predicate::named(predicate),
                Value::literal("yes"),
            ))
            .unwrap();
        }
        state
    }

    #[tokio::test]
    async fn batch_verify_publishes_the_evidence_behind_each_verdict() {
        let state = state_with_assertion("ex:thing", "ex:claims").await;
        let req = BatchVerifyAssertionsRequest {
            assertions: vec![
                AssertionRef {
                    subject: "ex:thing".into(),
                    predicate: "ex:claims".into(),
                },
                AssertionRef {
                    subject: "ex:absent".into(),
                    predicate: "ex:claims".into(),
                },
            ],
        };
        let json = serde_json::to_value(batch_verify_assertions(&state, req, None).await).unwrap();

        // ------------------------------------------------------------------
        // From here on: ONLY `json`.
        // ------------------------------------------------------------------
        let results = json["results"].as_array().expect("results");
        assert_eq!(results.len(), 2);

        // The assertion that exists must publish the triple its verdict is about,
        // identified so the client can confirm it is the triple it meant.
        let found = &results[0];
        assert_eq!(found["verified"], true, "{json}");
        let ev = &found["evidence"];
        assert_eq!(ev["found"], true);
        let t = &ev["triple"];
        let subject_bytes: Vec<u8> = {
            let s = t["subject_bytes"].as_str().expect("subject_bytes");
            (0..s.len() / 2)
                .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
                .collect()
        };
        assert!(
            String::from_utf8_lossy(&subject_bytes).contains("ex:thing"),
            "the evidence must name the triple that was evaluated"
        );

        // The assertion that does not exist must say so, rather than let
        // `verified: false` read as "evaluated and rejected".
        let missing = &results[1];
        assert_eq!(missing["verified"], false);
        assert_eq!(
            missing["evidence"]["found"], false,
            "not-found and rejected are different outcomes: {json}"
        );
        assert_eq!(missing["evidence"]["outcome"], "not_found");

        // And the verdicts must name the rule set they depend on.
        assert_eq!(json["rule_set"]["rule_count"], 0, "{json}");
        assert_eq!(json["rule_set"]["vacuous"], true);
        assert!(!json["procedure"].as_array().expect("procedure").is_empty());
    }

    #[tokio::test]
    async fn consistency_score_arithmetic_is_checkable_from_the_response() {
        let state = state_with_assertion("mayros:agent:a1:claim1", "ex:says").await;
        let json = serde_json::to_value(agent_consistency(&state, "a1", None).await).unwrap();

        // ------------------------------------------------------------------
        // From here on: ONLY `json`.
        // ------------------------------------------------------------------
        let units = json["assertions"].as_array().expect("assertions");
        assert_eq!(
            units.len() as u64,
            json["total"].as_u64().expect("total"),
            "the denominator must be enumerated, not merely counted: {json}"
        );
        let recomputed = units
            .iter()
            .filter(|u| u["verified"] == serde_json::json!(true))
            .count() as u64;
        assert_eq!(recomputed, json["verified"].as_u64().expect("verified"));
        let expected = recomputed as f64 / json["total"].as_u64().unwrap() as f64;
        assert!(
            (json["score"].as_f64().expect("score") - expected).abs() < 1e-9,
            "the score must be the arithmetic the response shows: {json}"
        );
        assert_eq!(json["rule_set"]["vacuous"], true, "{json}");
        assert!(!json["procedure"].as_array().expect("procedure").is_empty());
    }

    #[tokio::test]
    async fn an_unknown_agent_scores_zero_without_implying_a_failed_check() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let json = serde_json::to_value(agent_consistency(&state, "nobody", None).await).unwrap();
        assert_eq!(json["total"], 0);
        assert!(json["assertions"]
            .as_array()
            .expect("assertions")
            .is_empty());
        // 0.0 out of nothing is not "0% consistent"; the response must not let
        // that reading stand.
        let note = json["rule_set"]["note"].as_str().expect("note");
        assert!(!note.is_empty());
    }
}
