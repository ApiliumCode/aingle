// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG provenance business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::rest::dag::{
    action_to_dto, DagActionDto, DagStatsResponse, DagTipsResponse, PruneRequest, PruneResponse,
};
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

/// Return the current DAG tip hashes and their count.
pub async fn tips(state: &AppState) -> Result<DagTipsResponse> {
    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let tips = dag_store
        .tips()
        .map_err(|e| Error::Internal(e.to_string()))?;
    let tip_strings: Vec<String> = tips.iter().map(|h| h.to_hex()).collect();
    let count = tip_strings.len();

    Ok(DagTipsResponse {
        tips: tip_strings,
        count,
    })
}

/// Fetch a single DAG action by its hex hash. `NotFound` if absent.
pub async fn action(state: &AppState, hash: &str) -> Result<DagActionDto> {
    let action_hash = aingle_graph::dag::DagActionHash::from_hex(hash)
        .ok_or_else(|| Error::InvalidInput(format!("Invalid DAG action hash: {}", hash)))?;

    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let action = dag_store
        .get(&action_hash)
        .map_err(|e| Error::Internal(e.to_string()))?
        .ok_or_else(|| Error::NotFound(format!("DAG action {} not found", hash)))?;

    Ok(action_to_dto(&action))
}

/// Return an author's action chain, newest first, up to `limit`.
pub async fn chain(state: &AppState, author: &str, limit: usize) -> Result<Vec<DagActionDto>> {
    let author = aingle_graph::NodeId::named(author);

    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let actions = dag_store
        .chain(&author, limit)
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(actions.iter().map(action_to_dto).collect())
}

/// Return DAG statistics: action count and tip count.
pub async fn stats(state: &AppState) -> Result<DagStatsResponse> {
    let graph = state.graph.read().await;
    let dag_store = graph
        .dag_store()
        .ok_or_else(|| Error::Internal("DAG not enabled".into()))?;

    let action_count = dag_store.action_count();
    let tip_count = dag_store
        .tip_count()
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(DagStatsResponse {
        action_count,
        tip_count,
    })
}

/// Prune the DAG according to a retention policy, optionally checkpointing.
pub async fn prune(state: &AppState, req: PruneRequest) -> Result<PruneResponse> {
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

    Ok(PruneResponse {
        pruned_count: result.pruned_count,
        retained_count: result.retained_count,
        checkpoint_hash: result.checkpoint_hash.map(|h| h.to_hex()),
    })
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

    /// Enable the DAG on a fresh in-memory state, mirroring node startup.
    /// Without this, DAG service fns return `Error::Config("DAG not enabled")`.
    async fn enabled_state() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        state
    }

    #[tokio::test]
    async fn tips_of_empty_dag() {
        let state = enabled_state().await;
        let resp = tips(&state).await.unwrap();
        assert_eq!(resp.count, resp.tips.len());
    }

    #[tokio::test]
    async fn action_with_invalid_hash_is_invalid_input() {
        let state = enabled_state().await;
        let err = action(&state, "not-a-hash").await.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }

    #[tokio::test]
    async fn chain_of_unknown_author_is_empty() {
        let state = enabled_state().await;
        let c = chain(&state, "node:nobody", 10).await.unwrap();
        assert!(c.is_empty());
    }

    #[tokio::test]
    async fn stats_of_empty_dag() {
        let state = enabled_state().await;
        let s = stats(&state).await.unwrap();
        assert_eq!(s.action_count, 0);
    }

    #[tokio::test]
    async fn prune_keep_all_prunes_nothing() {
        let state = enabled_state().await;
        let resp = prune(
            &state,
            PruneRequest {
                policy: "keep_all".into(),
                value: 0,
                create_checkpoint: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.pruned_count, 0);
    }

    #[tokio::test]
    async fn prune_unknown_policy_is_invalid_input() {
        let state = enabled_state().await;
        let err = prune(
            &state,
            PruneRequest {
                policy: "bogus".into(),
                value: 0,
                create_checkpoint: false,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }
}
