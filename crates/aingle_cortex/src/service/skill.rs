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
            if result.matches.is_empty() {
                errors.push(format!(
                    "Assertion predicate '{}' requires proof but no PoL rules found",
                    ns_pred
                ));
            }
            checks.push(ManifestCheck {
                predicate: ns_pred,
                declared_predicate: assertion.predicate.clone(),
                require_proof: true,
                evaluated: true,
                outcome: if matched_rule_ids.is_empty() {
                    "no_matching_rule".to_string()
                } else {
                    "rule_matched".to_string()
                },
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

    let valid = errors.is_empty();
    ValidateManifestResponse {
        valid,
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
        "1. Read `checks` before `valid`. An entry with `evaluated: false` was never \
         examined — the manifest did not ask for proof on it — so it contributes \
         nothing to `valid` in either direction."
            .to_string(),
        "2. Read `rule_set`. If `vacuous` is true, no rule can match any probe, so every \
         `require_proof` assertion is reported as failing for a configuration reason, \
         not a manifest defect."
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
        // logic engine is never consulted and validation passes.
        let req = ValidateManifestRequest {
            namespace: "skill".into(),
            assertions: vec![AssertionDecl {
                predicate: "hasCapability".into(),
                require_proof: false,
            }],
        };
        let resp = validate_manifest(&state, req).await;
        assert!(resp.valid);
        assert!(resp.errors.is_empty());
    }

    #[tokio::test]
    async fn validate_manifest_proof_required_without_rules_is_invalid() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        // require_proof=true with an empty logic engine => no PoL rules match,
        // so the assertion is flagged as invalid.
        let req = ValidateManifestRequest {
            namespace: "skill".into(),
            assertions: vec![AssertionDecl {
                predicate: "provesIdentity".into(),
                require_proof: true,
            }],
        };
        let resp = validate_manifest(&state, req).await;
        assert!(!resp.valid);
        assert_eq!(resp.errors.len(), 1);
        assert!(resp.errors[0].contains("provesIdentity"));
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
        assert_eq!(json["rule_set"]["rule_count"], 0, "{json}");
        assert_eq!(json["rule_set"]["vacuous"], true, "{json}");

        let checks = json["checks"].as_array().expect("checks");
        assert_eq!(checks.len(), 2);
        // The proof-requiring assertion was probed; the other one never was, and
        // the response must not let its silence read as a pass.
        let probed = checks
            .iter()
            .find(|c| c["predicate"] == serde_json::json!("skill:provesIdentity"))
            .expect("probed assertion");
        assert_eq!(probed["evaluated"], true);
        assert_eq!(probed["outcome"], "no_matching_rule");
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
