// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Skill verification business logic shared by REST and MCP.
//!
//! Covers semantic skill manifest validation, temporary sandbox namespace
//! creation, and sandbox cleanup. The REST handlers in
//! [`crate::rest::skill_verification`] delegate to these functions so the MCP
//! tools and HTTP surface share a single implementation.

use crate::rest::{
    CreateSandboxRequest, CreateSandboxResponse, DeleteSandboxResponse, ValidateManifestRequest,
    ValidateManifestResponse,
};
use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, Triple, Value};

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
    let mut errors: Vec<String> = Vec::new();

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
            if result.matches.is_empty() {
                errors.push(format!(
                    "Assertion predicate '{}' requires proof but no PoL rules found",
                    ns_pred
                ));
            }
        }
    }

    let valid = errors.is_empty();
    ValidateManifestResponse { valid, errors }
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
