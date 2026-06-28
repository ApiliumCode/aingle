// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Local graph neighborhood for a single note: typed edges (link / semantic / tag)
//! up to depth 2 for the Akashi per-note graph panel (VC-2).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::service::triple_util::{obj_string, resolve_link_target};
use crate::service::vault_map::{basename, is_maps_path};
use crate::service::context::{note_context_cached, NEIGHBOR_FLOOR};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The typed local neighborhood graph around a center note.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct LocalGraph {
    /// The center note path.
    pub center: String,
    /// All nodes in this neighborhood (center + neighbors).
    pub nodes: Vec<GNode>,
    /// All typed edges in this neighborhood.
    pub edges: Vec<TypedEdge>,
    /// `true` when the embedder has enough dimensions for semantic edges.
    pub semantic_ready: bool,
}

/// A node in the local neighborhood graph.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GNode {
    /// Full relative path (canonical identity).
    pub id: String,
    /// Human-readable label (basename without extension).
    pub label: String,
    /// `"center"` for the focal note; `"note"` for all others.
    pub kind: String,
    /// Semantic cluster id. Always `-1` here (clustering is global / expensive).
    pub cluster: i64,
    /// Number of edges in THIS graph touching this node.
    pub degree: usize,
    /// Creation date sourced from the note's `created` frontmatter scalar (e.g. `"2025-09-14"`).
    /// `None` when the note has no `created` triple.
    pub timestamp: Option<String>,
}

/// A typed, optionally weighted edge in the local neighborhood graph.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TypedEdge {
    pub source: String,
    pub target: String,
    /// `"link"` | `"semantic"` | `"tag"`
    pub kind: String,
    /// Cosine similarity score — present only for semantic edges.
    pub score: Option<f32>,
    /// For tag edges: the shared tag name.
    pub label: Option<String>,
    /// Signed DAG action hash for semantic edges (🔒). `None` if unavailable.
    pub provenance_anchor: Option<String>,
}

// ---------------------------------------------------------------------------
// Private constants
// ---------------------------------------------------------------------------

const NODE_CAP: usize = 80;
const SEM_PER_NODE: usize = 5;
const MAX_DEPTH: usize = 2;
/// Max tag-edges added per (node, tag) pair — prevents explosion on popular tags.
const TAG_FANOUT_CAP: usize = 6;
/// Maximum frontier size before the per-node semantic pass at each BFS level.
/// Caps the depth-2 semantic N+1: a hub with many link-neighbors would otherwise
/// trigger one `note_context_cached` call per frontier node. Sorting the frontier
/// first ensures deterministic behavior when truncating.
const SEM_FRONTIER_CAP: usize = 16;

// ---------------------------------------------------------------------------
// Core function
// ---------------------------------------------------------------------------

