// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG introspection REST endpoints.
//!
//! ## Endpoints
//!
//! - `GET /api/v1/dag/tips` — Current DAG tip hashes and count
//! - `GET /api/v1/dag/action/:hash` — Single DagAction by hash
//! - `GET /api/v1/dag/history` — Mutations affecting a subject
//! - `GET /api/v1/dag/chain` — Author's action chain
//! - `GET /api/v1/dag/stats` — Action count, tip count, depth estimate

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
pub struct DagActionDto {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub seq: u64,
    pub timestamp: String,
    pub payload_type: String,
    pub payload_summary: String,
    pub signed: bool,
}

#[derive(Debug, Serialize)]
pub struct DagStatsResponse {
    pub action_count: usize,
    pub tip_count: usize,
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
    /// The peer URL to pull from (e.g. "http://node2:8080").
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
    50
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/v1/dag/tips
pub async fn get_dag_tips(State(state): State<AppState>) -> Result<Json<DagTipsResponse>> {
    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let tips = dag_store.tips().map_err(|e| Error::Internal(e.to_string()))?;
    let tip_strings: Vec<String> = tips.iter().map(|h| h.to_hex()).collect();
    let count = tip_strings.len();

    Ok(Json(DagTipsResponse {
        tips: tip_strings,
        count,
    }))
}

/// GET /api/v1/dag/action/:hash
pub async fn get_dag_action(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<DagActionDto>> {
    let action_hash = aingle_graph::dag::DagActionHash::from_hex(&hash)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid DAG action hash: {}", hash)))?;

    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let action = dag_store
        .get(&action_hash)
        .map_err(|e| Error::Internal(e.to_string()))?
        .ok_or_else(|| Error::NotFound(format!("DAG action {} not found", hash)))?;

    Ok(Json(action_to_dto(&action)))
}

/// GET /api/v1/dag/history?subject=X&triple_id=X&limit=N
pub async fn get_dag_history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<DagActionDto>>> {
    let graph = state.graph.read().await;

    // Subject-based lookup uses the dedicated subject index
    if let Some(ref subject) = query.subject {
        let actions = graph
            .dag_history_by_subject(subject, query.limit)
            .map_err(|e| Error::Internal(e.to_string()))?;
        return Ok(Json(actions.iter().map(action_to_dto).collect()));
    }

    // Triple-ID-based lookup uses the affected index
    if let Some(ref tid_hex) = query.triple_id {
        let mut bytes = [0u8; 32];
        if tid_hex.len() != 64 {
            return Err(Error::InvalidInput("triple_id must be 64 hex chars".into()));
        }
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&tid_hex[i * 2..i * 2 + 2], 16)
                .map_err(|_| Error::InvalidInput("Invalid hex in triple_id".into()))?;
        }

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
    let author = aingle_graph::NodeId::named(&query.author);

    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let actions = dag_store
        .chain(&author, query.limit)
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(Json(actions.iter().map(action_to_dto).collect()))
}

/// GET /api/v1/dag/stats
pub async fn get_dag_stats(State(state): State<AppState>) -> Result<Json<DagStatsResponse>> {
    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let action_count = dag_store.action_count();
    let tip_count = dag_store.tip_count().map_err(|e| Error::Internal(e.to_string()))?;

    Ok(Json(DagStatsResponse {
        action_count,
        tip_count,
    }))
}

/// POST /api/v1/dag/prune
pub async fn post_dag_prune(
    State(state): State<AppState>,
    Json(req): Json<PruneRequest>,
) -> Result<Json<PruneResponse>> {
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

    Ok(Json(PruneResponse {
        pruned_count: result.pruned_count,
        retained_count: result.retained_count,
        checkpoint_hash: result.checkpoint_hash.map(|h| h.to_hex()),
    }))
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

    let body = dag_graph.export(format);

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

    let mut pk_bytes = [0u8; 32];
    if query.public_key.len() != 64 {
        return Err(Error::InvalidInput("public_key must be 64 hex chars".into()));
    }
    for i in 0..32 {
        pk_bytes[i] = u8::from_str_radix(&query.public_key[i * 2..i * 2 + 2], 16)
            .map_err(|_| Error::InvalidInput("Invalid hex in public_key".into()))?;
    }

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

        if dag_store.contains(&hash).map_err(|e| Error::Internal(e.to_string()))? {
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

    let parents = dag_store.tips().map_err(|e| Error::Internal(e.to_string()))?;

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

fn action_to_dto(action: &aingle_graph::dag::DagAction) -> DagActionDto {
    let hash = action.compute_hash().to_hex();
    let parents: Vec<String> = action.parents.iter().map(|h| h.to_hex()).collect();

    let (payload_type, payload_summary) = match &action.payload {
        aingle_graph::dag::DagPayload::TripleInsert { triples } => {
            let summary = if triples.len() == 1 {
                let t = &triples[0];
                format!("{} -> {} -> {}", t.subject, t.predicate, t.object)
            } else {
                format!("{} triple(s)", triples.len())
            };
            ("triple:create".to_string(), summary)
        }
        aingle_graph::dag::DagPayload::TripleDelete { triple_ids, subjects } => {
            let summary = if !subjects.is_empty() {
                format!("{} triple(s) [{}]", triple_ids.len(), subjects.join(", "))
            } else {
                format!("{} triple(s)", triple_ids.len())
            };
            ("triple:delete".to_string(), summary)
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
            ("memory:op".to_string(), summary)
        }
        aingle_graph::dag::DagPayload::Batch { ops } => (
            "batch".to_string(),
            format!("{} ops", ops.len()),
        ),
        aingle_graph::dag::DagPayload::Genesis {
            triple_count,
            description,
        } => (
            "genesis".to_string(),
            format!("{} triples: {}", triple_count, description),
        ),
        aingle_graph::dag::DagPayload::Compact {
            pruned_count,
            retained_count,
            ref policy,
        } => (
            "compact".to_string(),
            format!("pruned {} / retained {} ({})", pruned_count, retained_count, policy),
        ),
        aingle_graph::dag::DagPayload::Noop => ("noop".to_string(), String::new()),
        aingle_graph::dag::DagPayload::Custom {
            payload_type,
            payload_summary,
            ..
        } => (payload_type.clone(), payload_summary.clone()),
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
    }
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
