// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Shortest verified path between two notes over the typed vault graph.
//!
//! Edges come from two sources, in the same spirit as `local_graph`:
//! - **link** edges: every `links_to` triple, traversed in both directions.
//! - **semantic** edges: the verified neighbors of the two endpoints
//!   (`note_context_cached`), so two topics that never linked each other can
//!   still meet through the link fabric between them. Endpoint-only semantic
//!   expansion keeps the search bounded; the fabric in between is structural.
//!
//! The result is a chain of typed hops, each carrying its evidence (kind,
//! similarity score, signed-provenance anchor when available) so an agent can
//! cite every step of the connection instead of asserting it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::service::triple_util::{basename, obj_string, resolve_link_target, strip_brackets};

/// Hard clamp on the requested hop budget.
const MAX_HOPS_CAP: usize = 6;
/// Default hop budget when the caller does not specify one.
const DEFAULT_MAX_HOPS: usize = 4;
/// Upper bound on BFS node expansions; keeps worst-case latency flat on
/// heavily linked vaults.
const SEARCH_BUDGET: usize = 2000;
/// Semantic neighbors fetched per endpoint.
const ENDPOINT_SEM_LIMIT: usize = 8;

/// One traversed edge in the connecting chain.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathHop {
    pub source: String,
    pub target: String,
    /// `"link"` | `"semantic"`
    pub kind: String,
    /// Cosine similarity — present only for semantic hops.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    /// Signed DAG action hash for semantic hops (🔒). `None` if unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_anchor: Option<String>,
}

/// Result of a path search between two notes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathResult {
    /// Resolved canonical path of the start note (input echoed if unresolved).
    pub from: String,
    /// Resolved canonical path of the goal note (input echoed if unresolved).
    pub to: String,
    pub found: bool,
    /// Node ids along the path, `from` first, `to` last. Empty when not found.
    pub nodes: Vec<String>,
    /// Typed edges connecting consecutive `nodes`. Empty when not found.
    pub hops: Vec<PathHop>,
    /// Hop budget actually used for the search (after clamping).
    pub max_hops: usize,
    /// Nodes expanded during the search (diagnostic).
    pub searched: usize,
    /// `false` when the embedder cannot produce semantic edges (hash fallback):
    /// the search still runs over link edges only.
    pub semantic_ready: bool,
    /// Human-readable explanation when `found` is `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn not_found(
    from: &str,
    to: &str,
    max_hops: usize,
    searched: usize,
    sem: bool,
    why: String,
) -> PathResult {
    PathResult {
        from: from.to_string(),
        to: to.to_string(),
        found: false,
        nodes: Vec::new(),
        hops: Vec::new(),
        max_hops,
        searched,
        semantic_ready: sem,
        note: Some(why),
    }
}

