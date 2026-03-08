//! REST endpoints for P2P status and peer management.

use crate::state::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;

/// Mount P2P routes.
pub fn p2p_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/p2p/status", get(p2p_status))
        .route("/api/v1/p2p/peers", get(list_peers).post(add_peer))
        .route("/api/v1/p2p/peers/{node_id}", delete(remove_peer))
}

#[derive(Deserialize)]
struct AddPeerRequest {
    addr: String,
}

async fn p2p_status(State(state): State<AppState>) -> impl IntoResponse {
    let p2p = match &state.p2p {
        Some(mgr) => mgr,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "P2P not enabled"})),
            )
                .into_response()
        }
    };
    let status = p2p.status().await;
    (StatusCode::OK, Json(serde_json::to_value(status).unwrap())).into_response()
}

async fn list_peers(State(state): State<AppState>) -> impl IntoResponse {
    let p2p = match &state.p2p {
        Some(mgr) => mgr,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "P2P not enabled"})),
            )
                .into_response()
        }
    };
    let status = p2p.status().await;
    (
        StatusCode::OK,
        Json(serde_json::to_value(&status.connected_peers).unwrap()),
    )
        .into_response()
}

async fn add_peer(
    State(state): State<AppState>,
    Json(body): Json<AddPeerRequest>,
) -> impl IntoResponse {
    let p2p = match &state.p2p {
        Some(mgr) => mgr,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "P2P not enabled"})),
            )
                .into_response()
        }
    };

    let addr = match body.addr.parse() {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid address"})),
            )
                .into_response()
        }
    };

    match p2p.add_peer(addr).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "connected", "addr": body.addr})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn remove_peer(
    State(state): State<AppState>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let p2p = match &state.p2p {
        Some(mgr) => mgr,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "P2P not enabled"})),
            )
                .into_response()
        }
    };

    // Find the peer address by matching node_id prefix in connected peers.
    let status = p2p.status().await;
    let peer_addr = status
        .connected_peers
        .iter()
        .find(|p| p.addr.contains(&node_id))
        .map(|p| p.addr.clone());

    match peer_addr {
        Some(addr_str) => {
            if let Ok(addr) = addr_str.parse() {
                p2p.remove_peer(addr).await;
                (
                    StatusCode::OK,
                    Json(serde_json::json!({"status": "disconnected"})),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "address parse error"})),
                )
                    .into_response()
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "peer not found"})),
        )
            .into_response(),
    }
}
