// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Grounded retrieval: turn a question into cited, provenance-backed context with
//! an explicit groundedness signal, so an LLM answers only from verifiable sources.

use crate::error::Result;
use crate::state::AppState;
use serde::Serialize;

/// Similarity at/above which retrieval is considered well-grounded.
const GROUND_HIGH: f32 = 0.55;
/// Similarity below which retrieval is considered ungrounded.
const GROUND_LOW: f32 = 0.30;

/// A cited chunk of source context.
#[derive(Debug, Clone, Serialize)]
pub struct ContextChunk {
    pub text: String,
    pub source: String,
    pub lines: String,
    pub relevance: f32,
    /// Hex hash of the signed DAG action that recorded this source — verifiable
    /// via the DAG history/action API. `None` when the source has no signed action.
    pub provenance_anchor: Option<String>,
    pub ingested_at: Option<String>,
}

/// The grounded answer context returned to the model.
#[derive(Debug, Clone, Serialize)]
pub struct GroundedContext {
    pub groundedness: String, // "grounded" | "weak" | "ungrounded"
    pub answer_context: Vec<ContextChunk>,
    pub gaps: Vec<String>,
    /// Instruction echoed to the model to keep it on the cited path.
    pub instruction: String,
}

use ineru::MemoryQuery;

/// Retrieve grounded context for `question`. Pulls the top-`k` semantically
/// similar chunks from Ineru, attaches each chunk's signed provenance from the
/// DAG (latest signed action affecting its source path), and computes a
/// groundedness signal from the best similarity.
pub async fn ground(state: &AppState, question: &str, k: usize) -> Result<GroundedContext> {
    let k = k.max(1);

    let results = {
        let mem = state.memory.read().await;
        mem.recall(&MemoryQuery::text(question).with_limit(k))
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?
    };

    let mut answer_context = Vec::new();
    let mut best: f32 = 0.0;
    for r in &results {
        // Only consider chunk memories produced by ingestion.
        if r.entry.entry_type != crate::service::ingest::CHUNK_ENTRY_TYPE {
            continue;
        }
        let d = &r.entry.data;
        let source = d.get("source_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let ls = d.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0);
        let le = d.get("line_end").and_then(|v| v.as_u64()).unwrap_or(0);
        let text = d.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let (sig, ingested_at) = signed_provenance(state, &source).await;

        best = best.max(r.relevance);
        answer_context.push(ContextChunk {
            text,
            source,
            lines: format!("{ls}-{le}"),
            relevance: r.relevance,
            provenance_anchor: sig,
            ingested_at,
        });
    }

    let groundedness = if best >= GROUND_HIGH {
        "grounded"
    } else if best >= GROUND_LOW && !answer_context.is_empty() {
        "weak"
    } else {
        "ungrounded"
    };

    let mut gaps = Vec::new();
    if answer_context.is_empty() {
        gaps.push(format!("No ingested source matches: {question:?}."));
    } else if groundedness == "weak" {
        gaps.push("Retrieved context is only weakly related to the question.".to_string());
    }

    Ok(GroundedContext {
        groundedness: groundedness.to_string(),
        answer_context,
        gaps,
        instruction: "Answer ONLY from answer_context and cite each claim as \
            source:lines. If groundedness is not \"grounded\", say so explicitly \
            and do not invent facts."
            .to_string(),
    })
}

/// Look up the latest signed DAG action affecting `source_path` and return its
/// action hash (as provenance identifier) and timestamp, if any.
///
/// Adaptation note: `DagActionDto` has no `signature` field. Instead it has
/// `hash: String` (action hash) and `signed: bool`. We return the action hash
/// as the provenance identifier when the action is signed, or None otherwise.
/// The timestamp field is `timestamp: String` which matches the plan exactly.
async fn signed_provenance(state: &AppState, source_path: &str) -> (Option<String>, Option<String>) {
    #[cfg(feature = "dag")]
    {
        if source_path.is_empty() {
            return (None, None);
        }
        if let Ok(actions) = crate::service::dag::history_by_subject(state, source_path, 1).await {
            if let Some(a) = actions.first() {
                // DagActionDto has `hash: String` and `signed: bool` rather than a
                // `signature` field, so we use the action hash as the provenance token
                // when the action is signed.
                let sig = if a.signed { Some(a.hash.clone()) } else { None };
                return (sig, Some(a.timestamp.clone()));
            }
        }
        (None, None)
    }
    #[cfg(not(feature = "dag"))]
    {
        let _ = (state, source_path);
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn enabled_state() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        state
    }

    #[tokio::test]
    async fn empty_memory_is_ungrounded() {
        let state = enabled_state().await;
        let g = ground(&state, "anything at all", 5).await.unwrap();
        assert_eq!(g.groundedness, "ungrounded");
        assert!(g.answer_context.is_empty());
        assert!(!g.gaps.is_empty());
    }

    #[tokio::test]
    async fn grounds_after_ingest_with_source() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("adr.md"),
            "# Storage\n\nWe chose sled because of its exclusive lock semantics.\n",
        )
        .unwrap();
        let state = enabled_state().await;
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let g = ground(&state, "exclusive lock semantics sled", 5).await.unwrap();
        assert!(!g.answer_context.is_empty(), "should retrieve the ingested chunk");
        assert_eq!(g.answer_context[0].source, "adr.md");
        assert_ne!(g.groundedness, "ungrounded");
    }
}
