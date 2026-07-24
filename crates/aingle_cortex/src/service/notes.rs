// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Signed note edits: a small, pure content-transform core plus an effectful
//! layer that writes a vault `.md` file and re-ingests it incrementally.
//!
//! The engine otherwise never touches vault files — the app owns the filesystem
//! and the engine owns the graph + signing. These tools bridge that gap for an
//! external AI: they write the file, then run the ordinary incremental ingest,
//! so the change flows through the same content-hash provenance + signed DAG
//! path as a human edit. There is no separate "note store"; the `.md` file is
//! the source of truth and the graph is derived from it.

use crate::error::{Error, Result};
use crate::state::AppState;

/// How [`apply_edit`] transforms a note's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditMode {
    /// Append `text` as a new trailing line.
    Append,
    /// Prepend `text` as a new leading line.
    Prepend,
    /// Replace the FIRST occurrence of `find` with `text` (no-op if absent).
    ReplaceText { find: String },
}

/// Outcome of a note edit, whether applied or previewed (`dry_run`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditResult {
    /// Vault-relative path of the edited note.
    pub target: String,
    /// blake3 content hash of the note before the edit.
    pub old_hash: String,
    /// blake3 content hash of the note after the edit.
    pub new_hash: String,
    /// Structural triples the edit would add (extractor diff).
    pub added: usize,
    /// Structural triples the edit would remove (extractor diff).
    pub removed: usize,
    /// Whether this was a preview (no write, no ingest).
    pub dry_run: bool,
    /// Whether the transform actually changed the content.
    pub changed: bool,
}

/// Outcome of staging a proposed note into the review inbox (`_inbox/`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProposeResult {
    /// Vault-relative path of the staged note (always under `_inbox/`).
    pub rel_path: String,
    /// Whether a new file was written. `false` when an `idempotency_key`
    /// matched an already-staged proposal, so nothing new was written.
    pub created: bool,
}

// ---------------------------------------------------------------------------
// Pure content transforms (unit-tested; no filesystem, no graph).
// ---------------------------------------------------------------------------

/// Apply an [`EditMode`] to `content`, returning the new content.
///
/// - `Append`: `text` becomes a new trailing line (a separating newline is
///   inserted when the content does not already end with one).
/// - `Prepend`: `text` becomes a new leading line.
/// - `ReplaceText { find }`: the first occurrence of `find` is replaced with
///   `text`; if `find` is absent (or empty) the content is returned unchanged.
pub fn apply_edit(content: &str, mode: EditMode, text: &str) -> String {
    match mode {
        EditMode::Append => {
            if content.is_empty() {
                return text.to_string();
            }
            let mut out = String::with_capacity(content.len() + text.len() + 1);
            out.push_str(content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(text);
            out
        }
        EditMode::Prepend => {
            if content.is_empty() {
                return text.to_string();
            }
            format!("{text}\n{content}")
        }
        EditMode::ReplaceText { find } => {
            if find.is_empty() {
                return content.to_string();
            }
            content.replacen(&find, text, 1)
        }
    }
}

/// Add `tag` to a note, frontmatter-aware and idempotent.
///
/// If the note already carries `tag` (in its frontmatter `tags:` list or as an
/// inline `#tag`, per the ingest extractor) this is a no-op. Otherwise, when a
/// frontmatter `tags:` list exists the tag is added there; else it is appended
/// as an inline `#tag` on a new trailing line.
pub fn add_tag(content: &str, tag: &str) -> String {
    let tag = tag.trim().trim_start_matches('#');
    if tag.is_empty() {
        return content.to_string();
    }
    // Idempotent: already present per the same semantics the graph records.
    if extracted_tags(content).contains(tag) {
        return content.to_string();
    }
    if let Some(updated) = frontmatter_tags_add(content, tag) {
        updated
    } else {
        apply_edit(content, EditMode::Append, &format!("#{tag}"))
    }
}

/// Remove `tag` from a note, frontmatter-aware and idempotent.
///
/// If a frontmatter `tags:` list contains the tag it is removed there; else any
/// inline `#tag` occurrences are stripped. Removing a tag the note does not
/// carry is a no-op.
pub fn remove_tag(content: &str, tag: &str) -> String {
    let tag = tag.trim().trim_start_matches('#');
    if tag.is_empty() {
        return content.to_string();
    }
    if let Some(updated) = frontmatter_tags_remove(content, tag) {
        return updated;
    }
    remove_inline_tag(content, tag)
}

/// The set of tags the ingest extractor would record for this content.
fn extracted_tags(content: &str) -> std::collections::HashSet<String> {
    aingle_ingest::extract("note.md", content)
        .triples
        .into_iter()
        .filter(|t| t.predicate == "tagged")
        .filter_map(|t| match t.object {
            aingle_ingest::ObjectValue::Text(s) => Some(s),
            aingle_ingest::ObjectValue::Node(_) => None,
        })
        .collect()
}

/// Index range `[open, close]` (line indices) of the leading `---` frontmatter
/// block, or `None` when the content has no frontmatter.
fn frontmatter_bounds(lines: &[&str]) -> Option<(usize, usize)> {
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return None;
    }
    let close_rel = lines[1..].iter().position(|l| l.trim_end() == "---")?;
    Some((0, close_rel + 1))
}

