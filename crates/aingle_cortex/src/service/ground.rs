// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Grounded retrieval: turn a question into cited, provenance-backed context with
//! an explicit groundedness signal, so an LLM answers only from verifiable sources.

use crate::error::Result;
use crate::state::AppState;
use serde::Serialize;

/// Number of strong chunks required to call retrieval "grounded". Requiring two
/// independent corroborating sources is a deliberate anti-hallucination policy:
/// a lone strong chunk is surfaced as "weak", not "grounded". The strong/weak
/// similarity cutoffs themselves come from the active embedder via
/// [`ineru::Embedder::relevance_thresholds`].
const MIN_CORROBORATING_CHUNKS: usize = 2;

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
    /// `true` when the index holds chunks but every stored embedding in the
    /// candidate pool is a placeholder (missing or all-zero), so no query can
    /// ground against it. This is the honest signal for a stale index that must
    /// be re-embedded — distinct from "ungrounded" (index is fine, topic absent).
    /// Never `true` for a healthy or genuinely empty index.
    #[serde(default)]
    pub index_stale: bool,
}

use ineru::MemoryQuery;

/// Retrieve grounded context for `question`. Pulls the top-`k` semantically
/// similar chunks from Ineru, attaches each chunk's signed provenance from the
/// DAG (latest signed action affecting its source path), and computes a
/// groundedness signal from the best similarity.
pub async fn ground(state: &AppState, question: &str, k: usize) -> Result<GroundedContext> {
    let k = k.max(1);
    let (ground_high, ground_low) = state.embedder.relevance_thresholds();

    let query_vec = state.embedder.embed_query(question);
    // Fetch a broad candidate pool: Ineru's composite recall score is keyword-
    // and importance-weighted (embedding is only a minor term), so we over-fetch
    // and re-rank by pure embedding cosine below. That makes grounding a true
    // semantic search whose scores match the embedder's `relevance_thresholds`.
    let fetch_limit = k.max(24);
    let results = {
        let mem = state.memory.read().await;
        mem.recall(
            &MemoryQuery::text(question)
                .with_limit(fetch_limit)
                .with_embedding(query_vec.clone()),
        )
        .map_err(|e| crate::error::Error::Internal(e.to_string()))?
    };

    let mut answer_context = Vec::new();
    // Track the health of the candidate pool so a placeholder/stale index is
    // reported honestly instead of masquerading as a plain "ungrounded" miss.
    let mut chunk_total = 0usize;
    let mut chunk_degenerate = 0usize;
    for r in &results {
        // Only consider chunk memories produced by ingestion.
        if r.entry.entry_type != crate::service::ingest::CHUNK_ENTRY_TYPE {
            continue;
        }
        chunk_total += 1;
        // Semantic relevance = cosine(query, chunk) from the active embedder,
        // not Ineru's composite recall score. A stored embedding that is missing
        // or all-zero is a placeholder (pending model persisted, or a poisoned
        // legacy index): it scores 0 against every query, so it can never ground
        // an answer. Skip it AND count it — a pool that is entirely degenerate is
        // the fingerprint of a stale index that needs re-embedding.
        let relevance = match &r.entry.embedding {
            Some(emb) if emb.0.iter().any(|x| *x != 0.0) => query_vec.cosine_similarity(emb),
            _ => {
                chunk_degenerate += 1;
                continue;
            }
        };
        let d = &r.entry.data;
        let source = d
            .get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ls = d.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0);
        let le = d.get("line_end").and_then(|v| v.as_u64()).unwrap_or(0);
        let text = d
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let (sig, ingested_at) = signed_provenance(state, &source).await;

        answer_context.push(ContextChunk {
            text,
            source,
            lines: format!("{ls}-{le}"),
            relevance,
            provenance_anchor: sig,
            ingested_at,
        });
    }

    // Re-rank by semantic relevance and keep the top-k.
    answer_context.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    answer_context.truncate(k);
    let best: f32 = answer_context.first().map(|c| c.relevance).unwrap_or(0.0);

    // Require at least MIN_CORROBORATING_CHUNKS strong matches for "grounded";
    // a single strong chunk is only "weak" (independent corroboration guard).
    let strong = answer_context
        .iter()
        .filter(|c| c.relevance >= ground_high)
        .count();
    let groundedness = if best >= ground_high && strong >= MIN_CORROBORATING_CHUNKS {
        "grounded"
    } else if best >= ground_low && !answer_context.is_empty() {
        "weak"
    } else {
        "ungrounded"
    };

    // A candidate pool that held chunks but whose every stored embedding was a
    // placeholder means the index is stale, NOT that the topic is absent. This is
    // the guard against the silent-retrieval failure: chunks exist, the engine
    // reports Ready, yet nothing can ever ground because the vectors are zeros.
    let index_stale = chunk_total > 0 && chunk_degenerate == chunk_total;

    let mut gaps = Vec::new();
    if index_stale {
        gaps.push(
            "The semantic index is stale: stored embeddings are placeholders, so no query \
             can be grounded. Re-index the vault to rebuild the embeddings."
                .to_string(),
        );
    } else if answer_context.is_empty() {
        gaps.push(format!("No ingested source matches: {question:?}."));
    } else if groundedness == "weak" {
        if best >= ground_high && strong < MIN_CORROBORATING_CHUNKS {
            gaps.push(
                "Only one source corroborates this; a second is needed to be grounded.".to_string(),
            );
        } else {
            gaps.push("Retrieved context is only weakly related to the question.".to_string());
        }
    } else if groundedness == "ungrounded" {
        // Chunks were retrieved but none are relevant enough to ground an answer.
        // Surface the gap so the engine stays honest rather than silently empty.
        gaps.push(
            "Retrieved context is not relevant enough to ground an answer on this topic."
                .to_string(),
        );
    }

    Ok(GroundedContext {
        groundedness: groundedness.to_string(),
        answer_context,
        gaps,
        instruction: "Answer ONLY from answer_context and cite each claim as \
            source:lines. If groundedness is not \"grounded\", say so explicitly \
            and do not invent facts."
            .to_string(),
        index_stale,
    })
}

