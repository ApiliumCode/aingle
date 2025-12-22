//! Statistics and health check endpoints

use axum::{extract::State, Json};
use serde::Serialize;

use crate::error::Result;
use crate::state::AppState;

/// Graph statistics response
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    /// Graph statistics
    pub graph: GraphStatsDto,
    /// Server statistics
    pub server: ServerStatsDto,
}

/// Graph statistics DTO
#[derive(Debug, Serialize)]
pub struct GraphStatsDto {
    /// Total number of triples
    pub triple_count: usize,
    /// Number of unique subjects
    pub subject_count: usize,
    /// Number of unique predicates
    pub predicate_count: usize,
    /// Number of unique objects
    pub object_count: usize,
}

/// Server statistics DTO
#[derive(Debug, Serialize)]
pub struct ServerStatsDto {
    /// Number of connected WebSocket clients
    pub connected_clients: usize,
    /// Server uptime in seconds
    pub uptime_seconds: u64,
    /// Version
    pub version: String,
}

/// Get graph and server statistics
///
/// GET /api/v1/stats
pub async fn get_stats(State(state): State<AppState>) -> Result<Json<StatsResponse>> {
    let stats = state.stats().await;

    Ok(Json(StatsResponse {
        graph: GraphStatsDto {
            triple_count: stats.triple_count,
            subject_count: stats.subject_count,
            predicate_count: stats.predicate_count,
            object_count: stats.object_count,
        },
        server: ServerStatsDto {
            connected_clients: stats.connected_clients,
            uptime_seconds: 0, // TODO: track actual uptime
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    }))
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Overall health status
    pub status: String,
    /// Component health
    pub components: ComponentHealth,
}

/// Component health
#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    /// Graph database health
    pub graph: ComponentStatus,
    /// Logic engine health
    pub logic: ComponentStatus,
}

/// Individual component status
#[derive(Debug, Serialize)]
pub struct ComponentStatus {
    /// Status: "healthy", "degraded", "unhealthy"
    pub status: String,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Health check endpoint
///
/// GET /api/v1/health
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    // Check graph health
    let graph_health = {
        let graph = state.graph.read().await;
        let stats = graph.stats();
        ComponentStatus {
            status: "healthy".to_string(),
            message: Some(format!("{} triples", stats.triple_count)),
        }
    };

    // Check logic engine health
    let logic_health = {
        let logic = state.logic.read().await;
        let stats = logic.stats();
        ComponentStatus {
            status: "healthy".to_string(),
            message: Some(format!("{} rules evaluated", stats.rules_evaluated)),
        }
    };

    // Determine overall status
    let overall_status = if graph_health.status == "healthy" && logic_health.status == "healthy" {
        "healthy"
    } else if graph_health.status == "unhealthy" || logic_health.status == "unhealthy" {
        "unhealthy"
    } else {
        "degraded"
    };

    Json(HealthResponse {
        status: overall_status.to_string(),
        components: ComponentHealth {
            graph: graph_health,
            logic: logic_health,
        },
    })
}
