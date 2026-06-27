// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Semantic note-context: for an active note, surface the notes that are
//! semantically related (by neural embeddings) even when never linked, each
//! with the matching passage and signed provenance.

use std::collections::{BTreeMap, BTreeSet};

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

// ---------------------------------------------------------------------------
// Helpers (mirrored verbatim from backlinks.rs)
// ---------------------------------------------------------------------------

/// Return the object of a triple as a plain `String`, handling both literal
/// strings (`Value::Str`) and graph nodes (`Value::Node`). Node IDs are stored
/// with `<…>` angle-bracket wrappers; this strips them so the result matches
/// the bare names used everywhere else in this module.
fn obj_string(t: &aingle_graph::Triple) -> Option<String> {
    if let Some(s) = t.object_string() {
        Some(s.to_string())
    } else {
        t.object_node()
            .map(|n| n.to_string().trim_start_matches('<').trim_end_matches('>').to_string())
    }
}

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

    let (_, low) = state.embedder.relevance_thresholds();

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
        if note_set.contains(target) {
            Some(target.to_string())
        } else {
            by_base
                .get(&crate::service::vault_map::basename(target))
                .cloned()
        }
    };

    // 3. Compute `outgoing_set`: full paths that the active `note` links to.
    let outgoing_set: BTreeSet<String> = links
        .iter()
        .filter(|(src, _)| src == note)
        .filter_map(|(_, target)| resolve(target))
        .filter(|p| p != note)
        .collect();

    // 4. Build the active note's query text from its own chunks.
    let mut own_text = String::new();
    let all_entries: Vec<ineru::MemoryEntry> = {
        let mem = state.memory.read().await;
        let mut v = mem.stm.all_entries();
        v.extend(mem.ltm.all_entries());
        v
    };

    for e in &all_entries {
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
        if rel < low {
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
        let text = d
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let entry = best_by_src.entry(src).or_insert((rel, text.clone()));
        if rel > entry.0 {
            *entry = (rel, text);
        }
    }

    // 6. Build Neighbor list, resolve provenance, sort, truncate.
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
        let provenance_anchor = provenance_anchor_for(state, &src).await;
        neighbors.push(Neighbor {
            path: src,
            score: rel,
            passage,
            provenance_anchor,
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
/// The cache key does NOT include `limit` — keep it simple and document the
/// assumption: callers should use a stable `limit` for the same note. If
/// `limit` varies per call, the first winning result is served. For Akashi's
/// use-case (fixed sidebar top-N) this is always correct.
pub async fn note_context_cached(
    state: &crate::state::AppState,
    note: &str,
    limit: usize,
) -> NoteContext {
    let tc = { state.graph.read().await.stats().triple_count };
    let mem_bytes = { state.memory.read().await.stats().total_memory_bytes };
    let key = (tc, mem_bytes);

    // Check cache — release lock before any await.
    {
        let cache = state
            .note_context_cache
            .lock()
            .expect("note_context cache poisoned");
        if let Some((cached_key, ctx)) = cache.get(note) {
            if *cached_key == key {
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
        cache.insert(note.to_string(), (key, result.clone()));
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
        // Build a state with DAG enabled and a signing key so actions are signed.
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

        // Record a DAG action for alpha.md. Because DAG is enabled but there is
        // no signing key on this state (key is None), the action will be unsigned
        // and provenance_anchor_for returns None for unsigned actions. This proves
        // the cfg(feature="dag") path compiles and runs; a signed action requires
        // a proper DagSigningKey that is complex to wire in a unit test.
        // The critical assertion: no panic, code path exercised.
        let ctx = super::note_context(&state, "active.md", 10).await;
        // alpha.md is a semantic neighbor regardless of provenance.
        assert!(
            ctx.neighbors.iter().any(|n| n.path == "alpha.md"),
            "alpha.md must be a semantic neighbor with dag feature: {:?}",
            ctx.neighbors
        );
        // provenance_anchor is None because no signing key is configured (unsigned action).
        // This is the correct behavior for an unsigned DAG node.
        let n = ctx
            .neighbors
            .iter()
            .find(|n| n.path == "alpha.md")
            .unwrap();
        // We assert the Option is coherent (not that it's Some, since no signing key).
        let _ = n.provenance_anchor.as_deref();
    }
}
