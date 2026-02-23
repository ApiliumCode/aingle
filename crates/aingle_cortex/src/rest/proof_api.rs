//! REST API endpoints for proof storage and verification

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::middleware::{is_in_namespace, RequestNamespace};
use crate::proofs::{ProofId, ProofMetadata, ProofType, StoredProof, SubmitProofRequest};
use crate::state::AppState;

/// Submit a new proof
///
/// POST /api/v1/proofs
pub async fn submit_proof(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Json(request): Json<SubmitProofRequest>,
) -> Result<Json<SubmitProofResponse>> {
    // Enforce namespace: submitter must belong to the namespace
    if let Some(axum::Extension(RequestNamespace(Some(ref ns)))) = ns_ext {
        if let Some(ref meta) = request.metadata {
            if let Some(ref submitter) = meta.submitter {
                if !is_in_namespace(submitter, ns) {
                    return Err(Error::Forbidden(format!(
                        "Submitter \"{}\" is not in namespace \"{}\"",
                        submitter, ns
                    )));
                }
            }
        }
    }

    let proof_id = state
        .proof_store
        .submit(request)
        .await
        .map_err(|e| Error::ValidationError(e.to_string()))?;

    let proof = state
        .proof_store
        .get(&proof_id)
        .await
        .ok_or_else(|| Error::Internal("Failed to retrieve submitted proof".to_string()))?;

    Ok(Json(SubmitProofResponse {
        proof_id,
        submitted_at: proof.created_at,
    }))
}

/// Submit multiple proofs in batch
///
/// POST /api/v1/proofs/batch
pub async fn submit_proofs_batch(
    State(state): State<AppState>,
    Json(request): Json<BatchSubmitRequest>,
) -> Result<Json<BatchSubmitResponse>> {
    let results = state.proof_store.submit_batch(request.proofs).await;

    let mut successful = Vec::new();
    let mut failed = Vec::new();

    for (idx, result) in results.into_iter().enumerate() {
        match result {
            Ok(proof_id) => successful.push(proof_id),
            Err(e) => failed.push(BatchError {
                index: idx,
                error: e.to_string(),
            }),
        }
    }

    Ok(Json(BatchSubmitResponse {
        successful_count: successful.len(),
        failed_count: failed.len(),
        successful,
        failed,
    }))
}

/// Get a proof by ID
///
/// GET /api/v1/proofs/:id
pub async fn get_proof(
    State(state): State<AppState>,
    Path(proof_id): Path<ProofId>,
) -> Result<Json<ProofResponse>> {
    let proof = state
        .proof_store
        .get(&proof_id)
        .await
        .ok_or_else(|| Error::NotFound(format!("Proof {} not found", proof_id)))?;

    Ok(Json(ProofResponse::from(proof)))
}

/// Verify a proof
///
/// GET /api/v1/proofs/:id/verify
pub async fn verify_proof_by_id(
    State(state): State<AppState>,
    Path(proof_id): Path<ProofId>,
) -> Result<Json<VerifyProofResponse>> {
    let result = state
        .proof_store
        .verify(&proof_id)
        .await
        .map_err(|e| Error::ValidationError(e.to_string()))?;

    Ok(Json(VerifyProofResponse {
        proof_id: proof_id.clone(),
        valid: result.valid,
        verified_at: result.verified_at,
        details: result.details,
        verification_time_us: result.verification_time_us,
    }))
}

