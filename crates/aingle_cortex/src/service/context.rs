// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Semantic note-context: for an active note, surface the notes that are
//! semantically related (by neural embeddings) even when never linked, each
//! with the matching passage and signed provenance.

use std::collections::{BTreeMap, BTreeSet};

use crate::service::triple_util::{obj_string, resolve_link_target};

/// The semantic context for one note — the semantically related notes, even
/// when never explicitly linked.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct NoteContext {
    /// `true` when the embedder has enough dimensions to produce meaningful
    /// semantic similarity (≥ `SEMANTIC_MIN_DIMS`). `false` means the hash
    /// fallback is active and no neighbor search was attempted.
    pub semantic_ready: bool,
    pub neighbors: Vec<Neighbor>,
}

/// A note that is semantically related to the active note.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Neighbor {
    /// Full relative path — the canonical identity used everywhere else.
    pub path: String,
    /// Best chunk cosine similarity against the active note's query vector.
    pub score: f32,
    /// The matching chunk text, ≤ 200 chars (char-safe), with `…` appended
    /// if truncated.
    pub passage: Option<String>,
    /// Hex hash of the signed DAG action that recorded this source (🔒 anchor).
    /// `None` when the feature is off or no signed action exists.
    pub provenance_anchor: Option<String>,
    /// `true` if the active note already has an explicit `links_to` edge to
    /// this neighbor — so the UI can distinguish "related and linked" from
    /// "related but not yet linked".
    pub already_linked: bool,
}

/// Minimum embedder dimensionality required to attempt semantic neighbor search.
/// The 64-d hash embedder does not produce meaningful cosine similarity for
/// cross-note retrieval; this gate keeps the result honest.
const SEMANTIC_MIN_DIMS: usize = 128;

/// Minimum cosine for a note to count as a semantic neighbor. Calibrated for
/// note-to-note neural similarity: multilingual-e5 assigns a high baseline
/// (~0.83) to any same-language text, so the embedder's grounding `low`
/// threshold (0.77) is too permissive here. Mirrors vault_map's
/// SEMANTIC_THRESHOLD rationale (related notes ~0.90+, unrelated ~0.81-0.83).
/// Follow-up: make this per-embedder if more neural models are added.
pub const NEIGHBOR_FLOOR: f32 = 0.88;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Retrieve a signed provenance anchor hash for a note path, if available.
async fn provenance_anchor_for(state: &crate::state::AppState, src: &str) -> Option<String> {
    #[cfg(feature = "dag")]
    {
        match crate::service::dag::history_by_subject(state, src, 1).await {
            Ok(a) => a.first().filter(|x| x.signed).map(|x| x.hash.clone()),
            Err(_) => None,
        }
    }
    #[cfg(not(feature = "dag"))]
    {
        let _ = (state, src);
        None
    }
}

// ---------------------------------------------------------------------------
// Core retrieval
// ---------------------------------------------------------------------------

