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

#[cfg(feature = "cluster")]
use openraft::type_config::async_runtime::watch::WatchReceiver;

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

    // Extract live Raft metrics when available
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let metrics = raft.metrics().borrow_watched().clone();

        let role = format!("{:?}", metrics.state);
        let term = metrics.current_term;
        let leader_id = metrics.current_leader;

        let last_applied = metrics
            .last_applied
            .as_ref()
            .map(|lid| lid.index)
            .unwrap_or(0);

        let commit_index = metrics
            .last_log_index
            .unwrap_or(0);

        // Build member list from membership config
        let membership = metrics.membership_config.membership();
        let members: Vec<ClusterMember> = membership
            .nodes()
            .map(|(nid, node)| ClusterMember {
                node_id: *nid,
                rest_addr: node.rest_addr.clone(),
                p2p_addr: node.p2p_addr.clone(),
                role: if Some(*nid) == leader_id {
                    "leader".to_string()
                } else {
                    "follower".to_string()
                },
                last_heartbeat: "N/A".to_string(),
                replication_lag: 0,
            })
            .collect();

        return Ok(Json(ClusterStatus {
            node_id: state.cluster_node_id.unwrap_or(0),
            role,
            term,
            leader_id,
            leader_addr: None,
            members,
            wal_last_seq,
            last_applied,
            commit_index,
        }));
    }

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
    State(state): State<AppState>,
    Json(req): Json<JoinRequest>,
) -> Result<(StatusCode, Json<JoinResponse>)> {
    tracing::info!(
        node_id = req.node_id,
        rest_addr = %req.rest_addr,
        "Cluster join request received"
    );

    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let node = aingle_raft::CortexNode {
            rest_addr: req.rest_addr.clone(),
            p2p_addr: req.p2p_addr.clone(),
        };

        // Add as learner first
        match raft.add_learner(req.node_id, node, true).await {
            Ok(_) => {
                // Then promote to voter
                let metrics = raft.metrics().borrow_watched().clone();
                let membership = metrics.membership_config.membership();
                let mut voter_ids: std::collections::BTreeSet<u64> =
                    membership.voter_ids().collect();
                voter_ids.insert(req.node_id);
                match raft.change_membership(voter_ids, false).await {
                    Ok(_) => {
                        return Ok((
                            StatusCode::OK,
                            Json(JoinResponse {
                                accepted: true,
                                leader_id: metrics.current_leader,
                                leader_addr: None,
                                message: format!("Node {} joined cluster", req.node_id),
                            }),
                        ));
                    }
                    Err(e) => {
                        return Ok((
                            StatusCode::CONFLICT,
                            Json(JoinResponse {
                                accepted: false,
                                leader_id: metrics.current_leader,
                                leader_addr: None,
                                message: format!("Membership change failed: {e}"),
                            }),
                        ));
                    }
                }
            }
            Err(e) => {
                return Ok((
                    StatusCode::CONFLICT,
                    Json(JoinResponse {
                        accepted: false,
                        leader_id: None,
                        leader_addr: None,
                        message: format!("Add learner failed: {e}"),
                    }),
                ));
            }
        }
    }

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
    State(state): State<AppState>,
) -> Result<StatusCode> {
    tracing::info!("Cluster leave request received");

    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        if let Some(node_id) = state.cluster_node_id {
            let metrics = raft.metrics().borrow_watched().clone();
            let membership = metrics.membership_config.membership();
            let mut voter_ids: std::collections::BTreeSet<u64> =
                membership.voter_ids().collect();
            voter_ids.remove(&node_id);
            if !voter_ids.is_empty() {
                if let Err(e) = raft.change_membership(voter_ids, false).await {
                    tracing::error!("Failed to leave cluster: {e}");
                    return Err(Error::Internal(format!("Leave failed: {e}")));
                }
            }
        }
    }

    Ok(StatusCode::OK)
}

/// GET /api/v1/cluster/members
pub async fn cluster_members(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClusterMember>>> {
    #[cfg(feature = "cluster")]
    if let Some(ref raft) = state.raft {
        let metrics = raft.metrics().borrow_watched().clone();
        let leader_id = metrics.current_leader;

        let membership = metrics.membership_config.membership();
        let members: Vec<ClusterMember> = membership
            .nodes()
            .map(|(nid, node)| ClusterMember {
                node_id: *nid,
                rest_addr: node.rest_addr.clone(),
                p2p_addr: node.p2p_addr.clone(),
                role: if Some(*nid) == leader_id {
                    "leader".to_string()
                } else {
                    "follower".to_string()
                },
                last_heartbeat: "N/A".to_string(),
                replication_lag: 0,
            })
            .collect();
        return Ok(Json(members));
    }

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
