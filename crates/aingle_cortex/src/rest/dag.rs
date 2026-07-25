// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG introspection REST endpoints.
//!
//! ## Endpoints
//!
//! - `GET /api/v1/dag/tips` — Current DAG tip hashes and count
//! - `GET /api/v1/dag/action/:hash` — Single DagAction by hash, **with the
//!   verification bundle** (signature, public key, canonical signed bytes)
//! - `GET /api/v1/dag/history` — Mutations affecting a subject
//! - `GET /api/v1/dag/chain` — Author's action chain
//! - `GET /api/v1/dag/stats` — Action count, tip count, node signing public key
//!
//! ## Verifiable vs. asserted
//!
//! `GET /api/v1/dag/verify/:hash` has this server check a signature and report
//! the verdict — convenient, but the verdict is still this server's word. The
//! independent path is `GET /api/v1/dag/action/:hash`: it returns
//! [`ActionVerificationDto`], from which a client rebuilds the signed bytes and
//! checks the Ed25519 signature itself. Everything else this module returns
//! about signing (`signed`, `signature_status`) is an assertion, and is
//! documented as such on the fields.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::state::AppState;

// ============================================================================
// DTOs
// ============================================================================

#[derive(Debug, Serialize)]
pub struct DagTipsResponse {
    pub tips: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct DagActionDto {
    pub hash: String,
    pub parents: Vec<String>,
    /// Human-readable rendering of the author (e.g. `<node:1>`). This is a
    /// display form, **not** the bytes that were signed — those are in
    /// `verification.canonical.author_json`.
    pub author: String,
    pub seq: u64,
    pub timestamp: String,
    /// Derived label for the payload kind. Computed by this server for display;
    /// it is not part of the signed record.
    pub payload_type: String,
    /// Derived one-line summary of the payload. Computed by this server for
    /// display; it is not part of the signed record. The signed record is
    /// `verification.canonical.payload_json`.
    pub payload_summary: String,
    /// Whether this action carries a signature.
    ///
    /// **This is an assertion by the server that serves the data, not proof.**
    /// It is retained for existing clients and keeps its original meaning
    /// (`true` ⇔ a signature is attached), but a client that needs proof must
    /// verify it: read `verification` and follow `verification.procedure`.
    /// Presenting `signed: true` to a user as "verified" is not warranted —
    /// only a completed signature check is. Prefer `signature_status`, which
    /// additionally distinguishes deliberately-unsigned actions.
    pub signed: bool,
    /// Precise signature state. Three values, never to be collapsed:
    ///
    /// - `"signed"` — a signature is attached; `verification` carries everything
    ///   needed to check it.
    /// - `"unsigned_by_design"` — the genesis action, which is deliberately
    ///   unsigned so that every node computes the same initial hash. Its absence
    ///   of a signature is a design property, not a gap.
    /// - `"unsigned"` — no signature and no design reason for that. Treat the
    ///   action's content as unattested.
    pub signature_status: String,
    /// Blake3 hex content hash of the source file, if present in the action's
    /// provenance. Extracted from the first provenanced triple in a
    /// `TripleInsert` (or the first `TripleInsert` inside a `Batch`).
    pub content_hash: Option<String>,
    /// Everything required to verify this action's signature **without trusting
    /// this server**: the signature bytes, the public key, and the literal
    /// inputs that reproduce the signed bytes byte-for-byte.
    ///
    /// Present only for `signature_status == "signed"`, and only on the
    /// single-action lookup (`GET /api/v1/dag/action/{hash}`, MCP
    /// `aingle_dag_action`). List responses omit it because the signed payload
    /// can be large; fetch an action by hash to obtain its proof.
    pub verification: Option<ActionVerificationDto>,
}

/// The verification bundle for a signed DAG action.
///
/// The point of this struct is that a client can reach a verdict from it alone.
/// It publishes the signature, the key, and — critically — the literal inputs to
/// the hash, because the hash preimage is a byte concatenation that cannot be
/// recovered from a re-encoded view of the action.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct ActionVerificationDto {
    /// Identifier of the canonical hashing/signing scheme (`aingle-dag-action-v1`).
    /// A client that does not implement this exact scheme must report "cannot
    /// verify" rather than guess.
    pub spec: String,
    /// Digest used for the action hash (`blake3-256`).
    pub hash_alg: String,
    /// Signature scheme (`ed25519`).
    pub signature_alg: String,
    /// What the signature covers: `action_hash_bytes` — the 32 **raw** bytes of
    /// the blake3 digest. Not the preimage, and not the hex string.
    pub signed_message: String,
    /// The signature, lowercase hex, 128 characters (64 bytes).
    pub signature: String,
    /// The Ed25519 public key, lowercase hex, 64 characters (32 bytes).
    ///
    /// `None` when this node does not hold a key that verifies this action —
    /// typically an action authored by a different node and replicated here.
    /// The signature is still published so a client holding the author's key can
    /// verify it independently.
    pub public_key: Option<String>,
    /// Stable identifier for the key: the hex public key itself, so it is
    /// self-describing and comparable across responses and restarts.
    pub key_id: Option<String>,
    /// Where `public_key` came from:
    ///
    /// - `"local_node_key"` — this node's own signing key, which was checked
    ///   against this signature before being published.
    /// - `"unknown_author"` — no key available here; verification requires the
    ///   author's key obtained elsewhere.
    pub public_key_source: String,
    /// The literal values that were hashed. Concatenate them per `procedure` to
    /// rebuild the signed bytes exactly.
    pub canonical: CanonicalActionDto,
    /// The verification procedure, spelled out step by step so a client (or an
    /// assistant acting for one) can execute it and state what it did.
    pub procedure: Vec<String>,
}

