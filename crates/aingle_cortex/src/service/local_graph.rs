// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Local graph neighborhood for a single note: typed edges (link / semantic / tag)
//! up to depth 2 for the Akashi per-note graph panel (VC-2).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::service::triple_util::{obj_string, resolve_link_target};
use crate::service::vault_map::{basename, is_maps_path};
use crate::service::context::{note_context, NEIGHBOR_FLOOR};

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
    let (notes, links_raw, tagged_raw): (Vec<String>, PairVec, PairVec) = {
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
        (ns, lnks, tags)
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
            for (src, dst) in links.iter().filter(|(s, _)| s == &n) {
                edges.push(TypedEdge {
                    source: src.clone(),
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
            // --- link edges (incoming to n) ---
            for (src, dst) in links.iter().filter(|(_, d)| d == &n) {
                edges.push(TypedEdge {
                    source: src.clone(),
                    target: dst.clone(),
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

            // --- semantic edges ---
            if semantic_grade {
                let ctx = note_context(state, &n, SEM_PER_NODE).await;
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
    let mut seen_sym: HashSet<(String, String, String)> = HashSet::new();
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
                let key = (lo, hi, e.kind.clone());
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
    let final_edges: Vec<TypedEdge> = deduped
        .into_iter()
        .filter(|e| kept_ids.contains(&e.source) && kept_ids.contains(&e.target))
        .collect();

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
}
