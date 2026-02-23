//! Skill verification REST endpoints.
//!
//! These endpoints support semantic skill validation, sandbox creation,
//! and cleanup for the Apilium Hub verification pipeline.

use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, Triple, Value};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// Request to validate a semantic skill manifest against PoL rules.
#[derive(Deserialize, Debug)]
pub struct ValidateManifestRequest {
    /// Assertions declared in the skill manifest.
    pub assertions: Vec<AssertionDecl>,
    /// The namespace to validate against.
    pub namespace: String,
}

/// A declared assertion in the skill manifest.
#[derive(Deserialize, Debug)]
pub struct AssertionDecl {
    /// The predicate this assertion targets.
    pub predicate: String,
    /// Whether the assertion requires a proof.
    #[serde(default)]
    pub require_proof: bool,
}

/// Response from manifest validation.
#[derive(Serialize, Debug)]
pub struct ValidateManifestResponse {
    /// Whether all assertions are valid.
    pub valid: bool,
    /// List of validation errors.
    pub errors: Vec<String>,
}

/// Request to create a sandbox namespace.
#[derive(Deserialize, Debug)]
pub struct CreateSandboxRequest {
    /// Desired namespace for the sandbox.
    pub namespace: String,
    /// Time-to-live in seconds (default: 300).
    #[serde(default = "default_ttl")]
    pub ttl_seconds: u64,
}

fn default_ttl() -> u64 {
    300
}

/// Response from sandbox creation.
#[derive(Serialize, Debug)]
pub struct CreateSandboxResponse {
    /// Sandbox identifier.
    pub id: String,
    /// The actual namespace assigned.
    pub namespace: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/skills/validate — Validate a semantic skill manifest.
///
/// Checks each declared assertion's predicate against the logic engine
/// to ensure the assertions are consistent with PoL rules.
pub async fn validate_manifest(
    State(state): State<AppState>,
    Json(req): Json<ValidateManifestRequest>,
) -> impl IntoResponse {
    let logic = state.logic.read().await;
    let mut errors: Vec<String> = Vec::new();

    for assertion in &req.assertions {
        let ns_pred = if assertion.predicate.contains(':') {
            assertion.predicate.clone()
        } else {
            format!("{}:{}", req.namespace, assertion.predicate)
        };

        // If require_proof is true, validate the assertion against the
        // logic engine by constructing a test triple and checking for
        // rejections. This ensures PoL rules exist for this predicate.
        if assertion.require_proof {
            let test_triple = Triple::new(
                NodeId::named(format!("{}:_test", req.namespace)),
                Predicate::named(&ns_pred),
                Value::literal("_test_value"),
            );
            let result = logic.validate(&test_triple);
            // If the engine has no matching rules at all, the result
            // will have zero matches — warn the author.
            if result.matches.is_empty() {
                errors.push(format!(
                    "Assertion predicate '{}' requires proof but no PoL rules found",
                    ns_pred
                ));
            }
        }
    }

    let valid = errors.is_empty();
    Json(ValidateManifestResponse { valid, errors })
}

/// POST /api/v1/skills/sandbox — Create a temporary sandbox namespace.
///
/// Creates an isolated namespace for testing a skill, with an automatic
/// TTL-based cleanup.
pub async fn create_sandbox(
    State(state): State<AppState>,
    Json(req): Json<CreateSandboxRequest>,
) -> impl IntoResponse {
    let sandbox_id = format!("sandbox-{}", uuid::Uuid::new_v4());
    let sandbox_ns = format!("{}:{}", req.namespace, sandbox_id);

    // Register the sandbox in the manager
    state
        .sandbox_manager
        .create(sandbox_id.clone(), sandbox_ns.clone(), req.ttl_seconds)
        .await;

    (
        StatusCode::CREATED,
        Json(CreateSandboxResponse {
            id: sandbox_id,
            namespace: sandbox_ns,
        }),
    )
}

/// DELETE /api/v1/skills/sandbox/:id — Clean up a sandbox namespace.
///
/// Removes all triples in the sandbox namespace and deregisters it.
pub async fn delete_sandbox(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
) -> impl IntoResponse {
    let removed = state.sandbox_manager.remove(&sandbox_id).await;

    if let Some(namespace) = removed {
        // Clean up all triples whose subject starts with the sandbox namespace.
        let graph = state.graph.write().await;
        let deleted = graph.delete_by_subject_prefix(&namespace).unwrap_or(0);

        Json(serde_json::json!({
            "deleted": true,
            "namespace": namespace,
            "triples_removed": deleted
        }))
    } else {
        Json(serde_json::json!({
            "deleted": false,
            "error": "sandbox not found"
        }))
    }
}

/// Create the skill verification sub-router.
pub fn skill_verification_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/api/v1/skills/validate",
            axum::routing::post(validate_manifest),
        )
        .route(
            "/api/v1/skills/sandbox",
            axum::routing::post(create_sandbox),
        )
        .route(
            "/api/v1/skills/sandbox/:id",
            axum::routing::delete(delete_sandbox),
        )
}
