// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Backlinks, outgoing links, and unlinked mentions for a note — the verified
//! link graph around a single note. Deterministic; reuses links_to triples,
//! Ineru chunk text (context + unlinked scan), and DAG provenance.

use serde::Serialize;
use std::collections::BTreeMap;

/// Verified link context for one note.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Backlinks {
    pub backlinks: Vec<BacklinkRef>,
    pub outgoing: Vec<String>,
    pub unlinked: Vec<String>,
}

/// A note that links to the target, with the link's context + provenance.
#[derive(Debug, Clone, Serialize)]
pub struct BacklinkRef {
    pub path: String,
    pub context: Option<String>,
    pub provenance_anchor: Option<String>,
}

/// Basename without directory or extension (wikilink resolution + titles).
fn basename(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file).to_string()
}

/// True if `text` mentions `word` as a whole (case-insensitive) token — avoids
/// "cat" matching "category". Tokens split on non-alphanumeric.
fn mentions_word(text: &str, word: &str) -> bool {
    let w = word.to_lowercase();
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|tok| tok == w)
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

/// Compute backlinks, outgoing links, and unlinked mentions for `note`.
pub async fn backlinks(state: &crate::state::AppState, note: &str) -> Backlinks {
    use aingle_graph::{Predicate, TriplePattern};

    let strip = |n: String| n.trim_start_matches('<').trim_end_matches('>').to_string();

    // Note set + basename index.
    let (notes, links): (Vec<String>, Vec<(String, String)>) = {
        let g = state.graph.read().await;
        let collect = |pred: &str| -> Vec<(String, String)> {
            g.find(TriplePattern::any().with_predicate(Predicate::named(pred)))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|t| {
                    t.object_string()
                        .map(|o| (strip(t.subject.to_string()), o.to_string()))
                })
                .collect()
        };
        let mut notes: Vec<String> = collect(crate::service::ingest::PRED_SOURCE_HASH)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        notes.sort();
        notes.dedup();
        let links = collect("links_to");
        (notes, links)
    };

    let note_set: std::collections::BTreeSet<&str> = notes.iter().map(|s| s.as_str()).collect();
    let mut by_base: BTreeMap<String, String> = BTreeMap::new();
    for n in &notes {
        by_base.entry(basename(n)).or_insert_with(|| n.clone());
    }
    let resolve = |target: &str| -> Option<String> {
        if note_set.contains(target) {
            Some(target.to_string())
        } else {
            by_base.get(&basename(target)).cloned()
        }
    };
    let active_base = basename(note);

    // Per-note chunk text (for context + unlinked scan).
    let mut text_of: BTreeMap<String, String> = BTreeMap::new();
    {
        let mem = state.memory.read().await;
        let mut entries = mem.stm.all_entries();
        entries.extend(mem.ltm.all_entries());
        for e in entries {
            if e.entry_type != crate::service::ingest::CHUNK_ENTRY_TYPE {
                continue;
            }
            if let (Some(p), Some(t)) = (
                e.data.get("source_path").and_then(|v| v.as_str()),
                e.data.get("text").and_then(|v| v.as_str()),
            ) {
                let buf = text_of.entry(p.to_string()).or_default();
                buf.push('\n');
                buf.push_str(t);
            }
        }
    }

    // Backlinks: sources linking to `note`.
    let mut seen = std::collections::BTreeSet::new();
    let mut backlinks: Vec<BacklinkRef> = Vec::new();
    let mut backlink_paths = std::collections::BTreeSet::new();
    for (src, target) in &links {
        if src == note || !note_set.contains(src.as_str()) {
            continue;
        }
        if resolve(target).as_deref() == Some(note) && seen.insert(src.clone()) {
            let context = text_of.get(src).and_then(|txt| {
                txt.lines()
                    .find(|l| {
                        l.contains("[[")
                            && l.to_lowercase().contains(&active_base.to_lowercase())
                    })
                    .map(|l| {
                        let t = l.trim();
                        if t.chars().count() > 200 {
                            let cut: String = t.chars().take(200).collect();
                            format!("{cut}…")
                        } else {
                            t.to_string()
                        }
                    })
            });
            let anchor = provenance_anchor_for(state, src).await;
            backlink_paths.insert(src.clone());
            backlinks.push(BacklinkRef {
                path: src.clone(),
                context,
                provenance_anchor: anchor,
            });
        }
    }
    backlinks.sort_by(|a, b| a.path.cmp(&b.path));

    // Outgoing: notes `note` links to.
    let mut outgoing: Vec<String> = links
        .iter()
        .filter(|(src, _)| src == note)
        .filter_map(|(_, target)| resolve(target))
        .filter(|p| p != note)
        .collect();
    outgoing.sort();
    outgoing.dedup();

    // Unlinked mentions: notes whose text names `active_base` but don't link it.
    let mut unlinked: Vec<String> = text_of
        .iter()
        .filter(|(p, _)| {
            p.as_str() != note
                && !backlink_paths.contains(p.as_str())
                && note_set.contains(p.as_str())
        })
        .filter(|(_, txt)| mentions_word(txt, &active_base))
        .map(|(p, _)| p.clone())
        .collect();
    unlinked.sort();
    unlinked.dedup();

    Backlinks {
        backlinks,
        outgoing,
        unlinked,
    }
}

