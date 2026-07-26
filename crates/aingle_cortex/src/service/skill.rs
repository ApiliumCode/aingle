// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Skill verification business logic shared by REST and MCP.
//!
//! Covers semantic skill manifest validation, temporary sandbox namespace
//! creation, and sandbox cleanup. The REST handlers in
//! [`crate::rest::skill_verification`] delegate to these functions so the MCP
//! tools and HTTP surface share a single implementation.

use crate::rest::pol_evidence::{pol_procedure, RuleSetFingerprint, TripleIdentity};
use crate::rest::{
    CreateSandboxRequest, CreateSandboxResponse, DeleteSandboxResponse, ManifestCheck,
    ValidateManifestRequest, ValidateManifestResponse,
};
use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, Triple, Value};

/// Why a manifest verdict cannot be turned into something a client checks.
const MANIFEST_LIMITATION: &str = "This verdict is not independently checkable and cannot be \
     made so. It is the result of running probe triples through the PoL rule set loaded on \
     THIS node; reproducing it requires those rules, which are node configuration and may \
     include conditions backed by Rust closures that cannot be serialized at all. What is \
     published instead is everything the verdict depended on — the rule set fingerprint and \
     the exact probe triples — so a caller can see what was done and report it accurately, \
     rather than a bare boolean it can only repeat.";

/// Validate a semantic skill manifest against the PoL logic engine.
///
/// For every declared assertion that requires a proof, a probe triple is run
/// through the logic engine; if no PoL rules match the predicate, a validation
/// error is recorded. Validation never mutates state. Returns a response whose
/// `valid` flag is `true` iff no errors were collected (mirrors the REST
/// handler exactly).
pub async fn validate_manifest(
    state: &AppState,
    req: ValidateManifestRequest,
) -> ValidateManifestResponse {
    let logic = state.logic.read().await;
    let rule_set = RuleSetFingerprint::of(&logic);
    let mut errors: Vec<String> = Vec::new();
    let mut checks: Vec<ManifestCheck> = Vec::new();

    for assertion in &req.assertions {
        let ns_pred = if assertion.predicate.contains(':') {
            assertion.predicate.clone()
        } else {
            format!("{}:{}", req.namespace, assertion.predicate)
        };

        if assertion.require_proof {
            let test_triple = Triple::new(
                NodeId::named(format!("{}:_test", req.namespace)),
                Predicate::named(&ns_pred),
                Value::literal("_test_value"),
            );
            let result = logic.validate(&test_triple);
            let matched_rule_ids: Vec<String> =
                result.matches.iter().map(|m| m.rule_id.clone()).collect();
            // Nothing can match a probe when nothing is enabled, and nothing can
            // match a *predicate* when no enabled rule is scoped to one. Both are
            // facts about this node's configuration, not about the manifest, and
            // reporting them as manifest errors is how a configuration gap gets
            // blamed on the thing being checked.
            let unevaluable = rule_set.vacuous || rule_set.predicate_scoped_rule_count == 0;
            let outcome = if !matched_rule_ids.is_empty() {
                "rule_matched"
            } else if unevaluable {
                "not_evaluated"
            } else {
                "no_matching_rule"
            };
            if matched_rule_ids.is_empty() {
                errors.push(if unevaluable {
                    format!(
                        "Assertion predicate '{}' requires proof, but this node has no \
                         predicate-scoped PoL rule to check it against — a configuration \
                         gap on this node, not a defect in the manifest.",
                        ns_pred
                    )
                } else {
                    format!(
                        "Assertion predicate '{}' requires proof but no PoL rules found",
                        ns_pred
                    )
                });
            }
            checks.push(ManifestCheck {
                predicate: ns_pred,
                declared_predicate: assertion.predicate.clone(),
                require_proof: true,
                evaluated: !unevaluable,
                outcome: outcome.to_string(),
                matched_rule_ids,
                // The probe is an artificial triple, not one of the skill's own
                // assertions. Publishing it stops "validated" from implying that
                // real data was inspected.
                probe: Some(TripleIdentity::of(&test_triple)),
            });
        } else {
            // Nothing happens for these, and nothing happening must not look
            // like a pass.
            checks.push(ManifestCheck {
                predicate: ns_pred,
                declared_predicate: assertion.predicate.clone(),
                require_proof: false,
                evaluated: false,
                outcome: "not_checked".to_string(),
                matched_rule_ids: Vec::new(),
                probe: None,
            });
        }
    }

    drop(logic);

    // A manifest whose every check was skipped or unevaluable was not examined,
    // and `valid: true` from an empty `errors` list would say otherwise.
    let (valid, outcome) = if checks.iter().any(|c| c.evaluated) {
        let ok = errors.is_empty();
        (
            Some(ok),
            if ok {
                crate::rest::pol_evidence::OUTCOME_VALID
            } else {
                crate::rest::pol_evidence::OUTCOME_INVALID
            },
        )
    } else {
        (None, crate::rest::pol_evidence::OUTCOME_NOT_EVALUATED)
    };

    ValidateManifestResponse {
        valid,
        outcome: outcome.to_string(),
        errors,
        checks,
        rule_set,
        procedure: manifest_procedure(),
        limitation: MANIFEST_LIMITATION.to_string(),
    }
}

