// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Proof validation endpoints.
//!
//! ## Asserted, reproducible, and neither
//!
//! `POST /api/v1/validate` answers with `valid` — this node's own PoL verdict,
//! documented on [`ValidateResponse`] as an assertion rather than proof — and
//! with `proof_hash`, which **is** reproducible: [`ValidationProofDto`] publishes
//! its exact preimage. The two must not be read as one thing: the digest commits
//! to which triples were submitted, never to the verdict on them.
//!
//! `GET /api/v1/proof/:hash` and `POST /api/v1/verify` are unimplemented and
//! return 404. They are kept so the routes fail loudly rather than being served
//! by something weaker; see the handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::middleware::RequestNamespace;
use crate::rest::triples::{TripleDto, ValueDto};
use crate::state::AppState;

/// Request to validate triples
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    /// Triples to validate
    pub triples: Vec<ValidateTripleInput>,
    /// Rule set to use (optional)
    pub rule_set: Option<String>,
}

/// Triple input for validation
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Debug, Deserialize)]
pub struct ValidateTripleInput {
    pub subject: String,
    pub predicate: String,
    pub object: ValueDto,
}

/// Validation response.
///
/// **`valid` is an assertion by this node, not proof.** It reports that no rule
/// in *this node's* loaded PoL rule set rejected the triples — which, on a node
/// with no rules loaded, is true of everything. Read `rule_set.vacuous` before
/// reading `valid`, and see [`crate::rest::pol_evidence`] for why this verdict
/// cannot be replayed from the response alone.
///
/// `proof_hash`, by contrast, *is* reproducible: `proof` publishes its exact
/// preimage.
#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    /// Overall validity, as evaluated by this node against its own rule set.
    ///
    /// **An assertion, not proof.** A client that needs certainty must obtain
    /// the rule set and re-run it; a client that cannot must report "this node
    /// reports valid", never "validated".
    pub valid: bool,
    /// Individual validation results
    pub results: Vec<TripleValidationResult>,
    /// Digest committing to the validated triples' identities.
    ///
    /// Present only when every triple was reported valid. **It commits to the
    /// inputs, not to the verdict** — see `proof.does_not_cover`. Reproduce it
    /// from `proof` rather than treating it as an opaque anchor.
    pub proof_hash: Option<String>,
    /// The exact preimage of `proof_hash`, so a client can recompute it.
    /// `None` whenever `proof_hash` is `None`.
    pub proof: Option<ValidationProofDto>,
    /// The rule set this node evaluated the triples against — including whether
    /// it was empty, in which case the verdict examined nothing.
    pub rule_set: crate::rest::pol_evidence::RuleSetFingerprint,
    /// Steps a caller should run and report on instead of relaying `valid`.
    pub procedure: Vec<String>,
}

/// The reproducible preimage of a [`ValidateResponse::proof_hash`].
///
/// `proof_hash` was previously an anchor with nothing behind it: a digest with
/// no published preimage and no way to fetch what it committed to. This makes it
/// checkable — and, just as importantly, states what it does *not* cover, since
/// a hash sitting next to `valid: true` reads as a proof of validity and is not.
#[derive(Debug, Serialize)]
pub struct ValidationProofDto {
    /// Identifier of the digest scheme (`aingle-validation-proof-v1`).
    pub spec: String,
    /// Digest algorithm (`blake3-256`).
    pub hash_alg: String,
    /// What the digest commits to.
    pub covers: String,
    /// What the digest does **not** commit to. Read this before citing it.
    pub does_not_cover: String,
    /// The preimage, as the list of ASCII triple-id hex strings that were
    /// hashed, in order. Concatenate them with no separator and digest the
    /// resulting ASCII bytes to reproduce `proof_hash`.
    pub preimage_parts: Vec<String>,
    /// The literal hash inputs of each validated triple, so the ids in
    /// `preimage_parts` are themselves recomputable rather than taken on trust.
    pub triples: Vec<crate::rest::pol_evidence::TripleIdentity>,
    /// Step-by-step reproduction procedure.
    pub procedure: Vec<String>,
}

/// Individual triple validation result
#[derive(Debug, Serialize)]
pub struct TripleValidationResult {
    /// Triple that was validated
    pub triple: TripleDto,
    /// Whether this triple is valid
    pub valid: bool,
    /// Validation messages
    pub messages: Vec<ValidationMessage>,
}

