// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Vault Map: a deterministic, offline map + navigation manual derived from the
//! semantic graph (links/tags/types) and neural embeddings (semantic topics).

use serde::Serialize;
use std::collections::BTreeMap;

/// The full vault map returned to the UI and the connected AI.
#[derive(Debug, Clone, Serialize, Default)]
pub struct VaultMap {
    pub totals: Totals,
    pub entry_points: Vec<EntryPoint>,
    pub topics: Vec<Topic>,
    pub tag_clusters: Vec<TagGroup>,
    pub orphans: Vec<String>,
    pub tags: Vec<TagCount>,
    pub types: Vec<TypeCount>,
    pub graph: GraphView,
    pub guidance: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Totals {
    pub notes: usize,
    pub links: usize,
    pub clusters: usize,
    pub orphans: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryPoint {
    pub path: String,
    pub title: String,
    pub in_links: usize,
    pub out_links: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Topic {
    pub id: usize,
    pub label: String,
    pub representative: String,
    pub notes: Vec<String>,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagGroup {
    pub tag: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeCount {
    pub ty: String,
    pub count: usize,
}

#[allow(dead_code)] // used in MM-1 assembly
#[derive(Debug, Clone, Serialize, Default)]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[allow(dead_code)] // used in MM-1 assembly
#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub cluster: i64,
    pub degree: usize,
}

#[allow(dead_code)] // used in MM-1 assembly
#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

/// Max nodes rendered in the visual graph (top-degree); larger vaults are capped.
#[allow(dead_code)] // used in MM-1 assembly
const GRAPH_NODE_CAP: usize = 600;

/// Basename without directory or extension, for wikilink resolution + titles.
pub(crate) fn basename(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file).to_string()
}

/// Structural inputs derived from the graph (no embeddings).
#[derive(Debug, Default)]
pub(crate) struct Structural {
    pub notes: Vec<String>,                       // note rel_paths, sorted
    pub in_deg: BTreeMap<String, usize>,          // note -> incoming resolved links
    pub out_deg: BTreeMap<String, usize>,         // note -> outgoing resolved links
    pub edges: Vec<(String, String)>,             // resolved (src note, dst note)
    pub tag_notes: BTreeMap<String, Vec<String>>, // tag -> notes
    pub type_counts: BTreeMap<String, usize>,     // type -> count
    pub link_count: usize,                        // total resolved links
}

pub(crate) fn derive_structural(graph: &aingle_graph::GraphDB) -> Structural {
    use aingle_graph::{Predicate, TriplePattern};

    let strip = |n: String| n.trim_start_matches('<').trim_end_matches('>').to_string();
    let find = |pred: &str| -> Vec<(String, String)> {
        graph
            .find(TriplePattern::any().with_predicate(Predicate::named(pred)))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| {
                let subj = strip(t.subject.to_string());
                t.object_string().map(|o| (subj, o.to_string()))
            })
            .collect()
    };

    // Note set from the source-hash registry.
    let mut notes: Vec<String> = find(crate::service::ingest::PRED_SOURCE_HASH)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    notes.sort();
    notes.dedup();

    // Basename -> note path index for wikilink resolution.
    let mut by_base: BTreeMap<String, String> = BTreeMap::new();
    for n in &notes {
        by_base.entry(basename(n)).or_insert_with(|| n.clone());
    }
    let resolve = |target: &str| -> Option<String> {
        // exact path first, else basename match
        if notes.iter().any(|n| n == target) {
            Some(target.to_string())
        } else {
            by_base.get(&basename(target)).cloned()
        }
    };

    let mut in_deg: BTreeMap<String, usize> = BTreeMap::new();
    let mut out_deg: BTreeMap<String, usize> = BTreeMap::new();
    let mut edges: Vec<(String, String)> = Vec::new();
    for (src, target) in find("links_to") {
        if !notes.iter().any(|n| n == &src) {
            continue;
        }
        if let Some(dst) = resolve(&target) {
            if dst == src {
                continue;
            }
            *out_deg.entry(src.clone()).or_default() += 1;
            *in_deg.entry(dst.clone()).or_default() += 1;
            edges.push((src, dst));
        }
    }
    let link_count = edges.len();

    let mut tag_notes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (note, tag) in find("tagged") {
        if notes.iter().any(|n| n == &note) {
            tag_notes.entry(tag).or_default().push(note);
        }
    }
    for v in tag_notes.values_mut() {
        v.sort();
        v.dedup();
    }

    let mut type_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_note, ty) in find("type") {
        *type_counts.entry(ty).or_default() += 1;
    }

    Structural {
        notes,
        in_deg,
        out_deg,
        edges,
        tag_notes,
        type_counts,
        link_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aingle_graph::{NodeId, Predicate, Triple, Value};

    pub(super) async fn graph_with(
        triples: &[(&str, &str, &str)],
    ) -> crate::state::AppState {
        let state = crate::state::AppState::with_db_path(":memory:", None).unwrap();
        {
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
        state
    }

    #[tokio::test]
    async fn structural_hubs_orphans_tags() {
        // a.md and b.md both link to hub.md; orphan.md links to nothing and is
        // linked by nothing. Tags group a.md + b.md under "storage".
        let state = graph_with(&[
            ("a.md", "aingle:source_hash", "h1"),
            ("b.md", "aingle:source_hash", "h2"),
            ("hub.md", "aingle:source_hash", "h3"),
            ("orphan.md", "aingle:source_hash", "h4"),
            ("a.md", "links_to", "hub"),
            ("b.md", "links_to", "hub"),
            ("a.md", "tagged", "storage"),
            ("b.md", "tagged", "storage"),
            ("a.md", "type", "note"),
        ])
        .await;

        let s = {
            let g = state.graph.read().await;
            super::derive_structural(&g)
        };
        assert_eq!(s.notes.len(), 4);
        assert_eq!(s.in_deg.get("hub.md").copied().unwrap_or(0), 2, "hub has 2 incoming");
        assert_eq!(s.out_deg.get("a.md").copied().unwrap_or(0), 1);
        assert_eq!(s.tag_notes.get("storage").map(|v| v.len()), Some(2));
        assert_eq!(s.link_count, 2);
    }
}