/// Batch verify multiple proofs
///
/// POST /api/v1/proofs/verify/batch
pub async fn verify_proofs_batch(
    State(state): State<AppState>,
    Json(request): Json<BatchVerifyRequest>,
) -> Result<Json<BatchVerifyResponse>> {
    let results = state.proof_store.batch_verify(&request.proof_ids).await;

    let mut verifications = Vec::new();
    for (idx, result) in results.into_iter().enumerate() {
        let proof_id = &request.proof_ids[idx];
        match result {
            Ok(verification) => {
                verifications.push(VerifyProofResponse {
                    proof_id: proof_id.clone(),
                    valid: verification.valid,
                    verified_at: verification.verified_at,
                    details: verification.details,
                    verification_time_us: verification.verification_time_us,
                });
            }
            Err(e) => {
                verifications.push(VerifyProofResponse {
                    proof_id: proof_id.clone(),
                    valid: false,
                    verified_at: chrono::Utc::now(),
                    details: vec![format!("Verification error: {}", e)],
                    verification_time_us: 0,
                });
            }
        }
    }

    let valid_count = verifications.iter().filter(|v| v.valid).count();

    Ok(Json(BatchVerifyResponse {
        total: verifications.len(),
        valid_count,
        invalid_count: verifications.len() - valid_count,
        verifications,
    }))
}

/// List proofs with optional filters
///
/// GET /api/v1/proofs
pub async fn list_proofs(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Query(params): Query<ListProofsQuery>,
) -> Result<Json<ListProofsResponse>> {
    let proofs = state.proof_store.list(params.proof_type).await;

    let mut filtered_proofs = proofs;

    // Filter by namespace: only show proofs whose submitter is in the namespace
    if let Some(axum::Extension(RequestNamespace(Some(ref ns)))) = ns_ext {
        filtered_proofs.retain(|p| {
            p.metadata
                .submitter
                .as_deref()
                .map(|s| is_in_namespace(s, ns))
                .unwrap_or(false)
        });
    }

    // Apply verified filter
    if let Some(verified) = params.verified {
        filtered_proofs.retain(|p| p.verified == verified);
    }

    // Apply limit
    let limit = params.limit.unwrap_or(100).min(1000);
    filtered_proofs.truncate(limit);

    let proofs_response: Vec<ProofResponse> = filtered_proofs
        .into_iter()
        .map(ProofResponse::from)
        .collect();

    Ok(Json(ListProofsResponse {
        count: proofs_response.len(),
        proofs: proofs_response,
    }))
}

/// Delete a proof
///
/// DELETE /api/v1/proofs/:id
pub async fn delete_proof(
    State(state): State<AppState>,
    ns_ext: Option<axum::Extension<RequestNamespace>>,
    Path(proof_id): Path<ProofId>,
) -> Result<Json<DeleteProofResponse>> {
    // Enforce namespace: verify the proof's submitter is in the namespace
    if let Some(axum::Extension(RequestNamespace(Some(ref ns)))) = ns_ext {
        if let Some(proof) = state.proof_store.get(&proof_id).await {
            if let Some(ref submitter) = proof.metadata.submitter {
                if !is_in_namespace(submitter, ns) {
                    return Err(Error::Forbidden(format!(
                        "Proof submitter is not in namespace \"{}\"",
                        ns
                    )));
                }
            }
        }
    }

    let deleted = state.proof_store.delete(&proof_id).await;

    if deleted {
        Ok(Json(DeleteProofResponse {
            proof_id,
            deleted: true,
        }))
    } else {
        Err(Error::NotFound(format!("Proof {} not found", proof_id)))
    }
}

/// Get proof statistics
///
/// GET /api/v1/proofs/stats
pub async fn get_proof_stats(State(state): State<AppState>) -> Result<Json<ProofStatsResponse>> {
    let stats = state.proof_store.stats().await;

    Ok(Json(ProofStatsResponse {
        total_proofs: stats.total_proofs,
        proofs_by_type: stats.proofs_by_type,
        total_verifications: stats.total_verifications,
        successful_verifications: stats.successful_verifications,
        failed_verifications: stats.failed_verifications,
        cache_hits: stats.cache_hits,
        cache_misses: stats.cache_misses,
        cache_hit_rate: if stats.cache_hits + stats.cache_misses > 0 {
            stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64
        } else {
            0.0
        },
        total_size_bytes: stats.total_size_bytes,
    }))
}

// Request/Response DTOs

