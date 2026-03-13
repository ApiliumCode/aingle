// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Internal Raft RPC endpoints for inter-node communication.
//!
//! These endpoints handle Raft protocol messages (AppendEntries, Vote,
//! InstallSnapshot) over HTTP. They are used by `HttpRaftRpcSender`
//! on other nodes to drive the Raft consensus protocol.
//!
//! ## Endpoints
//!
//! - `POST /internal/raft/append-entries` — AppendEntries RPC
//! - `POST /internal/raft/vote` — Vote RPC
//! - `POST /internal/raft/snapshot` — Install full snapshot

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};

use crate::error::Error;
use crate::rest::cluster_utils::validate_cluster_auth;
use crate::state::AppState;

type C = aingle_raft::CortexTypeConfig;

/// POST /internal/raft/append-entries
///
/// Receives a serialized `AppendEntriesRequest`, forwards to the local
/// Raft instance, and returns the serialized response.
pub async fn raft_append_entries(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, Error> {
    validate_cluster_auth(&headers, &state)?;

    let raft = state
        .raft
        .as_ref()
        .ok_or_else(|| Error::Internal("Raft not initialized".into()))?;

    let req: openraft::raft::AppendEntriesRequest<C> = serde_json::from_slice(&body)
        .map_err(|e| Error::Internal(format!("Deserialize AppendEntries: {e}")))?;

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        raft.append_entries(req),
    )
    .await
    .map_err(|_| Error::Timeout("AppendEntries RPC timed out (10s)".into()))?
    .map_err(|e| Error::Internal(format!("AppendEntries failed: {e}")))?;

    let payload = serde_json::to_vec(&resp)
        .map_err(|e| Error::Internal(format!("Serialize response: {e}")))?;

    Ok((StatusCode::OK, payload))
}

/// POST /internal/raft/vote
///
/// Receives a serialized `VoteRequest`, forwards to the local
/// Raft instance, and returns the serialized response.
pub async fn raft_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, Error> {
    validate_cluster_auth(&headers, &state)?;

    let raft = state
        .raft
        .as_ref()
        .ok_or_else(|| Error::Internal("Raft not initialized".into()))?;

    let req: openraft::raft::VoteRequest<C> = serde_json::from_slice(&body)
        .map_err(|e| Error::Internal(format!("Deserialize Vote: {e}")))?;

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        raft.vote(req),
    )
    .await
    .map_err(|_| Error::Timeout("Vote RPC timed out (10s)".into()))?
    .map_err(|e| Error::Internal(format!("Vote failed: {e}")))?;

    let payload = serde_json::to_vec(&resp)
        .map_err(|e| Error::Internal(format!("Serialize response: {e}")))?;

    Ok((StatusCode::OK, payload))
}

/// POST /internal/raft/snapshot
///
/// Receives a serialized snapshot envelope (vote + meta + data),
/// forwards to the local Raft instance via `install_full_snapshot`.
pub async fn raft_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, Error> {
    validate_cluster_auth(&headers, &state)?;

    let raft = state
        .raft
        .as_ref()
        .ok_or_else(|| Error::Internal("Raft not initialized".into()))?;

    // The envelope matches what CortexNetworkConnection::full_snapshot serializes:
    // { "vote": ..., "meta": ..., "data": [...] }
    let envelope: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| Error::Internal(format!("Deserialize snapshot envelope: {e}")))?;

    let vote: openraft::type_config::alias::VoteOf<C> =
        serde_json::from_value(envelope["vote"].clone())
            .map_err(|e| Error::Internal(format!("Deserialize vote: {e}")))?;

    let meta: openraft::type_config::alias::SnapshotMetaOf<C> =
        serde_json::from_value(envelope["meta"].clone())
            .map_err(|e| Error::Internal(format!("Deserialize snapshot meta: {e}")))?;

    let data: Vec<u8> = serde_json::from_value(envelope["data"].clone())
        .map_err(|e| Error::Internal(format!("Deserialize snapshot data: {e}")))?;

    let snapshot = openraft::Snapshot {
        meta,
        snapshot: std::io::Cursor::new(data),
    };

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        raft.install_full_snapshot(vote, snapshot),
    )
    .await
    .map_err(|_| Error::Timeout("InstallSnapshot RPC timed out (60s)".into()))?
    .map_err(|e| Error::Internal(format!("InstallSnapshot failed: {e}")))?;

    let payload = serde_json::to_vec(&resp)
        .map_err(|e| Error::Internal(format!("Serialize response: {e}")))?;

    Ok((StatusCode::OK, payload))
}

/// In-flight chunked snapshot buffer with creation timestamp for TTL.
struct SnapshotBuffer {
    data: Vec<u8>,
    expected_size: u64,
    created_at: std::time::Instant,
}

/// In-flight chunked snapshot buffers, keyed by snapshot_id.
/// Buffers older than `BUFFER_TTL` are evicted to prevent memory leaks
/// from abandoned transfers.
static SNAPSHOT_BUFFERS: std::sync::LazyLock<
    dashmap::DashMap<String, SnapshotBuffer>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);

/// Maximum time a partial snapshot buffer can live before eviction.
const BUFFER_TTL: std::time::Duration = std::time::Duration::from_secs(300); // 5 min

/// Maximum total memory across all in-flight snapshot buffers (256 MB).
const MAX_BUFFER_MEMORY: usize = 256 * 1024 * 1024;

