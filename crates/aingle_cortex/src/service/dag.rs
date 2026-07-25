// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG provenance business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::rest::dag::{
    action_to_dto, action_to_dto_verifiable, DagActionDto, DagStatsResponse, DagTipsResponse,
    PruneRequest, PruneResponse,
};
use crate::state::AppState;

/// Default action-history limit shared by REST and MCP endpoints.
pub(crate) const DEFAULT_HISTORY_LIMIT: usize = 50;

/// Return DAG actions affecting a subject, newest first, up to `limit`.
pub async fn history_by_subject(
    state: &AppState,
    subject: &str,
    limit: usize,
) -> Result<Vec<DagActionDto>> {
    let graph = state.graph.read().await;
    let actions = graph
        .dag_history_by_subject(subject, limit)
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(actions.iter().map(action_to_dto).collect())
}

/// Return the current DAG tip hashes and their count.
pub async fn tips(state: &AppState) -> Result<DagTipsResponse> {
    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let tips = dag_store
        .tips()
        .map_err(|e| Error::Internal(e.to_string()))?;
    let tip_strings: Vec<String> = tips.iter().map(|h| h.to_hex()).collect();
    let count = tip_strings.len();

    Ok(DagTipsResponse {
        tips: tip_strings,
        count,
    })
}

/// Fetch a single DAG action by its hex hash. `NotFound` if absent.
///
/// This is the verifiable lookup: the returned DTO carries the full
/// `verification` bundle for a signed action, so the caller can check the
/// signature itself instead of trusting this server's `signed` flag.
pub async fn action(state: &AppState, hash: &str) -> Result<DagActionDto> {
    let action_hash = aingle_graph::dag::DagActionHash::from_hex(hash)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid DAG action hash: {}", hash)))?;

    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let action = dag_store
        .get(&action_hash)
        .map_err(|e| Error::Internal(e.to_string()))?
        .ok_or_else(|| Error::NotFound(format!("DAG action {} not found", hash)))?;

    let node_key = state.dag_signing_key.as_ref().map(|k| k.verifying_key());
    Ok(action_to_dto_verifiable(&action, node_key.as_ref()))
}

/// Return an author's action chain, newest first, up to `limit`.
pub async fn chain(state: &AppState, author: &str, limit: usize) -> Result<Vec<DagActionDto>> {
    let author = aingle_graph::NodeId::named(author);

    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let actions = dag_store
        .chain(&author, limit)
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(actions.iter().map(action_to_dto).collect())
}

/// Return DAG statistics: action count and tip count.
pub async fn stats(state: &AppState) -> Result<DagStatsResponse> {
    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let action_count = dag_store.action_count();
    let tip_count = dag_store
        .tip_count()
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(DagStatsResponse {
        action_count,
        tip_count,
        // Published for out-of-band pinning: a client compares this against the
        // key offered with each signed action, so key substitution is visible.
        signing_public_key: state
            .dag_signing_key
            .as_ref()
            .map(|k| k.verifying_key().to_hex()),
    })
}

