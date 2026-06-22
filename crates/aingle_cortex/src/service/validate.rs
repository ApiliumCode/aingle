// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Triple validation business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::middleware::is_in_namespace;
use crate::rest::{
    TripleDto, TripleValidationResult, ValidateRequest, ValidateResponse, ValidationMessage,
};
use crate::state::{AppState, Event};
use aingle_graph::{NodeId, Predicate, Triple, Value};

/// Validate triple(s) against the logic engine.
///
/// Semantics preserved from the REST `POST /api/v1/validate` handler: each input
/// triple is run through the PoL logic engine and reported with per-triple
/// validity + messages. A `proof_hash` is generated only when every triple is
/// valid, and a `ValidationCompleted` event is broadcast in that case (matching
/// the handler's side-effect). Validation answering "this triple is invalid" is
/// a successful response (`valid:false`), NOT an error.
///
/// `namespace` enforces that input subjects fall within the request namespace;
/// REST passes the request namespace, MCP passes `None` (no namespace
/// enforcement). An out-of-namespace subject yields `Err(Error::Forbidden(..))`,
/// exactly as the REST handler does.
pub async fn validate_triples(
    state: &AppState,
    req: ValidateRequest,
    namespace: Option<String>,
) -> Result<ValidateResponse> {
    let logic = state.logic.read().await;

    let ns_filter = namespace;

    let mut results = Vec::new();
    let mut all_valid = true;

    for input in req.triples {
        // Enforce namespace on input subjects.
        if let Some(ref ns) = ns_filter {
            if !is_in_namespace(&input.subject, ns) {
                return Err(Error::Forbidden(format!(
                    "Subject \"{}\" is not in namespace \"{}\"",
                    input.subject, ns
                )));
            }
        }
        let object: Value = input.object.clone().into();

        // Create a triple for validation.
        let triple = Triple::new(
            NodeId::named(&input.subject),
            Predicate::named(&input.predicate),
            object,
        );

        // Validate using logic engine.
        let validation = logic.validate(&triple);

        let valid = validation.is_valid();
        if !valid {
            all_valid = false;
        }

        // Convert messages.
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

    drop(logic);

    // Generate a simple proof hash if all valid.
    let proof_hash = if all_valid {
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

    // Broadcast validation event (same side-effect as the REST handler).
    if let Some(ref hash) = proof_hash {
        state.broadcaster.broadcast(Event::ValidationCompleted {
            hash: hash.clone(),
            valid: all_valid,
            proof_hash: proof_hash.clone(),
        });
    }

    Ok(ValidateResponse {
        valid: all_valid,
        results,
        proof_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::{ValidateTripleInput, ValueDto};

    #[tokio::test]
    async fn validate_minimal_triple_returns_per_triple_result() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let req = ValidateRequest {
            triples: vec![ValidateTripleInput {
                subject: "ex:alice".to_string(),
                predicate: "ex:knows".to_string(),
                object: ValueDto::Node {
                    node: "ex:bob".to_string(),
                },
            }],
            rule_set: None,
        };

        let resp = validate_triples(&state, req, None)
            .await
            .expect("validation must return Ok for a well-formed triple");

        // One input => one per-triple result.
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].triple.subject, "ex:alice");
        assert_eq!(resp.results[0].triple.predicate, "ex:knows");
        // With no rules loaded, a plain triple validates and a proof hash is
        // produced (all_valid => Some).
        assert_eq!(resp.valid, resp.proof_hash.is_some());
    }

    #[tokio::test]
    async fn validate_empty_request_is_valid_with_proof_hash() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let req = ValidateRequest {
            triples: vec![],
            rule_set: None,
        };

        let resp = validate_triples(&state, req, None)
            .await
            .expect("empty validation must return Ok");
        // Vacuously valid: no triples failed, so all_valid stays true and a
        // (degenerate) proof hash is generated.
        assert!(resp.valid);
        assert!(resp.results.is_empty());
        assert!(resp.proof_hash.is_some());
    }
}
