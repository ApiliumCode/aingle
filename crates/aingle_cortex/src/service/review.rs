// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Review-inbox approvals as signed DAG actions.
//!
//! When a human APPROVES an agent-proposed note, we record a `review_approval`
//! DAG action signed with the node's key. This makes the curation decision
//! **cryptographically anchored** — who (the node), when (timestamp), and over
//! WHAT (the note's content hash) — so an approval can be verified later, not
//! merely asserted. This is Akashi's differentiator over a plain "human clicked
//! approve" inbox: approved knowledge carries verifiable provenance.

use crate::error::{Error, Result};
use crate::state::AppState;

/// A recorded, signed approval of a note (for the audit trail).
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ApprovalRecord {
    /// The approved note's vault-relative path.
    pub note_path: String,
    /// blake3 hex of the note content at approval time.
    pub content_hash: String,
    /// Who originally proposed it (e.g. `"mcp"`).
    pub source: String,
    /// RFC 3339 approval timestamp.
    pub approved_at: String,
    /// The signed action hash (Ed25519-signed), present only when signed.
    pub anchor: Option<String>,
}

/// The `payload_type` tag stamped on approval DAG actions.
pub const APPROVAL_PAYLOAD_TYPE: &str = "review_approval";

/// Record a signed `review_approval` DAG action for an approved note.
///
/// Returns the action anchor (blake3 hex of the signed action) on success, or
/// `None` if the graph has no DAG store (DAG disabled). Never partial: either the
/// action is persisted or an error is returned.
#[cfg(feature = "dag")]
pub async fn record_approval(
    state: &AppState,
    note_path: &str,
    content: &str,
    source: &str,
) -> Result<Option<String>> {
    let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
    let graph = state.graph.read().await;
    let Some(dag_store) = graph.dag_store() else {
        return Ok(None);
    };
    let author = state
        .dag_author
        .clone()
        .unwrap_or_else(|| aingle_graph::NodeId::named("node:local"));
    let seq = state
        .dag_seq_counter
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let parents = dag_store.tips().unwrap_or_default();
    let approved_at = chrono::Utc::now();

    let mut action = aingle_graph::dag::DagAction {
        parents,
        author,
        seq,
        timestamp: approved_at,
        payload: aingle_graph::dag::DagPayload::Custom {
            payload_type: APPROVAL_PAYLOAD_TYPE.to_string(),
            payload_summary: format!("Approved note: {note_path}"),
            payload: Some(serde_json::json!({
                "note_path": note_path,
                "content_hash": content_hash,
                "approved_at": approved_at.to_rfc3339(),
                "source": source,
            })),
            subject: Some(note_path.to_string()),
        },
        signature: None,
    };

    if let Some(ref key) = state.dag_signing_key {
        key.sign(&mut action);
    }
    let anchor = action.compute_hash().to_hex();
    let signed = action.signature.is_some();

    dag_store
        .put(&action)
        .map_err(|e| Error::Internal(format!("review approval DAG put failed: {e}")))?;

    Ok(signed.then_some(anchor))
}

/// List recorded approvals (newest first), for a verifiable audit trail.
#[cfg(feature = "dag")]
pub async fn list_approvals(state: &AppState, limit: usize) -> Vec<ApprovalRecord> {
    let graph = state.graph.read().await;
    let Some(dag_store) = graph.dag_store() else {
        return vec![];
    };
    // Approvals are authored by the node's identity; over-fetch its chain and
    // filter, since it also holds triple-insert actions.
    let author = state
        .dag_author
        .clone()
        .unwrap_or_else(|| aingle_graph::NodeId::named("node:local"));
    let chain = dag_store.chain(&author, limit.saturating_mul(8).max(64)).unwrap_or_default();
    let mut out: Vec<ApprovalRecord> = Vec::new();
    for action in chain {
        let aingle_graph::dag::DagPayload::Custom {
            payload_type,
            payload,
            subject,
            ..
        } = &action.payload
        else {
            continue;
        };
        if payload_type != APPROVAL_PAYLOAD_TYPE {
            continue;
        }
        let data = payload.as_ref();
        let get = |k: &str| {
            data.and_then(|v| v.get(k))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        let anchor = action.signature.as_ref().map(|_| action.compute_hash().to_hex());
        out.push(ApprovalRecord {
            note_path: subject.clone().or_else(|| get("note_path")).unwrap_or_default(),
            content_hash: get("content_hash").unwrap_or_default(),
            source: get("source").unwrap_or_else(|| "unknown".to_string()),
            approved_at: get("approved_at")
                .unwrap_or_else(|| action.timestamp.to_rfc3339()),
            anchor,
        });
    }
    out.sort_by(|a, b| b.approved_at.cmp(&a.approved_at));
    out.truncate(limit);
    out
}

#[cfg(all(test, feature = "dag"))]
mod tests {
    use super::*;

    async fn dag_state() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        state
    }

    #[tokio::test]
    async fn approval_is_signed_and_anchored_when_key_present() {
        // The differentiator: an approval carries a verifiable signature, not just
        // an assertion that "a human clicked approve".
        let mut state = dag_state().await;
        state.dag_signing_key = Some(std::sync::Arc::new(
            aingle_graph::dag::DagSigningKey::generate(),
        ));

        let content = "# Approved\n\nCurated knowledge.\n";
        let anchor = record_approval(&state, "notes/foo.md", content, "mcp")
            .await
            .unwrap()
            .expect("a signing key is present, so the approval must be anchored");
        assert_eq!(anchor.len(), 64, "anchor is a blake3 hex digest: {anchor}");

        let records = list_approvals(&state, 10).await;
        assert_eq!(records.len(), 1, "the approval must be listed: {records:?}");
        let r = &records[0];
        assert_eq!(r.note_path, "notes/foo.md");
        assert_eq!(r.source, "mcp");
        assert_eq!(
            r.content_hash,
            blake3::hash(content.as_bytes()).to_hex().to_string(),
            "content_hash must attest to the exact approved content"
        );
        assert_eq!(
            r.anchor.as_deref(),
            Some(anchor.as_str()),
            "the listed anchor must match the one returned at record time"
        );
    }

    #[tokio::test]
    async fn approval_persists_unsigned_when_no_key() {
        // Without a node key the decision is still recorded (audit trail), but it is
        // NOT anchored — the UI must be able to tell signed from unsigned.
        let state = dag_state().await; // no signing key set
        let out = record_approval(&state, "notes/bar.md", "body", "claude")
            .await
            .unwrap();
        assert!(out.is_none(), "no key ⇒ no verifiable anchor returned");

        let records = list_approvals(&state, 10).await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].note_path, "notes/bar.md");
        assert!(
            records[0].anchor.is_none(),
            "an unsigned approval must surface anchor = None"
        );
    }

    #[tokio::test]
    async fn approvals_are_newest_first_and_capped() {
        let mut state = dag_state().await;
        state.dag_signing_key = Some(std::sync::Arc::new(
            aingle_graph::dag::DagSigningKey::generate(),
        ));
        for i in 0..3 {
            record_approval(&state, &format!("notes/n{i}.md"), &format!("c{i}"), "mcp")
                .await
                .unwrap();
        }
        let all = list_approvals(&state, 10).await;
        assert_eq!(all.len(), 3);
        // Newest-first by approved_at (monotonic across the loop).
        assert!(
            all[0].approved_at >= all[1].approved_at && all[1].approved_at >= all[2].approved_at,
            "must be sorted newest-first: {all:?}"
        );
        let capped = list_approvals(&state, 2).await;
        assert_eq!(capped.len(), 2, "limit must cap the result set");
    }
}