/// Prune the DAG according to a retention policy, optionally checkpointing.
pub async fn prune(state: &AppState, req: PruneRequest) -> Result<PruneResponse> {
    let policy = match req.policy.as_str() {
        "keep_all" => aingle_graph::dag::RetentionPolicy::KeepAll,
        "keep_since" => aingle_graph::dag::RetentionPolicy::KeepSince { seconds: req.value },
        "keep_last" => aingle_graph::dag::RetentionPolicy::KeepLast(req.value as usize),
        "keep_depth" => aingle_graph::dag::RetentionPolicy::KeepDepth(req.value as usize),
        other => return Err(Error::InvalidInput(format!("Unknown policy: {}", other))),
    };

    let graph = state.graph.read().await;
    let result = graph
        .dag_prune(&policy, req.create_checkpoint)
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(PruneResponse {
        pruned_count: result.pruned_count,
        retained_count: result.retained_count,
        checkpoint_hash: result.checkpoint_hash.map(|h| h.to_hex()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_of_unknown_subject_is_empty() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        // A fresh in-memory graph has no DAG store; `dag_history_by_subject`
        // returns a "DAG not enabled" error until the DAG is enabled.
        // Enable it the way the node does at startup, then query.
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let h = history_by_subject(&state, "ex:nobody", 10).await.unwrap();
        assert!(h.is_empty());
    }

    /// Enable the DAG on a fresh in-memory state, mirroring node startup.
    /// Without this, DAG service fns return `Error::Config("DAG not enabled")`.
    async fn enabled_state() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        state
    }

    #[tokio::test]
    async fn tips_of_empty_dag() {
        let state = enabled_state().await;
        let resp = tips(&state).await.unwrap();
        assert_eq!(resp.count, resp.tips.len());
    }

    #[tokio::test]
    async fn action_with_invalid_hash_is_invalid_input() {
        let state = enabled_state().await;
        let err = action(&state, "not-a-hash").await.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }

    #[tokio::test]
    async fn chain_of_unknown_author_is_empty() {
        let state = enabled_state().await;
        let c = chain(&state, "node:nobody", 10).await.unwrap();
        assert!(c.is_empty());
    }

    #[tokio::test]
    async fn stats_of_empty_dag() {
        let state = enabled_state().await;
        let s = stats(&state).await.unwrap();
        assert_eq!(s.action_count, 0);
    }

    #[tokio::test]
    async fn prune_keep_all_prunes_nothing() {
        let state = enabled_state().await;
        let resp = prune(
            &state,
            PruneRequest {
                policy: "keep_all".into(),
                value: 0,
                create_checkpoint: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.pruned_count, 0);
    }

    // ========================================================================
    // Independent verifiability
    //
    // These tests are the acceptance criterion for provenance: they rebuild the
    // signed bytes and check the signature using ONLY the JSON a client receives.
    // Nothing below the "from here on" marker may touch server state, the
    // `DagAction` type, or any aingle_graph helper — if these pass, a third party
    // holding just the response can reach the same conclusion.
    // ========================================================================

    /// Decode a lowercase hex string of exactly `N` bytes. Deliberately written
    /// out here (rather than reusing a crate helper) so the client-side half of
    /// these tests depends on nothing but the response JSON.
    fn unhex(s: &str, n: usize) -> Vec<u8> {
        assert_eq!(s.len(), n * 2, "expected {n} bytes of hex, got {s:?}");
        (0..n)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex digit"))
            .collect()
    }

    fn tohex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Rebuild the exact byte string that was hashed, from the published
    /// canonical parts alone, following the documented v1 layout.
    fn rebuild_preimage(canonical: &serde_json::Value) -> Vec<u8> {
        let mut pre: Vec<u8> = Vec::new();

        let parents = canonical["parents"].as_array().expect("canonical.parents");
        pre.extend_from_slice(&(parents.len() as u64).to_le_bytes());
        for p in parents {
            pre.extend_from_slice(&unhex(p.as_str().expect("parent hex"), 32));
        }

        let author = canonical["author_json"]
            .as_str()
            .expect("canonical.author_json")
            .as_bytes();
        pre.extend_from_slice(&(author.len() as u64).to_le_bytes());
        pre.extend_from_slice(author);

        pre.extend_from_slice(
            &canonical["seq"]
                .as_u64()
                .expect("canonical.seq")
                .to_le_bytes(),
        );

        pre.extend_from_slice(
            canonical["timestamp_rfc3339"]
                .as_str()
                .expect("canonical.timestamp_rfc3339")
                .as_bytes(),
        );

        let payload = canonical["payload_json"]
            .as_str()
            .expect("canonical.payload_json")
            .as_bytes();
        pre.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        pre.extend_from_slice(payload);

        pre
    }

    /// Write one signed action through the ordinary server path and return its hash.
    async fn put_signed_action(state: &AppState, subject: &str) -> String {
        let graph = state.graph.read().await;
        let store = graph.dag_store().unwrap();
        let parents = store.tips().unwrap();
        let mut a = aingle_graph::dag::DagAction {
            parents,
            author: aingle_graph::NodeId::named("node:1"),
            seq: 1,
            timestamp: chrono::Utc::now(),
            payload: aingle_graph::dag::DagPayload::TripleInsert {
                triples: vec![aingle_graph::dag::TripleInsertPayload {
                    subject: subject.into(),
                    predicate: "note:title".into(),
                    object: serde_json::json!("Quarterly plan"),
                    provenance: Some(aingle_graph::dag::Provenance {
                        source_path: subject.into(),
                        line_start: 1,
                        line_end: 4,
                        content_hash: "0f".repeat(32),
                    }),
                }],
            },
            signature: None,
        };
        state.dag_signing_key.as_ref().unwrap().sign(&mut a);
        store.put(&a).unwrap().to_hex()
    }

    async fn signing_state() -> AppState {
        let mut state = enabled_state().await;
        state.dag_signing_key = Some(std::sync::Arc::new(
            aingle_graph::dag::DagSigningKey::from_seed(&[7u8; 32]),
        ));
        state
    }

    #[tokio::test]
    async fn signed_action_dto_alone_lets_a_client_verify_the_signature() {
        let state = signing_state().await;
        let hash = put_signed_action(&state, "notes/plan.md").await;

        let dto = action(&state, &hash).await.unwrap();
        let json = serde_json::to_value(&dto).unwrap();

        // ------------------------------------------------------------------
        // From here on: ONLY `json`. No server state, no aingle_graph types.
        // ------------------------------------------------------------------
        assert_eq!(
            json["signature_status"], "signed",
            "a signed action must say so precisely: {json}"
        );
        let v = &json["verification"];
        assert!(
            !v.is_null(),
            "a signed action must publish everything needed to verify it: {json}"
        );
        assert_eq!(v["spec"], "aingle-dag-action-v1");
        assert_eq!(v["hash_alg"], "blake3-256");
        assert_eq!(v["signature_alg"], "ed25519");

        // 1. Rebuild the signed bytes byte-for-byte from the published parts.
        let preimage = rebuild_preimage(&v["canonical"]);

        // 2. The digest of those bytes MUST be the advertised action hash. This is
        //    what binds the canonical parts to the identity of the action.
        let digest = blake3::hash(&preimage);
        assert_eq!(
            tohex(digest.as_bytes()),
            json["hash"].as_str().unwrap(),
            "blake3 of the reconstructed preimage must equal the advertised hash"
        );

        // 3. Ed25519-verify the signature over the 32 raw digest bytes.
        let pk: [u8; 32] = unhex(v["public_key"].as_str().expect("public_key"), 32)
            .try_into()
            .unwrap();
        let sig: [u8; 64] = unhex(v["signature"].as_str().expect("signature"), 64)
            .try_into()
            .unwrap();
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&pk).expect("valid Ed25519 public key");
        ed25519_dalek::Verifier::verify(
            &vk,
            digest.as_bytes(),
            &ed25519_dalek::Signature::from_bytes(&sig),
        )
        .expect("the published signature must verify against the published key");

        // 4. The signed content must agree with the human-readable fields, or the
        //    signature is over something other than what is being displayed.
        let c = &v["canonical"];
        assert_eq!(c["seq"], json["seq"]);
        assert_eq!(c["timestamp_rfc3339"], json["timestamp"]);
        assert_eq!(c["parents"], json["parents"]);
        assert!(
            c["payload_json"]
                .as_str()
                .unwrap()
                .contains("notes/plan.md"),
            "the signed payload must actually contain the cited source"
        );
    }

    #[tokio::test]
    async fn tampering_with_the_canonical_payload_breaks_the_hash() {
        // The negative control: if the reconstruction did not really depend on the
        // signed bytes, the test above would prove nothing.
        let state = signing_state().await;
        let hash = put_signed_action(&state, "notes/plan.md").await;
        let json = serde_json::to_value(action(&state, &hash).await.unwrap()).unwrap();

        let mut canonical = json["verification"]["canonical"].clone();
        let tampered = canonical["payload_json"]
            .as_str()
            .unwrap()
            .replace("Quarterly plan", "Quarterly plun");
        canonical["payload_json"] = serde_json::json!(tampered);

        let digest = blake3::hash(&rebuild_preimage(&canonical));
        assert_ne!(
            tohex(digest.as_bytes()),
            json["hash"].as_str().unwrap(),
            "a one-character edit to the signed payload must change the hash"
        );
    }

    #[tokio::test]
    async fn genesis_is_reported_as_unsigned_by_design_not_merely_unsigned() {
        let state = enabled_state().await;
        let genesis_hash = {
            let graph = state.graph.read().await;
            graph
                .dag_store()
                .unwrap()
                .init_or_migrate(0)
                .unwrap()
                .to_hex()
        };

        let json = serde_json::to_value(action(&state, &genesis_hash).await.unwrap()).unwrap();
        assert_eq!(
            json["signature_status"], "unsigned_by_design",
            "the genesis action is deliberately unsigned so every node agrees on \
             the initial hash — that is not the same as a missing signature: {json}"
        );
        assert!(
            json["verification"].is_null(),
            "there is nothing to verify on an unsigned action"
        );
        assert_eq!(json["signed"], false, "`signed` keeps its original meaning");
    }

    #[tokio::test]
    async fn an_action_written_without_a_key_is_reported_as_unsigned() {
        let state = enabled_state().await; // no signing key
        let hash = {
            let graph = state.graph.read().await;
            let store = graph.dag_store().unwrap();
            let parents = store.tips().unwrap();
            let a = aingle_graph::dag::DagAction {
                parents,
                author: aingle_graph::NodeId::named("node:1"),
                seq: 1,
                timestamp: chrono::Utc::now(),
                payload: aingle_graph::dag::DagPayload::Noop,
                signature: None,
            };
            store.put(&a).unwrap().to_hex()
        };

        let json = serde_json::to_value(action(&state, &hash).await.unwrap()).unwrap();
        assert_eq!(
            json["signature_status"], "unsigned",
            "a non-genesis action with no signature is plainly unsigned: {json}"
        );
        assert!(json["verification"].is_null());
    }

    #[tokio::test]
    async fn prune_unknown_policy_is_invalid_input() {
        let state = enabled_state().await;
        let err = prune(
            &state,
            PruneRequest {
                policy: "bogus".into(),
                value: 0,
                create_checkpoint: false,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }
}
