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
///
/// **`valid` is an assertion by this node, not proof.** It means "for every
/// assertion that declared `require_proof`, at least one enabled PoL rule matched
/// a probe triple on this node". That depends entirely on which rules this node
/// has loaded — see `rule_set` — and says nothing about the skill itself.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Serialize, Debug)]
pub struct ValidateManifestResponse {
    /// Whether every checked assertion found a matching rule, or `null` when no
    /// assertion was checked at all.
    ///
    /// **An assertion, not proof.** Assertions with `require_proof: false` are
    /// never checked, and no assertion can be checked on a node with no
    /// predicate-scoped rules — so a manifest can come back with nothing
    /// examined. That answers `null` / `not_evaluated` rather than `true`, which
    /// would report an unexamined manifest as passing. `checks` shows which
    /// entries were which.
    pub valid: Option<bool>,
    /// `valid` / `invalid` / `not_evaluated` for the manifest as a whole.
    pub outcome: String,
    /// List of validation errors.
    pub errors: Vec<String>,
    /// What was checked, per declared assertion — including the ones that were
    /// skipped, so silence cannot be read as a pass.
    pub checks: Vec<ManifestCheck>,
    /// The rule set the verdict depends on. When `vacuous`, every
    /// `require_proof` assertion necessarily fails to find a rule.
    pub rule_set: crate::rest::pol_evidence::RuleSetFingerprint,
    /// Steps a caller should run and report on instead of relaying `valid`.
    pub procedure: Vec<String>,
    /// Why this verdict cannot be made independently checkable, stated plainly
    /// rather than left for the caller to discover.
    pub limitation: String,
}

/// What was done for one declared assertion in a manifest.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Serialize, Debug)]
pub struct ManifestCheck {
    /// The namespace-resolved predicate that was (or would have been) probed.
    pub predicate: String,
    /// The predicate exactly as the manifest declared it.
    pub declared_predicate: String,
    /// Whether the manifest asked for this assertion to be backed by proof.
    pub require_proof: bool,
    /// Whether this node evaluated anything for this assertion. `false` when
    /// `require_proof` is `false` (nothing is probed then), and also when this
    /// node has no rule capable of matching a predicate at all.
    pub evaluated: bool,
    /// Outcome: `"rule_matched"`, `"no_matching_rule"`, `"not_evaluated"` (this
    /// node has no predicate-scoped rule to check against), or `"not_checked"`
    /// (the manifest asked for no proof).
    pub outcome: String,
    /// Ids of the enabled rules that matched the probe triple.
    pub matched_rule_ids: Vec<String>,
    /// Identity of the probe triple that was run through the rule engine, so a
    /// caller can see the artificial subject/value the check actually used
    /// rather than assume a real assertion was inspected. `None` when the
    /// assertion was not evaluated.
    pub probe: Option<crate::rest::pol_evidence::TripleIdentity>,
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
