// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Shared helpers for cluster-mode REST handlers.

use axum::http::HeaderMap;
use crate::error::Error;
use crate::state::AppState;

/// Convert a Raft `client_write` error into an appropriate HTTP error.
///
/// If the error is `ForwardToLeader` with a known leader address, returns
/// `Error::Redirect` so the client gets a 307 with the leader's URL.
pub fn handle_raft_write_error(
    e: openraft::error::RaftError<
        aingle_raft::CortexTypeConfig,
        openraft::error::ClientWriteError<aingle_raft::CortexTypeConfig>,
    >,
    _state: &AppState,
) -> Error {
    use openraft::error::{ClientWriteError, RaftError};

    match e {
        RaftError::APIError(api_err) => match api_err {
            ClientWriteError::ForwardToLeader(fwd) => {
                if let Some(leader_node) = fwd.leader_node {
                    Error::Redirect(format!("http://{}", leader_node.rest_addr))
                } else {
                    Error::Internal("Not leader; leader unknown".to_string())
                }
            }
            ClientWriteError::ChangeMembershipError(e) => {
                Error::Internal(format!("Membership change error: {e}"))
            }
        },
        RaftError::Fatal(f) => Error::Internal(format!("Raft fatal error: {f}")),
    }
}

/// Validate the `X-Cluster-Secret` header against the configured cluster secret.
///
/// Returns `Ok(())` if the secret matches or if no secret is configured.
/// Returns `Err(Error::AuthError)` if the secret is missing or incorrect.
pub fn validate_cluster_auth(headers: &HeaderMap, state: &AppState) -> Result<(), Error> {
    let expected = match &state.cluster_secret {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(()), // No secret configured — allow all
    };

    let provided = headers
        .get("x-cluster-secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected_bytes = expected.as_bytes();
    let provided_bytes = provided.as_bytes();
    // Constant-time comparison to prevent timing side-channel attacks.
    // Length check is not constant-time but doesn't leak the secret value.
    if expected_bytes.len() != provided_bytes.len()
        || subtle::ConstantTimeEq::ct_eq(expected_bytes, provided_bytes).unwrap_u8() != 1
    {
        return Err(Error::AuthError("Invalid or missing cluster secret".into()));
    }

    Ok(())
}
