//! REST endpoints for the Titans Memory subsystem.
//!
//! These endpoints expose the STM/LTM dual-memory architecture through
//! the Cortex REST API, allowing agents to store, recall, consolidate,
//! and checkpoint their memory state.
//!
//! ## Endpoints
//!
//! - `POST   /api/v1/memory/remember`      - Store in STM
//! - `POST   /api/v1/memory/recall`         - Query STM + LTM
//! - `POST   /api/v1/memory/consolidate`    - Force STM → LTM
//! - `GET    /api/v1/memory/stats`          - Memory statistics
//! - `DELETE /api/v1/memory/:id`            - Forget
//! - `POST   /api/v1/memory/checkpoint`     - Snapshot state
//! - `GET    /api/v1/memory/checkpoints`    - List snapshots
//! - `POST   /api/v1/memory/restore/:id`    - Restore snapshot

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use titans_memory::{MemoryEntry, MemoryId, MemoryQuery};

use crate::error::{Error, Result};
use crate::state::AppState;

// ============================================================================
// DTOs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RememberRequest {
    pub entry_type: String,
    pub data: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_importance")]
    pub importance: f32,
    pub embedding: Option<Vec<f32>>,
}

fn default_importance() -> f32 {
    0.7
}

#[derive(Debug, Serialize)]
pub struct RememberResponse {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct RecallRequest {
    pub text: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub entry_type: Option<String>,
    pub min_importance: Option<f32>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct MemoryResultDto {
    pub id: String,
    pub entry_type: String,
    pub data: serde_json::Value,
    pub tags: Vec<String>,
    pub importance: f32,
    pub relevance: f32,
    pub source: String,
    pub created_at: String,
    pub last_accessed: String,
    pub access_count: u32,
}

#[derive(Debug, Serialize)]
pub struct ConsolidateResponse {
    pub consolidated: usize,
}

#[derive(Debug, Serialize)]
pub struct MemoryStatsDto {
    pub stm_count: usize,
    pub stm_capacity: usize,
    pub ltm_entity_count: usize,
    pub ltm_link_count: usize,
    pub total_memory_bytes: usize,
}

#[derive(Debug, Deserialize)]
pub struct CheckpointRequest {
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckpointResponse {
    #[serde(rename = "checkpointId")]
    pub checkpoint_id: String,
}

#[derive(Debug, Serialize)]
pub struct CheckpointListDto {
    pub id: String,
    pub label: Option<String>,
    pub created_at: String,
    pub stm_count: usize,
    pub ltm_entity_count: usize,
}

// ============================================================================
// Handlers
// ============================================================================

/// Store a memory entry in Short-Term Memory.
pub async fn remember(
    State(state): State<AppState>,
    Json(req): Json<RememberRequest>,
) -> Result<(StatusCode, Json<RememberResponse>)> {
    let mut entry = MemoryEntry::new(&req.entry_type, req.data);

    if !req.tags.is_empty() {
        let tag_refs: Vec<&str> = req.tags.iter().map(|s| s.as_str()).collect();
        entry = entry.with_tags(&tag_refs);
    }

    entry = entry.with_importance(req.importance);

    if let Some(emb) = req.embedding {
        entry = entry.with_embedding(titans_memory::Embedding::new(emb));
    }

    let mut memory = state.memory.write().await;
    let id = memory
        .remember(entry)
        .map_err(|e| Error::Internal(format!("Memory store failed: {e}")))?;

    Ok((
        StatusCode::CREATED,
        Json(RememberResponse {
            id: id.to_hex(),
        }),
    ))
}

/// Query both STM and LTM for matching memories.
pub async fn recall(
    State(state): State<AppState>,
    Json(req): Json<RecallRequest>,
) -> Result<Json<Vec<MemoryResultDto>>> {
    let query = build_query(&req);
    let memory = state.memory.read().await;
    let results = memory
        .recall(&query)
        .map_err(|e| Error::Internal(format!("Memory recall failed: {e}")))?;

    let dtos: Vec<MemoryResultDto> = results
        .into_iter()
        .map(|r| MemoryResultDto {
            id: r.entry.id.to_hex(),
            entry_type: r.entry.entry_type.clone(),
            data: r.entry.data.clone(),
            tags: r.entry.tags.iter().map(|t| t.0.clone()).collect(),
            importance: r.entry.metadata.importance,
            relevance: r.relevance,
            source: match r.source {
                titans_memory::types::MemorySource::ShortTerm => "ShortTerm".to_string(),
                titans_memory::types::MemorySource::LongTerm => "LongTerm".to_string(),
            },
            created_at: r.entry.metadata.created_at.0.to_string(),
            last_accessed: r.entry.metadata.last_accessed.0.to_string(),
            access_count: r.entry.metadata.access_count,
        })
        .collect();

    Ok(Json(dtos))
}

/// Force consolidation of important STM entries into LTM.
pub async fn consolidate(
    State(state): State<AppState>,
) -> Result<Json<ConsolidateResponse>> {
    let mut memory = state.memory.write().await;
    let count = memory
        .consolidate()
        .map_err(|e| Error::Internal(format!("Consolidation failed: {e}")))?;

    Ok(Json(ConsolidateResponse {
        consolidated: count,
    }))
}

/// Get memory subsystem statistics.
pub async fn stats(State(state): State<AppState>) -> Result<Json<MemoryStatsDto>> {
    let memory = state.memory.read().await;
    let s = memory.stats();

    Ok(Json(MemoryStatsDto {
        stm_count: s.stm_count,
        stm_capacity: s.stm_capacity,
        ltm_entity_count: s.ltm_entity_count,
        ltm_link_count: s.ltm_link_count,
        total_memory_bytes: s.total_memory_bytes,
    }))
}

/// Forget (delete) a specific memory entry.
pub async fn forget(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let memory_id = MemoryId::from_hex(&id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid memory ID: {id}")))?;

    let mut memory = state.memory.write().await;
    memory
        .forget(&memory_id)
        .map_err(|e| Error::NotFound(format!("Memory not found: {e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Create a checkpoint (snapshot) of current memory state.
pub async fn checkpoint(
    State(state): State<AppState>,
    Json(req): Json<CheckpointRequest>,
) -> Result<(StatusCode, Json<CheckpointResponse>)> {
    // Checkpoint is a logical snapshot — we store STM+LTM stats as metadata
    // For now, create a proof-of-state in the proof store
    let memory = state.memory.read().await;
    let s = memory.stats();
    let label = req.label.unwrap_or_else(|| {
        format!("checkpoint-{}", chrono::Utc::now().timestamp())
    });

    let checkpoint_data = serde_json::json!({
        "label": label,
        "stm_count": s.stm_count,
        "ltm_entity_count": s.ltm_entity_count,
        "ltm_link_count": s.ltm_link_count,
        "total_memory_bytes": s.total_memory_bytes,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let proof_req = crate::proofs::SubmitProofRequest {
        proof_type: crate::proofs::ProofType::Knowledge,
        proof_data: checkpoint_data,
        metadata: Some(crate::proofs::ProofMetadata {
            submitter: Some("memory-system".to_string()),
            tags: vec!["checkpoint".to_string(), "memory".to_string()],
            extra: Default::default(),
        }),
    };

    let proof_id = state
        .proof_store
        .submit(proof_req)
        .await
        .map_err(|e| Error::Internal(format!("Checkpoint creation failed: {e}")))?;

    Ok((
        StatusCode::CREATED,
        Json(CheckpointResponse {
            checkpoint_id: proof_id,
        }),
    ))
}

/// List memory checkpoints.
pub async fn list_checkpoints(
    State(state): State<AppState>,
) -> Result<Json<Vec<CheckpointListDto>>> {
    let proofs = state
        .proof_store
        .list(Some(crate::proofs::ProofType::Knowledge))
        .await;

    let checkpoints: Vec<CheckpointListDto> = proofs
        .into_iter()
        .filter(|p| p.metadata.tags.contains(&"checkpoint".to_string()))
        .map(|p| {
            let data: serde_json::Value =
                serde_json::from_slice(&p.data).unwrap_or_default();
            CheckpointListDto {
                id: p.id.clone(),
                label: data.get("label").and_then(|v| v.as_str()).map(String::from),
                created_at: p.created_at.to_rfc3339(),
                stm_count: data
                    .get("stm_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
                ltm_entity_count: data
                    .get("ltm_entity_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
            }
        })
        .collect();

    Ok(Json(checkpoints))
}

/// Restore memory from a checkpoint.
///
/// Note: Full state restoration requires serialized memory dumps. For now,
/// this verifies the checkpoint exists and returns success.
pub async fn restore_checkpoint(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let proof = state
        .proof_store
        .get(&id)
        .await
        .ok_or_else(|| Error::NotFound(format!("Checkpoint not found: {id}")))?;

    if !proof.metadata.tags.contains(&"checkpoint".to_string()) {
        return Err(Error::InvalidInput("Not a memory checkpoint".to_string()));
    }

    // TODO: Implement full state restoration from serialized memory dump
    // For now, this validates the checkpoint exists
    tracing::info!(checkpoint_id = %id, "Memory checkpoint acknowledged for restoration");

    Ok(StatusCode::OK)
}

// ============================================================================
// Helpers
// ============================================================================

fn build_query(req: &RecallRequest) -> MemoryQuery {
    let mut query = if let Some(text) = &req.text {
        MemoryQuery::text(text)
    } else if !req.tags.is_empty() {
        let tag_refs: Vec<&str> = req.tags.iter().map(|s| s.as_str()).collect();
        MemoryQuery::tags(&tag_refs)
    } else {
        MemoryQuery::text("")
    };

    if let Some(limit) = req.limit {
        query = query.with_limit(limit);
    }

    if let Some(min_imp) = req.min_importance {
        query = query.with_min_importance(min_imp);
    }

    if let Some(entry_type) = &req.entry_type {
        query = MemoryQuery::entry_type(entry_type);
        if let Some(limit) = req.limit {
            query = query.with_limit(limit);
        }
        if let Some(min_imp) = req.min_importance {
            query = query.with_min_importance(min_imp);
        }
    }

    query
}

/// Create the memory sub-router.
pub fn memory_router() -> axum::Router<AppState> {
    use axum::routing::{delete, get, post};

    axum::Router::new()
        .route("/api/v1/memory/remember", post(remember))
        .route("/api/v1/memory/recall", post(recall))
        .route("/api/v1/memory/consolidate", post(consolidate))
        .route("/api/v1/memory/stats", get(stats))
        .route("/api/v1/memory/:id", delete(forget))
        .route("/api/v1/memory/checkpoint", post(checkpoint))
        .route("/api/v1/memory/checkpoints", get(list_checkpoints))
        .route("/api/v1/memory/restore/:id", post(restore_checkpoint))
}