/// The exact inputs to a DAG action's hash, in publishable form.
///
/// Every field is the literal value that went into the digest. See
/// `ActionVerificationDto::procedure` for the concatenation order.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct CanonicalActionDto {
    /// Parent action hashes, lowercase hex, in the order they were hashed.
    pub parents: Vec<String>,
    /// The author field as JSON (e.g. `{"Named":"node:1"}`) — these bytes were
    /// hashed, unlike the display form in `DagActionDto::author`.
    pub author_json: String,
    /// Per-author sequence number.
    pub seq: u64,
    /// The timestamp in the one textual rendering that was hashed (RFC 3339).
    /// Equal to `DagActionDto::timestamp`.
    pub timestamp_rfc3339: String,
    /// The payload as JSON. **This is the signed record of what changed** — the
    /// claim a signature actually attests to. Check your citation against this,
    /// not against `payload_summary`.
    pub payload_json: String,
}

#[derive(Debug, Serialize)]
pub struct DagStatsResponse {
    pub action_count: usize,
    pub tip_count: usize,
    /// This node's Ed25519 signing public key, lowercase hex.
    ///
    /// Published so a client can pin it out of band and compare it against the
    /// key offered with each signed action. Pinning is what turns a signature
    /// check into evidence about a *specific* signer: a server that substitutes
    /// its own key can produce signatures that verify but attest to nothing.
    /// `None` when this node has no signing key (it cannot sign).
    pub signing_public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub subject: Option<String>,
    pub triple_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ChainQuery {
    pub author: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct PruneRequest {
    /// "keep_all", "keep_since", "keep_last", or "keep_depth"
    pub policy: String,
    /// The numeric argument for the policy (seconds / count / depth).
    #[serde(default)]
    pub value: u64,
    /// Whether to create a Compact checkpoint action after pruning.
    #[serde(default)]
    pub create_checkpoint: bool,
}

#[derive(Debug, Serialize)]
pub struct PruneResponse {
    pub pruned_count: usize,
    pub retained_count: usize,
    pub checkpoint_hash: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TimeTravelResponse {
    pub target_hash: String,
    pub target_timestamp: String,
    pub actions_replayed: usize,
    pub triple_count: usize,
    pub triples: Vec<TimeTravelTriple>,
}

#[derive(Debug, Serialize)]
pub struct TimeTravelTriple {
    pub subject: String,
    pub predicate: String,
    pub object: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct PullRequest {
    /// The peer URL to pull from (e.g. "http://node2:19090").
    pub peer_url: String,
}

#[derive(Debug, Serialize)]
pub struct PullResponse {
    pub ingested: usize,
    pub already_had: usize,
    pub remote_tips: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DiffResponse {
    pub from: String,
    pub to: String,
    pub action_count: usize,
    pub actions: Vec<DagActionDto>,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// "dot", "mermaid", or "json" (default: "json").
    #[serde(default = "default_export_format")]
    pub format: String,
}

fn default_export_format() -> String {
    "json".into()
}

#[cfg(feature = "dag")]
#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    /// Hex-encoded Ed25519 public key (64 chars).
    pub public_key: String,
}

/// Request body for POST /api/v1/dag/actions.
#[derive(Debug, Deserialize)]
pub struct CreateDagActionRequest {
    /// Author identity. Defaults to the node's configured DAG author.
    pub author: Option<String>,
    /// A descriptive type tag (e.g., "checkpoint", "decision", "annotation").
    pub payload_type: String,
    /// A human-readable summary.
    pub payload_summary: String,
    /// Optional arbitrary payload data.
    pub payload: Option<serde_json::Value>,
    /// Optional subject for indexing in DAG history.
    pub subject: Option<String>,
    /// Whether to sign the action. Defaults to true if a signing key is configured.
    pub sign: Option<bool>,
}

/// Response for POST /api/v1/dag/actions.
#[derive(Debug, Serialize)]
pub struct CreateDagActionResponse {
    pub hash: String,
    pub seq: u64,
    pub timestamp: String,
    pub signed: bool,
}

fn default_limit() -> usize {
    crate::service::dag::DEFAULT_HISTORY_LIMIT
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/v1/dag/tips
pub async fn get_dag_tips(State(state): State<AppState>) -> Result<Json<DagTipsResponse>> {
    Ok(Json(crate::service::dag::tips(&state).await?))
}

/// GET /api/v1/dag/action/:hash
pub async fn get_dag_action(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<DagActionDto>> {
    Ok(Json(crate::service::dag::action(&state, &hash).await?))
}

/// GET /api/v1/dag/history?subject=X&triple_id=X&limit=N
pub async fn get_dag_history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<DagActionDto>>> {
    // Subject-based lookup uses the dedicated subject index (shared service logic)
    if let Some(ref subject) = query.subject {
        let actions = crate::service::dag::history_by_subject(&state, subject, query.limit).await?;
        return Ok(Json(actions));
    }

    let graph = state.graph.read().await;

    // Triple-ID-based lookup uses the affected index
    if let Some(ref tid_hex) = query.triple_id {
        let bytes = parse_hex32(tid_hex)
            .ok_or_else(|| Error::InvalidInput("triple_id must be 64 valid hex chars".into()))?;

        let actions = graph
            .dag_history(&bytes, query.limit)
            .map_err(|e| Error::Internal(e.to_string()))?;
        return Ok(Json(actions.iter().map(action_to_dto).collect()));
    }

    Err(Error::InvalidInput(
        "Either 'subject' or 'triple_id' query parameter is required".into(),
    ))
}

/// GET /api/v1/dag/chain?author=X&limit=N
pub async fn get_dag_chain(
    State(state): State<AppState>,
    Query(query): Query<ChainQuery>,
) -> Result<Json<Vec<DagActionDto>>> {
    Ok(Json(
        crate::service::dag::chain(&state, &query.author, query.limit).await?,
    ))
}

/// GET /api/v1/dag/stats
pub async fn get_dag_stats(State(state): State<AppState>) -> Result<Json<DagStatsResponse>> {
    Ok(Json(crate::service::dag::stats(&state).await?))
}

/// POST /api/v1/dag/prune
pub async fn post_dag_prune(
    State(state): State<AppState>,
    Json(req): Json<PruneRequest>,
) -> Result<Json<PruneResponse>> {
    Ok(Json(crate::service::dag::prune(&state, req).await?))
}

/// GET /api/v1/dag/export?format=dot|mermaid|json
pub async fn get_dag_export(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;

    let format = aingle_graph::dag::ExportFormat::from_str(&query.format).ok_or_else(|| {
        Error::InvalidInput(format!(
            "Unknown format '{}'. Use: dot, mermaid, json",
            query.format
        ))
    })?;

    let graph = state.graph.read().await;
    let dag_graph = graph
        .dag_export()
        .map_err(|e| Error::Internal(e.to_string()))?;

    let body = dag_graph
        .export(format)
        .map_err(|e| Error::Internal(e.to_string()))?;

    let content_type = match format {
        aingle_graph::dag::ExportFormat::Dot => "text/vnd.graphviz",
        aingle_graph::dag::ExportFormat::Mermaid => "text/plain",
        aingle_graph::dag::ExportFormat::Json => "application/json",
    };

    Ok(([(axum::http::header::CONTENT_TYPE, content_type)], body).into_response())
}

/// GET /api/v1/dag/verify/:hash?public_key=X — verify an action's Ed25519 signature
#[cfg(feature = "dag")]
pub async fn get_dag_verify(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Query(query): Query<VerifyQuery>,
) -> Result<Json<aingle_graph::dag::VerifyResult>> {
    let action_hash = aingle_graph::dag::DagActionHash::from_hex(&hash)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid hash: {}", hash)))?;

    let pk_bytes = parse_hex32(&query.public_key)
        .ok_or_else(|| Error::InvalidInput("public_key must be 64 valid hex chars".into()))?;

    let graph = state.graph.read().await;
    let action = graph
        .dag_action(&action_hash)
        .map_err(|e| Error::Internal(e.to_string()))?
        .ok_or_else(|| Error::NotFound(format!("DAG action {} not found", hash)))?;

    let result = graph
        .dag_verify(&action, &pk_bytes)
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(Json(result))
}

/// POST /api/v1/dag/sync — serve missing actions to a peer
pub async fn post_dag_sync(
    State(state): State<AppState>,
    Json(req): Json<aingle_graph::dag::SyncRequest>,
) -> Result<Json<aingle_graph::dag::SyncResponse>> {
    let graph = state.graph.read().await;

    let actions = if !req.want.is_empty() {
        // Serve specific requested actions
        let dag_store = graph
            .dag_store()
            .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;
        req.want
            .iter()
            .filter_map(|h| dag_store.get(h).ok().flatten())
            .collect()
    } else {
        // Compute what the requester is missing
        graph
            .dag_compute_missing(&req.local_tips)
            .map_err(|e| Error::Internal(e.to_string()))?
    };

    let tips = graph
        .dag_tips()
        .map_err(|e| Error::Internal(e.to_string()))?;

    let action_count = actions.len();

    Ok(Json(aingle_graph::dag::SyncResponse {
        actions,
        remote_tips: tips,
        action_count,
    }))
}

/// POST /api/v1/dag/sync/pull — pull missing DAG actions from a peer
pub async fn post_dag_pull(
    State(state): State<AppState>,
    Json(req): Json<PullRequest>,
) -> Result<Json<PullResponse>> {
    // Read our current tips
    let local_tips = {
        let graph = state.graph.read().await;
        graph
            .dag_tips()
            .map_err(|e| Error::Internal(e.to_string()))?
    };

    // Send sync request to peer
    let sync_req = aingle_graph::dag::SyncRequest {
        local_tips,
        want: vec![],
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Internal(format!("HTTP client error: {}", e)))?;

    let url = format!("{}/api/v1/dag/sync", req.peer_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&sync_req)
        .send()
        .await
        .map_err(|e| Error::Internal(format!("Failed to contact peer: {}", e)))?;

    if !resp.status().is_success() {
        return Err(Error::Internal(format!(
            "Peer returned status {}",
            resp.status()
        )));
    }

    let sync_resp: aingle_graph::dag::SyncResponse = resp
        .json()
        .await
        .map_err(|e| Error::Internal(format!("Invalid peer response: {}", e)))?;

    // Ingest received actions
    let graph = state.graph.read().await;
    let mut ingested = 0;
    let mut already_had = 0;

    for action in &sync_resp.actions {
        let hash = action.compute_hash();
        let dag_store = graph
            .dag_store()
            .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

        if dag_store
            .contains(&hash)
            .map_err(|e| Error::Internal(e.to_string()))?
        {
            already_had += 1;
        } else {
            graph
                .dag_ingest(action)
                .map_err(|e| Error::Internal(e.to_string()))?;
            ingested += 1;
        }
    }

    Ok(Json(PullResponse {
        ingested,
        already_had,
        remote_tips: sync_resp.remote_tips.iter().map(|h| h.to_hex()).collect(),
    }))
}

/// GET /api/v1/dag/at/:hash — reconstruct graph state at a specific DAG action
pub async fn get_dag_at(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<TimeTravelResponse>> {
    let action_hash = aingle_graph::dag::DagActionHash::from_hex(&hash)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid DAG action hash: {}", hash)))?;

    let graph = state.graph.read().await;
    let (snapshot_db, info) = graph
        .dag_at(&action_hash)
        .map_err(|e| Error::Internal(e.to_string()))?;

    let triples = snapshot_db
        .find(aingle_graph::TriplePattern::any())
        .map_err(|e| Error::Internal(e.to_string()))?
        .into_iter()
        .map(|t| TimeTravelTriple {
            subject: t.subject.to_string(),
            predicate: t.predicate.to_string(),
            object: triple_value_to_json(&t.object),
        })
        .collect();

    Ok(Json(TimeTravelResponse {
        target_hash: info.target_hash.to_hex(),
        target_timestamp: info.target_timestamp.to_rfc3339(),
        actions_replayed: info.actions_replayed,
        triple_count: info.triple_count,
        triples,
    }))
}

/// GET /api/v1/dag/diff?from=X&to=Y — actions between two DAG points
pub async fn get_dag_diff(
    State(state): State<AppState>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<DiffResponse>> {
    let from = aingle_graph::dag::DagActionHash::from_hex(&query.from)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid 'from' hash: {}", query.from)))?;
    let to = aingle_graph::dag::DagActionHash::from_hex(&query.to)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid 'to' hash: {}", query.to)))?;

    let graph = state.graph.read().await;
    let diff = graph
        .dag_diff(&from, &to)
        .map_err(|e| Error::Internal(e.to_string()))?;

    let actions: Vec<DagActionDto> = diff.actions.iter().map(action_to_dto).collect();
    let action_count = actions.len();

    Ok(Json(DiffResponse {
        from: query.from,
        to: query.to,
        action_count,
        actions,
    }))
}

/// POST /api/v1/dag/actions — create an explicit DAG action with arbitrary payload
pub async fn post_create_dag_action(
    State(state): State<AppState>,
    Json(req): Json<CreateDagActionRequest>,
) -> Result<(axum::http::StatusCode, Json<CreateDagActionResponse>)> {
    if req.payload_type.is_empty() {
        return Err(Error::InvalidInput("payload_type cannot be empty".into()));
    }

    let dag_author = if let Some(ref author) = req.author {
        aingle_graph::NodeId::named(author)
    } else {
        state
            .dag_author
            .clone()
            .unwrap_or_else(|| aingle_graph::NodeId::named("node:local"))
    };

    let dag_seq = state
        .dag_seq_counter
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let parents = dag_store
        .tips()
        .map_err(|e| Error::Internal(e.to_string()))?;

    let timestamp = chrono::Utc::now();
    let mut action = aingle_graph::dag::DagAction {
        parents,
        author: dag_author,
        seq: dag_seq,
        timestamp,
        payload: aingle_graph::dag::DagPayload::Custom {
            payload_type: req.payload_type,
            payload_summary: req.payload_summary,
            payload: req.payload,
            subject: req.subject,
        },
        signature: None,
    };

    // Sign unless explicitly disabled
    let should_sign = req.sign.unwrap_or(true);
    if should_sign {
        if let Some(ref key) = state.dag_signing_key {
            key.sign(&mut action);
        }
    }

    let signed = action.signature.is_some();
    let hash = dag_store
        .put(&action)
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateDagActionResponse {
            hash: hash.to_hex(),
            seq: dag_seq,
            timestamp: timestamp.to_rfc3339(),
            signed,
        }),
    ))
}

// ============================================================================
// Router
// ============================================================================

pub fn dag_router() -> Router<AppState> {
    let router = Router::new()
        .route("/api/v1/dag/tips", get(get_dag_tips))
        .route("/api/v1/dag/action/{hash}", get(get_dag_action))
        .route("/api/v1/dag/history", get(get_dag_history))
        .route("/api/v1/dag/chain", get(get_dag_chain))
        .route("/api/v1/dag/stats", get(get_dag_stats))
        .route("/api/v1/dag/prune", post(post_dag_prune))
        .route("/api/v1/dag/at/{hash}", get(get_dag_at))
        .route("/api/v1/dag/diff", get(get_dag_diff))
        .route("/api/v1/dag/export", get(get_dag_export))
        .route("/api/v1/dag/sync", post(post_dag_sync))
        .route("/api/v1/dag/sync/pull", post(post_dag_pull))
        .route("/api/v1/dag/actions", post(post_create_dag_action));

    #[cfg(feature = "dag")]
    let router = router.route("/api/v1/dag/verify/{hash}", get(get_dag_verify));

    router
}

// ============================================================================
// Helpers
// ============================================================================

/// Signature state string for `"signed"`.
pub(crate) const SIG_SIGNED: &str = "signed";
/// Signature state string for the deliberately-unsigned genesis action.
pub(crate) const SIG_UNSIGNED_BY_DESIGN: &str = "unsigned_by_design";
/// Signature state string for an action that simply has no signature.
pub(crate) const SIG_UNSIGNED: &str = "unsigned";

/// Lowercase-hex encode a byte slice.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The step-by-step verification procedure published with every signed action.
///
/// Written for a reader who has nothing but the response: it names the exact
/// byte layout, the order of operations, and — just as importantly — the checks
/// that a successful signature does *not* cover.
fn verification_procedure() -> Vec<String> {
    [
        "1. Build the signed bytes from `canonical`, concatenating with no separators \
         and no padding, in this exact order: (a) the number of parents as a u64 \
         little-endian; (b) each entry of `canonical.parents` decoded from hex to its \
         32 raw bytes, in order; (c) the UTF-8 byte length of `canonical.author_json` \
         as a u64 little-endian; (d) `canonical.author_json` as UTF-8; (e) \
         `canonical.seq` as a u64 little-endian; (f) `canonical.timestamp_rfc3339` as \
         UTF-8, with NO length prefix; (g) the UTF-8 byte length of \
         `canonical.payload_json` as a u64 little-endian; (h) `canonical.payload_json` \
         as UTF-8.",
        "2. Compute blake3-256 over those bytes. The lowercase hex of the 32-byte \
         digest MUST equal the action's `hash`. If it does not, the canonical parts \
         do not describe the action being served: STOP, verification has failed.",
        "3. Ed25519-verify `signature` (hex, 64 bytes) over the 32 RAW digest bytes \
         from step 2 — not over the preimage, and not over the hex string — using \
         `public_key` (hex, 32 bytes). If `public_key` is null, this node does not \
         hold the author's key: you can still verify with the key obtained \
         elsewhere, otherwise report that you could not verify.",
        "4. Check the signed content against what you are about to assert. \
         `canonical.payload_json` is the signed record; `payload_type`, \
         `payload_summary` and `author` are display fields computed by the server \
         and are NOT covered by the signature. `seq`, `timestamp` and `parents` must \
         equal their `canonical` counterparts.",
        "5. Pin `public_key` the first time you see it and compare it on every later \
         response. A signature only proves that whoever holds THAT key signed those \
         bytes; a server that substitutes a key of its own produces a signature that \
         verifies and attests to nothing. The node's key is also published as \
         `signing_public_key` on the DAG stats endpoint/tool for out-of-band pinning.",
        "6. Report what you actually did. If steps 1-3 pass, you may say the action's \
         content is signed by the pinned key. If any step could not be completed, say \
         so — do not fall back to the `signed` boolean, which is only this server's \
         own claim.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Convert a raw [`DagAction`] to its serializable DTO form, **without** the
/// verification bundle.
///
/// Used by the list-shaped endpoints (history, chain, diff), where attaching the
/// full signed payload of every action would multiply the response size. Those
/// responses still carry `hash` and `signature_status`; a client that wants proof
/// fetches the action by hash.
pub(crate) fn action_to_dto(action: &aingle_graph::dag::DagAction) -> DagActionDto {
    let hash = action.compute_hash().to_hex();
    let parents: Vec<String> = action.parents.iter().map(|h| h.to_hex()).collect();

    let (payload_type, payload_summary, content_hash) = match &action.payload {
        aingle_graph::dag::DagPayload::TripleInsert { triples } => {
            let summary = if triples.len() == 1 {
                let t = &triples[0];
                format!("{} -> {} -> {}", t.subject, t.predicate, t.object)
            } else {
                format!("{} triple(s)", triples.len())
            };
            // Extract content_hash from the first triple that carries provenance.
            // All triples from a single file ingest share the same content_hash.
            let content_hash = triples
                .iter()
                .find_map(|t| t.provenance.as_ref().map(|p| p.content_hash.clone()));
            ("triple:create".to_string(), summary, content_hash)
        }
        aingle_graph::dag::DagPayload::TripleDelete {
            triple_ids,
            subjects,
        } => {
            let summary = if !subjects.is_empty() {
                format!("{} triple(s) [{}]", triple_ids.len(), subjects.join(", "))
            } else {
                format!("{} triple(s)", triple_ids.len())
            };
            ("triple:delete".to_string(), summary, None)
        }
        aingle_graph::dag::DagPayload::MemoryOp { kind } => {
            let summary = match kind {
                aingle_graph::dag::MemoryOpKind::Store { entry_type, .. } => {
                    format!("Store({})", entry_type)
                }
                aingle_graph::dag::MemoryOpKind::Forget { memory_id } => {
                    format!("Forget({})", memory_id)
                }
                aingle_graph::dag::MemoryOpKind::Consolidate => "Consolidate".to_string(),
            };
            ("memory:op".to_string(), summary, None)
        }
        aingle_graph::dag::DagPayload::Batch { ops } => {
            // Search the ops for the first TripleInsert that has a provenanced triple.
            let content_hash = ops.iter().find_map(|op| {
                if let aingle_graph::dag::DagPayload::TripleInsert { triples } = op {
                    triples
                        .iter()
                        .find_map(|t| t.provenance.as_ref().map(|p| p.content_hash.clone()))
                } else {
                    None
                }
            });
            (
                "batch".to_string(),
                format!("{} ops", ops.len()),
                content_hash,
            )
        }
        aingle_graph::dag::DagPayload::Genesis {
            triple_count,
            description,
        } => (
            "genesis".to_string(),
            format!("{} triples: {}", triple_count, description),
            None,
        ),
        aingle_graph::dag::DagPayload::Compact {
            pruned_count,
            retained_count,
            ref policy,
        } => (
            "compact".to_string(),
            format!(
                "pruned {} / retained {} ({})",
                pruned_count, retained_count, policy
            ),
            None,
        ),
        aingle_graph::dag::DagPayload::Noop => ("noop".to_string(), String::new(), None),
        aingle_graph::dag::DagPayload::Custom {
            payload_type,
            payload_summary,
            ..
        } => (payload_type.clone(), payload_summary.clone(), None),
    };

    DagActionDto {
        hash,
        parents,
        author: action.author.to_string(),
        seq: action.seq,
        timestamp: action.timestamp.to_rfc3339(),
        payload_type,
        payload_summary,
        signed: action.signature.is_some(),
        signature_status: signature_status(action).to_string(),
        content_hash,
        verification: None,
    }
}

/// Classify an action's signature state without collapsing the three cases.
fn signature_status(action: &aingle_graph::dag::DagAction) -> &'static str {
    if action.signature.is_some() {
        SIG_SIGNED
    } else if action.is_unsigned_by_design() {
        SIG_UNSIGNED_BY_DESIGN
    } else {
        SIG_UNSIGNED
    }
}

/// Convert a raw [`DagAction`] to its DTO form **with** the verification bundle
/// when the action is signed.
///
/// `node_key` is this node's verifying key, if it has one. It is published with
/// the action only after it is checked against the signature here, so the DTO
/// never offers a key that cannot possibly verify the action it accompanies —
/// an action replicated from another node reports `public_key: null` and
/// `public_key_source: "unknown_author"` instead of a key that would fail and
/// look like tampering.
///
/// The key check performed here is a convenience, not the client's evidence: the
/// client must still run `verification.procedure` itself. That is the whole point
/// of the bundle.
pub(crate) fn action_to_dto_verifiable(
    action: &aingle_graph::dag::DagAction,
    node_key: Option<&aingle_graph::dag::DagVerifyingKey>,
) -> DagActionDto {
    let mut dto = action_to_dto(action);

    let Some(sig) = action.signature.as_ref() else {
        return dto;
    };

    let canonical = action.canonical();
    let (public_key, public_key_source) = match node_key {
        Some(k) if k.verify(action).unwrap_or(false) => {
            (Some(k.to_hex()), "local_node_key".to_string())
        }
        _ => (None, "unknown_author".to_string()),
    };

    dto.verification = Some(ActionVerificationDto {
        spec: aingle_graph::dag::CANONICAL_SPEC.to_string(),
        hash_alg: aingle_graph::dag::HASH_ALG.to_string(),
        signature_alg: aingle_graph::dag::SIGNATURE_ALG.to_string(),
        signed_message: aingle_graph::dag::SIGNED_MESSAGE.to_string(),
        signature: to_hex(sig),
        key_id: public_key.clone(),
        public_key,
        public_key_source,
        canonical: CanonicalActionDto {
            parents: canonical.parents.iter().map(|h| h.to_hex()).collect(),
            author_json: canonical.author_json,
            seq: canonical.seq,
            timestamp_rfc3339: canonical.timestamp_rfc3339,
            payload_json: canonical.payload_json,
        },
        procedure: verification_procedure(),
    });
    dto
}

/// Parse a 64-character hex string into a 32-byte array.
///
/// Returns `None` if `hex` is not exactly 64 characters or contains non-hex digits.
fn parse_hex32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn triple_value_to_json(v: &aingle_graph::Value) -> serde_json::Value {
    match v {
        aingle_graph::Value::String(s) => serde_json::Value::String(s.clone()),
        aingle_graph::Value::Integer(i) => serde_json::json!(*i),
        aingle_graph::Value::Float(f) => serde_json::json!(*f),
        aingle_graph::Value::Boolean(b) => serde_json::json!(*b),
        aingle_graph::Value::Json(j) => j.clone(),
        aingle_graph::Value::Node(n) => serde_json::json!({ "node": n.to_string() }),
        aingle_graph::Value::DateTime(dt) => serde_json::Value::String(dt.clone()),
        aingle_graph::Value::Null => serde_json::Value::Null,
        _ => serde_json::Value::String(format!("{:?}", v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aingle_graph::dag::{DagAction, DagPayload, Provenance, TripleInsertPayload};
    use aingle_graph::NodeId;
    use chrono::Utc;

    fn test_action(payload: DagPayload) -> DagAction {
        DagAction {
            parents: vec![],
            author: NodeId::named("node:test"),
            seq: 0,
            timestamp: Utc::now(),
            payload,
            signature: None,
        }
    }

    #[test]
    fn action_to_dto_extracts_content_hash_from_triple_insert() {
        let provenance = Provenance {
            source_path: "vault/note.md".into(),
            line_start: 1,
            line_end: 3,
            content_hash: "deadbeef".into(),
        };
        let action = test_action(DagPayload::TripleInsert {
            triples: vec![TripleInsertPayload {
                subject: "note://note".into(),
                predicate: "note:title".into(),
                object: serde_json::json!("Test Note"),
                provenance: Some(provenance),
            }],
        });

        let dto = action_to_dto(&action);

        assert_eq!(
            dto.content_hash,
            Some("deadbeef".into()),
            "content_hash must be extracted from TripleInsert provenance"
        );
    }

    #[test]
    fn action_to_dto_extracts_content_hash_from_batch_with_triple_insert() {
        let provenance = Provenance {
            source_path: "vault/doc.md".into(),
            line_start: 5,
            line_end: 10,
            content_hash: "cafebabe".into(),
        };
        let action = test_action(DagPayload::Batch {
            ops: vec![
                DagPayload::TripleInsert {
                    triples: vec![TripleInsertPayload {
                        subject: "note://doc".into(),
                        predicate: "note:body".into(),
                        object: serde_json::json!("content"),
                        provenance: Some(provenance),
                    }],
                },
                DagPayload::Noop,
            ],
        });

        let dto = action_to_dto(&action);

        assert_eq!(
            dto.content_hash,
            Some("cafebabe".into()),
            "content_hash must be extracted from first TripleInsert inside Batch"
        );
    }

    #[test]
    fn action_to_dto_content_hash_none_for_triple_insert_without_provenance() {
        let action = test_action(DagPayload::TripleInsert {
            triples: vec![TripleInsertPayload {
                subject: "s".into(),
                predicate: "p".into(),
                object: serde_json::json!("o"),
                provenance: None,
            }],
        });

        let dto = action_to_dto(&action);

        assert_eq!(
            dto.content_hash, None,
            "content_hash must be None when no provenance is present"
        );
    }

    #[test]
    fn action_to_dto_content_hash_none_for_genesis() {
        let action = test_action(DagPayload::Genesis {
            triple_count: 0,
            description: "root".into(),
        });

        let dto = action_to_dto(&action);

        assert_eq!(
            dto.content_hash, None,
            "Genesis actions have no content_hash"
        );
    }

    #[test]
    fn action_to_dto_content_hash_none_for_noop() {
        let action = test_action(DagPayload::Noop);

        let dto = action_to_dto(&action);

        assert_eq!(dto.content_hash, None, "Noop actions have no content_hash");
    }
}