/// Validation message
#[derive(Debug, Serialize)]
pub struct ValidationMessage {
    /// Message level: "info", "warning", "error"
    pub level: String,
    /// Message text
    pub message: String,
    /// Rule that generated this message
    pub rule: Option<String>,
}

/// Validate triples against logic rules
///
/// POST /api/v1/validate
///
/// Delegates to [`crate::service::validate::validate_triples`] so this endpoint
/// and the MCP `aingle_validate` tool return the same verdict *and the same
/// evidence*. They were separate implementations of the same logic; publishing
/// the reproducible preimage of `proof_hash` from only one of them would have
/// left the other quietly weaker.
pub async fn validate_triples(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>> {
    let namespace = ns_ext.and_then(|axum::Extension(RequestNamespace(ns))| ns);
    let resp = crate::service::validate::validate_triples(&state, req, namespace).await?;
    Ok(Json(resp))
}

/// Proof data structure
#[derive(Debug, Serialize)]
pub struct ProofDto {
    /// Proof hash
    pub hash: String,
    /// Proof steps
    pub steps: Vec<ProofStepDto>,
    /// Whether proof is valid
    pub valid: bool,
    /// When proof was verified
    pub verified_at: String,
    /// Root hash
    pub root: String,
}

/// Proof step
#[derive(Debug, Serialize)]
pub struct ProofStepDto {
    /// Step index
    pub index: usize,
    /// Rule applied
    pub rule: String,
    /// Premises used
    pub premises: Vec<String>,
    /// Conclusion derived
    pub conclusion: String,
}

/// Get a proof by hash
///
/// GET /api/v1/proof/:hash
///
/// **Not implemented: always 404.** A `proof_hash` from `POST /api/v1/validate`
/// cannot be resolved into a stored proof here — that hash is a digest of the
/// submitted triples' identities and nothing was ever stored under it. Reproduce
/// it from `ValidateResponse::proof` instead. For stored ZK proofs, use
/// `GET /api/v1/proofs/{id}`, which returns the proof bytes and a replay bundle.
pub async fn get_proof(
    State(_state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<ProofDto>> {
    // Returning 404 is the honest answer: there is no proof store behind this
    // route. Synthesizing a `valid: true` response here would be worse than
    // useless — it would be a verdict about nothing.
    Err(Error::NotFound(format!("Proof {} not found", hash)))
}

/// Request to verify a proof
#[derive(Debug, Deserialize)]
pub struct VerifyProofRequest {
    /// Proof hash to verify
    pub proof_hash: String,
    /// Optional: expected statements
    pub statements: Option<Vec<StatementInput>>,
}

/// Statement input for verification
#[derive(Debug, Deserialize)]
pub struct StatementInput {
    pub subject: String,
    pub predicate: String,
    pub object: ValueDto,
}

/// Verify proof response.
///
/// Shape of the (unimplemented) `POST /api/v1/verify` response. Were it
/// implemented, `valid` here would be a server assertion exactly like the one on
/// [`crate::rest::VerifyProofResponse`], and would need the same replay bundle to
/// be worth anything.
#[derive(Debug, Serialize)]
pub struct VerifyProofResponse {
    /// Whether proof is valid — a server assertion, not proof.
    pub valid: bool,
    /// Verification details
    pub details: VerificationDetails,
}

/// Verification details
#[derive(Debug, Serialize)]
pub struct VerificationDetails {
    /// Proof hash
    pub proof_hash: String,
    /// Number of steps verified
    pub steps_verified: usize,
    /// Statements covered by proof
    pub statements_covered: usize,
    /// Verification timestamp
    pub verified_at: String,
}

/// Verify a proof
///
/// POST /api/v1/verify
///
/// **Not implemented: always 404.** There is no proof store behind this route, so
/// it cannot verify anything. Use `GET /api/v1/proofs/{id}/verify` for stored ZK
/// proofs — that endpoint returns the material to replay the check rather than
/// only a verdict.
pub async fn verify_proof(
    State(_state): State<AppState>,
    Json(req): Json<VerifyProofRequest>,
) -> Result<Json<VerifyProofResponse>> {
    // A 404 is the honest answer. An endpoint named "verify" that answered
    // `valid: true` without checking anything would be the worst possible
    // version of the defect this module documents.
    Err(Error::NotFound(format!(
        "Proof {} not found",
        req.proof_hash
    )))
}
