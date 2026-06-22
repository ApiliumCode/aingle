// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Skill verification REST endpoints.
//!
//! These endpoints support semantic skill validation, sandbox creation,
//! and cleanup for the Apilium Hub verification pipeline.

use crate::state::AppState;
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
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Deserialize, Debug)]
pub struct ValidateManifestRequest {
    /// Assertions declared in the skill manifest.
    pub assertions: Vec<AssertionDecl>,
    /// The namespace to validate against.
    pub namespace: String,
}

/// A declared assertion in the skill manifest.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Deserialize, Debug)]
pub struct AssertionDecl {
    /// The predicate this assertion targets.
    pub predicate: String,
    /// Whether the assertion requires a proof.
    #[serde(default)]
    pub require_proof: bool,
}

/// Response from manifest validation.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Serialize, Debug)]
pub struct ValidateManifestResponse {
    /// Whether all assertions are valid.
    pub valid: bool,
    /// List of validation errors.
    pub errors: Vec<String>,
}

/// Request to create a sandbox namespace.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Serialize, Debug)]
pub struct CreateSandboxResponse {
    /// Sandbox identifier.
    pub id: String,
    /// The actual namespace assigned.
    pub namespace: String,
}

/// Request identifying a sandbox by id.
///
/// Used as the MCP input for the sandbox-delete tool. (REST extracts the id
/// from the path, so this struct is MCP-only.)
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Deserialize, Debug)]
pub struct DeleteSandboxRequest {
    /// The sandbox identifier to delete.
    pub id: String,
}

/// Response from sandbox deletion.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Serialize, Debug)]
pub struct DeleteSandboxResponse {
    /// Whether the sandbox was found and removed.
    pub deleted: bool,
    /// The namespace that was cleaned up (present only when deleted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Number of triples removed (present only when deleted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triples_removed: Option<usize>,
    /// Error message (present only when not deleted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    Json(crate::service::skill::validate_manifest(&state, req).await)
}

/// POST /api/v1/skills/sandbox — Create a temporary sandbox namespace.
///
/// Creates an isolated namespace for testing a skill, with an automatic
/// TTL-based cleanup.
pub async fn create_sandbox(
    State(state): State<AppState>,
    Json(req): Json<CreateSandboxRequest>,
) -> impl IntoResponse {
    let resp = crate::service::skill::create_sandbox(&state, req).await;
    (StatusCode::CREATED, Json(resp))
}

/// DELETE /api/v1/skills/sandbox/:id — Clean up a sandbox namespace.
///
/// Removes all triples in the sandbox namespace and deregisters it.
pub async fn delete_sandbox(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
) -> impl IntoResponse {
    Json(crate::service::skill::delete_sandbox(&state, &sandbox_id).await)
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
            "/api/v1/skills/sandbox/{id}",
            axum::routing::delete(delete_sandbox),
        )
}
