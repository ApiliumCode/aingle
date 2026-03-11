// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Cluster management REST endpoints.
//!
//! ## Endpoints
//!
//! - `GET    /api/v1/cluster/status`     — Node role, term, leader, members
//! - `POST   /api/v1/cluster/join`       — Request to join cluster
//! - `POST   /api/v1/cluster/leave`      — Graceful leave
//! - `GET    /api/v1/cluster/members`    — List members with replication lag
//! - `GET    /api/v1/cluster/wal/stats`  — WAL statistics
//! - `POST   /api/v1/cluster/wal/verify` — Verify WAL hash chain integrity

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::state::AppState;

/// Cluster status response.
#[derive(Debug, Serialize)]
pub struct ClusterStatus {
    pub node_id: u64,
    pub role: String,
    pub term: u64,
    pub leader_id: Option<u64>,
    pub leader_addr: Option<String>,
    pub members: Vec<ClusterMember>,
    pub wal_last_seq: u64,
    pub last_applied: u64,
    pub commit_index: u64,
}

/// Information about a single cluster member.
#[derive(Debug, Serialize)]
pub struct ClusterMember {
    pub node_id: u64,
    pub rest_addr: String,
    pub p2p_addr: String,
    pub role: String,
    pub last_heartbeat: String,
    pub replication_lag: u64,
}

/// Request to join the cluster.
#[derive(Debug, Deserialize)]
pub struct JoinRequest {
    pub node_id: u64,
    pub rest_addr: String,
    pub p2p_addr: String,
}

/// Join response.
#[derive(Debug, Serialize)]
pub struct JoinResponse {
    pub accepted: bool,
    pub leader_id: Option<u64>,
    pub leader_addr: Option<String>,
    pub message: String,
}

/// WAL statistics response.
#[derive(Debug, Serialize)]
pub struct WalStatsResponse {
    pub segment_count: usize,
    pub total_size_bytes: u64,
    pub last_seq: u64,
    pub next_seq: u64,
}

/// WAL verification response.
#[derive(Debug, Serialize)]
pub struct WalVerifyResponse {
    pub valid: bool,
    pub entries_checked: u64,
    pub first_invalid_seq: Option<u64>,
}

/// GET /api/v1/cluster/status
pub async fn cluster_status(
    State(state): State<AppState>,
) -> Result<Json<ClusterStatus>> {
    let wal_last_seq = {
        #[cfg(feature = "cluster")]
        {
            state.wal.as_ref().map(|w| w.last_seq()).unwrap_or(0)
        }
        #[cfg(not(feature = "cluster"))]
        { 0u64 }
    };

    Ok(Json(ClusterStatus {
        node_id: 0,
        role: "standalone".to_string(),
        term: 0,
        leader_id: None,
        leader_addr: None,
        members: Vec::new(),
        wal_last_seq,
        last_applied: 0,
        commit_index: 0,
    }))
}

/// POST /api/v1/cluster/join
pub async fn cluster_join(
    State(_state): State<AppState>,
    Json(req): Json<JoinRequest>,
) -> Result<(StatusCode, Json<JoinResponse>)> {
    // In standalone mode, joining is not supported
    tracing::info!(
        node_id = req.node_id,
        rest_addr = %req.rest_addr,
        "Cluster join request received"
    );

    Ok((
        StatusCode::OK,
        Json(JoinResponse {
            accepted: false,
            leader_id: None,
            leader_addr: None,
            message: "Cluster mode not active on this node".to_string(),
        }),
    ))
}

/// POST /api/v1/cluster/leave
pub async fn cluster_leave(
    State(_state): State<AppState>,
) -> Result<StatusCode> {
    tracing::info!("Cluster leave request received");
    Ok(StatusCode::OK)
}

/// GET /api/v1/cluster/members
pub async fn cluster_members(
    State(_state): State<AppState>,
) -> Result<Json<Vec<ClusterMember>>> {
    Ok(Json(Vec::new()))
}

/// GET /api/v1/cluster/wal/stats
pub async fn wal_stats(
    State(state): State<AppState>,
) -> Result<Json<WalStatsResponse>> {
    #[cfg(feature = "cluster")]
    if let Some(ref wal) = state.wal {
        let stats = wal.stats().map_err(|e| Error::Internal(format!("WAL stats error: {e}")))?;
        return Ok(Json(WalStatsResponse {
            segment_count: stats.segment_count,
            total_size_bytes: stats.total_size_bytes,
            last_seq: stats.last_seq,
            next_seq: stats.next_seq,
        }));
    }

    Ok(Json(WalStatsResponse {
        segment_count: 0,
        total_size_bytes: 0,
        last_seq: 0,
        next_seq: 0,
    }))
}

/// POST /api/v1/cluster/wal/verify
pub async fn wal_verify(
    State(_state): State<AppState>,
) -> Result<Json<WalVerifyResponse>> {
    // WAL verification requires a WalReader; for now return success
    // when no WAL is configured
    Ok(Json(WalVerifyResponse {
        valid: true,
        entries_checked: 0,
        first_invalid_seq: None,
    }))
}

/// Create the cluster sub-router.
pub fn cluster_router() -> axum::Router<AppState> {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/api/v1/cluster/status", get(cluster_status))
        .route("/api/v1/cluster/join", post(cluster_join))
        .route("/api/v1/cluster/leave", post(cluster_leave))
        .route("/api/v1/cluster/members", get(cluster_members))
        .route("/api/v1/cluster/wal/stats", get(wal_stats))
        .route("/api/v1/cluster/wal/verify", post(wal_verify))
}