/// Build the typed local neighborhood graph for `note` at BFS depth `depth`.
pub async fn local_graph(
    state: &crate::state::AppState,
    note: &str,
    depth: usize,
) -> LocalGraph {
    use aingle_graph::{Predicate, TriplePattern};

    let depth = depth.clamp(1, MAX_DEPTH);
    let semantic_grade = state.embedder.dimensions() >= 128;

    let strip = |n: String| n.trim_start_matches('<').trim_end_matches('>').to_string();

    // -----------------------------------------------------------------------
    // 1. Load structural data from the graph once.
    // -----------------------------------------------------------------------
    // notes: all ingested note paths
    // links_raw: (subject, object-string) for every links_to triple
    // tagged_raw: (subject, tag) for every tagged triple
    type PairVec = Vec<(String, String)>;
    let (notes, links_raw, tagged_raw, created_map): (Vec<String>, PairVec, PairVec, BTreeMap<String, String>) = {
        let g = state.graph.read().await;
        let collect = |pred: &str| -> PairVec {
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
        let lnks = collect("links_to");
        let tags = collect("tagged");
        // Build created-date map: note_path → date. "date" as fallback, "created" takes precedence.
        let mut cmap: BTreeMap<String, String> = collect("date").into_iter().collect();
        for (k, v) in collect("created") {
            cmap.insert(k, v);
        }
        (ns, lnks, tags, cmap)
    };

    let note_set: BTreeSet<&str> = notes.iter().map(|s| s.as_str()).collect();

    // Basename index for wikilink resolution.
    let mut by_base: BTreeMap<String, String> = BTreeMap::new();
    for n in &notes {
        by_base
            .entry(basename(n))
            .or_insert_with(|| n.clone());
    }

    let resolve = |target: &str| -> Option<String> {
        resolve_link_target(target, &note_set, &by_base)
    };

    // Resolved outgoing links: (src, dst) — both are full paths, neither a maps path.
    let links: Vec<(String, String)> = links_raw
        .iter()
        .filter_map(|(src, tgt)| resolve(tgt).map(|dst| (src.clone(), dst)))
        .filter(|(src, dst)| src != dst)
        .filter(|(src, _)| note_set.contains(src.as_str()) && !is_maps_path(src))
        .filter(|(_, dst)| note_set.contains(dst.as_str()) && !is_maps_path(dst))
        .collect();

    // Pre-index links for O(1) per-node lookup in the BFS loop — avoids
    // re-scanning the full `links` vec twice per node.
    let mut by_src: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut by_dst: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (src, dst) in &links {
        by_src.entry(src.clone()).or_default().push(dst.clone());
        by_dst.entry(dst.clone()).or_default().push(src.clone());
    }

    // tag_of_note: note → set<tag>
    // notes_of_tag: tag → vec<note> (sorted, deduped)
    let mut tag_of_note: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut notes_of_tag: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (note_path, tag) in &tagged_raw {
        if note_set.contains(note_path.as_str()) && !is_maps_path(note_path) {
            tag_of_note
                .entry(note_path.clone())
                .or_default()
                .insert(tag.clone());
            notes_of_tag
                .entry(tag.clone())
                .or_default()
                .push(note_path.clone());
        }
    }
    for v in notes_of_tag.values_mut() {
        v.sort();
        v.dedup();
    }

    // -----------------------------------------------------------------------
    // 2. BFS to collect edges.
    // -----------------------------------------------------------------------
    let mut edges: Vec<TypedEdge> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(note.to_string());

    let mut frontier: VecDeque<String> = VecDeque::new();
    frontier.push_back(note.to_string());

    let mut semantic_ready = semantic_grade;

    for _level in 0..depth {
        let mut next_frontier: Vec<String> = Vec::new();
        while let Some(n) = frontier.pop_front() {
            if is_maps_path(&n) {
                continue;
            }

            // --- link edges (outgoing from n) ---
            if let Some(dsts) = by_src.get(&n) {
                for dst in dsts {
                    edges.push(TypedEdge {
                        source: n.clone(),
                        target: dst.clone(),
                        kind: "link".to_string(),
                        score: None,
                        label: None,
                        provenance_anchor: None,
                    });
                    if !visited.contains(dst) {
                        visited.insert(dst.clone());
                        next_frontier.push(dst.clone());
                    }
                }
            }
            // --- link edges (incoming to n) ---
            if let Some(srcs) = by_dst.get(&n) {
                for src in srcs {
                    edges.push(TypedEdge {
                        source: src.clone(),
                        target: n.clone(),
                        kind: "link".to_string(),
                        score: None,
                        label: None,
                        provenance_anchor: None,
                    });
                    if !visited.contains(src) {
                        visited.insert(src.clone());
                        next_frontier.push(src.clone());
                    }
                }
            }

            // --- semantic edges ---
            if semantic_grade {
                let ctx = note_context_cached(state, &n, SEM_PER_NODE).await;
                if !ctx.semantic_ready {
                    semantic_ready = false;
                } else {
                    for nb in ctx.neighbors {
                        if nb.score < NEIGHBOR_FLOOR {
                            continue;
                        }
                        if is_maps_path(&nb.path) {
                            continue;
                        }
                        edges.push(TypedEdge {
                            source: n.clone(),
                            target: nb.path.clone(),
                            kind: "semantic".to_string(),
                            score: Some(nb.score),
                            label: None,
                            provenance_anchor: nb.provenance_anchor,
                        });
                        if !visited.contains(&nb.path) {
                            visited.insert(nb.path.clone());
                            next_frontier.push(nb.path.clone());
                        }
                    }
                }
            }

            // --- tag edges ---
            if let Some(tags) = tag_of_note.get(&n) {
                for tag in tags {
                    if let Some(peers) = notes_of_tag.get(tag) {
                        let mut added = 0usize;
                        for peer in peers {
                            if peer == &n || is_maps_path(peer) {
                                continue;
                            }
                            if added >= TAG_FANOUT_CAP {
                                break;
                            }
                            edges.push(TypedEdge {
                                source: n.clone(),
                                target: peer.clone(),
                                kind: "tag".to_string(),
                                score: None,
                                label: Some(tag.clone()),
                                provenance_anchor: None,
                            });
                            if !visited.contains(peer) {
                                visited.insert(peer.clone());
                                next_frontier.push(peer.clone());
                            }
                            added += 1;
                        }
                    }
                }
            }
        }

        // Cap the next frontier before promoting to bound semantic cost at the
        // next level (≤ SEM_FRONTIER_CAP × note_context_cached calls).
        // Depth-1 behavior is identical: next_frontier is never used again.
        next_frontier.sort();
        next_frontier.truncate(SEM_FRONTIER_CAP);
        for n in next_frontier {
            frontier.push_back(n);
        }
    }

    // -----------------------------------------------------------------------
    // 3. Deduplicate edges.
    // -----------------------------------------------------------------------
    // Links are directional — dedupe by (source, target, kind).
    // Semantic/tag are symmetric — dedupe order-insensitively by (min,max,kind).
    let mut seen_link: HashSet<(String, String)> = HashSet::new();
    let mut seen_sym: HashSet<(String, String, String, String)> = HashSet::new();
    let mut deduped: Vec<TypedEdge> = Vec::new();

    for e in edges {
        // Remove self-loops.
        if e.source == e.target {
            continue;
        }
        match e.kind.as_str() {
            "link" => {
                let key = (e.source.clone(), e.target.clone());
                if seen_link.insert(key) {
                    deduped.push(e);
                }
            }
            _ => {
                // symmetric kinds: (tag, semantic)
                let (lo, hi) = if e.source <= e.target {
                    (e.source.clone(), e.target.clone())
                } else {
                    (e.target.clone(), e.source.clone())
                };
                // Include the tag label so a pair sharing two distinct tags
                // yields two edges. Semantic label is always None → "" → no clash.
                let tag_label = if e.kind == "tag" {
                    e.label.clone().unwrap_or_default()
                } else {
                    String::new()
                };
                let key = (lo, hi, e.kind.clone(), tag_label);
                if seen_sym.insert(key) {
                    deduped.push(e);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // 4. Collect all node ids referenced by edges, plus the center.
    // -----------------------------------------------------------------------
    let mut all_node_ids: HashSet<String> = HashSet::new();
    all_node_ids.insert(note.to_string());
    for e in &deduped {
        all_node_ids.insert(e.source.clone());
        all_node_ids.insert(e.target.clone());
    }

    // -----------------------------------------------------------------------
    // 5. Cap: keep center + highest-degree nodes; drop edges to removed nodes.
    // -----------------------------------------------------------------------
    let mut degree_map: HashMap<String, usize> = HashMap::new();
    for id in &all_node_ids {
        degree_map.insert(id.clone(), 0);
    }
    for e in &deduped {
        *degree_map.entry(e.source.clone()).or_default() += 1;
        *degree_map.entry(e.target.clone()).or_default() += 1;
    }

    let kept_ids: HashSet<String> = if all_node_ids.len() > NODE_CAP {
        // Always keep center; fill remaining slots by degree descending.
        let mut by_degree: Vec<(String, usize)> = degree_map
            .iter()
            .filter(|(id, _)| id.as_str() != note)
            .map(|(id, &d)| (id.clone(), d))
            .collect();
        by_degree.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut kept: HashSet<String> = HashSet::new();
        kept.insert(note.to_string());
        for (id, _) in by_degree.into_iter().take(NODE_CAP - 1) {
            kept.insert(id);
        }
        kept
    } else {
        all_node_ids.clone()
    };

    // Drop edges that reference removed nodes.
    let mut final_edges: Vec<TypedEdge> = deduped
        .into_iter()
        .filter(|e| kept_ids.contains(&e.source) && kept_ids.contains(&e.target))
        .collect();
    // Sort for stable cross-run output.
    final_edges.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.target.cmp(&b.target))
            .then(a.kind.cmp(&b.kind))
    });

    // Recompute degree map for final kept set.
    let mut final_degree: HashMap<String, usize> = HashMap::new();
    for id in &kept_ids {
        final_degree.insert(id.clone(), 0);
    }
    for e in &final_edges {
        *final_degree.entry(e.source.clone()).or_default() += 1;
        *final_degree.entry(e.target.clone()).or_default() += 1;
    }

    // Build nodes vector.
    let mut nodes: Vec<GNode> = kept_ids
        .iter()
        .map(|id| {
            let kind = if id == note { "center" } else { "note" }.to_string();
            let degree = *final_degree.get(id).unwrap_or(&0);
            GNode {
                label: basename(id),
                id: id.clone(),
                kind,
                cluster: -1,
                degree,
                timestamp: created_map.get(id).cloned(),
            }
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    LocalGraph {
        center: note.to_string(),
        nodes,
        edges: final_edges,
        semantic_ready,
    }
}

// ---------------------------------------------------------------------------
// Cached variant
// ---------------------------------------------------------------------------

/// Like [`local_graph`] but memoised on `(triple_count, total_memory_bytes)`.
///
/// Map key is `(note_path, depth)`. Cap: 256 entries (clear-on-exceed).
pub async fn local_graph_cached(
    state: &crate::state::AppState,
    note: &str,
    depth: usize,
) -> LocalGraph {
    let tc = { state.graph.read().await.stats().triple_count };
    let mem_bytes = { state.memory.read().await.stats().total_memory_bytes };
    let version_key = (tc, mem_bytes);
    let map_key = (note.to_string(), depth);

    // Check cache — release lock before any await.
    {
        let cache = state
            .local_graph_cache
            .lock()
            .expect("local_graph cache poisoned");
        if let Some((cached_key, graph)) = cache.get(&map_key) {
            if *cached_key == version_key {
                return graph.clone();
            }
        }
    }

    // Compute without holding the mutex.
    let result = local_graph(state, note, depth).await;

    // Store result.
    {
        let mut cache = state
            .local_graph_cache
            .lock()
            .expect("local_graph cache poisoned");
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
    // Stub embedder: 128-dim (same as context.rs tests).
    // text with "alpha" → e0=[1,0,…], "zzz" → e1=[0,1,…], else → e2=[0,0,1,…]
    // Cosine(alpha,alpha) = 1.0 ≥ NEIGHBOR_FLOOR(0.88) → passes.
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
                v[2] = 1.0;
            }
            Embedding::new(v)
        }

        fn embed_query(&self, text: &str) -> Embedding {
            self.embed_passage(text)
        }

        fn dimensions(&self) -> usize {
            128
        }

        fn relevance_thresholds(&self) -> (f32, f32) {
            (0.5, 0.1)
        }
    }

    fn stub_state() -> AppState {
        AppState::with_db_path_and_embedder(":memory:", None, Arc::new(StubEmbedder)).unwrap()
    }

    async fn insert_triple_node(state: &AppState, s: &str, p: &str, o_node: &str) {
        let g = state.graph.write().await;
        g.insert(Triple::new(
            NodeId::named(s),
            Predicate::named(p),
            Value::Node(NodeId::named(o_node)),
        ))
        .unwrap();
    }

    async fn insert_triple_lit(state: &AppState, s: &str, p: &str, o: &str) {
        let g = state.graph.write().await;
        g.insert(Triple::new(
            NodeId::named(s),
            Predicate::named(p),
            Value::literal(o),
        ))
        .unwrap();
    }

    async fn register_note(state: &AppState, path: &str) {
        insert_triple_lit(state, path, crate::service::ingest::PRED_SOURCE_HASH, "h").await;
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

    fn e0() -> Vec<f32> {
        let mut v = vec![0.0_f32; 128];
        v[0] = 1.0;
        v
    }

    // -----------------------------------------------------------------------
    // 1. link_edge_from_wikilink
    // -----------------------------------------------------------------------
    /// A `links_to` triple (Value::Node) from a.md to b yields a "link" edge a→b,
    /// and center is "a.md".
    #[tokio::test]
    async fn link_edge_from_wikilink() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        // wikilink stored as Value::Node (basename without extension)
        insert_triple_node(&state, "a.md", "links_to", "b").await;

        let g = super::local_graph(&state, "a.md", 1).await;
        assert_eq!(g.center, "a.md");
        let link = g.edges.iter().find(|e| e.kind == "link");
        assert!(link.is_some(), "must have a link edge: {:?}", g.edges);
        let link = link.unwrap();
        assert_eq!(link.source, "a.md");
        assert_eq!(link.target, "b.md");
    }

    // -----------------------------------------------------------------------
    // 2. semantic_edge_from_neighbor
    // -----------------------------------------------------------------------
    /// With the stub 128-d embedder and alpha-topic chunks, a.md and b.md both
    /// project onto e0. note_context yields them as mutual neighbors with score
    /// 1.0 ≥ NEIGHBOR_FLOOR → a "semantic" edge with score.is_some().
    #[tokio::test]
    async fn semantic_edge_from_neighbor() {
        let state = stub_state();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        insert_chunk(&state, "a.md", "alpha content for a", e0()).await;
        insert_chunk(&state, "b.md", "alpha content for b", e0()).await;

        let g = super::local_graph(&state, "a.md", 1).await;
        assert!(g.semantic_ready, "StubEmbedder(128d) must be semantic_ready");
        let sem = g.edges.iter().find(|e| e.kind == "semantic");
        assert!(sem.is_some(), "must have a semantic edge: {:?}", g.edges);
        assert!(sem.unwrap().score.is_some(), "semantic edge must carry a score");
    }

    // -----------------------------------------------------------------------
    // 3. semantic_edge_carries_provenance (dag-gated)
    // -----------------------------------------------------------------------
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn semantic_edge_carries_provenance() {
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
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        insert_chunk(&state, "a.md", "alpha content for a", e0()).await;
        insert_chunk(&state, "b.md", "alpha content for b", e0()).await;

        // Record a signed DAG action for b.md so provenance_anchor_for("b.md") is Some.
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
                    payload_summary: "b.md ingested".to_string(),
                    payload: None,
                    subject: Some("b.md".to_string()),
                },
                signature: None,
            };
            let key = aingle_graph::dag::DagSigningKey::generate();
            key.sign(&mut action);
            dag_store.put(&action).expect("put signed action must succeed");
        }

        let g = super::local_graph(&state, "a.md", 1).await;
        let sem = g
            .edges
            .iter()
            .find(|e| e.kind == "semantic" && e.target == "b.md")
            .expect("must have semantic edge a→b");
        assert!(
            sem.provenance_anchor.is_some(),
            "semantic edge to b.md must carry provenance_anchor when a signed DAG action exists: {:?}",
            sem
        );
    }

    // -----------------------------------------------------------------------
    // 4. tag_edge_from_shared_tag
    // -----------------------------------------------------------------------
    /// a.md and b.md both tagged "x" → a "tag" edge with label == Some("x").
    #[tokio::test]
    async fn tag_edge_from_shared_tag() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        insert_triple_lit(&state, "a.md", "tagged", "x").await;
        insert_triple_lit(&state, "b.md", "tagged", "x").await;

        let g = super::local_graph(&state, "a.md", 1).await;
        let tag_edge = g.edges.iter().find(|e| e.kind == "tag");
        assert!(tag_edge.is_some(), "must have a tag edge: {:?}", g.edges);
        assert_eq!(
            tag_edge.unwrap().label.as_deref(),
            Some("x"),
            "tag edge label must be the shared tag"
        );
    }

    // -----------------------------------------------------------------------
    // 5. hash_embedder_omits_semantic
    // -----------------------------------------------------------------------
    /// The default 64-dim hash embedder fails the semantic gate → semantic_ready==false,
    /// no "semantic" edges; link and tag edges still appear.
    #[tokio::test]
    async fn hash_embedder_omits_semantic() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        // A link edge (so we know other edges work).
        insert_triple_node(&state, "a.md", "links_to", "b").await;

        let g = super::local_graph(&state, "a.md", 1).await;
        assert!(!g.semantic_ready, "64-dim hash embedder must set semantic_ready=false");
        assert!(
            g.edges.iter().all(|e| e.kind != "semantic"),
            "no semantic edges with hash embedder: {:?}",
            g.edges
        );
        // Link edges still present.
        assert!(
            g.edges.iter().any(|e| e.kind == "link"),
            "link edges must still appear: {:?}",
            g.edges
        );
    }

    // -----------------------------------------------------------------------
    // 6. maps_excluded
    // -----------------------------------------------------------------------
    /// Notes under `_maps/` are never included in the graph even when they
    /// share tags or links with the center note.
    #[tokio::test]
    async fn maps_excluded() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "a.md").await;
        register_note(&state, "_maps/vault-map.md").await;
        insert_triple_lit(&state, "a.md", "tagged", "x").await;
        insert_triple_lit(&state, "_maps/vault-map.md", "tagged", "x").await;
        // Also a direct link to make sure links are filtered too.
        insert_triple_node(&state, "a.md", "links_to", "vault-map").await;

        let g = super::local_graph(&state, "a.md", 1).await;
        assert!(
            !g.nodes.iter().any(|n| n.id.starts_with("_maps/")),
            "_maps/ nodes must be excluded: {:?}",
            g.nodes
        );
        assert!(
            !g.edges.iter().any(|e| e.target.starts_with("_maps/") || e.source.starts_with("_maps/")),
            "_maps/ edges must be excluded: {:?}",
            g.edges
        );
    }

    // -----------------------------------------------------------------------
    // 7. caps_respected
    // -----------------------------------------------------------------------
    /// With more than NODE_CAP neighbors, nodes.len() <= NODE_CAP and center is present.
    #[tokio::test]
    async fn caps_respected() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "center.md").await;
        // Create NODE_CAP + 10 = 90 notes, each sharing a tag with center.md.
        for i in 0..90 {
            let path = format!("note{i}.md");
            register_note(&state, &path).await;
            insert_triple_lit(&state, &path, "tagged", "bigtag").await;
        }
        insert_triple_lit(&state, "center.md", "tagged", "bigtag").await;

        let g = super::local_graph(&state, "center.md", 1).await;
        assert!(
            g.nodes.len() <= super::NODE_CAP,
            "nodes.len() ({}) must be <= NODE_CAP ({}): center present: {}",
            g.nodes.len(),
            super::NODE_CAP,
            g.nodes.iter().any(|n| n.id == "center.md")
        );
        assert!(
            g.nodes.iter().any(|n| n.id == "center.md"),
            "center must always be in the graph: {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }

    // -----------------------------------------------------------------------
    // 8. depth_two_expands_frontier
    // -----------------------------------------------------------------------
    /// A→B→C via wikilinks: depth=2 reaches c.md, depth=1 does not.
    #[tokio::test]
    async fn depth_two_expands_frontier() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        register_note(&state, "c.md").await;
        insert_triple_node(&state, "a.md", "links_to", "b").await;
        insert_triple_node(&state, "b.md", "links_to", "c").await;

        let g1 = super::local_graph(&state, "a.md", 1).await;
        assert!(
            !g1.nodes.iter().any(|n| n.id == "c.md"),
            "depth=1 must NOT include c.md: {:?}",
            g1.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );

        let g2 = super::local_graph(&state, "a.md", 2).await;
        assert!(
            g2.nodes.iter().any(|n| n.id == "c.md"),
            "depth=2 must include c.md (reached via a→b→c): {:?}",
            g2.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }


    // -----------------------------------------------------------------------
    // 9. incoming_link_edge
    // -----------------------------------------------------------------------
    /// X links_to A (incoming); local_graph("a.md", 1) must include x→a link edge.
    #[tokio::test]
    async fn incoming_link_edge() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "x.md").await;
        register_note(&state, "a.md").await;
        insert_triple_node(&state, "x.md", "links_to", "a").await;

        let g = super::local_graph(&state, "a.md", 1).await;
        let link = g
            .edges
            .iter()
            .find(|e| e.kind == "link" && e.source == "x.md" && e.target == "a.md");
        assert!(
            link.is_some(),
            "incoming link x→a must appear in graph centered on a.md: {:?}",
            g.edges
        );
    }


    // -----------------------------------------------------------------------
    // 10. pair_with_link_and_semantic_keeps_both
    // -----------------------------------------------------------------------
    /// A links_to B AND B is A's semantic neighbor → both a link edge AND a
    /// semantic edge must be present for the pair (different dedup sets).
    #[tokio::test]
    async fn pair_with_link_and_semantic_keeps_both() {
        let state = stub_state();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        insert_triple_node(&state, "a.md", "links_to", "b").await;
        insert_chunk(&state, "a.md", "alpha content for a", e0()).await;
        insert_chunk(&state, "b.md", "alpha content for b", e0()).await;

        let g = super::local_graph(&state, "a.md", 1).await;
        let has_link = g
            .edges
            .iter()
            .any(|e| e.kind == "link" && e.source == "a.md" && e.target == "b.md");
        let has_sem = g.edges.iter().any(|e| {
            e.kind == "semantic"
                && ((e.source == "a.md" && e.target == "b.md")
                    || (e.source == "b.md" && e.target == "a.md"))
        });
        assert!(has_link, "link edge a→b must be present: {:?}", g.edges);
        assert!(has_sem, "semantic edge a↔b must be present: {:?}", g.edges);
    }


    // -----------------------------------------------------------------------
    // 11. symmetric_semantic_dedup
    // -----------------------------------------------------------------------
    /// With a→b and b→a semantic edges produced at different BFS levels, dedup
    /// must yield exactly ONE semantic edge for the pair.
    #[tokio::test]
    async fn symmetric_semantic_dedup() {
        let state = stub_state();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        insert_chunk(&state, "a.md", "alpha content for a", e0()).await;
        insert_chunk(&state, "b.md", "alpha content for b", e0()).await;

        // depth=2: level-1 processes a.md → finds b.md; level-2 processes b.md → finds a.md.
        // Both produce a↔b semantic edge candidates. Dedup keeps exactly one.
        let g = super::local_graph(&state, "a.md", 2).await;
        let sem_count = g
            .edges
            .iter()
            .filter(|e| {
                e.kind == "semantic"
                    && ((e.source == "a.md" && e.target == "b.md")
                        || (e.source == "b.md" && e.target == "a.md"))
            })
            .count();
        assert_eq!(
            sem_count,
            1,
            "symmetric a↔b semantic must yield exactly ONE edge, got {sem_count}: {:?}",
            g.edges
        );
    }


    // -----------------------------------------------------------------------
    // 12. local_graph_cached_hit_and_invalidation
    // -----------------------------------------------------------------------
    /// Cache hit: second call with unchanged graph returns same result.
    /// Invalidation: after a graph mutation, the next call recomputes.
    #[tokio::test]
    async fn local_graph_cached_hit_and_invalidation() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        insert_triple_node(&state, "a.md", "links_to", "b").await;

        // First call: computes and caches.
        let g1 = super::local_graph_cached(&state, "a.md", 1).await;
        assert!(g1.nodes.iter().any(|n| n.id == "b.md"), "b.md must be in graph");

        // Second call: graph/memory unchanged → cache hit → identical result.
        let g2 = super::local_graph_cached(&state, "a.md", 1).await;
        assert_eq!(
            g1.nodes.len(),
            g2.nodes.len(),
            "cache hit must return same node count"
        );

        // Mutate: add c.md and a link a→c (changes triple_count).
        register_note(&state, "c.md").await;
        insert_triple_node(&state, "a.md", "links_to", "c").await;

        // Third call: version mismatch → invalidated → c.md appears.
        let g3 = super::local_graph_cached(&state, "a.md", 1).await;
        assert!(
            g3.nodes.iter().any(|n| n.id == "c.md"),
            "after mutation, c.md must appear in recomputed result: {:?}",
            g3.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }


    // -----------------------------------------------------------------------
    // 13. cache_cap_clears_when_exceeded
    // -----------------------------------------------------------------------
    /// When local_graph_cache exceeds 256 entries, the next insert clears the map
    /// first, then inserts the new entry — so len() == 1 afterward.
    #[tokio::test]
    async fn cache_cap_clears_when_exceeded() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        // Pre-fill with 257 dummy entries to exceed the cap.
        {
            let mut cache = state.local_graph_cache.lock().unwrap();
            for i in 0..257usize {
                cache.insert(
                    (format!("dummy_{i}.md"), 1usize),
                    ((0, 0), super::LocalGraph::default()),
                );
            }
        }
        assert_eq!(
            state.local_graph_cache.lock().unwrap().len(),
            257,
            "pre-condition: cache must have 257 dummy entries"
        );

        // Call for a key not in the cache; cap fires before insert.
        let _ = super::local_graph_cached(&state, "fresh.md", 1).await;

        let cache = state.local_graph_cache.lock().unwrap();
        assert_eq!(
            cache.len(),
            1,
            "cap must clear oversized cache then insert one entry; got {} entries",
            cache.len()
        );
        assert!(
            cache.contains_key(&("fresh.md".to_string(), 1usize)),
            "fresh.md must be in cache after cap-and-insert"
        );
    }


    // -----------------------------------------------------------------------
    // timestamp field: created triple → GNode.timestamp
    // -----------------------------------------------------------------------

    /// A `created` triple for a note must surface its value in `GNode.timestamp`.
    /// A note without a `created` triple must have `GNode.timestamp == None`.
    #[tokio::test]
    async fn gnode_timestamp_from_created_triple() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        register_note(&state, "a.md").await;
        register_note(&state, "b.md").await;
        insert_triple_node(&state, "a.md", "links_to", "b").await;
        insert_triple_lit(&state, "a.md", "created", "2025-03-15").await;

        let g = super::local_graph(&state, "a.md", 1).await;
        let node_a = g.nodes.iter().find(|n| n.id == "a.md")
            .expect("a.md must be in graph");
        assert_eq!(
            node_a.timestamp,
            Some("2025-03-15".to_string()),
            "GNode.timestamp must come from the created triple"
        );
        let node_b = g.nodes.iter().find(|n| n.id == "b.md")
            .expect("b.md must be in graph");
        assert_eq!(
            node_b.timestamp,
            None,
            "GNode without created triple must have timestamp=None"
        );
    }

    // -----------------------------------------------------------------------
    // 14. frontier_cap_bounds_semantic (optional perf guard)
    // -----------------------------------------------------------------------
    /// A hub with >SEM_FRONTIER_CAP link-neighbors at depth=2 still completes
    /// and the result satisfies NODE_CAP and includes the center.
    #[tokio::test]
    async fn frontier_cap_bounds_semantic() {
        let state = stub_state();
        register_note(&state, "center.md").await;
        register_note(&state, "hub.md").await;
        insert_triple_node(&state, "center.md", "links_to", "hub").await;
        insert_chunk(&state, "center.md", "alpha content center", e0()).await;
        insert_chunk(&state, "hub.md", "alpha content hub", e0()).await;

        // 20 spokes — more than SEM_FRONTIER_CAP (16).
        for i in 0..20usize {
            let path = format!("spoke{i}.md");
            register_note(&state, &path).await;
            insert_triple_node(&state, "hub.md", "links_to", &format!("spoke{i}")).await;
            insert_chunk(&state, &path, "alpha content spoke", e0()).await;
        }

        let g = super::local_graph(&state, "center.md", 2).await;
        assert!(
            g.nodes.len() <= super::NODE_CAP,
            "nodes must be ≤ NODE_CAP ({}), got {}",
            super::NODE_CAP,
            g.nodes.len()
        );
        assert!(
            g.nodes.iter().any(|n| n.id == "center.md"),
            "center must always be in the graph"
        );
    }


    // -----------------------------------------------------------------------
    // 15. neural_local_graph_has_semantic_edge  (real e5 model, gated)
    // -----------------------------------------------------------------------
    /// End-to-end acceptance test using the real multilingual-e5-small model.
    /// Skipped when the model files are absent. Requires `ORT_DYLIB_PATH`.
    ///
    /// Two same-topic Spanish notes (dog care) must share a semantic edge;
    /// an off-topic note (elections) must not appear (below NEIGHBOR_FLOOR=0.88).
    #[cfg(feature = "neural-embeddings")]
    #[tokio::test]
    async fn neural_local_graph_has_semantic_edge() {
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
            eprintln!(
                "skipping neural_local_graph_has_semantic_edge: e5 model not found at {model_dir}"
            );
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
        // Two same-topic notes about dog care (reused from neural_note_context_finds_same_topic).
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

        let g = super::local_graph(&state, "perros1.md", 1).await;

        assert!(
            g.semantic_ready,
            "neural embedder (384d) must set semantic_ready=true"
        );

        // There must be a semantic edge connecting perros1↔perros2 (either orientation).
        let has_sem_edge = g.edges.iter().any(|e| {
            e.kind == "semantic"
                && ((e.source == "perros1.md" && e.target == "perros2.md")
                    || (e.source == "perros2.md" && e.target == "perros1.md"))
        });
        assert!(
            has_sem_edge,
            "perros1.md and perros2.md (same-topic) must share a semantic edge: {:?}",
            g.edges
        );

        // elecciones.md is off-topic; cosine vs perros1 is below NEIGHBOR_FLOOR (0.88).
        assert!(
            !g.edges.iter().any(|e| {
                e.kind == "semantic"
                    && (e.source == "elecciones.md" || e.target == "elecciones.md")
            }),
            "off-topic elecciones.md must not have a semantic edge (below NEIGHBOR_FLOOR=0.88): {:?}",
            g.edges
        );
    }
}