/// What a caller should check and report for a manifest verdict.
fn manifest_procedure() -> Vec<String> {
    let mut steps = vec![
        "1. Read `outcome` and `checks` before `valid`. An entry with \
         `evaluated: false` was never examined — either the manifest did not ask for \
         proof on it, or this node has no rule that could check it — so it contributes \
         nothing to `valid` in either direction. When NO check was evaluated, `valid` \
         is null and `outcome` is `not_evaluated`."
            .to_string(),
        "2. Read `rule_set`. If `vacuous` is true, or `predicate_scoped_rule_count` is \
         0, no rule can match any probe, so every `require_proof` assertion is reported \
         as `not_evaluated` for a configuration reason on this node — not a manifest \
         defect. Say that, and do not report the manifest as bad."
            .to_string(),
        "3. Note that each check ran against a synthetic probe triple (published as \
         `probe`), not against the skill's real assertions. A matching rule means such \
         a rule EXISTS for that predicate; it does not mean any actual assertion was \
         validated."
            .to_string(),
    ];
    steps.extend(
        pol_procedure()
            .into_iter()
            .map(|s| format!("(per check) {s}")),
    );
    steps
}

/// Create a temporary sandbox namespace and register it in the sandbox manager.
///
/// Generates a unique sandbox id and derived namespace, registers it with the
/// requested TTL, and returns the id/namespace. Mutates sandbox state (mirrors
/// the REST handler).
pub async fn create_sandbox(state: &AppState, req: CreateSandboxRequest) -> CreateSandboxResponse {
    let sandbox_id = format!("sandbox-{}", uuid::Uuid::new_v4());
    let sandbox_ns = format!("{}:{}", req.namespace, sandbox_id);

    state
        .sandbox_manager
        .create(sandbox_id.clone(), sandbox_ns.clone(), req.ttl_seconds)
        .await;

    CreateSandboxResponse {
        id: sandbox_id,
        namespace: sandbox_ns,
    }
}

