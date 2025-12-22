//! Proof validation endpoints

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::rest::triples::{TripleDto, ValueDto};
use crate::state::{AppState, Event};
use aingle_graph::{NodeId, Predicate, Triple, Value};

/// Request to validate triples
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    /// Triples to validate
    pub triples: Vec<ValidateTripleInput>,
    /// Rule set to use (optional)
    pub rule_set: Option<String>,
}

/// Triple input for validation
#[derive(Debug, Deserialize)]
pub struct ValidateTripleInput {
    pub subject: String,
    pub predicate: String,
    pub object: ValueDto,
}

/// Validation response
#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    /// Overall validity
    pub valid: bool,
    /// Individual validation results
    pub results: Vec<TripleValidationResult>,
    /// Proof hash if generated
    pub proof_hash: Option<String>,
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
pub async fn validate_triples(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>> {
    let logic = state.logic.read().await;

    let mut results = Vec::new();
    let mut all_valid = true;

    for input in req.triples {
        let object: Value = input.object.clone().into();

        // Create a triple for validation
        let triple = Triple::new(
            NodeId::named(&input.subject),
            Predicate::named(&input.predicate),
            object,
        );

        // Validate using logic engine
        let validation = logic.validate(&triple);

        let valid = validation.is_valid();
        if !valid {
            all_valid = false;
        }

        // Convert messages
        let mut messages = Vec::new();
        for rejection in &validation.rejections {
            messages.push(ValidationMessage {
                level: "error".to_string(),
                message: rejection.reason.clone(),
                rule: Some(rejection.rule_id.clone()),
            });
        }
        for warning in &validation.warnings {
            messages.push(ValidationMessage {
                level: "warning".to_string(),
                message: warning.message.clone(),
                rule: Some(warning.rule_id.clone()),
            });
        }

        let triple_dto = TripleDto {
            id: Some(triple.id().to_hex()),
            subject: input.subject.clone(),
            predicate: input.predicate.clone(),
            object: input.object,
            created_at: None,
        };

        results.push(TripleValidationResult {
            triple: triple_dto,
            valid,
            messages,
        });
    }

    // Generate a simple proof hash if all valid
    let proof_hash = if all_valid {
        // Simple hash of all triple hashes
        let mut hasher = blake3::Hasher::new();
        for result in &results {
            if let Some(ref id) = result.triple.id {
                hasher.update(id.as_bytes());
            }
        }
        Some(hasher.finalize().to_hex().to_string())
    } else {
        None
    };

    // Broadcast validation event
    if let Some(ref hash) = proof_hash {
        state.broadcaster.broadcast(Event::ValidationCompleted {
            hash: hash.clone(),
            valid: all_valid,
            proof_hash: proof_hash.clone(),
        });
    }

    Ok(Json(ValidateResponse {
        valid: all_valid,
        results,
        proof_hash,
    }))
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
pub async fn get_proof(
    State(_state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<ProofDto>> {
    // For now, return a placeholder - proof storage not implemented yet
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

/// Verify proof response
#[derive(Debug, Serialize)]
pub struct VerifyProofResponse {
    /// Whether proof is valid
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
pub async fn verify_proof(
    State(_state): State<AppState>,
    Json(req): Json<VerifyProofRequest>,
) -> Result<Json<VerifyProofResponse>> {
    // For now, return not found - proof verification not implemented yet
    Err(Error::NotFound(format!(
        "Proof {} not found",
        req.proof_hash
    )))
}
