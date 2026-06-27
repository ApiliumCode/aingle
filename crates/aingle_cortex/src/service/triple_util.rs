// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Shared triple-object extraction and wikilink-resolution helpers.
//!
//! # Why a shared module?
//! `obj_string` was previously duplicated verbatim in `backlinks`, `context`,
//! and `vault_map`. A copy-paste drift on exactly this helper caused a real bug
//! (node-valued `links_to` triples were silently dropped). This module is the
//! single source of truth; every consumer must import from here.

/// Return the object of a triple as a plain `String`, handling both literal
/// strings (`Value::Str`) and graph nodes (`Value::Node`). Node IDs are stored
/// with `<…>` angle-bracket wrappers; this strips them so the result matches
/// the bare names used everywhere else in the service layer.
pub(crate) fn obj_string(t: &aingle_graph::Triple) -> Option<String> {
    if let Some(s) = t.object_string() {
        Some(s.to_string())
    } else {
        t.object_node()
            .map(|n| n.to_string().trim_start_matches('<').trim_end_matches('>').to_string())
    }
}

/// Basename without directory or extension (for wikilink resolution).
fn basename(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file).to_string()
}

/// Strip the extension from the last path segment only. Input must already be
/// slash-normalized (forward slashes). Returns the path-without-ext.
/// "b/note.md" → "b/note", "b/note" → "b/note", "note.md" → "note".
fn path_without_ext(path: &str) -> String {
    if let Some(idx) = path.rfind('/') {
        let dir = &path[..=idx]; // includes the trailing '/'
        let file = &path[idx + 1..];
        let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file);
        format!("{dir}{stem}")
    } else {
        path.rsplit_once('.').map(|(s, _)| s).unwrap_or(path).to_string()
    }
}

/// Resolve a wikilink `target` to a full note path. Order mirrors the editor's
/// `wikilinks.ts`:
/// 1. Exact path match (after normalizing `\\`→`/`).
/// 2. When `target` is path-qualified (contains `/`), find a note whose
///    slash-normalized path-without-extension equals the target's.
///    This handles `[[dir/note]]` → `dir/note.md` without collapsing to the
///    alphabetically-first note that shares a bare basename.
/// 3. Basename fallback via `by_base`.
pub(crate) fn resolve_link_target(
    target: &str,
    note_set: &std::collections::BTreeSet<&str>,
    by_base: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    // Normalize backslash to forward slash for consistent matching.
    let t_norm = target.replace('\\', "/");
    let t_ref: &str = &t_norm;

    // (1) Exact path match.
    if note_set.contains(t_ref) {
        return Some(t_norm);
    }

    // (2) Path-qualified: find a note whose path-without-ext (slash-normalized)
    //     equals the target's path-without-ext.
    if t_norm.contains('/') {
        let t_ne = path_without_ext(t_ref);
        for &p in note_set.iter() {
            let p_norm = p.replace('\\', "/");
            if path_without_ext(&p_norm) == t_ne {
                return Some(p.to_string());
            }
        }
    }

    // (3) Basename fallback.
    by_base.get(&basename(t_ref)).cloned()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::resolve_link_target;

    #[test]
    fn exact_path_match() {
        // "b/note.md" exists verbatim — must return it, not "a/note.md".
        let notes = vec!["a/note.md".to_string(), "b/note.md".to_string()];
        let note_set: BTreeSet<&str> = notes.iter().map(|s| s.as_str()).collect();
        let mut by_base: BTreeMap<String, String> = BTreeMap::new();
        by_base.insert("note".to_string(), "a/note.md".to_string());

        assert_eq!(
            resolve_link_target("b/note.md", &note_set, &by_base).as_deref(),
            Some("b/note.md")
        );
    }

    #[test]
    fn path_qualified_resolves_correct_note_not_alphabetical_first() {
        // "[[b/note]]" (no extension) must resolve to "b/note.md", NOT "a/note.md".
        // by_base["note"] = "a/note.md" (first alphabetically — the collision
        // that previously caused the bug).
        let notes = vec!["a/note.md".to_string(), "b/note.md".to_string()];
        let note_set: BTreeSet<&str> = notes.iter().map(|s| s.as_str()).collect();
        let mut by_base: BTreeMap<String, String> = BTreeMap::new();
        by_base.insert("note".to_string(), "a/note.md".to_string());

        assert_eq!(
            resolve_link_target("b/note", &note_set, &by_base).as_deref(),
            Some("b/note.md"),
            "path-qualified target must not collapse to the alphabetically-first basename match"
        );
    }

    #[test]
    fn bare_basename_unique_fallback() {
        // No path component → falls through to by_base.
        let notes = vec!["dir/note.md".to_string()];
        let note_set: BTreeSet<&str> = notes.iter().map(|s| s.as_str()).collect();
        let mut by_base: BTreeMap<String, String> = BTreeMap::new();
        by_base.insert("note".to_string(), "dir/note.md".to_string());

        assert_eq!(
            resolve_link_target("note", &note_set, &by_base).as_deref(),
            Some("dir/note.md")
        );
    }
}