/// Parse a frontmatter `tags:` value (`[a, b]`, bare `a, b`, or single `a`).
fn parse_tag_list(val: &str) -> Vec<String> {
    let inner = val.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Join a list back into content, preserving the original trailing newline.
fn rejoin(lines: &[String], trailing_newline: bool) -> String {
    let mut s = lines.join("\n");
    if trailing_newline {
        s.push('\n');
    }
    s
}

/// Find the `tags:` line index within the frontmatter block, if present.
fn tags_line_index(lines: &[&str], close: usize) -> Option<usize> {
    (1..close).find(|&i| {
        lines[i]
            .split_once(':')
            .map(|(k, _)| k.trim() == "tags")
            .unwrap_or(false)
    })
}

/// Add `tag` to a frontmatter `tags:` list. Returns `None` when there is no
/// frontmatter or no `tags:` line (so the caller falls back to an inline tag).
fn frontmatter_tags_add(content: &str, tag: &str) -> Option<String> {
    let lines_ref: Vec<&str> = content.lines().collect();
    let (_, close) = frontmatter_bounds(&lines_ref)?;
    let idx = tags_line_index(&lines_ref, close)?;

    let mut lines: Vec<String> = lines_ref.iter().map(|s| s.to_string()).collect();
    let (_, val) = lines[idx].split_once(':').unwrap();
    let mut tags = parse_tag_list(val);
    if tags.iter().any(|t| t == tag) {
        return Some(content.to_string());
    }
    tags.push(tag.to_string());
    lines[idx] = format!("tags: [{}]", tags.join(", "));
    Some(rejoin(&lines, content.ends_with('\n')))
}

/// Remove `tag` from a frontmatter `tags:` list. Returns `Some` only when a
/// `tags:` line existed AND contained the tag (so it was actually removed);
/// otherwise `None`, letting the caller try inline removal.
fn frontmatter_tags_remove(content: &str, tag: &str) -> Option<String> {
    let lines_ref: Vec<&str> = content.lines().collect();
    let (_, close) = frontmatter_bounds(&lines_ref)?;
    let idx = tags_line_index(&lines_ref, close)?;

    let (_, val) = lines_ref[idx].split_once(':').unwrap();
    let tags = parse_tag_list(val);
    if !tags.iter().any(|t| t == tag) {
        return None;
    }
    let remaining: Vec<String> = tags.into_iter().filter(|t| t != tag).collect();

    let mut lines: Vec<String> = lines_ref.iter().map(|s| s.to_string()).collect();
    lines[idx] = format!("tags: [{}]", remaining.join(", "));
    Some(rejoin(&lines, content.ends_with('\n')))
}

/// Strip inline `#tag` occurrences (word-bounded so `#tag` never matches inside
/// a longer `#tagfoo`). Returns the content unchanged when the tag is absent.
fn remove_inline_tag(content: &str, tag: &str) -> String {
    // Groups: 1 = the boundary before the tag (`^`/whitespace, dropped), 2 = the
    // trailing boundary char (a non-tag char or line end, preserved).
    let pattern = format!(r"(?m)(^|\s)#{}([^A-Za-z0-9_/-]|$)", regex::escape(tag));
    match regex::Regex::new(&pattern) {
        Ok(re) => re.replace_all(content, "$2").into_owned(),
        Err(_) => content.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Effectful layer: resolve → read → transform → (write + re-ingest).
// ---------------------------------------------------------------------------

/// Edit a note by an [`EditMode`]. See [`apply_transform`] for the flow.
pub async fn edit_note(
    state: &AppState,
    note_rel: &str,
    mode: EditMode,
    text: &str,
    dry_run: bool,
) -> Result<EditResult> {
    apply_transform(state, note_rel, dry_run, move |old| {
        apply_edit(old, mode, text)
    })
    .await
}

/// Add a tag to a note (an [`edit_note`] specialisation using [`add_tag`]).
pub async fn tag_add(
    state: &AppState,
    note_rel: &str,
    tag: &str,
    dry_run: bool,
) -> Result<EditResult> {
    apply_transform(state, note_rel, dry_run, move |old| add_tag(old, tag)).await
}

/// Remove a tag from a note (an [`edit_note`] specialisation using [`remove_tag`]).
pub async fn tag_remove(
    state: &AppState,
    note_rel: &str,
    tag: &str,
    dry_run: bool,
) -> Result<EditResult> {
    apply_transform(state, note_rel, dry_run, move |old| remove_tag(old, tag)).await
}

/// Create a directory (and parents) inside the vault, after the same safety
/// checks as an edit. Idempotent: an existing directory is fine.
pub async fn create_folder(state: &AppState, rel: &str) -> Result<String> {
    let root = vault_root(state)?;
    let rel_norm = rel.replace('\\', "/");
    let pol = state.mcp_policy_snapshot();
    if pol.is_hidden(&rel_norm) {
        return Err(Error::Forbidden(format!(
            "folder '{rel_norm}' is inside an excluded folder"
        )));
    }
    let path = resolve_in_root(&root, &rel_norm)?;
    std::fs::create_dir_all(&path)
        .map_err(|e| Error::Internal(format!("cannot create folder '{rel_norm}': {e}")))?;
    Ok(rel_norm)
}

/// Stage a *proposed* note into the vault's review inbox (`_inbox/<name>.md`).
///
/// This is the entry point a Web Clipper / external AI uses to add content that
/// a human reviews before it becomes part of the vault. Unlike [`edit_note`],
/// this deliberately does **not** ingest: the ingest walk skips top-level
/// `_inbox/`, so a staged note stays unindexed and unsigned until a human
/// approves it and moves it out of `_inbox/` (that approval flow lives in the
/// app, not here).
///
/// - `name` is sanitized into a safe `_inbox/<name>.md` filename: path
///   separators, `..`, control chars, and reserved characters are stripped or
///   replaced; an empty result falls back to `source` then a default. The file
///   name is uniquified so an existing pending proposal is never clobbered.
/// - If `content` does not already begin with a `---` frontmatter block, one is
///   prepended (`source`, `status: pending`, optional `clipped`, and `tags`),
///   kept FLAT-scalar so the ordinary ingest can extract it later. Values are
///   sanitized so they cannot break a flat `key: value` line.
/// - When `idempotency_key` is set, a repeated call whose key already staged a
///   (still-present) note returns that note's path with `created: false` instead
///   of writing a duplicate (MV3 service workers can double-send).
pub async fn propose_note(
    state: &AppState,
    name: &str,
    content: &str,
    source: Option<&str>,
    clipped: Option<&str>,
    tags: &[String],
    idempotency_key: Option<&str>,
) -> Result<ProposeResult> {
    let root = vault_root(state)?;
    let inbox = root.join("_inbox");

    // Idempotency: a prior call with this key that produced a still-present note
    // returns that note rather than staging a duplicate.
    if let Some(key) = idempotency_key {
        if let Some(existing) = lookup_idempotent(&root, &inbox, key) {
            return Ok(ProposeResult {
                rel_path: existing,
                created: false,
            });
        }
    }

    let stem = safe_stem(name, source);
    let (final_path, file_name) = unique_inbox_path(&inbox, &stem);
    let rel = format!("_inbox/{file_name}");

    // Defense in depth: the resolved staging path must stay within the vault.
    // (`_inbox/` is the intended target and is NOT rejected via `is_hidden`.)
    let _ = resolve_in_root(&root, &rel)?;

    // Wrap with flat-scalar frontmatter unless the caller already built one.
    let body = if has_leading_frontmatter(content) {
        content.to_string()
    } else {
        wrap_frontmatter(content, source, clipped, tags)
    };

    std::fs::create_dir_all(&inbox)
        .map_err(|e| Error::Internal(format!("cannot create _inbox staging folder: {e}")))?;
    std::fs::write(&final_path, &body)
        .map_err(|e| Error::Internal(format!("cannot stage proposed note '{rel}': {e}")))?;

    // Record the idempotency marker only after a successful write (best-effort:
    // a failed marker write must not undo a successfully staged note).
    if let Some(key) = idempotency_key {
        record_idempotent(&inbox, key, &rel);
    }

    Ok(ProposeResult {
        rel_path: rel,
        created: true,
    })
}

/// True when `content`'s first line is a bare `---` (a leading frontmatter
/// fence), mirroring the ingest frontmatter detector.
fn has_leading_frontmatter(content: &str) -> bool {
    content.lines().next().map(|l| l.trim_end()) == Some("---")
}

/// Sanitize `content`-external text into a flat `key: value` scalar: control
/// characters (including newlines) become spaces and the result is trimmed, so
/// it can never break the single-line frontmatter it is written into.
fn flat_value(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Sanitize a tag into a flat list element: strip control chars and the list
/// delimiters `,[]` (which would break `tags: [..]`), drop a leading `#`.
fn flat_tag(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            c if c.is_control() => ' ',
            ',' | '[' | ']' => ' ',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_string()
}

/// Build a flat-scalar frontmatter block and prepend it to `content`.
fn wrap_frontmatter(
    content: &str,
    source: Option<&str>,
    clipped: Option<&str>,
    tags: &[String],
) -> String {
    let mut fm = String::from("---\n");
    if let Some(src) = source {
        let v = flat_value(src);
        if !v.is_empty() {
            fm.push_str(&format!("source: {v}\n"));
        }
    }
    fm.push_str("status: pending\n");
    if let Some(c) = clipped {
        let v = flat_value(c);
        if !v.is_empty() {
            fm.push_str(&format!("clipped: {v}\n"));
        }
    }
    let clean_tags: Vec<String> = tags
        .iter()
        .map(|t| flat_tag(t))
        .filter(|t| !t.is_empty())
        .collect();
    if !clean_tags.is_empty() {
        fm.push_str(&format!("tags: [{}]\n", clean_tags.join(", ")));
    }
    fm.push_str("---\n\n");
    fm.push_str(content);
    fm
}

/// Sanitize a proposed `name` into a safe file stem (no extension). Falls back
/// to `source`, then a fixed default, when `name` sanitizes to nothing.
fn safe_stem(name: &str, source: Option<&str>) -> String {
    if let Some(s) = clean_stem(name) {
        return s;
    }
    if let Some(src) = source {
        if let Some(s) = clean_stem(src) {
            return s;
        }
    }
    "proposed-note".to_string()
}

/// Clean a raw string into a safe filename stem, or `None` if nothing usable
/// remains. Reserved/hostile characters are replaced, control chars dropped, a
/// trailing `.md` removed, leading/trailing dots stripped (so `.`/`..` collapse
/// to empty), and length capped.
fn clean_stem(raw: &str) -> Option<String> {
    let mut out = String::new();
    for c in raw.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => out.push('-'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    let trimmed = out.trim();
    let trimmed = trimmed.strip_suffix(".md").unwrap_or(trimmed);
    let cleaned = trimmed.trim().trim_matches('.').trim();
    let capped: String = cleaned.chars().take(120).collect();
    let capped = capped.trim().trim_matches('.').trim().to_string();
    if capped.is_empty() {
        None
    } else {
        Some(capped)
    }
}

/// Choose a non-clobbering `<stem>.md` (then `<stem>-2.md`, ...) inside `inbox`.
fn unique_inbox_path(inbox: &std::path::Path, stem: &str) -> (std::path::PathBuf, String) {
    let first = format!("{stem}.md");
    if !inbox.join(&first).exists() {
        return (inbox.join(&first), first);
    }
    for n in 2..100_000 {
        let name = format!("{stem}-{n}.md");
        if !inbox.join(&name).exists() {
            return (inbox.join(&name), name);
        }
    }
    // Practically unreachable fallback: a content-derived unique suffix.
    let name = format!("{stem}-{}.md", blake3::hash(stem.as_bytes()).to_hex());
    (inbox.join(&name), name)
}

/// Filesystem path of the idempotency marker for `key`, kept under a dot-folder
/// inside `_inbox/` (itself unindexed) so it never leaks into the graph.
fn idempotency_marker(inbox: &std::path::Path, key: &str) -> std::path::PathBuf {
    let hash = blake3::hash(key.as_bytes()).to_hex().to_string();
    inbox.join(".aingle").join("idempotency").join(hash)
}

/// Return the vault-relative path a prior call with `key` staged, but only if
/// that note file still exists (a removed note lets a fresh proposal proceed).
fn lookup_idempotent(root: &std::path::Path, inbox: &std::path::Path, key: &str) -> Option<String> {
    let marker = idempotency_marker(inbox, key);
    let rel = std::fs::read_to_string(&marker).ok()?;
    let rel = rel.trim().to_string();
    if rel.is_empty() {
        return None;
    }
    if root.join(&rel).exists() {
        Some(rel)
    } else {
        None
    }
}

/// Best-effort record of `key -> rel` so a retry can be deduplicated.
fn record_idempotent(inbox: &std::path::Path, key: &str, rel: &str) {
    let marker = idempotency_marker(inbox, key);
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker, rel);
}

/// Snapshot the configured vault root, or a `BadRequest` if the host never set it.
fn vault_root(state: &AppState) -> Result<std::path::PathBuf> {
    state.vault_root_snapshot().ok_or_else(|| {
        Error::BadRequest("vault root is not configured; the host must call set_vault_root".into())
    })
}

/// The shared edit flow: confirm a vault root, reject folder-excluded or
/// root-escaping paths, read the note, transform it, and — unless `dry_run` —
/// write it back and re-ingest incrementally so the change is signed into the
/// DAG. Always reports the extractor triple diff (`added`/`removed`).
async fn apply_transform(
    state: &AppState,
    note_rel: &str,
    dry_run: bool,
    transform: impl FnOnce(&str) -> String,
) -> Result<EditResult> {
    let root = vault_root(state)?;
    let rel_norm = note_rel.replace('\\', "/");

    // Folder-exclusion: never edit a note the active MCP policy hides.
    let pol = state.mcp_policy_snapshot();
    if pol.is_hidden(&rel_norm) {
        return Err(Error::Forbidden(format!(
            "note '{rel_norm}' is inside an excluded folder"
        )));
    }

    let path = resolve_in_root(&root, &rel_norm)?;

    // The note must already exist; the app creates files, these tools edit them.
    let old = std::fs::read_to_string(&path)
        .map_err(|_| Error::NotFound(format!("note '{rel_norm}' not found in the vault")))?;

    // Defense in depth against symlink escapes: canonicalize and re-check prefix.
    {
        let root_canon = std::fs::canonicalize(&root)
            .map_err(|e| Error::Internal(format!("cannot canonicalize vault root: {e}")))?;
        let path_canon = std::fs::canonicalize(&path)
            .map_err(|_| Error::NotFound(format!("note '{rel_norm}' not found in the vault")))?;
        if !path_canon.starts_with(&root_canon) {
            return Err(Error::Forbidden(
                "resolved note path escapes the vault root".into(),
            ));
        }
    }

    let new = transform(&old);
    let old_hash = blake3::hash(old.as_bytes()).to_hex().to_string();
    let new_hash = blake3::hash(new.as_bytes()).to_hex().to_string();
    let (added, removed) = diff_triples(&rel_norm, &old, &new);
    let changed = old != new;

    if !dry_run && changed {
        std::fs::write(&path, &new)
            .map_err(|e| Error::Internal(format!("cannot write note '{rel_norm}': {e}")))?;
        // Incremental ingest: the changed file is purged + re-inserted, each
        // insert signing a DAG action carrying the new content hash.
        let root_str = root.to_string_lossy().to_string();
        crate::service::ingest::ingest_path(state, &root_str, None).await?;
    }

    Ok(EditResult {
        target: rel_norm,
        old_hash,
        new_hash,
        added,
        removed,
        dry_run,
        changed,
    })
}

/// Resolve `rel_norm` against `root`, rejecting absolute paths and `..` escapes
/// up front (before any filesystem access).
fn resolve_in_root(root: &std::path::Path, rel_norm: &str) -> Result<std::path::PathBuf> {
    use std::path::Component;
    if rel_norm.trim().is_empty() {
        return Err(Error::BadRequest("note path must not be empty".into()));
    }
    let relp = std::path::Path::new(rel_norm);
    for comp in relp.components() {
        match comp {
            Component::ParentDir => {
                return Err(Error::Forbidden(
                    "note path must not escape the vault root ('..')".into(),
                ));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(Error::BadRequest("note path must be vault-relative".into()));
            }
            _ => {}
        }
    }
    Ok(root.join(relp))
}

/// Count structural triples added/removed between two versions of a note, by
/// running the ingest extractor over each and diffing on (subject, predicate,
/// object) identity — provenance (line numbers, content hash) is ignored.
fn diff_triples(rel: &str, old: &str, new: &str) -> (usize, usize) {
    use aingle_ingest::{ObjectValue, ProvenancedTriple};
    fn key(t: &ProvenancedTriple) -> (String, String, String) {
        let obj = match &t.object {
            ObjectValue::Node(n) => format!("node:{n}"),
            ObjectValue::Text(s) => format!("text:{s}"),
        };
        (t.subject.clone(), t.predicate.clone(), obj)
    }
    let old_keys: std::collections::HashSet<(String, String, String)> =
        aingle_ingest::extract(rel, old)
            .triples
            .iter()
            .map(key)
            .collect();
    let new_keys: std::collections::HashSet<(String, String, String)> =
        aingle_ingest::extract(rel, new)
            .triples
            .iter()
            .map(key)
            .collect();
    let added = new_keys.difference(&old_keys).count();
    let removed = old_keys.difference(&new_keys).count();
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Pure transforms -------------------------------------------------

    #[test]
    fn append_adds_trailing_line() {
        assert_eq!(
            apply_edit("# A\n\nbody", EditMode::Append, "new line"),
            "# A\n\nbody\nnew line"
        );
        // A content that already ends with a newline does not get a blank line.
        assert_eq!(apply_edit("body\n", EditMode::Append, "x"), "body\nx");
        // Empty content becomes just the text.
        assert_eq!(apply_edit("", EditMode::Append, "x"), "x");
    }

    #[test]
    fn prepend_adds_leading_line() {
        assert_eq!(
            apply_edit("body\n", EditMode::Prepend, "top"),
            "top\nbody\n"
        );
        assert_eq!(apply_edit("", EditMode::Prepend, "top"), "top");
    }

    #[test]
    fn replace_first_occurrence_only() {
        assert_eq!(
            apply_edit("a x b x c", EditMode::ReplaceText { find: "x".into() }, "Y"),
            "a Y b x c"
        );
    }

    #[test]
    fn replace_absent_is_noop() {
        let c = "nothing to see";
        assert_eq!(
            apply_edit(c, EditMode::ReplaceText { find: "zzz".into() }, "Y"),
            c
        );
        // An empty find is a no-op, not an insert.
        assert_eq!(
            apply_edit(c, EditMode::ReplaceText { find: "".into() }, "Y"),
            c
        );
    }

    #[test]
    fn add_tag_into_frontmatter_list() {
        let md = "---\ntitle: x\ntags: [alpha, beta]\n---\n\nbody\n";
        let out = add_tag(md, "gamma");
        assert!(out.contains("tags: [alpha, beta, gamma]"), "got: {out}");
        // Adding an existing tag is a no-op.
        assert_eq!(add_tag(&out, "gamma"), out);
        assert_eq!(add_tag(md, "alpha"), md);
    }

    #[test]
    fn add_tag_inline_when_no_frontmatter_list() {
        let md = "# Note\n\nbody\n";
        let out = add_tag(md, "idea");
        assert!(out.contains("#idea"), "got: {out}");
        // The extractor sees it as a tag => idempotent second add.
        assert_eq!(add_tag(&out, "idea"), out);
        // Frontmatter without a tags line also falls back to inline.
        let fm = "---\ntitle: x\n---\n\nbody\n";
        let out2 = add_tag(fm, "idea");
        assert!(out2.contains("#idea"), "got: {out2}");
    }

    #[test]
    fn remove_tag_from_frontmatter_and_inline() {
        let md = "---\ntags: [alpha, beta]\n---\n\nbody\n";
        let out = remove_tag(md, "alpha");
        assert!(out.contains("tags: [beta]"), "got: {out}");
        assert!(!super::extracted_tags(&out).contains("alpha"));

        let inline = "# Note\n\ntagged with #idea here\n";
        let out2 = remove_tag(inline, "idea");
        assert!(!out2.contains("#idea"), "got: {out2}");
        assert!(!super::extracted_tags(&out2).contains("idea"));
    }

    #[test]
    fn remove_absent_tag_is_noop() {
        let md = "---\ntags: [alpha]\n---\n\nno inline here\n";
        assert_eq!(remove_tag(md, "zzz"), md);
        let plain = "# Note\n\njust text\n";
        assert_eq!(remove_tag(plain, "idea"), plain);
    }

    #[test]
    fn remove_inline_tag_respects_word_boundary() {
        // `#idea` must not be stripped out of `#ideabank`.
        let md = "see #ideabank not the same\n";
        assert_eq!(remove_tag(md, "idea"), md);
    }

    // ---- Effectful edits over a real vault -------------------------------

    async fn enabled_state() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        state
    }

    async fn action_count(state: &AppState) -> usize {
        let g = state.graph.read().await;
        g.dag_store().unwrap().action_count()
    }

    /// A ready state whose vault root is set and which has ingested `note.md`.
    async fn vault_state(body: &str) -> (AppState, tempfile::TempDir) {
        let state = enabled_state().await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), body).unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        state.set_vault_root(dir.path().to_path_buf());
        (state, dir)
    }

    #[tokio::test]
    async fn append_changes_file_and_signs_dag() {
        let (state, dir) = vault_state("# Note\n\nbody\n").await;
        let before = action_count(&state).await;

        let res = edit_note(&state, "note.md", EditMode::Append, "See [[other]].", false)
            .await
            .unwrap();
        assert!(res.changed && !res.dry_run);

        // File on disk carries the appended line.
        let on_disk = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
        assert!(on_disk.contains("See [[other]]."), "got: {on_disk}");

        // The edit is signed: new DAG actions exist, and the note's subject has
        // a visible provenance history.
        assert!(action_count(&state).await > before);
        let g = state.graph.read().await;
        let hist = g.dag_history_by_subject("note.md", 20).unwrap();
        assert!(!hist.is_empty(), "note.md must have signed history");
    }

    #[tokio::test]
    async fn prepend_and_replace_change_the_file() {
        let (state, dir) = vault_state("middle\n").await;
        edit_note(&state, "note.md", EditMode::Prepend, "top", false)
            .await
            .unwrap();
        let after_prepend = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
        assert!(after_prepend.starts_with("top\n"), "got: {after_prepend}");

        edit_note(
            &state,
            "note.md",
            EditMode::ReplaceText {
                find: "middle".into(),
            },
            "MIDDLE",
            false,
        )
        .await
        .unwrap();
        let after_replace = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
        assert!(after_replace.contains("MIDDLE"), "got: {after_replace}");
    }

    #[tokio::test]
    async fn dry_run_leaves_file_and_dag_untouched_but_reports_diff() {
        let (state, dir) = vault_state("# Note\n\nbody\n").await;
        let before_actions = action_count(&state).await;
        let before_bytes = std::fs::read(dir.path().join("note.md")).unwrap();

        // A tag add would create a new `tagged` triple => non-empty diff.
        let res = tag_add(&state, "note.md", "fresh", true).await.unwrap();
        assert!(res.dry_run);
        assert!(res.added >= 1, "dry-run diff must be non-empty: {res:?}");
        assert_ne!(res.old_hash, res.new_hash);

        // Nothing was written and no DAG action was recorded.
        let after_bytes = std::fs::read(dir.path().join("note.md")).unwrap();
        assert_eq!(before_bytes, after_bytes, "dry_run must not write the file");
        assert_eq!(
            before_actions,
            action_count(&state).await,
            "dry_run must not sign a DAG action"
        );
    }

    #[tokio::test]
    async fn path_escaping_root_is_rejected() {
        let (state, _dir) = vault_state("# Note\n\nbody\n").await;
        let err = edit_note(&state, "../outside.md", EditMode::Append, "x", false)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Forbidden(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn excluded_folder_is_rejected() {
        let state = enabled_state().await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Private")).unwrap();
        std::fs::write(dir.path().join("Private").join("secret.md"), "# S\n\nx\n").unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        state.set_vault_root(dir.path().to_path_buf());
        state.set_mcp_policy(crate::mcp::policy::McpPolicy {
            excluded_folders: vec!["Private".into()],
            permission: crate::mcp::policy::Permission::ReadWrite,
            require_grounding: false,
        });

        let err = edit_note(&state, "Private/secret.md", EditMode::Append, "x", false)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Forbidden(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn edit_without_vault_root_errors() {
        let state = enabled_state().await; // vault_root left unset
        let err = edit_note(&state, "note.md", EditMode::Append, "x", false)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::BadRequest(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn tag_add_surfaces_in_graph_and_is_idempotent() {
        use aingle_graph::{Predicate, TriplePattern};

        let (state, _dir) = vault_state("# Note\n\nbody\n").await;
        tag_add(&state, "note.md", "roadmap", false).await.unwrap();

        let tagged_now = {
            let g = state.graph.read().await;
            g.find(TriplePattern::any().with_predicate(Predicate::named("tagged")))
                .unwrap()
                .into_iter()
                .filter_map(|t| crate::service::triple_util::obj_string(&t))
                .collect::<Vec<_>>()
        };
        assert!(
            tagged_now.iter().any(|s| s == "roadmap"),
            "tag must appear as a `tagged` triple: {tagged_now:?}"
        );

        // A second identical tag add is a no-op: content unchanged, so no write.
        let res = tag_add(&state, "note.md", "roadmap", false).await.unwrap();
        assert!(
            !res.changed,
            "re-adding an existing tag must be a no-op: {res:?}"
        );
    }

    // ---- Review-inbox staging (propose_note) -----------------------------

    /// A ready state whose vault root is set to a fresh (empty) temp dir.
    async fn inbox_state() -> (AppState, tempfile::TempDir) {
        let state = enabled_state().await;
        let dir = tempfile::tempdir().unwrap();
        state.set_vault_root(dir.path().to_path_buf());
        (state, dir)
    }

    #[tokio::test]
    async fn propose_stages_note_into_inbox_with_frontmatter() {
        let (state, dir) = inbox_state().await;
        let res = propose_note(
            &state,
            "My Clipped Idea",
            "Some body text from the web.",
            Some("https://example.com/article"),
            Some("2026-07-24T10:00:00Z"),
            &["research".to_string(), "web".to_string()],
            None,
        )
        .await
        .unwrap();

        assert!(res.created, "a fresh proposal is created: {res:?}");
        assert!(
            res.rel_path.starts_with("_inbox/"),
            "must stage in _inbox: {res:?}"
        );
        assert!(res.rel_path.ends_with(".md"), "must be a .md file: {res:?}");

        let on_disk = std::fs::read_to_string(dir.path().join(&res.rel_path)).unwrap();
        assert!(
            on_disk.starts_with("---\n"),
            "must wrap frontmatter: {on_disk}"
        );
        assert!(
            on_disk.contains("source: https://example.com/article"),
            "must record source: {on_disk}"
        );
        assert!(
            on_disk.contains("status: pending"),
            "must be pending: {on_disk}"
        );
        assert!(
            on_disk.contains("clipped: 2026-07-24T10:00:00Z"),
            "{on_disk}"
        );
        assert!(
            on_disk.contains("tags: [research, web]"),
            "must record tags: {on_disk}"
        );
        assert!(
            on_disk.contains("Some body text from the web."),
            "body kept: {on_disk}"
        );
    }

    #[tokio::test]
    async fn propose_sanitizes_traversing_name() {
        let (state, dir) = inbox_state().await;
        // A name with separators / traversal must not escape `_inbox/`.
        let res = propose_note(
            &state,
            "../../etc/passwd",
            "body",
            Some("clipper"),
            None,
            &[],
            None,
        )
        .await
        .unwrap();
        assert!(
            res.rel_path.starts_with("_inbox/"),
            "sanitized name must stay in _inbox: {res:?}"
        );
        // Exactly one path component under `_inbox/` (no nested dirs created).
        let after = res.rel_path.strip_prefix("_inbox/").unwrap();
        assert!(!after.contains('/'), "no separators survive: {res:?}");
        assert!(
            dir.path().join(&res.rel_path).exists(),
            "file landed on disk"
        );
        // Nothing escaped to a sibling of the vault root.
        assert!(!dir.path().parent().unwrap().join("passwd").exists());
    }

    #[tokio::test]
    async fn propose_keeps_existing_frontmatter_intact() {
        let (state, dir) = inbox_state().await;
        let caller_built = "---\ntitle: Prebuilt\nstatus: pending\ntags: [a, b]\n---\n\nBody.\n";
        let res = propose_note(
            &state,
            "prebuilt",
            caller_built,
            Some("ignored"),
            None,
            &[],
            None,
        )
        .await
        .unwrap();
        let on_disk = std::fs::read_to_string(dir.path().join(&res.rel_path)).unwrap();
        assert_eq!(
            on_disk, caller_built,
            "existing frontmatter must be left intact"
        );
        // No injected second `source:`/frontmatter fence.
        assert_eq!(
            on_disk.matches("---").count(),
            2,
            "no extra fence: {on_disk}"
        );
    }

    #[tokio::test]
    async fn staged_note_is_not_ingested() {
        let (state, dir) = inbox_state().await;
        let before = action_count(&state).await;
        let res = propose_note(
            &state,
            "draft",
            "Body about widgets.",
            Some("clipper"),
            None,
            &[],
            None,
        )
        .await
        .unwrap();
        assert!(res.rel_path.starts_with("_inbox/"));
        // Staging never signs a DAG action.
        assert_eq!(
            before,
            action_count(&state).await,
            "staging must not sign the DAG"
        );

        // Even a full ingest of the vault root leaves the staged note unindexed
        // (the walk skips top-level `_inbox/`).
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        let sources = crate::service::ingest::list_sources(&state).await.unwrap();
        assert!(
            !sources
                .iter()
                .any(|s| s.path.replace('\\', "/").starts_with("_inbox/")),
            "staged note must not be ingested: {sources:?}"
        );
    }

    #[tokio::test]
    async fn propose_idempotency_key_dedups() {
        let (state, dir) = inbox_state().await;
        let key = "clip-abc-123";
        let first = propose_note(
            &state,
            "dup",
            "same body",
            Some("clipper"),
            None,
            &[],
            Some(key),
        )
        .await
        .unwrap();
        assert!(first.created, "first call creates: {first:?}");

        let second = propose_note(
            &state,
            "dup",
            "same body",
            Some("clipper"),
            None,
            &[],
            Some(key),
        )
        .await
        .unwrap();
        assert!(
            !second.created,
            "second identical call must not create: {second:?}"
        );
        assert_eq!(first.rel_path, second.rel_path, "must return the same path");

        // Only one staged note (plus the marker dot-folder) exists.
        let md_count = std::fs::read_dir(dir.path().join("_inbox"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
            .count();
        assert_eq!(md_count, 1, "idempotent retry must not stage a duplicate");
    }

    #[tokio::test]
    async fn propose_without_vault_root_errors() {
        let state = enabled_state().await; // vault_root left unset
        let err = propose_note(&state, "x", "body", None, None, &[], None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::BadRequest(_)), "got: {err:?}");
    }
}
