// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Graph statistics business logic shared by REST and MCP.

use crate::error::Result;
use crate::rest::{GraphStatsDto, ServerStatsDto, StatsResponse};
use crate::state::AppState;

/// Compute graph and server statistics (triple count and related metrics).
pub async fn graph_stats(state: &AppState) -> Result<StatsResponse> {
    let stats = state.stats().await;

    Ok(StatsResponse {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stats_on_empty_graph() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let stats = graph_stats(&state).await.unwrap();
        assert_eq!(stats.graph.triple_count, 0);
        assert_eq!(stats.graph.subject_count, 0);
        assert_eq!(stats.graph.predicate_count, 0);
        assert_eq!(stats.graph.object_count, 0);
    }
}