/// Find the shortest typed path between `from_raw` and `to_raw`.
pub async fn find_path(
    state: &crate::state::AppState,
    from_raw: &str,
    to_raw: &str,
    max_hops: Option<usize>,
) -> PathResult {
    use aingle_graph::{Predicate, TriplePattern};

    let max_hops = max_hops.unwrap_or(DEFAULT_MAX_HOPS).clamp(1, MAX_HOPS_CAP);

    // -----------------------------------------------------------------------
    // 1. Structural data: note set + links (one graph read, backlinks-style).
    // -----------------------------------------------------------------------
    let (notes, links): (Vec<String>, Vec<(String, String)>) = {
        let g = state.graph.read().await;
        let collect = |pred: &str| -> Vec<(String, String)> {
            g.find(TriplePattern::any().with_predicate(Predicate::named(pred)))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|t| {
                    obj_string(&t).map(|o| (strip_brackets(&t.subject.to_string()).to_string(), o))
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
    let mut by_base: BTreeMap<String, String> = BTreeMap::new();
    for n in &notes {
        by_base.entry(basename(n)).or_insert_with(|| n.clone());
    }

    // -----------------------------------------------------------------------
    // 2. Resolve the endpoints with the same rules wikilinks use.
    // -----------------------------------------------------------------------
    let resolve = |raw: &str| -> Option<String> {
        if note_set.contains(raw) {
            return Some(raw.to_string());
        }
        resolve_link_target(raw, &note_set, &by_base)
    };
    let Some(from) = resolve(from_raw) else {
        return not_found(
            from_raw,
            to_raw,
            max_hops,
            0,
            false,
            format!("start note not found: {from_raw}"),
        );
    };
    let Some(to) = resolve(to_raw) else {
        return not_found(
            &from,
            to_raw,
            max_hops,
            0,
            false,
            format!("goal note not found: {to_raw}"),
        );
    };
    if from == to {
        return PathResult {
            from: from.clone(),
            to,
            found: true,
            nodes: vec![from],
            hops: Vec::new(),
            max_hops,
            searched: 0,
            semantic_ready: true,
            note: None,
        };
    }

    // -----------------------------------------------------------------------
    // 3. Adjacency: undirected link edges + endpoint semantic edges.
    // -----------------------------------------------------------------------
    type Edge = (String, String, Option<f32>, Option<String>); // (peer, kind, score, anchor)
    let mut adj: BTreeMap<String, Vec<Edge>> = BTreeMap::new();
    let mut add = |a: &str, b: &str, kind: &str, score: Option<f32>, anchor: Option<String>| {
        adj.entry(a.to_string()).or_default().push((
            b.to_string(),
            kind.to_string(),
            score,
            anchor.clone(),
        ));
        adj.entry(b.to_string()).or_default().push((
            a.to_string(),
            kind.to_string(),
            score,
            anchor,
        ));
    };

    for (src, target) in &links {
        if !note_set.contains(src.as_str()) {
            continue;
        }
        if let Some(dst) = resolve_link_target(target, &note_set, &by_base) {
            if src != &dst {
                add(src, &dst, "link", None, None);
            }
        }
    }

    let mut semantic_ready = false;
    for endpoint in [&from, &to] {
        let ctx =
            crate::service::context::note_context_cached(state, endpoint, ENDPOINT_SEM_LIMIT).await;
        semantic_ready |= ctx.semantic_ready;
        for n in ctx.neighbors {
            if endpoint != &n.path {
                add(
                    endpoint,
                    &n.path,
                    "semantic",
                    Some(n.score),
                    n.provenance_anchor,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // 4. BFS with parent tracking (shortest by hop count).
    // -----------------------------------------------------------------------
    let mut parent: BTreeMap<String, (String, String, Option<f32>, Option<String>)> =
        BTreeMap::new();
    let mut depth: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    depth.insert(from.clone(), 0);
    queue.push_back(from.clone());
    let mut searched = 0usize;
    let mut reached = false;

    while let Some(cur) = queue.pop_front() {
        let d = depth[&cur];
        if d >= max_hops {
            continue;
        }
        searched += 1;
        if searched > SEARCH_BUDGET {
            break;
        }
        for (peer, kind, score, anchor) in adj.get(&cur).cloned().unwrap_or_default() {
            if depth.contains_key(&peer) {
                continue;
            }
            depth.insert(peer.clone(), d + 1);
            parent.insert(peer.clone(), (cur.clone(), kind, score, anchor));
            if peer == to {
                reached = true;
                queue.clear();
                break;
            }
            queue.push_back(peer);
        }
        if reached {
            break;
        }
    }

    if !reached {
        return not_found(
            &from,
            &to,
            max_hops,
            searched,
            semantic_ready,
            format!("no connection within {max_hops} hops"),
        );
    }

    // -----------------------------------------------------------------------
    // 5. Reconstruct the chain, `from` → `to`.
    // -----------------------------------------------------------------------
    let mut nodes = vec![to.clone()];
    let mut hops: Vec<PathHop> = Vec::new();
    let mut cur = to.clone();
    while cur != from {
        let (prev, kind, score, anchor) = parent[&cur].clone();
        hops.push(PathHop {
            source: prev.clone(),
            target: cur.clone(),
            kind,
            score,
            provenance_anchor: anchor,
        });
        nodes.push(prev.clone());
        cur = prev;
    }
    nodes.reverse();
    hops.reverse();

    PathResult {
        from,
        to,
        found: true,
        nodes,
        hops,
        max_hops,
        searched,
        semantic_ready,
        note: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use aingle_graph::{NodeId, Predicate, Triple, Value};

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

    fn state() -> AppState {
        AppState::with_db_path(":memory:", None).unwrap()
    }

    /// A --links_to--> B: one link hop.
    #[tokio::test]
    async fn direct_link_one_hop() {
        let s = state();
        register_note(&s, "a.md").await;
        register_note(&s, "b.md").await;
        insert_triple_node(&s, "a.md", "links_to", "b.md").await;

        let r = find_path(&s, "a.md", "b.md", None).await;
        assert!(r.found);
        assert_eq!(r.nodes, vec!["a.md", "b.md"]);
        assert_eq!(r.hops.len(), 1);
        assert_eq!(r.hops[0].kind, "link");
    }

    /// A → C ← B (both link INTO c): undirected traversal finds A-C-B.
    #[tokio::test]
    async fn two_hops_undirected() {
        let s = state();
        for n in ["a.md", "b.md", "c.md"] {
            register_note(&s, n).await;
        }
        insert_triple_node(&s, "a.md", "links_to", "c.md").await;
        insert_triple_node(&s, "b.md", "links_to", "c.md").await;

        let r = find_path(&s, "a.md", "b.md", None).await;
        assert!(r.found);
        assert_eq!(r.nodes, vec!["a.md", "c.md", "b.md"]);
        assert_eq!(r.hops.len(), 2);
    }

    /// Disconnected notes: found=false with an explanation.
    #[tokio::test]
    async fn disconnected_not_found() {
        let s = state();
        register_note(&s, "a.md").await;
        register_note(&s, "b.md").await;

        let r = find_path(&s, "a.md", "b.md", None).await;
        assert!(!r.found);
        assert!(r.note.unwrap().contains("no connection"));
    }

    /// Endpoints resolve by basename, wikilink-style.
    #[tokio::test]
    async fn resolves_by_basename() {
        let s = state();
        register_note(&s, "chars/ashitaka.md").await;
        register_note(&s, "films/mononoke.md").await;
        insert_triple_node(&s, "chars/ashitaka.md", "links_to", "films/mononoke.md").await;

        let r = find_path(&s, "ashitaka", "mononoke", None).await;
        assert!(
            r.found,
            "basename resolution should find the notes: {:?}",
            r.note
        );
        assert_eq!(r.from, "chars/ashitaka.md");
        assert_eq!(r.to, "films/mononoke.md");
    }

    /// A chain longer than max_hops is honestly not found.
    #[tokio::test]
    async fn respects_hop_budget() {
        let s = state();
        for n in ["a.md", "b.md", "c.md", "d.md"] {
            register_note(&s, n).await;
        }
        insert_triple_node(&s, "a.md", "links_to", "b.md").await;
        insert_triple_node(&s, "b.md", "links_to", "c.md").await;
        insert_triple_node(&s, "c.md", "links_to", "d.md").await;

        let tight = find_path(&s, "a.md", "d.md", Some(2)).await;
        assert!(!tight.found);
        let loose = find_path(&s, "a.md", "d.md", Some(3)).await;
        assert!(loose.found);
        assert_eq!(loose.hops.len(), 3);
    }

    /// Unknown endpoint: explicit resolution error.
    #[tokio::test]
    async fn unknown_note_reported() {
        let s = state();
        register_note(&s, "a.md").await;
        let r = find_path(&s, "a.md", "ghost", None).await;
        assert!(!r.found);
        assert!(r.note.unwrap().contains("goal note not found"));
    }

    /// Same note both ends: trivial found with zero hops.
    #[tokio::test]
    async fn identity_path() {
        let s = state();
        register_note(&s, "a.md").await;
        let r = find_path(&s, "a.md", "a.md", None).await;
        assert!(r.found);
        assert_eq!(r.nodes, vec!["a.md"]);
        assert!(r.hops.is_empty());
    }
}
