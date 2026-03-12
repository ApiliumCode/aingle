// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! REST endpoints for the Ineru memory subsystem.
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
use ineru::{MemoryEntry, MemoryId, MemoryQuery};

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
    // Cluster mode: route through Raft
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let raft_req = aingle_raft::CortexRequest {
            kind: aingle_wal::WalEntryKind::MemoryStore {
                memory_id: String::new(), // assigned by state machine
                entry_type: req.entry_type.clone(),
                data: req.data.clone(),
                importance: req.importance,
            },
        };
        let resp = raft
            .client_write(raft_req)
            .await
            .map_err(|e| Error::Internal(format!("Raft write failed: {e}")))?;

        if !resp.response().success {
            return Err(Error::Internal(
                resp.response()
                    .detail
                    .clone()
                    .unwrap_or_else(|| "Raft memory store failed".to_string()),
            ));
        }

        return Ok((
            StatusCode::CREATED,
            Json(RememberResponse {
                id: "raft".to_string(),
            }),
        ));
    }

    // Non-cluster mode: direct write
    #[cfg(feature = "cluster")]
    let wal_data = req.data.clone();
    let mut entry = MemoryEntry::new(&req.entry_type, req.data);

    if !req.tags.is_empty() {
        let tag_refs: Vec<&str> = req.tags.iter().map(|s| s.as_str()).collect();
        entry = entry.with_tags(&tag_refs);
    }

    entry = entry.with_importance(req.importance);

    if let Some(emb) = req.embedding {
        entry = entry.with_embedding(ineru::Embedding::new(emb));
    }

    let mut memory = state.memory.write().await;
    let id = memory
        .remember(entry)
        .map_err(|e| Error::Internal(format!("Memory store failed: {e}")))?;

    // Append to WAL (legacy cluster path)
    #[cfg(feature = "cluster")]
    if let Some(ref wal) = state.wal {
        let _ = wal.append(aingle_wal::WalEntryKind::MemoryStore {
            memory_id: id.to_hex(),
            entry_type: req.entry_type.clone(),
            data: wal_data.clone(),
            importance: req.importance,
        });
    }

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
                ineru::types::MemorySource::ShortTerm => "ShortTerm".to_string(),
                ineru::types::MemorySource::LongTerm => "LongTerm".to_string(),
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

    // Append to WAL (cluster mode)
    #[cfg(feature = "cluster")]
    if let Some(ref wal) = state.wal {
        let _ = wal.append(aingle_wal::WalEntryKind::MemoryConsolidate {
            consolidated_count: count,
        });
    }

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
    // Cluster mode: route through Raft
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let raft_req = aingle_raft::CortexRequest {
            kind: aingle_wal::WalEntryKind::MemoryForget {
                memory_id: id.clone(),
            },
        };
        let resp = raft
            .client_write(raft_req)
            .await
            .map_err(|e| Error::Internal(format!("Raft write failed: {e}")))?;

        if !resp.response().success {
            return Err(Error::Internal(
                resp.response()
                    .detail
                    .clone()
                    .unwrap_or_else(|| "Raft forget failed".to_string()),
            ));
        }

        return Ok(StatusCode::NO_CONTENT);
    }

    // Non-cluster mode: direct delete
    let memory_id = MemoryId::from_hex(&id)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid memory ID: {id}")))?;

    let mut memory = state.memory.write().await;
    memory
        .forget(&memory_id)
        .map_err(|e| Error::NotFound(format!("Memory not found: {e}")))?;

    // Append to WAL (legacy cluster path)
    #[cfg(feature = "cluster")]
    if let Some(ref wal) = state.wal {
        let _ = wal.append(aingle_wal::WalEntryKind::MemoryForget {
            memory_id: id.clone(),
        });
    }

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
// Vector Search (Motomeru)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct VectorSearchRequest {
    pub embedding: Vec<f32>,
    pub k: usize,
    #[serde(default = "default_min_similarity")]
    pub min_similarity: f32,
    pub entry_type: Option<String>,
    pub tags: Option<Vec<String>>,
}

fn default_min_similarity() -> f32 {
    0.0
}

#[derive(Debug, Serialize)]
pub struct VectorIndexStatsDto {
    pub point_count: usize,
    pub deleted_count: usize,
    pub dimensions: usize,
    pub memory_bytes: usize,
}

/// Vector search over memory entries using HNSW index.
pub async fn vector_search(
    State(state): State<AppState>,
    Json(req): Json<VectorSearchRequest>,
) -> Result<Json<Vec<MemoryResultDto>>> {
    let memory = state.memory.read().await;
    let results = memory.ltm.vector_search_memories(&req.embedding, req.k, req.min_similarity);

    let mut dtos: Vec<MemoryResultDto> = results
        .into_iter()
        .map(|(entry, similarity)| MemoryResultDto {
            id: entry.id.to_hex(),
            entry_type: entry.entry_type.clone(),
            data: entry.data.clone(),
            tags: entry.tags.iter().map(|t| t.0.clone()).collect(),
            importance: entry.metadata.importance,
            relevance: similarity,
            source: "LongTerm".to_string(),
            created_at: entry.metadata.created_at.0.to_string(),
            last_accessed: entry.metadata.last_accessed.0.to_string(),
            access_count: entry.metadata.access_count,
        })
        .collect();

    // Apply optional filters
    if let Some(ref entry_type) = req.entry_type {
        dtos.retain(|d| &d.entry_type == entry_type);
    }
    if let Some(ref tags) = req.tags {
        if !tags.is_empty() {
            dtos.retain(|d| tags.iter().any(|t| d.tags.contains(t)));
        }
    }

    Ok(Json(dtos))
}

/// Get HNSW vector index statistics.
pub async fn vector_index_stats(
    State(state): State<AppState>,
) -> Result<Json<VectorIndexStatsDto>> {
    let memory = state.memory.read().await;
    let stats = memory.ltm.hnsw_index()
        .map(|idx| idx.stats())
        .unwrap_or(ineru::hnsw::HnswStats {
            point_count: 0,
            deleted_count: 0,
            dimensions: 0,
            max_layer: 0,
            memory_bytes: 0,
        });

    Ok(Json(VectorIndexStatsDto {
        point_count: stats.point_count,
        deleted_count: stats.deleted_count,
        dimensions: stats.dimensions,
        memory_bytes: stats.memory_bytes,
    }))
}

/// Force rebuild of the HNSW vector index.
pub async fn rebuild_vector_index(
    State(state): State<AppState>,
) -> Result<StatusCode> {
    let mut memory = state.memory.write().await;
    if let Some(hnsw) = memory.ltm.hnsw_index_mut() {
        hnsw.rebuild();
        tracing::info!("HNSW index rebuilt, {} active points", hnsw.len());
    }
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
        .route("/api/v1/memory/{id}", delete(forget))
        .route("/api/v1/memory/checkpoint", post(checkpoint))
        .route("/api/v1/memory/checkpoints", get(list_checkpoints))
        .route("/api/v1/memory/restore/{id}", post(restore_checkpoint))
        // Motomeru: HNSW vector search endpoints
        .route("/api/v1/memory/search", post(vector_search))
        .route("/api/v1/memory/index/stats", get(vector_index_stats))
        .route("/api/v1/memory/index/rebuild", post(rebuild_vector_index))
}
