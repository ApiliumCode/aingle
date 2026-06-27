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
    /// Path to the user's identity note (`me.md`) if present — read this first.
    pub identity: Option<String>,
    /// Note paths tagged as reusable skills/processes (the "skill map").
    pub skills: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Default)]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub cluster: i64,
    pub degree: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

/// Max nodes rendered in the visual graph (top-degree); larger vaults are capped.
const GRAPH_NODE_CAP: usize = 600;

/// Tags (case-insensitive) that mark a note as a reusable skill/process.
const SKILL_TAGS: [&str; 6] = ["skill", "process", "sop", "workflow", "how-to", "howto"];

/// Basename without directory or extension, for wikilink resolution + titles.
pub(crate) fn basename(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file).to_string()
}

/// True for paths under the generated maps folder (excluded from the vault map).
pub(crate) fn is_maps_path(path: &str) -> bool {
    path.starts_with("_maps/") || path.starts_with("_maps\\")
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
    notes.retain(|n| !is_maps_path(n));

    // O(log n) membership set — avoids linear scans during link/tag resolution.
    let note_set: std::collections::BTreeSet<&str> = notes.iter().map(|s| s.as_str()).collect();

    // Basename -> note path index for wikilink resolution.
    let mut by_base: BTreeMap<String, String> = BTreeMap::new();
    for n in &notes {
        by_base.entry(basename(n)).or_insert_with(|| n.clone());
    }
    let resolve = |target: &str| -> Option<String> {
        // exact path first, else basename match
        if note_set.contains(target) {
            Some(target.to_string())
        } else {
            by_base.get(&basename(target)).cloned()
        }
    };

    let mut in_deg: BTreeMap<String, usize> = BTreeMap::new();
    let mut out_deg: BTreeMap<String, usize> = BTreeMap::new();
    let mut edges: Vec<(String, String)> = Vec::new();
    for (src, target) in find("links_to") {
        if !note_set.contains(src.as_str()) {
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
        if note_set.contains(note.as_str()) {
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

/// Cosine similarity between two raw vectors (same length).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let ma = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if ma == 0.0 || mb == 0.0 {
        0.0
    } else {
        dot / (ma * mb)
    }
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
                let mx = mean_sim(x, &members, &names, vecs);
                let my = mean_sim(y, &members, &names, vecs);
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

fn mean_sim(self_idx: usize, members: &[usize], names: &[&String], vecs: &BTreeMap<String, Vec<f32>>) -> f32 {
    if members.len() <= 1 {
        return 1.0;
    }
    let v = &vecs[names[self_idx]];
    let mut sum = 0.0;
    let mut cnt = 0;
    for &m in members {
        if m == self_idx {
            continue;
        }
        sum += cosine(v, &vecs[names[m]]);
        cnt += 1;
    }
    if cnt == 0 { 1.0 } else { sum / cnt as f32 }
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

/// Cosine threshold for semantic topic membership (note-level mean vectors).
/// Calibrated for E5; the hash embedder produces denser similarities but topics
/// remain a useful secondary facet.
const SEMANTIC_THRESHOLD: f32 = 0.88;
/// Above this note count, skip O(n^2) semantic clustering (tag clusters remain).
const SEMANTIC_NOTE_CAP: usize = 2000;

/// Compute the full vault map (uncached).
pub async fn compute_vault_map(state: &crate::state::AppState) -> VaultMap {
    let s = {
        let g = state.graph.read().await;
        derive_structural(&g)
    };

    // Hubs / entry points: top by in-degree, tie-break out-degree.
    let mut entry_points: Vec<EntryPoint> = s
        .notes
        .iter()
        .map(|p| EntryPoint {
            path: p.clone(),
            title: basename(p),
            in_links: s.in_deg.get(p).copied().unwrap_or(0),
            out_links: s.out_deg.get(p).copied().unwrap_or(0),
        })
        .collect();
    entry_points.sort_by(|a, b| {
        b.in_links
            .cmp(&a.in_links)
            .then(b.out_links.cmp(&a.out_links))
            .then(a.path.cmp(&b.path))
    });
    entry_points.retain(|e| e.in_links > 0 || e.out_links > 0);
    entry_points.truncate(20);

    // Orphans.
    let orphans: Vec<String> = s
        .notes
        .iter()
        .filter(|p| {
            s.in_deg.get(*p).copied().unwrap_or(0) == 0
                && s.out_deg.get(*p).copied().unwrap_or(0) == 0
        })
        .cloned()
        .collect();

    // Semantic topics (capped).
    let topics = if s.notes.len() <= SEMANTIC_NOTE_CAP {
        let mem = state.memory.read().await;
        let all_vecs = per_note_vectors(&mem);
        let vecs: std::collections::BTreeMap<String, Vec<f32>> = all_vecs
            .into_iter()
            .filter(|(p, _)| s.notes.iter().any(|n| n == p))
            .collect();
        if vecs.len() >= 2 {
            cluster_semantic(&vecs, SEMANTIC_THRESHOLD)
        } else {
            Vec::new()
        }
    } else {
        log::info!(
            "vault_map: {} notes > cap {}, skipping semantic clustering (tag clusters used)",
            s.notes.len(),
            SEMANTIC_NOTE_CAP
        );
        Vec::new()
    };

    // Tag clusters + tag index.
    let mut tag_clusters: Vec<TagGroup> = s
        .tag_notes
        .iter()
        .map(|(tag, notes)| TagGroup { tag: tag.clone(), notes: notes.clone() })
        .collect();
    tag_clusters.sort_by(|a, b| b.notes.len().cmp(&a.notes.len()).then(a.tag.cmp(&b.tag)));
    let mut tags: Vec<TagCount> = s
        .tag_notes
        .iter()
        .map(|(tag, notes)| TagCount { tag: tag.clone(), count: notes.len() })
        .collect();
    tags.sort_by(|a, b| b.count.cmp(&a.count).then(a.tag.cmp(&b.tag)));

    let mut types: Vec<TypeCount> = s
        .type_counts
        .iter()
        .map(|(ty, count)| TypeCount { ty: ty.clone(), count: *count })
        .collect();
    types.sort_by(|a, b| b.count.cmp(&a.count).then(a.ty.cmp(&b.ty)));

    // Cluster id per note (for graph coloring).
    let mut cluster_of: BTreeMap<String, i64> = BTreeMap::new();
    for t in &topics {
        for npath in &t.notes {
            cluster_of.insert(npath.clone(), t.id as i64);
        }
    }

    // GraphView (cap by degree).
    let mut ranked: Vec<&String> = s.notes.iter().collect();
    ranked.sort_by(|a, b| {
        let da =
            s.in_deg.get(*a).copied().unwrap_or(0) + s.out_deg.get(*a).copied().unwrap_or(0);
        let db =
            s.in_deg.get(*b).copied().unwrap_or(0) + s.out_deg.get(*b).copied().unwrap_or(0);
        db.cmp(&da).then(a.cmp(b))
    });
    if s.notes.len() > GRAPH_NODE_CAP {
        log::info!(
            "vault_map: {} notes > graph cap {}, rendering the {} most-connected",
            s.notes.len(),
            GRAPH_NODE_CAP,
            GRAPH_NODE_CAP
        );
    }
    let kept: std::collections::BTreeSet<String> =
        ranked.into_iter().take(GRAPH_NODE_CAP).cloned().collect();
    let nodes: Vec<GraphNode> = kept
        .iter()
        .map(|p| GraphNode {
            id: p.clone(),
            label: basename(p),
            cluster: cluster_of.get(p).copied().unwrap_or(-1),
            degree: s.in_deg.get(p).copied().unwrap_or(0)
                + s.out_deg.get(p).copied().unwrap_or(0),
        })
        .collect();
    let edges: Vec<GraphEdge> = s
        .edges
        .iter()
        .filter(|(a, b)| kept.contains(a) && kept.contains(b))
        .map(|(a, b)| GraphEdge { source: a.clone(), target: b.clone() })
        .collect();

    let totals = Totals {
        notes: s.notes.len(),
        links: s.link_count,
        clusters: topics.len(),
        orphans: orphans.len(),
    };

    // Identity: the root `me.md` (exact rel_path), read first by the AI.
    let identity = s
        .notes
        .iter()
        .find(|n| n.as_str() == "me.md" || n.as_str() == "me.markdown")
        .cloned();

    // Skills: notes tagged with any SKILL_TAGS value (case-insensitive).
    let mut skills: Vec<String> = Vec::new();
    for (tag, notes) in &s.tag_notes {
        if SKILL_TAGS.contains(&tag.to_lowercase().as_str()) {
            skills.extend(notes.iter().cloned());
        }
    }
    skills.sort();
    skills.dedup();

    let guidance = if totals.notes == 0 {
        "Vault not yet indexed. Once notes are ingested, this map lists entry-point (hub) \
         notes, topic clusters, and orphans so you can navigate accurately."
            .to_string()
    } else {
        let mut g = String::new();
        if identity.is_some() {
            g.push_str("Read me.md first for the user's identity and preferences. ");
        }
        g.push_str(&format!(
            "This vault has {} notes, {} links, {} topics, {} orphans. To answer about a topic, \
             start at its entry_points and the topic's representative note, then follow links. \
             Ground every claim with aingle_ground (it returns signed provenance). Orphan notes \
             are unconnected and may be incomplete.",
            totals.notes, totals.links, totals.clusters, totals.orphans
        ));
        if !skills.is_empty() {
            g.push_str(" Follow the skill notes (skill-map) for the user's documented processes.");
        }
        g
    };

    VaultMap {
        totals,
        entry_points,
        topics,
        tag_clusters,
        orphans,
        tags,
        types,
        graph: GraphView { nodes, edges },
        guidance,
        identity,
        skills,
    }
}

/// Cached vault map, keyed on `(graph triple_count, memory bytes)`. The graph
/// count invalidates on structural change; the memory-bytes signal invalidates
/// when chunk content/embeddings change even if the triple count is unchanged
/// (e.g. a same-structure prose edit) — so semantic topics don't go stale.
pub async fn vault_map_cached(state: &crate::state::AppState) -> VaultMap {
    let tc = { state.graph.read().await.stats().triple_count };
    let mem_bytes = { state.memory.read().await.stats().total_memory_bytes };
    let key = (tc, mem_bytes);
    {
        let cache = state.vault_map_cache.lock().expect("vault_map cache poisoned");
        if let Some((cached_key, map)) = cache.as_ref() {
            if *cached_key == key {
                return map.clone();
            }
        }
    }
    // The cache mutex is intentionally released before the async compute to avoid
    // holding it across an `.await` point.
    let map = compute_vault_map(state).await;
    let mut cache = state.vault_map_cache.lock().expect("vault_map cache poisoned");
    *cache = Some((key, map.clone()));
    map
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
            // self-link: "a" resolves to "a.md" via basename → must be skipped
            ("a.md", "links_to", "a"),
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
        // Self-link must not be counted as incoming for a.md.
        assert_eq!(s.in_deg.get("a.md").copied().unwrap_or(0), 0, "self-link must not count as incoming");
        // type_counts must reflect the triple ("a.md","type","note").
        assert_eq!(s.type_counts.get("note"), Some(&1));
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

    #[tokio::test]
    async fn vault_map_cached_assembles_and_caches() {
        let state = graph_with(&[
            ("a.md", "aingle:source_hash", "h1"),
            ("hub.md", "aingle:source_hash", "h2"),
            ("orphan.md", "aingle:source_hash", "h3"),
            ("a.md", "links_to", "hub"),
            ("a.md", "tagged", "storage"),
        ])
        .await;

        let m1 = super::vault_map_cached(&state).await;
        assert_eq!(m1.totals.notes, 3);
        assert_eq!(m1.totals.links, 1);
        assert_eq!(m1.totals.orphans, 1); // orphan.md
        assert!(m1.entry_points.iter().any(|e| e.path == "hub.md" && e.in_links == 1));
        assert!(m1.tag_clusters.iter().any(|t| t.tag == "storage"));
        assert!(!m1.guidance.is_empty());
        assert!(!m1.graph.nodes.is_empty());

        // Cached: no graph change → identical totals (and cheap).
        let m2 = super::vault_map_cached(&state).await;
        assert_eq!(m2.totals.notes, m1.totals.notes);
    }

    #[tokio::test]
    async fn excludes_maps_folder_notes() {
        let state = graph_with(&[
            ("real.md", "aingle:source_hash", "h1"),
            ("hub.md", "aingle:source_hash", "h2"),
            ("_maps/vault-map.md", "aingle:source_hash", "h3"),
            ("_maps/vault-map.md", "links_to", "hub"),
            ("real.md", "links_to", "hub"),
        ])
        .await;

        let map = super::vault_map_cached(&state).await;
        assert_eq!(map.totals.notes, 2, "_maps/ notes excluded from the count");
        assert!(!map.graph.nodes.iter().any(|n| n.id.starts_with("_maps/")));
        assert!(!map.entry_points.iter().any(|e| e.path.starts_with("_maps/")));
        let hub = map.entry_points.iter().find(|e| e.path == "hub.md").expect("hub");
        assert_eq!(hub.in_links, 1, "the _maps link to hub must be excluded");
    }

    #[tokio::test]
    async fn detects_identity_and_skills() {
        let state = graph_with(&[
            ("me.md", "aingle:source_hash", "h0"),
            ("note.md", "aingle:source_hash", "h1"),
            ("deploy.md", "aingle:source_hash", "h2"),
            ("writing.md", "aingle:source_hash", "h3"),
            ("deploy.md", "tagged", "sop"),
            ("writing.md", "tagged", "process"),
            ("note.md", "tagged", "misc"),
        ])
        .await;

        let map = super::vault_map_cached(&state).await;
        assert_eq!(map.identity.as_deref(), Some("me.md"));
        assert!(map.skills.contains(&"deploy.md".to_string()));
        assert!(map.skills.contains(&"writing.md".to_string()));
        assert!(!map.skills.contains(&"note.md".to_string()), "non-skill tag excluded");
        assert!(map.guidance.contains("me.md"), "guidance points at identity");
    }

    #[tokio::test]
    async fn vault_map_cache_invalidates_on_change() {
        let state = graph_with(&[("a.md", "aingle:source_hash", "h1")]).await;
        let m1 = super::vault_map_cached(&state).await;
        assert_eq!(m1.totals.notes, 1);
        {
            let g = state.graph.write().await;
            g.insert(Triple::new(
                NodeId::named("b.md"),
                Predicate::named("aingle:source_hash"),
                Value::literal("h2"),
            ))
            .unwrap();
        }
        let m2 = super::vault_map_cached(&state).await;
        assert_eq!(m2.totals.notes, 2, "cache must invalidate when triple_count changes");
    }
}