/// Compute the semantic neighbors of `note` — up to `limit` related notes,
/// ranked by embedding cosine similarity, each with a matching passage and
/// optional signed provenance anchor.
pub async fn note_context(
    state: &crate::state::AppState,
    note: &str,
    limit: usize,
) -> NoteContext {
    use aingle_graph::{Predicate, TriplePattern};
    use ineru::MemoryQuery;

    // 1. Semantic gate: only proceed when the embedder is neural-grade.
    if state.embedder.dimensions() < SEMANTIC_MIN_DIMS {
        return NoteContext {
            semantic_ready: false,
            neighbors: vec![],
        };
    }

    // 2. Build the note set (subjects of PRED_SOURCE_HASH) + basename index,
    //    and collect all links_to triples.
    let strip =
        |n: String| n.trim_start_matches('<').trim_end_matches('>').to_string();

    let (notes, links): (Vec<String>, Vec<(String, String)>) = {
        let g = state.graph.read().await;
        let collect = |pred: &str| -> Vec<(String, String)> {
            g.find(TriplePattern::any().with_predicate(Predicate::named(pred)))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|t| {
                    obj_string(&t).map(|o| (strip(t.subject.to_string()), o))
                })
                .collect()
        };
        let mut ns: Vec<String> = collect(crate::service::ingest::PRED_SOURCE_HASH)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        ns.sort();
        ns.dedup();
        let links = collect("links_to");
        (ns, links)
    };

    let note_set: BTreeSet<&str> = notes.iter().map(|s| s.as_str()).collect();

    // basename → first full path (for wikilink resolution).
    let mut by_base: BTreeMap<String, String> = BTreeMap::new();
    for n in &notes {
        by_base
            .entry(crate::service::vault_map::basename(n))
            .or_insert_with(|| n.clone());
    }

    let resolve = |target: &str| -> Option<String> {
        resolve_link_target(target, &note_set, &by_base)
    };

    // 3. Compute `outgoing_set`: full paths that the active `note` links to.
    let outgoing_set: BTreeSet<String> = links
        .iter()
        .filter(|(src, _)| src == note)
        .filter_map(|(_, target)| resolve(target))
        .filter(|p| p != note)
        .collect();

    // 4. Build the active note's query text from its own chunks.
    //    Read STM and LTM separately and filter to `note` immediately — avoids
    //    allocating a merged Vec of every entry in memory.
    let mut own_text = String::new();
    let (stm_entries, ltm_entries) = {
        let mem = state.memory.read().await;
        (mem.stm.all_entries(), mem.ltm.all_entries())
    };
    for e in stm_entries.iter().chain(ltm_entries.iter()) {
        if e.entry_type != crate::service::ingest::CHUNK_ENTRY_TYPE {
            continue;
        }
        if let (Some(p), Some(t)) = (
            e.data.get("source_path").and_then(|v| v.as_str()),
            e.data.get("text").and_then(|v| v.as_str()),
        ) {
            if p == note {
                own_text.push('\n');
                own_text.push_str(t);
            }
        }
    }

    let query_text: String = if own_text.trim().is_empty() {
        crate::service::vault_map::basename(note)
    } else {
        own_text.clone()
    };

    let q = state.embedder.embed_query(&query_text);

    // 5. Over-fetch from memory and re-rank by cosine similarity.
    let fetch_limit = (limit * 8).max(48);
    let results = {
        let mem = state.memory.read().await;
        mem.recall(
            &MemoryQuery::text(&query_text)
                .with_embedding(q.clone())
                .with_limit(fetch_limit),
        )
        .unwrap_or_default()
    };

    // Per-source best (rel, text).
    let mut best_by_src: BTreeMap<String, (f32, String)> = BTreeMap::new();

    for r in &results {
        if r.entry.entry_type != crate::service::ingest::CHUNK_ENTRY_TYPE {
            continue;
        }
        let emb = match &r.entry.embedding {
            Some(e) => e,
            None => continue,
        };
        let rel = q.cosine_similarity(emb);
        if rel < NEIGHBOR_FLOOR {
            continue;
        }
        let d = &r.entry.data;
        let src = match d.get("source_path").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if src == note {
            continue;
        }
        if crate::service::vault_map::is_maps_path(&src) {
            continue;
        }
        if !note_set.contains(src.as_str()) {
            continue;
        }
        // Only clone the chunk text when actually inserting or replacing the
        // best entry — avoids a clone on every already-occupied iteration.
        let text = d.get("text").and_then(|v| v.as_str()).unwrap_or("");
        match best_by_src.entry(src) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert((rel, text.to_string()));
            }
            std::collections::btree_map::Entry::Occupied(mut e) => {
                if rel > e.get().0 {
                    *e.get_mut() = (rel, text.to_string());
                }
            }
        }
    }

    // 6. Build Neighbor list (provenance is None for now), sort by score desc,
    //    truncate to `limit`, then resolve provenance only for the survivors.
    //    This cuts up to ~48 DAG reads (fetch_limit) down to `limit` (≤ 10).
    let mut neighbors: Vec<Neighbor> = Vec::with_capacity(best_by_src.len());
    for (src, (rel, chunk_text)) in best_by_src {
        let passage = Some({
            let t = chunk_text.trim();
            if t.chars().count() > 200 {
                let cut: String = t.chars().take(200).collect();
                format!("{cut}…")
            } else {
                t.to_string()
            }
        });
        let already_linked = outgoing_set.contains(&src);
        neighbors.push(Neighbor {
            path: src,
            score: rel,
            passage,
            provenance_anchor: None,
            already_linked,
        });
    }

    // NaN-safe descending sort (mirrors ground.rs).
    neighbors.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    neighbors.truncate(limit);

    // Resolve provenance only for the survivors (typically ≤ limit DAG reads).
    for n in &mut neighbors {
        n.provenance_anchor = provenance_anchor_for(state, &n.path).await;
    }

    NoteContext {
        semantic_ready: true,
        neighbors,
    }
}

