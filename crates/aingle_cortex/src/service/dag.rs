// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG provenance business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::rest::dag::{action_to_dto, DagActionDto};
use crate::state::AppState;

/// Return DAG actions affecting a subject, newest first, up to `limit`.
pub async fn history_by_subject(
    state: &AppState,
    subject: &str,
    limit: usize,
) -> Result<Vec<DagActionDto>> {
    let graph = state.graph.read().await;
    let actions = graph
        .dag_history_by_subject(subject, limit)
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(actions.iter().map(action_to_dto).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_of_unknown_subject_is_empty() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        // A fresh in-memory graph has no DAG store; `dag_history_by_subject`
        // returns a "DAG not enabled" error until the DAG is enabled.
        // Enable it the way the node does at startup, then query.
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let h = history_by_subject(&state, "ex:nobody", 10).await.unwrap();
        assert!(h.is_empty());
    }
}