/// Evict expired snapshot buffers to reclaim memory.
fn evict_stale_buffers() {
    SNAPSHOT_BUFFERS.retain(|id, buf| {
        let alive = buf.created_at.elapsed() < BUFFER_TTL;
        if !alive {
            tracing::warn!(snapshot_id = %id, "Evicting stale snapshot buffer");
        }
        alive
    });
}

/// POST /internal/raft/snapshot-chunk
///
/// Receives a single chunk of a streamed snapshot. Chunks are buffered
/// in memory and assembled when the final chunk arrives.
pub async fn raft_snapshot_chunk(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, Error> {
    validate_cluster_auth(&headers, &state)?;

    // Evict stale buffers on each request (cheap: DashMap::retain is O(n))
    evict_stale_buffers();

    let chunk: aingle_raft::network::RaftMessage = serde_json::from_slice(&body)
        .map_err(|e| Error::Internal(format!("Deserialize snapshot chunk: {e}")))?;

    match chunk {
        aingle_raft::network::RaftMessage::SnapshotChunk {
            snapshot_id,
            offset,
            total_size,
            is_final,
            data,
        } => {
            // Reject snapshots that would exceed memory budget
            if total_size as usize > MAX_BUFFER_MEMORY {
                return Err(Error::Internal(format!(
                    "Snapshot too large: {total_size} bytes exceeds {MAX_BUFFER_MEMORY} limit"
                )));
            }

            // Append chunk to buffer
            let mut buf = SNAPSHOT_BUFFERS
                .entry(snapshot_id.clone())
                .or_insert_with(|| SnapshotBuffer {
                    data: Vec::with_capacity(total_size as usize),
                    expected_size: total_size,
                    created_at: std::time::Instant::now(),
                });

            // Extend buffer to accommodate this chunk
            let required = offset as usize + data.len();
            if buf.data.len() < required {
                buf.data.resize(required, 0);
            }
            buf.data[offset as usize..offset as usize + data.len()].copy_from_slice(&data);

            if is_final {
                // Remove buffer and validate completeness
                let full_buf = SNAPSHOT_BUFFERS
                    .remove(&snapshot_id)
                    .ok_or_else(|| Error::Internal("Snapshot buffer missing on final chunk".into()))?
                    .1;

                if (full_buf.data.len() as u64) != full_buf.expected_size {
                    return Err(Error::Internal(format!(
                        "Snapshot size mismatch: got {} bytes, expected {}",
                        full_buf.data.len(),
                        full_buf.expected_size
                    )));
                }

                // Delegate to the monolithic snapshot handler
                let result = install_full_snapshot_from_bytes(&state, &full_buf.data).await?;
                Ok((StatusCode::OK, result))
            } else {
                // ACK this chunk
                let ack = aingle_raft::network::RaftMessage::SnapshotChunkAck {
                    snapshot_id,
                    next_offset: offset + data.len() as u64,
                };
                let payload = serde_json::to_vec(&ack)
                    .map_err(|e| Error::Internal(format!("Serialize chunk ack: {e}")))?;
                Ok((StatusCode::OK, payload))
            }
        }
        _ => Err(Error::Internal("Expected SnapshotChunk message".into())),
    }
}

/// Shared logic: install a full snapshot from its raw bytes.
async fn install_full_snapshot_from_bytes(
    state: &AppState,
    data: &[u8],
) -> Result<Vec<u8>, Error> {
    let raft = state
        .raft
        .as_ref()
        .ok_or_else(|| Error::Internal("Raft not initialized".into()))?;

    let envelope: serde_json::Value = serde_json::from_slice(data)
        .map_err(|e| Error::Internal(format!("Deserialize snapshot envelope: {e}")))?;

    let vote: openraft::type_config::alias::VoteOf<C> =
        serde_json::from_value(envelope["vote"].clone())
            .map_err(|e| Error::Internal(format!("Deserialize vote: {e}")))?;

    let meta: openraft::type_config::alias::SnapshotMetaOf<C> =
        serde_json::from_value(envelope["meta"].clone())
            .map_err(|e| Error::Internal(format!("Deserialize snapshot meta: {e}")))?;

    let snap_data: Vec<u8> = serde_json::from_value(envelope["data"].clone())
        .map_err(|e| Error::Internal(format!("Deserialize snapshot data: {e}")))?;

    let snapshot = openraft::Snapshot {
        meta,
        snapshot: std::io::Cursor::new(snap_data),
    };

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        raft.install_full_snapshot(vote, snapshot),
    )
    .await
    .map_err(|_| Error::Timeout("InstallSnapshot timed out (60s)".into()))?
    .map_err(|e| Error::Internal(format!("InstallSnapshot failed: {e}")))?;

    serde_json::to_vec(&resp)
        .map_err(|e| Error::Internal(format!("Serialize response: {e}")))
}

/// Create the internal Raft RPC sub-router.
pub fn raft_rpc_router() -> axum::Router<AppState> {
    use axum::routing::post;

    axum::Router::new()
        .route("/internal/raft/append-entries", post(raft_append_entries))
        .route("/internal/raft/vote", post(raft_vote))
        .route("/internal/raft/snapshot", post(raft_snapshot))
        .route("/internal/raft/snapshot-chunk", post(raft_snapshot_chunk))
}