/// Delete a sandbox namespace by id, removing all triples under it.
///
/// Deregisters the sandbox from the manager and, if it existed, deletes every
/// triple whose subject begins with the sandbox namespace. Returns a response
/// describing whether anything was deleted. Deleting an unknown id yields
/// `{ deleted: false, error: "sandbox not found" }` (mirrors the REST handler).
pub async fn delete_sandbox(state: &AppState, sandbox_id: &str) -> DeleteSandboxResponse {
    let removed = state.sandbox_manager.remove(sandbox_id).await;

    if let Some(namespace) = removed {
        let graph = state.graph.write().await;
        let deleted = graph.delete_by_subject_prefix(&namespace).unwrap_or(0);

        DeleteSandboxResponse {
            deleted: true,
            namespace: Some(namespace),
            triples_removed: Some(deleted),
            error: None,
        }
    } else {
        DeleteSandboxResponse {
            deleted: false,
            namespace: None,
            triples_removed: None,
            error: Some("sandbox not found".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::AssertionDecl;

    #[tokio::test]
    async fn validate_manifest_no_proof_required_is_valid() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        // A minimal manifest: one assertion that does not require proof, so the
        // logic engine is never consulted. Nothing was examined, so the answer
        // is `not_evaluated` — `valid: true` would report an unexamined manifest
        // as having passed.
        let req = ValidateManifestRequest {
            namespace: "skill".into(),
            assertions: vec![AssertionDecl {
                predicate: "hasCapability".into(),
                require_proof: false,
            }],
        };
        let resp = validate_manifest(&state, req).await;
        assert_eq!(resp.valid, None);
        assert_eq!(resp.outcome, "not_evaluated");
        assert!(resp.errors.is_empty());
    }

    #[tokio::test]
    async fn validate_manifest_proof_required_without_a_predicate_rule_is_not_evaluated() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        // The default rule set checks well-formedness, not specific predicates,
        // so no rule can match this probe. That is a gap in THIS NODE'S
        // configuration; blaming the manifest for it would be the same category
        // of dishonesty as calling an unexamined triple valid.
        let req = ValidateManifestRequest {
            namespace: "skill".into(),
            assertions: vec![AssertionDecl {
                predicate: "provesIdentity".into(),
                require_proof: true,
            }],
        };
        let resp = validate_manifest(&state, req).await;
        assert_eq!(resp.valid, None);
        assert_eq!(resp.outcome, "not_evaluated");
        assert_eq!(resp.errors.len(), 1);
        assert!(resp.errors[0].contains("provesIdentity"));
        assert!(
            resp.errors[0].contains("configuration"),
            "the error must name whose gap this is: {}",
            resp.errors[0]
        );
        assert_eq!(resp.checks[0].outcome, "not_evaluated");
        assert!(!resp.checks[0].evaluated);
    }

    #[tokio::test]
    async fn a_predicate_scoped_rule_set_actually_evaluates_a_manifest() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut logic = state.logic.write().await;
            logic.add_rule(
                aingle_logic::Rule::authority("proves_identity")
                    .name("Identity assertions are checked")
                    .when_predicate("skill:provesIdentity")
                    .accept()
                    .build(),
            );
        }
        let req = ValidateManifestRequest {
            namespace: "skill".into(),
            assertions: vec![
                AssertionDecl {
                    predicate: "provesIdentity".into(),
                    require_proof: true,
                },
                AssertionDecl {
                    predicate: "hasNoRule".into(),
                    require_proof: true,
                },
            ],
        };
        let resp = validate_manifest(&state, req).await;

        // The predicate with a rule was genuinely checked...
        assert!(resp.checks[0].evaluated);
        assert_eq!(resp.checks[0].outcome, "rule_matched");
        assert_eq!(
            resp.checks[0].matched_rule_ids,
            vec!["proves_identity".to_string()]
        );
        // ...and the one without it is a real manifest finding now that this
        // node demonstrably can check predicates.
        assert_eq!(resp.checks[1].outcome, "no_matching_rule");
        assert_eq!(resp.valid, Some(false));
        assert_eq!(resp.outcome, "invalid");
    }

    /// `aingle_validate_skill` answers `valid` from the PoL rule set this node
    /// has loaded. That dependency was invisible: a caller could not tell whether
    /// `valid: true` meant "the rules accepted it" or "there are no rules". The
    /// response has to publish the rule set its verdict rests on, and the probe
    /// triples it actually ran.
    #[tokio::test]
    async fn validate_manifest_publishes_the_rule_set_its_verdict_depends_on() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let req = ValidateManifestRequest {
            namespace: "skill".into(),
            assertions: vec![
                AssertionDecl {
                    predicate: "provesIdentity".into(),
                    require_proof: true,
                },
                AssertionDecl {
                    predicate: "hasCapability".into(),
                    require_proof: false,
                },
            ],
        };
        let json = serde_json::to_value(validate_manifest(&state, req).await).unwrap();

        // ------------------------------------------------------------------
        // From here on: ONLY `json`.
        // ------------------------------------------------------------------
        assert!(
            json["rule_set"]["rule_count"].as_u64().expect("count") > 0,
            "{json}"
        );
        assert_eq!(json["rule_set"]["vacuous"], false, "{json}");
        assert!(
            !json["rule_set"]["rules"]
                .as_array()
                .expect("rules")
                .is_empty(),
            "the rules behind the verdict must be enumerated, not just counted: {json}"
        );

        let checks = json["checks"].as_array().expect("checks");
        assert_eq!(checks.len(), 2);
        // Neither assertion was examined — one asked for no proof, and the
        // default rule set has no predicate-scoped rule to probe with. The
        // response must not let either silence read as a pass.
        let probed = checks
            .iter()
            .find(|c| c["predicate"] == serde_json::json!("skill:provesIdentity"))
            .expect("probed assertion");
        assert_eq!(probed["evaluated"], false);
        assert_eq!(probed["outcome"], "not_evaluated");
        let skipped = checks
            .iter()
            .find(|c| c["predicate"] == serde_json::json!("skill:hasCapability"))
            .expect("skipped assertion");
        assert_eq!(
            skipped["evaluated"], false,
            "an assertion that declares no proof requirement is not checked at all: {json}"
        );
        assert_eq!(skipped["outcome"], "not_checked");

        assert!(!json["procedure"].as_array().expect("procedure").is_empty());
        assert!(json["limitation"].as_str().expect("limitation").len() > 20);
    }

    #[tokio::test]
    async fn create_sandbox_returns_id_and_namespace() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let req = CreateSandboxRequest {
            namespace: "skill".into(),
            ttl_seconds: 300,
        };
        let resp = create_sandbox(&state, req).await;
        assert!(resp.id.starts_with("sandbox-"));
        assert!(resp.namespace.starts_with("skill:sandbox-"));
        // The sandbox is registered: removing it returns its namespace.
        let removed = state.sandbox_manager.remove(&resp.id).await;
        assert_eq!(removed.as_deref(), Some(resp.namespace.as_str()));
    }

    #[tokio::test]
    async fn create_then_delete_sandbox_succeeds() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let created = create_sandbox(
            &state,
            CreateSandboxRequest {
                namespace: "skill".into(),
                ttl_seconds: 300,
            },
        )
        .await;

        let resp = delete_sandbox(&state, &created.id).await;
        assert!(resp.deleted);
        assert_eq!(resp.namespace.as_deref(), Some(created.namespace.as_str()));
        assert_eq!(resp.triples_removed, Some(0));
        assert!(resp.error.is_none());
    }

    #[tokio::test]
    async fn delete_unknown_sandbox_reports_not_found() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let resp = delete_sandbox(&state, "sandbox-does-not-exist").await;
        assert!(!resp.deleted);
        assert!(resp.namespace.is_none());
        assert_eq!(resp.error.as_deref(), Some("sandbox not found"));
    }
}