// ---------------------------------------------------------------------------
// Cached variant
// ---------------------------------------------------------------------------

/// Like [`note_context`] but memoised on `(triple_count, total_memory_bytes)`.
///
/// The map key is `(note_path, limit)` so that MCP calls with different `limit`
/// values are cached independently and never serve a stale neighbor count.
pub async fn note_context_cached(
    state: &crate::state::AppState,
    note: &str,
    limit: usize,
) -> NoteContext {
    let tc = { state.graph.read().await.stats().triple_count };
    let mem_bytes = { state.memory.read().await.stats().total_memory_bytes };
    let version_key = (tc, mem_bytes);
    let map_key = (note.to_string(), limit);

    // Check cache — release lock before any await.
    {
        let cache = state
            .note_context_cache
            .lock()
            .expect("note_context cache poisoned");
        if let Some((cached_key, ctx)) = cache.get(&map_key) {
            if *cached_key == version_key {
                return ctx.clone();
            }
        }
    }

    // Compute without holding the mutex.
    let result = note_context(state, note, limit).await;

    // Store result.
    {
        let mut cache = state
            .note_context_cache
            .lock()
            .expect("note_context cache poisoned");
        // Simple growth cap: if more than 256 entries are cached, clear entirely
        // before inserting. This bounds memory without per-entry LRU bookkeeping;
        // a typical Akashi session edits far fewer than 256 (note, limit) pairs.
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(map_key, (version_key, result.clone()));
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aingle_graph::{NodeId, Predicate, Triple, Value};
    use ineru::{Embedder, Embedding, MemoryEntry};

    use crate::state::AppState;

    // -----------------------------------------------------------------------
    // Stub embedder: 128-dim, deterministic, text-content-aware.
    // - text containing "alpha" → [1.0, 0.0, 0.0, …]  (unit basis e0)
    // - text containing "zzz"   → [0.0, 1.0, 0.0, …]  (unit basis e1)
    // - query for "alpha"       → [1.0, 0.0, 0.0, …]  (same basis)
    // Cosine("alpha","alpha") = 1.0  ≥ low threshold (0.1)  → pass
    // Cosine("alpha","zzz")   = 0.0  <  low threshold       → filtered out
    // -----------------------------------------------------------------------
    struct StubEmbedder;

    impl Embedder for StubEmbedder {
        fn embed_passage(&self, text: &str) -> Embedding {
            let mut v = vec![0.0_f32; 128];
            if text.contains("alpha") {
                v[0] = 1.0;
            } else if text.contains("zzz") {
                v[1] = 1.0;
            } else {
                // default: non-zero to avoid zero-vector edge case
                v[2] = 1.0;
            }
            Embedding::new(v)
        }

        fn embed_query(&self, text: &str) -> Embedding {
            // Reuse passage embedding logic for query — correct for symmetric
            // tests; for real asymmetric models the trait would differ.
            self.embed_passage(text)
        }

        fn dimensions(&self) -> usize {
            128
        }

        fn relevance_thresholds(&self) -> (f32, f32) {
            // high=0.5, low=0.1 — alpha/alpha scores 1.0 (pass), alpha/zzz
            // scores 0.0 (filtered).
            (0.5, 0.1)
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn stub_state() -> AppState {
        AppState::with_db_path_and_embedder(
            ":memory:",
            None,
            Arc::new(StubEmbedder),
        )
        .unwrap()
    }

    async fn insert_triples(state: &AppState, triples: &[(&str, &str, &str)]) {
        let g = state.graph.write().await;
        for (s, p, o) in triples {
            g.insert(Triple::new(
                NodeId::named(*s),
                Predicate::named(*p),
                Value::literal(*o),
            ))
            .unwrap();
        }
    }

    async fn insert_chunk(state: &AppState, source_path: &str, text: &str, emb: Vec<f32>) {
        let mut mem = state.memory.write().await;
        let mut e = MemoryEntry::new(
            crate::service::ingest::CHUNK_ENTRY_TYPE,
            serde_json::json!({ "text": text, "source_path": source_path }),
        );
        e.embedding = Some(Embedding::new(emb));
        mem.remember(e).unwrap();
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// Default state uses 64-d hash embedder → semantic gate fires → short-circuit.
    #[tokio::test]
    async fn hash_grade_embedder_short_circuits() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let ctx = super::note_context(&state, "active.md", 5).await;
        assert!(!ctx.semantic_ready, "64-d hash embedder must not be semantic_ready");
        assert!(ctx.neighbors.is_empty());
    }

    /// The "alpha" note scores 1.0 (cosine of identical unit vectors) and appears
    /// as neighbor #1; the "zzz" note scores 0.0 and is filtered below low threshold.
    #[tokio::test]
    async fn same_topic_ranks_above_off_topic() {
        let state = stub_state();

        insert_triples(
            &state,
            &[
                ("active.md", "aingle:source_hash", "h0"),
                ("alpha.md", "aingle:source_hash", "h1"),
                ("zzz.md", "aingle:source_hash", "h2"),
            ],
        )
        .await;

        // Active note's own chunk (alpha text → e0 query vector).
        let e0 = vec![1.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let e1 = {
            let mut v = vec![0.0_f32; 128];
            v[1] = 1.0;
            v
        };
        insert_chunk(&state, "active.md", "alpha content for active", e0.clone()).await;
        insert_chunk(&state, "alpha.md", "alpha related content", e0.clone()).await;
        insert_chunk(&state, "zzz.md", "zzz completely unrelated orthogonal", e1).await;

        let ctx = super::note_context(&state, "active.md", 10).await;
        assert!(ctx.semantic_ready);
        assert!(
            ctx.neighbors.iter().any(|n| n.path == "alpha.md"),
            "alpha.md must be a neighbor: {:?}",
            ctx.neighbors
        );
        assert!(
            !ctx.neighbors.iter().any(|n| n.path == "zzz.md"),
            "zzz.md must be filtered (cosine 0.0 < low threshold): {:?}",
            ctx.neighbors
        );
        // alpha.md is first (highest score).
        assert_eq!(ctx.neighbors[0].path, "alpha.md");
    }

    /// passage is present and its char count is ≤ 201 (200 + optional ellipsis).
    /// An accented long chunk proves no byte-slice panic.
    #[tokio::test]
    async fn passage_present_and_char_safe() {
        let state = stub_state();

        insert_triples(
            &state,
            &[
                ("active.md", "aingle:source_hash", "h0"),
                ("related.md", "aingle:source_hash", "h1"),
            ],
        )
        .await;

        let e0: Vec<f32> = {
            let mut v = vec![0.0; 128];
            v[0] = 1.0;
            v
        };
        // Long chunk with accented chars to exercise char-safe truncation.
        let long_text = format!("alpha {}", "áéíóú ".repeat(80));
        insert_chunk(&state, "active.md", "alpha active note", e0.clone()).await;
        insert_chunk(&state, "related.md", &long_text, e0.clone()).await;

        let ctx = super::note_context(&state, "active.md", 10).await;
        assert!(ctx.semantic_ready);
        let n = ctx
            .neighbors
            .iter()
            .find(|n| n.path == "related.md")
            .expect("related.md must be a neighbor");
        let passage = n.passage.as_ref().expect("passage must be present");
        assert!(
            passage.chars().count() <= 201,
            "passage must be ≤ 201 chars (200 + ellipsis), got {}",
            passage.chars().count()
        );
    }

    /// `already_linked` is `true` when the active note has a `links_to` triple
    /// whose object is `Value::Node` (the real ingest format — NOT a literal).
    #[tokio::test]
    async fn already_linked_from_node_object() {
        let state = stub_state();

        {
            let g = state.graph.write().await;
            for (s, p) in [
                ("active.md", "aingle:source_hash"),
                ("alpha.md", "aingle:source_hash"),
            ] {
                g.insert(Triple::new(
                    NodeId::named(s),
                    Predicate::named(p),
                    Value::literal("h"),
                ))
                .unwrap();
            }
            // links_to stored as a NODE object — how real ingest produces it.
            g.insert(Triple::new(
                NodeId::named("active.md"),
                Predicate::named("links_to"),
                Value::Node(NodeId::named("alpha")),
            ))
            .unwrap();
        }

        let e0: Vec<f32> = {
            let mut v = vec![0.0_f32; 128];
            v[0] = 1.0;
            v
        };
        insert_chunk(&state, "active.md", "alpha active note", e0.clone()).await;
        insert_chunk(&state, "alpha.md", "alpha related content", e0.clone()).await;

        let ctx = super::note_context(&state, "active.md", 10).await;
        assert!(ctx.semantic_ready);
        let n = ctx
            .neighbors
            .iter()
            .find(|n| n.path == "alpha.md")
            .expect("alpha.md must be a neighbor");
        assert!(
            n.already_linked,
            "alpha.md must have already_linked=true (node-valued links_to): {:?}",
            n
        );
    }

    /// Notes under `_maps/` are excluded even when their embeddings match.
    #[tokio::test]
    async fn maps_excluded() {
        let state = stub_state();

        insert_triples(
            &state,
            &[
                ("active.md", "aingle:source_hash", "h0"),
                ("_maps/vault-map.md", "aingle:source_hash", "h1"),
            ],
        )
        .await;

        let e0: Vec<f32> = {
            let mut v = vec![0.0_f32; 128];
            v[0] = 1.0;
            v
        };
        insert_chunk(&state, "active.md", "alpha active", e0.clone()).await;
        insert_chunk(&state, "_maps/vault-map.md", "alpha maps content", e0.clone()).await;

        let ctx = super::note_context(&state, "active.md", 10).await;
        assert!(ctx.semantic_ready);
        assert!(
            !ctx.neighbors.iter().any(|n| n.path.starts_with("_maps/")),
            "maps paths must be excluded: {:?}",
            ctx.neighbors
        );
    }

    /// Without the `dag` feature the provenance anchor is always `None`.
    /// With the `dag` feature, a signed action anchors the source and is surfaced.
    #[cfg(not(feature = "dag"))]
    #[tokio::test]
    async fn provenance_none_without_dag() {
        let state = stub_state();

        insert_triples(
            &state,
            &[
                ("active.md", "aingle:source_hash", "h0"),
                ("alpha.md", "aingle:source_hash", "h1"),
            ],
        )
        .await;

        let e0: Vec<f32> = {
            let mut v = vec![0.0_f32; 128];
            v[0] = 1.0;
            v
        };
        insert_chunk(&state, "active.md", "alpha active", e0.clone()).await;
        insert_chunk(&state, "alpha.md", "alpha related", e0).await;

        let ctx = super::note_context(&state, "active.md", 10).await;
        let n = ctx
            .neighbors
            .iter()
            .find(|n| n.path == "alpha.md")
            .expect("alpha.md must be neighbor");
        assert!(
            n.provenance_anchor.is_none(),
            "provenance must be None without dag feature"
        );
    }

    /// With the `dag` feature, a signed DAG action for the neighbor yields a
    /// non-None provenance_anchor.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn provenance_present_when_signed() {
        let state = AppState::with_db_path_and_embedder(
            ":memory:",
            None,
            Arc::new(StubEmbedder),
        )
        .unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        insert_triples(
            &state,
            &[
                ("active.md", "aingle:source_hash", "h0"),
                ("alpha.md", "aingle:source_hash", "h1"),
            ],
        )
        .await;

        let e0: Vec<f32> = {
            let mut v = vec![0.0_f32; 128];
            v[0] = 1.0;
            v
        };
        insert_chunk(&state, "active.md", "alpha active", e0.clone()).await;
        insert_chunk(&state, "alpha.md", "alpha related", e0).await;

        // Record a signed Custom DAG action whose subject is "alpha.md" so that
        // history_by_subject("alpha.md") returns a signed entry and
        // provenance_anchor_for returns Some(hash_hex).
        {
            let graph = state.graph.read().await;
            let dag_store = graph.dag_store().expect("DAG must be enabled");
            let parents = dag_store.tips().expect("tips must be readable");
            let mut action = aingle_graph::dag::DagAction {
                parents,
                author: aingle_graph::NodeId::named("test"),
                seq: 0,
                timestamp: chrono::Utc::now(),
                payload: aingle_graph::dag::DagPayload::Custom {
                    payload_type: "ingest".to_string(),
                    payload_summary: "alpha.md ingested".to_string(),
                    payload: None,
                    subject: Some("alpha.md".to_string()),
                },
                signature: None,
            };
            let key = aingle_graph::dag::DagSigningKey::generate();
            key.sign(&mut action);
            dag_store.put(&action).expect("put signed action must succeed");
        }

        let ctx = super::note_context(&state, "active.md", 10).await;
        assert!(
            ctx.neighbors.iter().any(|n| n.path == "alpha.md"),
            "alpha.md must be a semantic neighbor with dag feature: {:?}",
            ctx.neighbors
        );
        let n = ctx
            .neighbors
            .iter()
            .find(|n| n.path == "alpha.md")
            .unwrap();
        assert!(
            n.provenance_anchor.is_some(),
            "provenance_anchor must be Some when a signed DAG action is recorded for the source: {:?}",
            n
        );
    }

    // -----------------------------------------------------------------------
    // Cache tests (item 1 + item 5)
    // -----------------------------------------------------------------------

    /// `note_context_cached` must return an identical result on the second call
    /// (cache hit), and a fresh result after graph/memory mutation (invalidation).
    /// This test locks both the hit path and the version-change recompute path.
    #[tokio::test]
    async fn note_context_cached_hit_and_invalidation() {
        let state = stub_state();

        insert_triples(
            &state,
            &[
                ("active.md", "aingle:source_hash", "h0"),
                ("alpha.md", "aingle:source_hash", "h1"),
            ],
        )
        .await;

        let e0: Vec<f32> = {
            let mut v = vec![0.0_f32; 128];
            v[0] = 1.0;
            v
        };
        insert_chunk(&state, "active.md", "alpha active note", e0.clone()).await;
        insert_chunk(&state, "alpha.md", "alpha related content", e0.clone()).await;

        // First call: computes and caches.
        let ctx1 = super::note_context_cached(&state, "active.md", 10).await;
        assert!(ctx1.semantic_ready, "StubEmbedder is 128d → semantic_ready");
        assert!(!ctx1.neighbors.is_empty(), "alpha.md must be a neighbor");

        // Second call: graph/memory unchanged → must return the cached result.
        let ctx2 = super::note_context_cached(&state, "active.md", 10).await;
        assert_eq!(
            ctx1.neighbors.len(),
            ctx2.neighbors.len(),
            "cache hit: neighbor count must be identical"
        );
        assert_eq!(
            ctx1.neighbors[0].path,
            ctx2.neighbors[0].path,
            "cache hit: top neighbor must be identical"
        );

        // Mutate: add beta.md (changes triple_count AND total_memory_bytes).
        insert_triples(&state, &[("beta.md", "aingle:source_hash", "h2")]).await;
        insert_chunk(&state, "beta.md", "alpha beta content", e0.clone()).await;

        // Third call: version mismatch → cache must be invalidated; beta.md appears.
        let ctx3 = super::note_context_cached(&state, "active.md", 10).await;
        assert!(
            ctx3.neighbors.iter().any(|n| n.path == "beta.md"),
            "after mutation (triple_count+memory_bytes changed), beta.md must appear: {:?}",
            ctx3.neighbors
        );
    }

    /// When the note_context_cache exceeds 256 entries, inserting a new result
    /// must clear the map first so the cache never grows without bound.
    #[tokio::test]
    async fn cache_cap_clears_when_exceeded() {
        let state = stub_state();

        // Pre-fill the cache with 257 dummy entries to exceed the cap.
        {
            let mut cache = state.note_context_cache.lock().unwrap();
            for i in 0..257usize {
                cache.insert(
                    (format!("dummy_{i}.md"), 0usize),
                    ((0, 0), super::NoteContext { semantic_ready: false, neighbors: vec![] }),
                );
            }
        }
        assert_eq!(
            state.note_context_cache.lock().unwrap().len(),
            257,
            "pre-condition: cache must have 257 dummy entries"
        );

        // Call note_context_cached for a fresh note (not in cache).
        // The cap must clear the map before inserting this new entry.
        let _ = super::note_context_cached(&state, "fresh.md", 5).await;

        let cache = state.note_context_cache.lock().unwrap();
        assert_eq!(
            cache.len(),
            1,
            "cap must clear the oversized cache before inserting; got {} entries",
            cache.len()
        );
        assert!(
            cache.contains_key(&("fresh.md".to_string(), 5usize)),
            "fresh.md must be in the cache after the cap-and-insert"
        );
    }

    // -----------------------------------------------------------------------
    // Optional nit
    // -----------------------------------------------------------------------

    /// An active note with NO chunks falls back to the basename as query text
    /// and still surfaces neighbors. The active note must never appear as its
    /// own neighbor (self-match guard).
    #[tokio::test]
    async fn no_chunks_falls_back_to_basename_and_never_self_matches() {
        let state = stub_state();

        insert_triples(
            &state,
            &[
                ("active.md", "aingle:source_hash", "h0"),
                ("related.md", "aingle:source_hash", "h1"),
            ],
        )
        .await;

        // active.md has NO chunks. StubEmbedder: basename("active.md") = "active"
        // → v[2] = 1.0 (default case, no "alpha" or "zzz"). related.md chunk
        // "general content" → v[2] = 1.0. Cosine = 1.0 ≥ low threshold (0.1).
        let e_default: Vec<f32> = {
            let mut v = vec![0.0_f32; 128];
            v[2] = 1.0;
            v
        };
        insert_chunk(&state, "related.md", "general related content", e_default).await;

        let ctx = super::note_context(&state, "active.md", 10).await;
        assert!(ctx.semantic_ready, "StubEmbedder is 128d → semantic_ready");
        assert!(
            !ctx.neighbors.iter().any(|n| n.path == "active.md"),
            "active.md must never be its own neighbor: {:?}",
            ctx.neighbors
        );
        assert!(
            ctx.neighbors.iter().any(|n| n.path == "related.md"),
            "basename-fallback must still surface related.md: {:?}",
            ctx.neighbors
        );
    }

    /// End-to-end acceptance test for the real neural embedder: same-topic notes
    /// must surface as semantic neighbors while an off-topic note is filtered out.
    /// Gated on the `neural-embeddings` feature and skips if the model files are
    /// absent. Requires `ORT_DYLIB_PATH` to point at an onnxruntime shared library.
    #[cfg(feature = "neural-embeddings")]
    #[tokio::test]
    async fn neural_note_context_finds_same_topic() {
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
            eprintln!("skipping neural_note_context_finds_same_topic: e5 model not found at {model_dir}");
            return;
        }

        let embedder = crate::embedder::build_embedder(Some(&model_dir));
        assert_eq!(
            embedder.dimensions(),
            384,
            "neural embedder must be active (384d)"
        );

        let state =
            AppState::with_db_path_and_embedder(":memory:", None, embedder).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let dir = tempfile::tempdir().unwrap();
        // Two same-topic notes about dog care — sentences reused from
        // neural_grounding_is_topical in ground.rs for reliable embedding behaviour.
        std::fs::write(
            dir.path().join("perros1.md"),
            "# Cuidado de perros\n\nLos perros necesitan paseos diarios, agua fresca y una dieta equilibrada para estar sanos.\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("perros2.md"),
            "# Mascotas\n\nUn perro sano requiere ejercicio diario, hidratación constante y alimentación balanceada.\n",
        )
        .unwrap();
        // Off-topic note: elections have no semantic overlap with dog care.
        std::fs::write(
            dir.path().join("elecciones.md"),
            "# Elecciones\n\nLos resultados de las elecciones presidenciales determinan el futuro del país.\n",
        )
        .unwrap();

        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let ctx = super::note_context(&state, "perros1.md", 5).await;

        assert!(
            ctx.semantic_ready,
            "neural embedder (384d) must set semantic_ready=true"
        );

        assert!(
            ctx.neighbors.iter().any(|n| n.path == "perros2.md"),
            "perros2.md (same-topic sibling) must be a semantic neighbor of perros1.md: {:?}",
            ctx.neighbors
        );

        let sibling = ctx
            .neighbors
            .iter()
            .find(|n| n.path == "perros2.md")
            .unwrap();
        assert!(
            sibling.passage.is_some(),
            "perros2.md neighbor must include a matching passage: {:?}",
            sibling
        );

        // elecciones.md is semantically orthogonal to dog care; its cosine against
        // the perros1.md query vector (~0.83) must not reach NEIGHBOR_FLOOR (0.88).
        assert!(
            !ctx.neighbors.iter().any(|n| n.path == "elecciones.md"),
            "off-topic elecciones.md must not appear as a neighbor (below NEIGHBOR_FLOOR=0.88): {:?}",
            ctx.neighbors
        );
    }
}