#[derive(Debug, Serialize)]
pub struct SubmitProofResponse {
    pub proof_id: ProofId,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct BatchSubmitRequest {
    pub proofs: Vec<SubmitProofRequest>,
}

#[derive(Debug, Serialize)]
pub struct BatchSubmitResponse {
    pub successful_count: usize,
    pub failed_count: usize,
    pub successful: Vec<ProofId>,
    pub failed: Vec<BatchError>,
}

#[derive(Debug, Serialize)]
pub struct BatchError {
    pub index: usize,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct ProofResponse {
    pub id: ProofId,
    pub proof_type: ProofType,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub verified: bool,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: ProofMetadata,
    pub size_bytes: usize,
}

impl From<StoredProof> for ProofResponse {
    fn from(proof: StoredProof) -> Self {
        let size_bytes = proof.size_bytes();
        Self {
            id: proof.id,
            proof_type: proof.proof_type,
            created_at: proof.created_at,
            verified: proof.verified,
            verified_at: proof.verified_at,
            metadata: proof.metadata,
            size_bytes,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VerifyProofResponse {
    pub proof_id: ProofId,
    pub valid: bool,
    pub verified_at: chrono::DateTime<chrono::Utc>,
    pub details: Vec<String>,
    pub verification_time_us: u64,
}

#[derive(Debug, Deserialize)]
pub struct BatchVerifyRequest {
    pub proof_ids: Vec<ProofId>,
}

#[derive(Debug, Serialize)]
pub struct BatchVerifyResponse {
    pub total: usize,
    pub valid_count: usize,
    pub invalid_count: usize,
    pub verifications: Vec<VerifyProofResponse>,
}

#[derive(Debug, Deserialize)]
pub struct ListProofsQuery {
    pub proof_type: Option<ProofType>,
    pub verified: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ListProofsResponse {
    pub count: usize,
    pub proofs: Vec<ProofResponse>,
}

#[derive(Debug, Serialize)]
pub struct DeleteProofResponse {
    pub proof_id: ProofId,
    pub deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct ProofStatsResponse {
    pub total_proofs: usize,
    pub proofs_by_type: std::collections::HashMap<String, usize>,
    pub total_verifications: usize,
    pub successful_verifications: usize,
    pub failed_verifications: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub cache_hit_rate: f64,
    pub total_size_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::extract::State as AxumState;

    #[tokio::test]
    async fn test_submit_and_get_proof() {
        let state = AppState::new();

        let request = SubmitProofRequest {
            proof_type: ProofType::Knowledge,
            proof_data: serde_json::json!({
                "commitment": vec![0u8; 32],
                "challenge": vec![1u8; 32],
                "response": vec![2u8; 32],
            }),
            metadata: None,
        };

        let response = submit_proof(AxumState(state.clone()), None, Json(request))
            .await
            .unwrap();

        let proof_id = response.0.proof_id.clone();
        assert!(!proof_id.is_empty());

        let get_response = get_proof(AxumState(state), Path(proof_id)).await.unwrap();

        assert_eq!(get_response.0.proof_type, ProofType::Knowledge);
    }

    #[tokio::test]
    async fn test_list_proofs() {
        let state = AppState::new();

        // Submit multiple proofs
        for _ in 0..3 {
            let request = SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({"test": "data"}),
                metadata: None,
            };
            submit_proof(AxumState(state.clone()), None, Json(request))
                .await
                .unwrap();
        }

        let query = ListProofsQuery {
            proof_type: None,
            verified: None,
            limit: Some(10),
        };

        let response = list_proofs(AxumState(state), None, Query(query)).await.unwrap();

        assert_eq!(response.0.count, 3);
    }

    #[tokio::test]
    async fn test_proof_stats() {
        let state = AppState::new();

        let request = SubmitProofRequest {
            proof_type: ProofType::Equality,
            proof_data: serde_json::json!({"test": "data"}),
            metadata: None,
        };

        submit_proof(AxumState(state.clone()), None, Json(request))
            .await
            .unwrap();

        let response = get_proof_stats(AxumState(state)).await.unwrap();

        assert_eq!(response.0.total_proofs, 1);
    }
}
