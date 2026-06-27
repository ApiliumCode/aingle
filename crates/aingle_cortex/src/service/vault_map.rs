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

use ineru::Embedding;

/// Cosine similarity between two raw vectors (same length).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    Embedding::new(a.to_vec()).cosine_similarity(&Embedding::new(b.to_vec()))
}

/// Connected-components clustering over a cosine-similarity graph: notes whose
/// cosine >= `threshold` are linked; each connected component is a topic. Labeled
/// by the most central note (highest mean cosine to its component). Deterministic
/// (inputs are a sorted BTreeMap). O(n^2) — the caller caps n.
pub(crate) fn cluster_semantic(vecs: &BTreeMap<String, Vec<f32>>, threshold: f32) -> Vec<Topic> {
    let names: Vec<&String> = vecs.keys().collect();
    let n = names.len();
    // union-find
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if cosine(&vecs[names[i]], &vecs[names[j]]) >= threshold {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    // group by root
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut topics: Vec<Topic> = Vec::new();
    for (id, (_root, members)) in groups.into_iter().enumerate() {
        // central note = max mean cosine to the rest of its group
        let central = *members
            .iter()
            .max_by(|&&x, &&y| {
                let mx = mean_sim(&vecs[names[x]], &members, &names, vecs);
                let my = mean_sim(&vecs[names[y]], &members, &names, vecs);
                mx.partial_cmp(&my).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        let mut notes: Vec<String> = members.iter().map(|&m| names[m].clone()).collect();
        notes.sort();
        let rep = names[central].clone();
        topics.push(Topic {
            id,
            label: basename(&rep),
            representative: rep,
            size: notes.len(),
            notes,
        });
    }
    topics.sort_by(|a, b| b.size.cmp(&a.size).then(a.label.cmp(&b.label)));
    topics
}

fn mean_sim(
    v: &[f32],
    members: &[usize],
    names: &[&String],
    vecs: &BTreeMap<String, Vec<f32>>,
) -> f32 {
    if members.len() <= 1 {
        return 1.0;
    }
    let mut sum = 0.0;
    let mut cnt = 0;
    for &m in members {
        let other = &vecs[names[m]];
        if !std::ptr::eq(other.as_ptr(), v.as_ptr()) {
            sum += cosine(v, other);
            cnt += 1;
        }
    }
    if cnt == 0 {
        1.0
    } else {
        sum / cnt as f32
    }
}

/// Mean per-note embedding from Ineru `doc_chunk` entries, grouped by source_path.
pub(crate) fn per_note_vectors(mem: &ineru::IneruMemory) -> BTreeMap<String, Vec<f32>> {
    let mut sums: BTreeMap<String, (Vec<f32>, usize)> = BTreeMap::new();
    let mut entries = mem.stm.all_entries();
    entries.extend(mem.ltm.all_entries());
    for e in entries {
        if e.entry_type != crate::service::ingest::CHUNK_ENTRY_TYPE {
            continue;
        }
        let Some(path) = e.data.get("source_path").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(emb) = e.embedding.as_ref() else { continue };
        let entry = sums.entry(path.to_string()).or_insert_with(|| (vec![0.0; emb.0.len()], 0));
        if entry.0.len() == emb.0.len() {
            for (acc, x) in entry.0.iter_mut().zip(&emb.0) {
                *acc += *x;
            }
            entry.1 += 1;
        }
    }
    sums.into_iter()
        .filter(|(_, (_, c))| *c > 0)
        .map(|(p, (mut v, c))| {
            for x in &mut v {
                *x /= c as f32;
            }
            (p, v)
        })
        .collect()
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

    #[test]
    fn semantic_clusters_group_similar_notes() {
        // Three notes: a & b have near-identical vectors, c is far.
        let mut vecs: BTreeMap<String, Vec<f32>> = BTreeMap::new();
        vecs.insert("a.md".into(), vec![1.0, 0.0, 0.0]);
        vecs.insert("b.md".into(), vec![0.99, 0.01, 0.0]);
        vecs.insert("c.md".into(), vec![0.0, 0.0, 1.0]);

        let topics = super::cluster_semantic(&vecs, 0.9);
        // a & b together, c alone → 2 topics
        assert_eq!(topics.len(), 2);
        let big = topics.iter().max_by_key(|t| t.size).unwrap();
        assert_eq!(big.size, 2);
        assert!(
            big.notes.contains(&"a.md".to_string())
                && big.notes.contains(&"b.md".to_string())
        );
    }
}
