// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Proof verification business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::rest::{GetProofRequest, ProofResponse, VerifyProofByIdRequest, VerifyProofResponse};
use crate::state::AppState;

/// Fetch a stored proof by its ID.
///
/// Semantics (preserved from the REST `GET /api/v1/proofs/:id` handler):
/// - Proof exists -> `Ok(ProofResponse)`.
/// - Proof does not exist -> `Err(Error::NotFound(..))`.
pub async fn get_proof(state: &AppState, req: GetProofRequest) -> Result<ProofResponse> {
    let proof_id = req.proof_id;

    let proof = state
        .proof_store
        .get(&proof_id)
        .await
        .ok_or_else(|| Error::NotFound(format!("Proof {} not found", proof_id)))?;

    Ok(ProofResponse::from(proof))
}

/// Verify a stored proof by its ID.
///
/// Semantics (preserved from commit 53cca2c, "proof verify endpoint returns
/// 200+valid:false instead of 422"):
/// - Proof exists and verifies cleanly -> `Ok(VerifyProofResponse { valid, .. })`.
/// - Proof exists but its data is malformed / fails verification at the ZK
///   layer -> `Ok(VerifyProofResponse { valid: false, .. })` with the error in
///   `details`. This is NOT an `Err`: verification answering "this proof is not
///   valid" is a successful answer, not a server error.
/// - Proof does not exist -> `Err(Error::NotFound(..))`.
pub async fn verify_proof(
    state: &AppState,
    req: VerifyProofByIdRequest,
) -> Result<VerifyProofResponse> {
    let proof_id = req.proof_id;

    match state.proof_store.verify(&proof_id).await {
        Ok(result) => Ok(VerifyProofResponse {
            proof_id: proof_id.clone(),
            valid: result.valid,
            verified_at: result.verified_at,
            details: result.details,
            verification_time_us: result.verification_time_us,
        }),
        Err(crate::proofs::VerificationError::ProofNotFound(_)) => {
            Err(Error::NotFound(format!("Proof {} not found", proof_id)))
        }
        Err(e) => {
            // Verification infrastructure error (bad proof data format, ZK error,
            // etc.) -> 200 with valid=false + error details instead of 422.
            Ok(VerifyProofResponse {
                proof_id: proof_id.clone(),
                valid: false,
                verified_at: chrono::Utc::now(),
                details: vec![format!("Verification error: {}", e)],
                verification_time_us: 0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proofs::{ProofType, SubmitProofRequest};

    #[tokio::test]
    async fn verifying_invalid_proof_returns_valid_false() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        // Submit a proof whose `proof_data` is structurally-valid JSON but is
        // NOT a parseable `aingle_zk::ZkProof` envelope. The proof therefore
        // EXISTS in the store (so we don't hit the ProofNotFound path), but the
        // verifier fails to deserialize it -> the service must return
        // Ok(valid: false), NOT Err.
        let proof_id = state
            .proof_store
            .submit(SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({ "garbage": "not-a-zk-proof" }),
                metadata: None,
            })
            .await
            .expect("submit should succeed; only verification is expected to fail");

        let req = VerifyProofByIdRequest {
            proof_id: proof_id.clone(),
        };

        let resp = verify_proof(&state, req)
            .await
            .expect("invalid proof must return Ok (200), not Err");
        assert!(!resp.valid, "bogus proof data must yield valid:false");
        assert_eq!(resp.proof_id, proof_id);
    }

    #[tokio::test]
    async fn getting_missing_proof_returns_not_found() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let req = GetProofRequest {
            proof_id: "does-not-exist".to_string(),
        };

        let err = get_proof(&state, req)
            .await
            .expect_err("missing proof must return Err(NotFound)");
        assert!(
            matches!(err, Error::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn getting_existing_proof_round_trips() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let proof_id = state
            .proof_store
            .submit(SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({ "some": "data" }),
                metadata: None,
            })
            .await
            .expect("submit should succeed");

        let resp = get_proof(
            &state,
            GetProofRequest {
                proof_id: proof_id.clone(),
            },
        )
        .await
        .expect("stored proof must be fetchable");

        assert_eq!(resp.id, proof_id);
        assert_eq!(resp.proof_type, ProofType::Schnorr);
    }
}