#[cfg(test)]
mod tests {
    use crate::state::AppState;
    use aingle_graph::{NodeId, Predicate, Triple, Value};

    async fn graph_with(triples: &[(&str, &str, &str)]) -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let g = state.graph.write().await;
            for (s, p, o) in triples {
                g.insert(Triple::new(NodeId::named(*s), Predicate::named(*p), Value::literal(*o)))
                    .unwrap();
            }
        }
        state
    }

    #[tokio::test]
    async fn backlinks_outgoing_unlinked() {
        let state = graph_with(&[
            ("a.md", "aingle:source_hash", "h1"),
            ("b.md", "aingle:source_hash", "h2"),
            ("c.md", "aingle:source_hash", "h3"),
            ("target.md", "aingle:source_hash", "h4"),
            ("a.md", "links_to", "target"),  // a → target (backlink)
            ("target.md", "links_to", "b"),  // target → b (outgoing)
        ])
        .await;
        // c.md mentions "target" in text but does not link it (unlinked).
        {
            let mut mem = state.memory.write().await;
            let mut e = ineru::MemoryEntry::new(
                crate::service::ingest::CHUNK_ENTRY_TYPE,
                serde_json::json!({ "text": "See target for details.", "source_path": "c.md" }),
            );
            e.embedding = Some(ineru::Embedding::new(vec![0.0; 8]));
            mem.remember(e).unwrap();
        }

        let r = super::backlinks(&state, "target.md").await;
        assert!(r.backlinks.iter().any(|b| b.path == "a.md"), "a links to target");
        assert!(r.outgoing.contains(&"b.md".to_string()), "target links to b");
        assert!(r.unlinked.contains(&"c.md".to_string()), "c mentions target unlinked");
        assert!(!r.unlinked.contains(&"a.md".to_string()), "a is a backlink, not unlinked");
    }

    #[tokio::test]
    async fn context_truncation_is_char_safe() {
        let state = graph_with(&[
            ("t.md", "aingle:source_hash", "h1"),
            ("src.md", "aingle:source_hash", "h2"),
            ("src.md", "links_to", "t"),
        ])
        .await;
        {
            let mut mem = state.memory.write().await;
            // A line with accented chars whose byte length far exceeds 200 around the cut.
            let long = format!("[[t]] {}", "áéíóú ".repeat(80));
            let mut e = ineru::MemoryEntry::new(
                crate::service::ingest::CHUNK_ENTRY_TYPE,
                serde_json::json!({ "text": long, "source_path": "src.md" }),
            );
            e.embedding = Some(ineru::Embedding::new(vec![0.0; 8]));
            mem.remember(e).unwrap();
        }
        // Must not panic; context should be present and ≤ 201 chars (200 + ellipsis).
        let r = super::backlinks(&state, "t.md").await;
        let b = r.backlinks.iter().find(|b| b.path == "src.md").expect("backlink");
        let ctx = b.context.as_ref().expect("context");
        assert!(ctx.chars().count() <= 201);
    }
}