/// Look up the latest signed DAG action affecting `source_path` and return its
/// action hash (as provenance identifier) and timestamp, if any.
///
/// Adaptation note: `DagActionDto` has no `signature` field. Instead it has
/// `hash: String` (action hash) and `signed: bool`. We return the action hash
/// as the provenance identifier when the action is signed, or None otherwise.
/// The timestamp field is `timestamp: String` which matches the plan exactly.
async fn signed_provenance(
    state: &AppState,
    source_path: &str,
) -> (Option<String>, Option<String>) {
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
        assert!(
            !g.index_stale,
            "a genuinely empty index is not stale — there are no chunks to be placeholders"
        );
    }

    /// A 384-dim embedder that emits ONLY zero vectors — reproduces the poisoned
    /// index (a placeholder model that got persisted, or a same-dim swap that
    /// left every stored vector at zero).
    struct Zero384;
    impl ineru::Embedder for Zero384 {
        fn embed_passage(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.0; 384])
        }
        fn embed_query(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.0; 384])
        }
        fn dimensions(&self) -> usize {
            384
        }
    }

    #[tokio::test]
    async fn stale_index_is_reported_not_silently_ungrounded() {
        // The regression: chunks EXIST and the engine reports Ready, yet every
        // stored embedding is a placeholder so nothing can ever ground. Before the
        // fix this returned a plain "ungrounded" and looked like an empty vault.
        // Now it must raise `index_stale` and say a re-index is required.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("n.md"),
            "# N\n\nsled has exclusive lock semantics.\n",
        )
        .unwrap();
        let state =
            AppState::with_db_path_and_embedder(":memory:", None, std::sync::Arc::new(Zero384))
                .unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let g = ground(&state, "exclusive lock semantics", 5).await.unwrap();
        assert!(
            g.index_stale,
            "an all-placeholder candidate pool must be reported as a stale index"
        );
        assert_eq!(
            g.groundedness, "ungrounded",
            "a stale index cannot ground anything"
        );
        assert!(
            g.gaps.iter().any(|s| s.to_lowercase().contains("stale")),
            "the gap must tell the user to re-index; got {:?}",
            g.gaps
        );
    }

    #[tokio::test]
    async fn single_corroborating_chunk_is_weak_not_grounded() {
        // One source, one chunk: even a strong similarity match must not be called
        // "grounded" — with the placeholder embedder a lone high score can be
        // spurious, so a single corroborating chunk is downgraded to "weak".
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("note.md"),
            "# Note\n\nWe chose sled for its exclusive lock semantics.\n",
        )
        .unwrap();
        let state = enabled_state().await;
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        // Query the chunk almost verbatim so the lone chunk scores well above HIGH.
        let g = ground(&state, "We chose sled for its exclusive lock semantics.", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.is_empty(),
            "should retrieve the one chunk"
        );
        assert_eq!(
            g.groundedness, "weak",
            "a single corroborating chunk must be weak, not grounded; ctx: {:?}",
            g.answer_context
        );
    }

    #[tokio::test]
    async fn two_corroborating_sources_are_grounded() {
        // The same fact stated in two separate files yields two strong chunks for a
        // matching query — that independent corroboration is what makes it grounded.
        let dir = tempfile::tempdir().unwrap();
        let fact = "# Doc\n\nThe quorum read requires a valid leader lease.\n";
        std::fs::write(dir.path().join("a.md"), fact).unwrap();
        std::fs::write(dir.path().join("b.md"), fact).unwrap();
        let state = enabled_state().await;
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let g = ground(&state, "The quorum read requires a valid leader lease.", 5)
            .await
            .unwrap();
        let strong = g
            .answer_context
            .iter()
            .filter(|c| c.relevance >= 0.55)
            .count();
        assert!(
            strong >= 2,
            "two sources should both score strongly; ctx: {:?}",
            g.answer_context
        );
        assert_eq!(
            g.groundedness, "grounded",
            "two corroborating strong chunks must be grounded; ctx: {:?}",
            g.answer_context
        );
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

        let g = ground(&state, "exclusive lock semantics sled", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.is_empty(),
            "should retrieve the ingested chunk"
        );
        assert_eq!(g.answer_context[0].source, "adr.md");
        assert_ne!(g.groundedness, "ungrounded");
    }

    /// End-to-end acceptance test for the real neural embedder: a topical query
    /// must be grounded while an off-topic query is ungrounded. Gated on the
    /// `neural-embeddings` feature and skips if the model files are absent.
    /// Requires `ORT_DYLIB_PATH` to point at an onnxruntime dynamic library.
    #[cfg(feature = "neural-embeddings")]
    #[tokio::test]
    async fn neural_grounding_is_topical() {
        let model_dir = std::env::var("INERU_E5_MODEL_DIR").unwrap_or_else(|_| {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../ineru/test-models/multilingual-e5-small"
            )
            .to_string()
        });
        if !std::path::Path::new(&model_dir)
            .join("onnx/model.onnx")
            .exists()
        {
            eprintln!("skipping: e5 model not found at {model_dir}");
            return;
        }

        let embedder = crate::embedder::build_embedder(Some(&model_dir));
        assert_eq!(embedder.dimensions(), 384, "neural embedder must be active");

        let state = AppState::with_db_path_and_embedder(":memory:", None, embedder).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("dogs.md"),
            "# Cuidado de perros\n\nLos perros necesitan paseos diarios, agua fresca y una dieta equilibrada para estar sanos.\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("dogs2.md"),
            "# Mascotas\n\nUn perro sano requiere ejercicio diario, hidratación constante y alimentación balanceada.\n",
        )
        .unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let topical = ground(&state, "¿Cómo debo cuidar a mi perro?", 5)
            .await
            .unwrap();
        assert_ne!(
            topical.groundedness, "ungrounded",
            "a dog-care question must find the dog-care notes; ctx: {:?}",
            topical.answer_context
        );

        let off_topic = ground(
            &state,
            "¿Cuál fue el resultado de las elecciones presidenciales?",
            5,
        )
        .await
        .unwrap();
        assert_eq!(
            off_topic.groundedness, "ungrounded",
            "an unrelated question must be ungrounded; ctx: {:?}",
            off_topic.answer_context
        );
    }
}
