// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Incremental vault ingestion: walk a directory, extract triples and chunks,
//! write them into the graph (with signed DAG provenance) and Ineru memory,
//! and maintain a per-file source-hash registry triple for idempotent re-runs.

use crate::error::{Error, Result};
use crate::rest::ValueDto;
use crate::service::triples::{delete_triple, insert_triple_inner};
use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, TriplePattern};
use aingle_ingest::{extract, ObjectValue};
use ineru::{MemoryEntry, MemoryId, MemoryMetadata};

// Bring the graph error type into scope for duplicate-matching in ingest logic.
use aingle_graph::Error as GraphError;

/// The predicate used to anchor the per-file content-hash registry triple.
pub const PRED_SOURCE_HASH: &str = "aingle:source_hash";

/// Ineru `entry_type` used for ingested text chunks. Grounding filters on this.
pub const CHUNK_ENTRY_TYPE: &str = "doc_chunk";

/// File extensions ingested as text (source, docs, config). Broad on purpose: a
/// real project is far more than a handful of languages, and a narrow allowlist
/// silently drops most of a codebase (e.g. a Swift/Kotlin/Go project). Binary
/// files are still skipped when reading as UTF-8 fails, `.gitignore` prunes build
/// output, the noise denylist drops generated/lock files, and byte-bounded
/// chunking keeps any single large file safe.
const INGEST_EXTENSIONS: &[&str] = &[
    // prose / docs
    "md",
    "markdown",
    "mdx",
    "txt",
    "rst",
    "org",
    "adoc", //
    // config / structure
    "toml",
    "json",
    "jsonc",
    "yaml",
    "yml",
    "xml",
    "ini",
    "cfg",
    "conf",
    "gradle",
    "properties",
    // web markup / styling
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    // source
    "rs",
    "ts",
    "tsx",
    "js",
    "jsx",
    "mjs",
    "cjs",
    "py",
    "go",
    "swift",
    "kt",
    "kts",
    "java",
    "c",
    "h",
    "cc",
    "cpp",
    "cxx",
    "hpp",
    "hh",
    "cs",
    "rb",
    "php",
    "scala",
    "sh",
    "bash",
    "zsh",
    "lua",
    "dart",
    "r",
    "m",
    "mm",
    "vue",
    "svelte",
    "sql",
];

/// Extensionless files ingested by well-known name (matched lowercased).
const INGEST_FILENAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    "readme",
    "license",
    "cmakelists.txt",
    "gemfile",
    "rakefile",
    "procfile",
    "vagrantfile",
];

/// Returns whether `path` should be ingested as a text file: a broad extension
/// allowlist plus a few extensionless names, minus a denylist of generated,
/// minified, or lock files that carry no semantic signal even when they slip
/// past `.gitignore`.
fn is_ingestable_file(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();

    // Noise denylist: minified bundles, source maps, lockfiles, generated files.
    if lower.ends_with(".min.js")
        || lower.ends_with(".min.css")
        || lower.ends_with(".map")
        || lower.ends_with(".lock")
        || lower.contains(".generated.")
        || matches!(lower.as_str(), "package-lock.json" | "pnpm-lock.yaml")
    {
        return false;
    }

    // Known extensionless files (Dockerfile, Makefile, …).
    if INGEST_FILENAMES.contains(&lower.as_str()) {
        return true;
    }

    // Extension allowlist.
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => INGEST_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// One ingested source file and its content hash at ingest time.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceRecord {
    pub path: String,
    pub content_hash: String,
}

/// Summary statistics returned by `ingest_path`.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct IngestReport {
    /// Total number of files encountered during the walk.
    pub files_seen: usize,
    /// Files that were newly ingested (hash changed or first time).
    pub files_ingested: usize,
    /// Files skipped because their content hash matched the registry.
    pub files_skipped: usize,
    /// Files whose facts were RETRACTED because the source disappeared from disk
    /// (or became excluded). Only ever non-zero for the path-targeted
    /// [`ingest_paths`]: the full walk cannot tell "gone" from "never seen".
    pub files_removed: usize,
    /// Total triples written (structural + registry).
    pub triples_written: usize,
    /// Total text chunks written to Ineru memory.
    pub chunks_written: usize,
    /// The files ingested in this run, with their content hashes.
    pub sources: Vec<SourceRecord>,
}

/// Walk `root_path`, extract structural triples and text chunks from each file,
/// write them to the graph (with DAG provenance) and Ineru memory, and maintain
/// a per-file source-hash registry triple for incremental skip on unchanged files.
///
/// `namespace` is forwarded to the audit log (use `None` for internal/background calls).
pub async fn ingest_path(
    state: &AppState,
    root_path: &str,
    namespace: Option<String>,
) -> Result<IngestReport> {
    ingest_path_with_progress(state, root_path, namespace, None).await
}

/// Top-level vault directories the ingest walk never descends into.
///
/// - `_inbox` — the review staging area. Notes an agent PROPOSES land here and
///   must NOT be indexed until a human approves them (see the review-inbox
///   feature).
/// - `.trash` — recoverable deletes keep their original relative path under
///   `.trash/`. Ingesting them would resurrect a deleted note's facts as brand
///   new triples under a `.trash/…` subject, so deleting a note would ADD to the
///   graph instead of retracting from it.
pub const SKIPPED_TOP_LEVEL_DIRS: &[&str] = &["_inbox", ".trash"];

/// A progress reporter invoked once per candidate file as `(processed, total)`.
/// `Sync` so it can be held across the ingest's `.await` points.
pub type IngestProgress<'a> = &'a (dyn Fn(usize, usize) + Sync);

/// Like [`ingest_path`], but reports incremental progress after each candidate
/// file. Used by the app's initial ingest to drive a *determinate* progress bar
/// while a vault (re-)embeds (e.g. after an embedder-dimension change). The
/// callback is called on EVERY file (skipped or embedded); throttling to the UI
/// is the caller's concern.
pub async fn ingest_path_with_progress(
    state: &AppState,
    root_path: &str,
    namespace: Option<String>,
    on_progress: Option<IngestProgress<'_>>,
) -> Result<IngestReport> {
    let mut report = IngestReport::default();

    // Resilience: if the vault working-copy root does not exist yet, start empty
    // instead of taking the whole engine down. This happens on macOS when a
    // post-update restart races with vault/working-copy setup, or when an iCloud
    // vault has not synced down at boot. A missing root is a soft, transient
    // condition — a later ingest (once the vault is mounted) will pick up files.
    if !std::path::Path::new(root_path).exists() {
        tracing::warn!(
            path = %root_path,
            "ingest root does not exist yet; starting empty (engine stays up)"
        );
        return Ok(report);
    }

    // Build a walk that respects .gitignore / .ignore files
    let walker = ignore::WalkBuilder::new(root_path)
        .hidden(false)
        .git_ignore(true)
        .build();

    let mut files: Vec<(String, String)> = Vec::new(); // (rel_path, content)

    for entry in walker {
        // A single unreadable entry (a file that vanished mid-walk, an iCloud
        // `.icloud` placeholder that is not downloaded, a permission error on a
        // subpath) must not abort the entire ingest — log it and skip.
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!("skipping unreadable path during ingest walk: {e}");
                continue;
            }
        };
        let path = entry.path();

        // Skip directories
        if !path.is_file() {
            continue;
        }

        // Skip the vault's non-knowledge subtrees. Only the FIRST path component
        // is checked, so a nested folder that happens to share one of these
        // names deeper in the tree is unaffected.
        if let Ok(rel) = path.strip_prefix(root_path) {
            if is_skipped_top_level(rel) {
                continue;
            }
        }

        // Only ingest recognized text files (broad source/docs/config allowlist,
        // minus generated/lock/minified noise). Binary files are additionally
        // skipped below when reading them as UTF-8 fails.
        if !is_ingestable_file(path) {
            continue;
        }

        // A file that cannot be read (iCloud placeholder not downloaded, a race
        // where it was deleted, a permission error) should be skipped, not fatal.
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!(
                    "skipping unreadable file during ingest: {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        report.files_seen += 1;

        // Compute relative path from root_path for use as the note subject
        let rel_path = path
            .strip_prefix(root_path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        files.push((rel_path, content));
    }

    // Smallest files first: with an embed cost roughly proportional to content
    // size, ascending order maximizes time-to-value — the bulk of a corpus
    // becomes searchable in the first minutes of a full (re-)index while the
    // few multi-megabyte outliers grind at the end, instead of one giant file
    // early in walk order stalling visible progress near 0% for minutes.
    // Incremental runs are unaffected (unchanged files are skipped either way).
    files.sort_by_key(|(_, content)| content.len());

    let total = files.len();
    for (idx, (rel_path, content)) in files.into_iter().enumerate() {
        // Report progress before the (potentially slow) embed of this file, so the
        // UI advances steadily; the caller throttles what actually reaches the UI.
        if let Some(cb) = on_progress {
            cb(idx, total);
        }
        ingest_one_file(state, &rel_path, &content, namespace.clone(), &mut report).await?;
    }

    // Stamp which git commit/branch this (re-)ingest reflects, when the root is a
    // git working tree — so the graph records the git state it was built from.
    // No-op for a non-git vault; best-effort so provenance never fails an ingest.
    let _ = crate::service::git_provenance::record_git_provenance(
        state,
        root_path,
        report.files_ingested,
    )
    .await;

    // Final tick: report completion (100%) so the UI can settle to done.
    if let Some(cb) = on_progress {
        cb(total, total);
    }

    Ok(report)
}

/// Ingest ONE already-read file: hash-compare against the signed registry, skip
/// when unchanged, otherwise purge the source's stale facts/chunks, reconcile its
/// `task:`/`card:` nodes by diff, write the fresh triples + chunks, and refresh
/// the source-hash registry triple. `report` is updated in place.
///
/// This is the single implementation of "ingest a file", shared verbatim by the
/// full walk ([`ingest_path_with_progress`]) and the path-targeted
/// [`ingest_paths`] — so an incremental re-ingest can never drift from a full one.
async fn ingest_one_file(
    state: &AppState,
    rel_path: &str,
    content: &str,
    namespace: Option<String>,
    report: &mut IngestReport,
) -> Result<()> {
    let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();

    // Check registry: does a triple (rel_path, aingle:source_hash, <hash>) already exist?
    let existing_hash = {
        let graph = state.graph.read().await;
        let pattern = TriplePattern::any()
            .with_subject(NodeId::named(rel_path))
            .with_predicate(Predicate::named(PRED_SOURCE_HASH));
        graph
            .find(pattern)
            .map_err(|e| Error::Internal(format!("graph find error: {e}")))?
            .into_iter()
            .next()
            .and_then(|t| t.object_string().map(|s| s.to_string()))
    };

    if let Some(ref existing) = existing_hash {
        if existing == &content_hash {
            // File unchanged — skip
            report.files_skipped += 1;
            return Ok(());
        }
    }

    report.files_ingested += 1;

    // The file changed (or it's a re-ingest with a different hash): purge all
    // prior facts and chunks for this source before writing the fresh ones, so
    // stale structural triples and Ineru chunks don't linger and leak into
    // grounded retrieval.
    // Extract first, so task reconciliation can diff the note's new tasks
    // against its existing ones before anything is purged or written.
    let extraction = extract(rel_path, content);

    if existing_hash.is_some() {
        purge_source(state, rel_path, namespace.clone()).await?;
        // Task and card nodes live under `task:` / `card:` subjects that the
        // note-scoped purge above intentionally leaves alone; reconcile them
        // by diff so their signed history stays minimal — only the triples
        // that actually changed are retracted (e.g. completing one task or
        // rescheduling one card), and unchanged facts are untouched.
        reconcile_note_facts(state, rel_path, &extraction.triples, namespace.clone()).await?;
    }

    // Write structural triples
    for pt in &extraction.triples {
        let object_dto = match &pt.object {
            ObjectValue::Node(n) => ValueDto::Node { node: n.clone() },
            ObjectValue::Text(t) => ValueDto::String(t.clone()),
        };

        #[cfg(feature = "dag")]
        let prov = Some(pt.provenance.clone());
        #[cfg(not(feature = "dag"))]
        let _prov = ();

        let result = insert_triple_inner(
            state,
            object_dto,
            &pt.subject,
            &pt.predicate,
            #[cfg(feature = "dag")]
            prov,
            #[cfg(not(feature = "dag"))]
            None,
            namespace.clone(),
            None,
        )
        .await;

        match result {
            Ok(_) => {
                report.triples_written += 1;
            }
            Err(Error::GraphError(GraphError::Duplicate(_))) => {
                // Triple already exists — counts as already-written (idempotent)
                report.triples_written += 1;
            }
            Err(e) => {
                return Err(Error::Internal(format!("triple insert error: {e}")));
            }
        }
    }

    // Write text chunks to Ineru memory. Embed the file's chunks in ONE batched
    // inference (amortizes the per-call model overhead that made indexing slow —
    // a neural embedder pays fixed cost per invocation, so per-chunk calls were
    // the bottleneck), then persist them under a SINGLE memory-lock acquisition.
    if !extraction.chunks.is_empty() {
        let texts: Vec<String> = extraction.chunks.iter().map(|c| c.text.clone()).collect();
        let embeddings = state.embedder.embed_passages(&texts);

        let mut mem = state.memory.write().await;
        for (chunk, embedding) in extraction.chunks.iter().zip(embeddings) {
            let mut entry = MemoryEntry::new(
                CHUNK_ENTRY_TYPE,
                serde_json::json!({
                    "text": chunk.text,
                    "source_path": chunk.provenance.source_path,
                    "line_start": chunk.provenance.line_start,
                    "line_end": chunk.provenance.line_end,
                    "content_hash": chunk.provenance.content_hash,
                }),
            );
            entry.metadata = MemoryMetadata::with_source(&chunk.provenance.source_path);
            entry.metadata.importance = 0.6;
            entry.embedding = Some(embedding);

            mem.remember(entry)
                .map_err(|e| Error::Internal(format!("memory write error: {e}")))?;
            report.chunks_written += 1;
        }
    }

    // Write/update the source-hash registry triple
    #[cfg(feature = "dag")]
    let registry_prov = Some(aingle_graph::dag::Provenance {
        source_path: rel_path.to_string(),
        line_start: 0,
        line_end: 0,
        content_hash: content_hash.clone(),
    });
    #[cfg(not(feature = "dag"))]
    let _registry_prov = ();

    insert_triple_inner(
        state,
        ValueDto::String(content_hash.clone()),
        rel_path,
        PRED_SOURCE_HASH,
        #[cfg(feature = "dag")]
        registry_prov,
        #[cfg(not(feature = "dag"))]
        None,
        namespace.clone(),
        None,
    )
    .await
    .map_err(|e| Error::Internal(format!("registry triple insert error: {e}")))?;

    report.triples_written += 1;

    report.sources.push(SourceRecord {
        path: rel_path.to_string(),
        content_hash,
    });

    Ok(())
}

/// Whether a vault-relative path lives under one of [`SKIPPED_TOP_LEVEL_DIRS`].
///
/// Only the FIRST path component is checked, so a nested folder that happens to
/// share one of these names deeper in the tree is unaffected. Shared by the full
/// walk and [`ingest_paths`].
fn is_skipped_top_level(rel: &std::path::Path) -> bool {
    rel.components()
        .next()
        .is_some_and(|c| SKIPPED_TOP_LEVEL_DIRS.iter().any(|d| c.as_os_str() == *d))
}

/// Normalize a caller-supplied vault-relative path into the canonical
/// forward-slashed form used as a note subject, REJECTING anything that could
/// escape the vault root.
///
/// Rejected (returns `None`): absolute paths, drive prefixes, any `..`
/// component, and the empty path. `.` components are dropped. Backslashes are
/// treated as separators so a Windows-shaped path normalizes identically on
/// every platform (the whole codebase already stores `a/b/c`).
fn normalize_vault_rel(raw: &str) -> Option<String> {
    use std::path::Component;

    let unified = raw.replace('\\', "/");
    let path = std::path::Path::new(&unified);
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            // RootDir / Prefix / ParentDir would let a caller address anything
            // outside the vault — never accepted.
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// The `.gitignore` / `.ignore` rules that apply between a vault root and a
/// target file, so a path-targeted ingest excludes the same files the full walk
/// excludes for free via `ignore::WalkBuilder`.
///
/// Matchers are built lazily per directory and reused for the whole batch. The
/// nearest matcher wins, as git does (a deeper `!unignored` beats a shallower
/// `ignored`). Global gitignore, `.git/info/exclude` and ignore files ABOVE the
/// vault root are not consulted; the only divergence that can produce is
/// ingesting a file the walk would have skipped, never missing a change.
struct IgnoreChain {
    root: std::path::PathBuf,
    per_dir: std::collections::HashMap<std::path::PathBuf, ignore::gitignore::Gitignore>,
}

impl IgnoreChain {
    fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
            per_dir: std::collections::HashMap::new(),
        }
    }

    fn matcher(&mut self, dir: &std::path::Path) -> &ignore::gitignore::Gitignore {
        self.per_dir.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut builder = ignore::gitignore::GitignoreBuilder::new(dir);
            // Both files the walk honours; a missing/invalid one is simply no rules.
            builder.add(dir.join(".gitignore"));
            builder.add(dir.join(".ignore"));
            builder
                .build()
                .unwrap_or_else(|_| ignore::gitignore::Gitignore::empty())
        })
    }

    /// Whether `abs` (a path under the vault root) is excluded by ignore rules.
    fn is_ignored(&mut self, abs: &std::path::Path) -> bool {
        let Some(parent) = abs.parent() else {
            return false;
        };
        // Directories from the file's own folder up to the vault root, deepest first.
        let mut dirs: Vec<std::path::PathBuf> = Vec::new();
        let mut cursor = Some(parent);
        while let Some(dir) = cursor {
            dirs.push(dir.to_path_buf());
            if dir == self.root.as_path() {
                break;
            }
            cursor = dir.parent();
        }

        for dir in dirs {
            // `matched_path_or_any_parents` also catches "an ancestor directory is
            // ignored" (the `target/` or `node_modules/` case), which is what makes
            // a single root-level `.gitignore` enough for most repos.
            let decision = match self.matcher(&dir).matched_path_or_any_parents(abs, false) {
                ignore::Match::Ignore(_) => Some(true),
                ignore::Match::Whitelist(_) => Some(false),
                ignore::Match::None => None,
            };
            if let Some(ignored) = decision {
                return ignored;
            }
        }
        false
    }
}

/// Ingest ONLY the named vault-relative paths — the incremental counterpart of
/// [`ingest_path`], for a file watcher that already knows what changed.
///
/// Each path goes through exactly the same rules as the full walk: the
/// [`SKIPPED_TOP_LEVEL_DIRS`] exclusion, the ingestable-file allowlist, the
/// `.gitignore` / `.ignore` rules, and then the shared
/// hash-compare → skip / purge-stale → insert body ([`ingest_one_file`]). Files
/// are read one at a time, so a targeted run's memory is bounded by the single
/// largest changed file rather than by the whole vault.
///
/// **Deletions.** A listed path that is no longer a readable file on disk, or that
/// has become excluded, has its facts RETRACTED — every triple it authored is
/// signed-deleted (AIngle is a DAG: retraction, never physical erasure), its
/// `task:` / `card:` nodes are tombstoned, its Ineru chunks forgotten and its
/// source-hash registry triple dropped — and is counted in
/// [`IngestReport::files_removed`]. The full walk cannot do this (it never learns
/// that a file vanished), so targeting a path strictly *adds* correctness here.
///
/// Paths that are absolute, rooted, or contain `..` are rejected and skipped
/// (logged): a vault-relative API must never be able to address the wider disk.
pub async fn ingest_paths(
    state: &AppState,
    root_path: &str,
    rel_paths: &[String],
    namespace: Option<String>,
) -> Result<IngestReport> {
    let mut report = IngestReport::default();
    let root = std::path::Path::new(root_path);
    let mut ignores = IgnoreChain::new(root);
    let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();

    for raw in rel_paths {
        let Some(rel) = normalize_vault_rel(raw) else {
            tracing::warn!(path = %raw, "ingest_paths: rejecting a path that is not vault-relative");
            continue;
        };
        if !done.insert(rel.clone()) {
            continue; // the same file touched twice in one burst
        }

        let abs = root.join(&rel);
        let excluded = is_skipped_top_level(std::path::Path::new(&rel))
            || !is_ingestable_file(&abs)
            || ignores.is_ignored(&abs);

        // Gone (deleted, moved to `.trash`, renamed away) or no longer eligible:
        // retract whatever this source previously contributed. A no-op for a path
        // that was never ingested, so a spurious event costs one registry lookup.
        if excluded || !abs.is_file() {
            if retract_source(state, &rel, namespace.clone()).await? {
                report.files_removed += 1;
            }
            continue;
        }

        // A file that cannot be read (iCloud placeholder not downloaded, a race
        // where it was deleted, a permission error) should be skipped, not fatal —
        // the same tolerance the walk applies.
        let content = match std::fs::read_to_string(&abs) {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!(
                    "skipping unreadable file during targeted ingest: {}: {e}",
                    abs.display()
                );
                continue;
            }
        };

        report.files_seen += 1;
        ingest_one_file(state, &rel, &content, namespace.clone(), &mut report).await?;
    }

    // Stamp the git state this incremental ingest reflects — but only when it
    // actually changed something, so a no-op watcher cycle does not append a
    // provenance action per keystroke-save. Best-effort, never fails an ingest.
    if report.files_ingested > 0 || report.files_removed > 0 {
        let _ = crate::service::git_provenance::record_git_provenance(
            state,
            root_path,
            report.files_ingested,
        )
        .await;
    }

    Ok(report)
}

/// Retract everything `rel_path` ever contributed to the graph, because the file
/// is gone (or no longer eligible for ingest). Returns whether the source was
/// known — i.e. whether anything was actually retracted.
///
/// Uses the same primitives a changed file uses, so a delete leaves the graph in
/// exactly the state "this file never existed, and here is the signed history of
/// why": [`purge_source`] signed-deletes every triple the file authored (its
/// structural facts AND its source-hash registry triple) and forgets its chunks,
/// then [`reconcile_note_facts`] with an EMPTY new set tombstones the note's
/// `task:` / `card:` nodes.
async fn retract_source(
    state: &AppState,
    rel_path: &str,
    namespace: Option<String>,
) -> Result<bool> {
    let known = {
        let graph = state.graph.read().await;
        let pattern = TriplePattern::any()
            .with_subject(NodeId::named(rel_path))
            .with_predicate(Predicate::named(PRED_SOURCE_HASH));
        !graph
            .find(pattern)
            .map_err(|e| Error::Internal(format!("graph find error: {e}")))?
            .is_empty()
    };
    if !known {
        return Ok(false);
    }

    purge_source(state, rel_path, namespace.clone()).await?;
    reconcile_note_facts(state, rel_path, &[], namespace).await?;
    Ok(true)
}

/// Remove every fact and chunk previously ingested from `rel_path`, so a changed
/// file's stale data can't survive a re-ingest.
///
/// Deletes all graph triples whose subject is `rel_path` (its structural facts
/// plus the source-hash registry triple) and forgets every Ineru chunk whose
/// `metadata.source` is `rel_path`. Inbound links from *other* files (where
/// `rel_path` is the object, not the subject) are left untouched.
async fn purge_source(state: &AppState, rel_path: &str, namespace: Option<String>) -> Result<()> {
    // Graph: delete every triple authored by this source (subject == rel_path).
    let stale_ids: Vec<String> = {
        let graph = state.graph.read().await;
        let pattern = TriplePattern::any().with_subject(NodeId::named(rel_path));
        graph
            .find(pattern)
            .map_err(|e| Error::Internal(format!("graph find error: {e}")))?
            .into_iter()
            .map(|t| t.id().to_hex())
            .collect()
    };
    for hex_id in stale_ids {
        // Best-effort: a concurrently-removed triple is fine to skip.
        let _ = delete_triple(state, &hex_id, namespace.clone(), None).await;
    }

    // Ineru: forget every chunk that came from this source.
    {
        let mut mem = state.memory.write().await;
        let ids: Vec<MemoryId> = mem
            .stm
            .all_entries()
            .into_iter()
            .chain(mem.ltm.all_entries())
            .filter(|e| e.entry_type == CHUNK_ENTRY_TYPE && e.metadata.source == rel_path)
            .map(|e| e.id)
            .collect();
        for id in ids {
            let _ = mem.forget(&id);
        }
    }

    Ok(())
}

/// Semantic identity of a task/card triple across re-ingests: the subject,
/// predicate and object text together. An unchanged triple keeps the same
/// identity, so it is neither retracted nor re-signed.
fn fact_identity(subject: &str, predicate: &str, object: &str) -> String {
    format!("{subject}\u{1}{predicate}\u{1}{object}")
}

/// Whether a subject is a note-scoped fact node reconciled by diff on re-ingest
/// (a `task:` or `card:` node linked to its note via `in_note`).
fn is_note_fact_subject(subject: &str) -> bool {
    subject.starts_with("task:") || subject.starts_with("card:")
}

/// Reconcile a note's `task:` / `card:` nodes on re-ingest: retract only the
/// triples the current content no longer produces, and leave the rest in place.
/// Because each retraction and insert is a signed DAG action, this keeps a task's
/// or card's verifiable history minimal — completing a task is one `status`
/// retract + insert, and reviewing a card rewrites only that card's `card_*`
/// facts, not a churn of every fact in the note; a removed task/card is
/// tombstoned.
async fn reconcile_note_facts(
    state: &AppState,
    rel_path: &str,
    new_triples: &[aingle_ingest::ProvenancedTriple],
    namespace: Option<String>,
) -> Result<()> {
    use crate::service::triple_util::{obj_string, strip_brackets};
    use aingle_ingest::ObjectValue;

    // Identity of every task/card triple the current content produces.
    let new_keys: std::collections::HashSet<String> = new_triples
        .iter()
        .filter(|t| is_note_fact_subject(&t.subject))
        .map(|t| {
            let obj = match &t.object {
                ObjectValue::Node(n) => n.as_str(),
                ObjectValue::Text(s) => s.as_str(),
            };
            fact_identity(&t.subject, &t.predicate, obj)
        })
        .collect();

    // Existing task/card triples for this note (found via their `in_note` link)
    // whose identity is no longer produced — the stale ones to retract.
    let stale_ids: Vec<String> = {
        let graph = state.graph.read().await;
        let subjects: std::collections::HashSet<String> = graph
            .find(TriplePattern::any().with_predicate(Predicate::named("in_note")))
            .map_err(|e| Error::Internal(format!("graph find error: {e}")))?
            .into_iter()
            .filter(|t| obj_string(t).as_deref() == Some(rel_path))
            .map(|t| strip_brackets(&t.subject.to_string()).to_string())
            .filter(|s| is_note_fact_subject(s))
            .collect();

        let mut stale = Vec::new();
        for subj in &subjects {
            for t in graph
                .find(TriplePattern::any().with_subject(NodeId::named(subj)))
                .map_err(|e| Error::Internal(format!("graph find error: {e}")))?
            {
                let predicate = strip_brackets(t.predicate.as_ref());
                let object = obj_string(&t).unwrap_or_default();
                if !new_keys.contains(&fact_identity(subj, predicate, &object)) {
                    stale.push(t.id().to_hex());
                }
            }
        }
        stale
    };

    for hex_id in stale_ids {
        // Best-effort: a concurrently-removed triple is fine to skip.
        let _ = delete_triple(state, &hex_id, namespace.clone(), None).await;
    }
    Ok(())
}

/// List all source files recorded in the signed registry (path + content hash).
pub async fn list_sources(state: &AppState) -> Result<Vec<SourceRecord>> {
    let graph = state.graph.read().await;
    let pattern = TriplePattern::any().with_predicate(Predicate::named(PRED_SOURCE_HASH));
    let triples = graph
        .find(pattern)
        .map_err(|e| Error::Internal(format!("graph find error: {e}")))?;
    Ok(triples
        .iter()
        .filter_map(|t| {
            // `NodeId::to_string` renders the IRI form `<path>`; strip the angle
            // brackets so the path matches the clean form used by `ingest_path`'s
            // report and the chunk provenance (round-trippable into other tools).
            let path = t
                .subject
                .to_string()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            t.object_string().map(|h| SourceRecord {
                path,
                content_hash: h.to_string(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &std::path::Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn broad_source_and_doc_extensions_are_ingestable() {
        // A real project — not just the old md/rs/py/ts handful. Regression for
        // "a Swift/iOS project ingests as almost nothing".
        for f in [
            "App.swift",
            "View.kt",
            "Main.java",
            "server.go",
            "Component.tsx",
            "util.jsx",
            "lib.rb",
            "index.php",
            "query.sql",
            "config.yaml",
            "styles.scss",
            "Widget.dart",
            "notes.md",
            "main.c",
            "engine.cpp",
        ] {
            assert!(
                is_ingestable_file(std::path::Path::new(f)),
                "{f} should be ingestable"
            );
        }
    }

    #[test]
    fn wellknown_extensionless_files_are_ingestable() {
        for f in [
            "Dockerfile",
            "Makefile",
            "README",
            "LICENSE",
            "CMakeLists.txt",
        ] {
            assert!(
                is_ingestable_file(std::path::Path::new(f)),
                "{f} should be ingestable"
            );
        }
    }

    #[test]
    fn generated_lock_and_minified_noise_is_skipped() {
        for f in [
            "app.min.js",
            "styles.min.css",
            "bundle.js.map",
            "package-lock.json",
            "yarn.lock",
            "Cargo.lock",
            "Podfile.lock",
            "pnpm-lock.yaml",
            "types.generated.ts",
        ] {
            assert!(
                !is_ingestable_file(std::path::Path::new(f)),
                "{f} should be skipped as noise"
            );
        }
    }

    #[test]
    fn binaries_and_unknown_types_are_skipped() {
        for f in ["photo.png", "archive.zip", "app.bin", "font.woff2", "noext"] {
            assert!(
                !is_ingestable_file(std::path::Path::new(f)),
                "{f} should be skipped"
            );
        }
    }

    async fn enabled_state() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        state
    }

    #[tokio::test]
    async fn ingest_writes_triples_and_chunks() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "note.md",
            "# Title\n\nWe use [[sled]] for storage. #durability\n",
        );
        let state = enabled_state().await;

        let report = ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        assert_eq!(report.files_seen, 1);
        assert_eq!(report.files_ingested, 1);
        assert!(report.triples_written >= 3); // heading + links_to + tagged + registry
        assert!(report.chunks_written >= 1);

        let mem = state.memory.read().await;
        let hits = mem.recall_text("sled storage").unwrap();
        assert!(!hits.is_empty());
    }

    #[tokio::test]
    async fn task_reingest_is_diff_aware() {
        use crate::service::tasks::list_tasks;
        use crate::service::triple_util::{obj_string, strip_brackets};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        let state = enabled_state().await;

        // Two open tasks.
        write(dir.path(), "todos.md", "# Todos\n\n- [ ] Task A\n- [ ] Task B\n");
        ingest_path(&state, path, None).await.unwrap();
        let rows = list_tasks(&state, None).await;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.status == "todo"));

        // Complete A, keep B — re-ingest the changed note.
        write(dir.path(), "todos.md", "# Todos\n\n- [x] Task A\n- [ ] Task B\n");
        ingest_path(&state, path, None).await.unwrap();
        let rows = list_tasks(&state, None).await;
        assert_eq!(rows.len(), 2, "still exactly two tasks — no orphans or duplicates");
        assert_eq!(rows.iter().find(|r| r.text == "Task A").unwrap().status, "done");
        assert_eq!(rows.iter().find(|r| r.text == "Task B").unwrap().status, "todo");

        // The old `status=todo` triple for A must be gone (exactly one remains).
        {
            let g = state.graph.read().await;
            let a_subj: String = g
                .find(TriplePattern::any().with_predicate(Predicate::named("task_text")))
                .unwrap()
                .into_iter()
                .find(|t| obj_string(t).as_deref() == Some("Task A"))
                .map(|t| strip_brackets(&t.subject.to_string()).to_string())
                .expect("Task A node");
            let statuses = g
                .find(
                    TriplePattern::any()
                        .with_subject(NodeId::named(&a_subj))
                        .with_predicate(Predicate::named("status")),
                )
                .unwrap();
            assert_eq!(statuses.len(), 1, "no stale status triple should remain for A");
        }

        // Remove A from the note — its task node is retracted, B survives.
        write(dir.path(), "todos.md", "# Todos\n\n- [ ] Task B\n");
        ingest_path(&state, path, None).await.unwrap();
        let rows = list_tasks(&state, None).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "Task B");
    }

    #[tokio::test]
    async fn card_review_reconciles_only_the_changed_card() {
        // Rewriting ONE card's SRS comment must retract/re-sign only that card's
        // facts; every other card in the note is untouched (its triple ids are
        // stable), so a review is minimal signed churn, not a re-sign of the deck.
        use crate::service::cards::list_cards;
        use crate::service::triple_util::obj_string;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        let state = enabled_state().await;

        // Two cards, each with a sticky id so identity survives answer edits.
        write(
            dir.path(),
            "deck.md",
            "# Deck\n\n\
             Q1 term #card <!-- srs id=aaaaaaaaaaaa ef=2.5 due=2026-07-20 -->\n\
             Q2 term #card <!-- srs id=bbbbbbbbbbbb ef=2.5 due=2026-08-01 -->\n",
        );
        ingest_path(&state, path, None).await.unwrap();
        let rows = list_cards(&state, "2026-07-24").await;
        assert_eq!(rows.len(), 2);

        // Snapshot Q2's `card_due` triple id — it must NOT change when only Q1 is reviewed.
        let q2_due_id_before = card_field_triple_id(&state, "card:deck.md#bbbbbbbbbbbb", "card_due")
            .await
            .expect("Q2 card_due before");

        // Review Q1: reschedule it (new due + ef), keep its front text and id.
        write(
            dir.path(),
            "deck.md",
            "# Deck\n\n\
             Q1 term #card <!-- srs id=aaaaaaaaaaaa ef=2.8 due=2026-09-01 -->\n\
             Q2 term #card <!-- srs id=bbbbbbbbbbbb ef=2.5 due=2026-08-01 -->\n",
        );
        ingest_path(&state, path, None).await.unwrap();

        let rows = list_cards(&state, "2026-07-24").await;
        assert_eq!(
            rows.len(),
            2,
            "still exactly two cards — no orphans or duplicates"
        );
        let q1 = rows
            .iter()
            .find(|r| r.subject == "card:deck.md#aaaaaaaaaaaa")
            .unwrap();
        assert_eq!(q1.due.as_deref(), Some("2026-09-01"), "Q1 rescheduled");
        assert_eq!(q1.ef.as_deref(), Some("2.8"));

        // Exactly one `card_due` triple remains for Q1 (no stale one lingering).
        {
            let g = state.graph.read().await;
            let dues = g
                .find(
                    TriplePattern::any()
                        .with_subject(NodeId::named("card:deck.md#aaaaaaaaaaaa"))
                        .with_predicate(Predicate::named("card_due")),
                )
                .unwrap();
            assert_eq!(dues.len(), 1, "no stale card_due should remain for Q1");
            assert_eq!(obj_string(&dues[0]).as_deref(), Some("2026-09-01"));
        }

        // Q2's `card_due` triple id is unchanged — it was neither retracted nor re-signed.
        let q2_due_id_after = card_field_triple_id(&state, "card:deck.md#bbbbbbbbbbbb", "card_due")
            .await
            .expect("Q2 card_due after");
        assert_eq!(
            q2_due_id_before, q2_due_id_after,
            "reviewing Q1 must not churn Q2's signed facts"
        );
    }

    /// Hex id of the single triple `(subject, predicate, *)`, if present.
    async fn card_field_triple_id(
        state: &AppState,
        subject: &str,
        predicate: &str,
    ) -> Option<String> {
        let g = state.graph.read().await;
        g.find(
            TriplePattern::any()
                .with_subject(NodeId::named(subject))
                .with_predicate(Predicate::named(predicate)),
        )
        .ok()?
        .into_iter()
        .next()
        .map(|t| t.id().to_hex())
    }

    #[tokio::test]
    async fn progress_callback_reports_per_file_and_completes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.md", "# A\n\nalpha\n");
        write(dir.path(), "b.md", "# B\n\nbeta\n");
        write(dir.path(), "c.md", "# C\n\ngamma\n");
        let state = enabled_state().await;

        let calls = std::sync::Mutex::new(Vec::<(usize, usize)>::new());
        let cb = |done: usize, total: usize| calls.lock().unwrap().push((done, total));
        let report =
            ingest_path_with_progress(&state, dir.path().to_str().unwrap(), None, Some(&cb))
                .await
                .unwrap();

        assert_eq!(report.files_seen, 3);
        let calls = calls.into_inner().unwrap();
        assert!(!calls.is_empty(), "callback must fire");
        assert!(
            calls.iter().all(|&(_, t)| t == 3),
            "total must be the file count: {calls:?}"
        );
        assert_eq!(
            *calls.last().unwrap(),
            (3, 3),
            "must finish at 100% (3/3): {calls:?}"
        );
    }

    #[tokio::test]
    async fn reingesting_unchanged_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "note.md", "# Title\n\nStable [[content]].\n");
        let state = enabled_state().await;
        let root = dir.path().to_str().unwrap();

        ingest_path(&state, root, None).await.unwrap();
        let actions_after_first = {
            let g = state.graph.read().await;
            g.dag_store().unwrap().action_count()
        };

        let report2 = ingest_path(&state, root, None).await.unwrap();
        let actions_after_second = {
            let g = state.graph.read().await;
            g.dag_store().unwrap().action_count()
        };

        assert_eq!(report2.files_skipped, 1);
        assert_eq!(report2.files_ingested, 0);
        assert_eq!(
            actions_after_first, actions_after_second,
            "re-ingesting unchanged files must write zero new DAG actions"
        );
    }

    #[tokio::test]
    async fn changed_file_reingests() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "note.md", "# A\n\nFirst [[x]].\n");
        let state = enabled_state().await;
        let root = dir.path().to_str().unwrap();

        ingest_path(&state, root, None).await.unwrap();
        write(dir.path(), "note.md", "# A\n\nSecond [[y]] changed.\n");
        let report = ingest_path(&state, root, None).await.unwrap();
        assert_eq!(report.files_ingested, 1);
        assert_eq!(report.files_skipped, 0);
    }

    #[tokio::test]
    async fn changed_file_purges_stale_chunks() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "note.md", "# A\n\nWe use sled for storage.\n");
        let state = enabled_state().await;
        let root = dir.path().to_str().unwrap();
        ingest_path(&state, root, None).await.unwrap();

        // Change the file so the old sentence no longer exists in the source.
        write(dir.path(), "note.md", "# A\n\nWe use rocksdb now.\n");
        ingest_path(&state, root, None).await.unwrap();

        // Querying the OLD sentence verbatim must not surface the stale chunk:
        // re-ingesting a changed file must forget the previous chunks for it.
        let g = crate::service::ground::ground(&state, "We use sled for storage.", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.iter().any(|c| c.text.contains("sled")),
            "stale 'sled' chunk should be purged on re-ingest, got: {:?}",
            g.answer_context
        );
    }

    #[tokio::test]
    async fn changed_file_purges_stale_triples() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "note.md", "# A\n\nSee [[sled]].\n");
        let state = enabled_state().await;
        let root = dir.path().to_str().unwrap();
        ingest_path(&state, root, None).await.unwrap();

        // Repoint the wikilink: the old links_to:sled triple must not linger.
        write(dir.path(), "note.md", "# A\n\nSee [[rocksdb]].\n");
        ingest_path(&state, root, None).await.unwrap();

        let graph = state.graph.read().await;
        let links = graph
            .find(
                TriplePattern::any()
                    .with_subject(NodeId::named("note.md"))
                    .with_predicate(Predicate::named("links_to")),
            )
            .unwrap();
        assert_eq!(
            links.len(),
            1,
            "stale links_to should be purged, leaving only the new link, got: {links:?}"
        );
    }

    #[tokio::test]
    async fn inbox_staging_area_is_excluded_from_ingest() {
        // Trust-critical: notes an agent PROPOSES land in top-level `_inbox/` and
        // must NOT be indexed until a human approves them. If the walk indexed them,
        // unreviewed content would leak into retrieval/grounding.
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "kept.md",
            "# Kept\n\nWe use [[sled]] for storage.\n",
        );
        std::fs::create_dir_all(dir.path().join("_inbox")).unwrap();
        write(
            &dir.path().join("_inbox"),
            "proposal.md",
            "# Proposal\n\nUnreviewed claim about [[quantum]].\n",
        );
        let state = enabled_state().await;
        let root = dir.path().to_str().unwrap();

        let report = ingest_path(&state, root, None).await.unwrap();
        assert_eq!(report.files_seen, 1, "only the approved note is walked");
        assert_eq!(report.files_ingested, 1);

        // The staged proposal's content must be unreachable via grounding.
        let g = crate::service::ground::ground(&state, "Unreviewed claim about quantum", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.iter().any(|c| c.source.contains("_inbox")),
            "no _inbox source may appear in retrieval, got: {:?}",
            g.answer_context
        );
    }

    #[tokio::test]
    async fn trash_is_excluded_from_ingest() {
        // Deleting a note moves it to `.trash/<orig path>` INSIDE the vault. If the
        // walk indexed it, the delete would RESURRECT the note's facts as brand new
        // triples under a `.trash/…` subject — deleting would add to the graph.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "kept.md", "# Kept\n\nWe use [[sled]].\n");
        std::fs::create_dir_all(dir.path().join(".trash").join("notes")).unwrap();
        write(
            &dir.path().join(".trash").join("notes"),
            "deleted.md",
            "# Deleted\n\nA retracted claim about [[quantum]].\n",
        );
        let state = enabled_state().await;
        let root = dir.path().to_str().unwrap();

        let report = ingest_path(&state, root, None).await.unwrap();
        assert_eq!(report.files_seen, 1, "only the live note is walked");
        assert_eq!(report.files_ingested, 1);

        let g = crate::service::ground::ground(&state, "A retracted claim about quantum", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.iter().any(|c| c.source.contains(".trash")),
            "no .trash source may appear in retrieval, got: {:?}",
            g.answer_context
        );
    }

    #[test]
    fn skipped_top_level_dirs_covers_inbox_and_trash() {
        assert!(SKIPPED_TOP_LEVEL_DIRS.contains(&"_inbox"));
        assert!(SKIPPED_TOP_LEVEL_DIRS.contains(&".trash"));
    }

    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn approval_is_listed_after_ingest_with_named_author() {
        // Reproduces the desktop app's exact path: a named dag_author + an ingest
        // that writes provenance DAG actions (advancing tips + the author index),
        // THEN a review approval — which must show up in list_approvals. Guards
        // against a seq collision or a parent-validation failure making the signed
        // approval silently unrecorded.
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "note.md",
            "# A\n\nWe use [[sled]]. #durability\n",
        );
        let mut state = enabled_state().await;
        state.dag_author = Some(NodeId::named("Alice"));
        let root = dir.path().to_str().unwrap();

        ingest_path(&state, root, Some("initial".into()))
            .await
            .unwrap();

        crate::service::review::record_approval(&state, "note.md", "approved body", "mcp")
            .await
            .unwrap();

        let approvals = crate::service::review::list_approvals(&state, 50).await;
        assert_eq!(
            approvals.len(),
            1,
            "the approval must be listed after an ingest under the same named author; got {approvals:?}"
        );
        assert_eq!(approvals[0].note_path, "note.md");
    }

    // ─── Path-targeted (incremental) ingest ──────────────────────────────────

    /// All triples in the graph as comparable `(subject, predicate, object)`
    /// tuples, sorted — a stable fingerprint of graph STATE (triple ids and DAG
    /// action hashes deliberately excluded, since those encode *how* the state
    /// was reached).
    async fn triple_fingerprint(state: &AppState) -> Vec<(String, String, String)> {
        use crate::service::triple_util::{obj_string, strip_brackets};
        let graph = state.graph.read().await;
        let mut rows: Vec<(String, String, String)> = graph
            .find(TriplePattern::any())
            .unwrap()
            .into_iter()
            .map(|t| {
                (
                    strip_brackets(&t.subject.to_string()).to_string(),
                    strip_brackets(t.predicate.as_ref()).to_string(),
                    obj_string(&t).unwrap_or_default(),
                )
            })
            .collect();
        rows.sort();
        rows
    }

    #[test]
    fn vault_rel_paths_that_escape_the_root_are_rejected() {
        for bad in [
            "../secrets.md",
            "notes/../../etc/passwd",
            "/etc/passwd",
            "",
            ".",
        ] {
            assert!(
                normalize_vault_rel(bad).is_none(),
                "{bad:?} must be rejected"
            );
        }
        assert_eq!(normalize_vault_rel("a/b.md").as_deref(), Some("a/b.md"));
        assert_eq!(normalize_vault_rel("a\\b.md").as_deref(), Some("a/b.md"));
        assert_eq!(normalize_vault_rel("./a.md").as_deref(), Some("a.md"));
    }

    #[tokio::test]
    async fn ingest_paths_only_touches_the_named_file() {
        // The release blocker in one assertion: with three notes indexed, editing
        // ONE must process exactly one file — the other two are not read, not
        // hashed, not even counted as skipped.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        write(dir.path(), "a.md", "# A\n\nalpha\n");
        write(dir.path(), "b.md", "# B\n\nbeta\n");
        write(dir.path(), "c.md", "# C\n\ngamma\n");
        let state = enabled_state().await;
        ingest_path(&state, root, None).await.unwrap();

        write(dir.path(), "b.md", "# B\n\nbeta revised [[gamma]]\n");
        let report = ingest_paths(&state, root, &["b.md".to_string()], None)
            .await
            .unwrap();

        assert_eq!(report.files_seen, 1, "only b.md may be read");
        assert_eq!(report.files_ingested, 1);
        assert_eq!(
            report.files_skipped, 0,
            "a.md and c.md must not be visited at all"
        );
        assert_eq!(report.sources.len(), 1);
        assert_eq!(report.sources[0].path, "b.md");
    }

    #[tokio::test]
    async fn ingest_paths_skips_an_unchanged_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        write(dir.path(), "note.md", "# A\n\nstable\n");
        let state = enabled_state().await;
        ingest_path(&state, root, None).await.unwrap();
        let actions_before = {
            let g = state.graph.read().await;
            g.dag_store().unwrap().action_count()
        };

        let report = ingest_paths(&state, root, &["note.md".to_string()], None)
            .await
            .unwrap();

        assert_eq!(report.files_skipped, 1);
        assert_eq!(report.files_ingested, 0);
        let actions_after = {
            let g = state.graph.read().await;
            g.dag_store().unwrap().action_count()
        };
        assert_eq!(
            actions_before, actions_after,
            "a targeted re-ingest of an unchanged file must write zero DAG actions"
        );
    }

    #[tokio::test]
    async fn ingest_paths_adds_a_brand_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        write(dir.path(), "a.md", "# A\n\nalpha\n");
        let state = enabled_state().await;
        ingest_path(&state, root, None).await.unwrap();

        write(dir.path(), "new.md", "# New\n\nWe use [[sled]].\n");
        let report = ingest_paths(&state, root, &["new.md".to_string()], None)
            .await
            .unwrap();

        assert_eq!(report.files_ingested, 1);
        assert_eq!(report.files_removed, 0);
        let sources = list_sources(&state).await.unwrap();
        assert!(sources.iter().any(|s| s.path == "new.md"));
    }

    #[tokio::test]
    async fn ingest_paths_retracts_a_deleted_file() {
        // Deletions were previously INVISIBLE to ingest: nothing walked the
        // registry, so a removed note's facts lingered forever. A targeted path
        // knows the file is gone, so it can retract it.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        write(dir.path(), "keep.md", "# Keep\n\nkept\n");
        write(
            dir.path(),
            "gone.md",
            "# Gone\n\nWe use [[sled]].\n\n- [ ] Doomed task\n",
        );
        let state = enabled_state().await;
        ingest_path(&state, root, None).await.unwrap();

        std::fs::remove_file(dir.path().join("gone.md")).unwrap();
        let report = ingest_paths(&state, root, &["gone.md".to_string()], None)
            .await
            .unwrap();

        assert_eq!(report.files_removed, 1);
        assert_eq!(report.files_ingested, 0);

        // Its triples (structural + registry) are gone from the graph…
        {
            let graph = state.graph.read().await;
            let left = graph
                .find(TriplePattern::any().with_subject(NodeId::named("gone.md")))
                .unwrap();
            assert!(left.is_empty(), "deleted note still has triples: {left:?}");
        }
        // …its task node is tombstoned…
        let tasks = crate::service::tasks::list_tasks(&state, None).await;
        assert!(
            tasks.is_empty(),
            "a deleted note's tasks must be retracted, got {tasks:?}"
        );
        // …it is off the source registry…
        let sources = list_sources(&state).await.unwrap();
        assert!(!sources.iter().any(|s| s.path == "gone.md"));
        assert!(sources.iter().any(|s| s.path == "keep.md"));
        // …and its chunks no longer surface in grounded retrieval.
        let g = crate::service::ground::ground(&state, "We use sled", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.iter().any(|c| c.source == "gone.md"),
            "a deleted note's chunks must be forgotten, got: {:?}",
            g.answer_context
        );
    }

    #[tokio::test]
    async fn ingest_paths_retracting_an_unknown_path_is_a_noop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let state = enabled_state().await;

        let report = ingest_paths(&state, root, &["never-existed.md".to_string()], None)
            .await
            .unwrap();

        assert_eq!(report.files_removed, 0);
        assert_eq!(report.files_ingested, 0);
        assert_eq!(report.files_seen, 0);
    }

    #[tokio::test]
    async fn ingest_paths_applies_the_same_exclusions_as_the_walk() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::create_dir_all(dir.path().join("_inbox")).unwrap();
        std::fs::create_dir_all(dir.path().join(".trash")).unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        write(dir.path(), ".gitignore", "build/\n");
        write(&dir.path().join("_inbox"), "proposal.md", "# Staged\n");
        write(&dir.path().join(".trash"), "deleted.md", "# Deleted\n");
        write(&dir.path().join("build"), "out.md", "# Generated\n");
        write(dir.path(), "image.png", "not really a png");
        let state = enabled_state().await;

        let report = ingest_paths(
            &state,
            root,
            &[
                "_inbox/proposal.md".to_string(),
                ".trash/deleted.md".to_string(),
                "build/out.md".to_string(),
                "image.png".to_string(),
                "../escape.md".to_string(),
            ],
            None,
        )
        .await
        .unwrap();

        assert_eq!(report.files_seen, 0, "every path must be excluded");
        assert_eq!(report.files_ingested, 0);
        assert_eq!(report.files_removed, 0);
        assert!(list_sources(&state).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn targeted_ingest_reaches_the_same_graph_state_as_a_full_walk() {
        // Signed-DAG semantics must be identical: for the same edit, incremental
        // and full ingest must land on the same triples.
        let seed = |dir: &std::path::Path| {
            write(dir, "a.md", "# A\n\nalpha links [[b]]\n");
            write(dir, "b.md", "# B\n\n- [ ] Task B\n");
            write(dir, "c.md", "# C\n\ngamma\n");
        };
        let edit = |dir: &std::path::Path| {
            write(dir, "b.md", "# B\n\n- [x] Task B\n\nnow links [[c]]\n");
        };

        // Targeted.
        let d1 = tempfile::tempdir().unwrap();
        let r1 = d1.path().to_str().unwrap();
        seed(d1.path());
        let s1 = enabled_state().await;
        ingest_path(&s1, r1, None).await.unwrap();
        edit(d1.path());
        ingest_paths(&s1, r1, &["b.md".to_string()], None)
            .await
            .unwrap();

        // Full walk.
        let d2 = tempfile::tempdir().unwrap();
        let r2 = d2.path().to_str().unwrap();
        seed(d2.path());
        let s2 = enabled_state().await;
        ingest_path(&s2, r2, None).await.unwrap();
        edit(d2.path());
        ingest_path(&s2, r2, None).await.unwrap();

        assert_eq!(
            triple_fingerprint(&s1).await,
            triple_fingerprint(&s2).await,
            "incremental and full ingest must produce identical graph state"
        );
    }

    #[tokio::test]
    async fn ingest_missing_path_is_graceful() {
        // Regression: on macOS a post-update restart can race with vault
        // working-copy setup (or an iCloud vault hasn't synced down), so the
        // ingest root does not exist yet. That must NOT take the whole engine
        // down with a fatal "walk error: ... No such file or directory"; it
        // should start empty and let the engine come up.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("vaults").join("deadbeef-not-created");
        assert!(!missing.exists());
        let state = enabled_state().await;

        let report = ingest_path(&state, missing.to_str().unwrap(), None)
            .await
            .expect("ingesting a missing path must not error");

        assert_eq!(report.files_seen, 0);
        assert_eq!(report.files_ingested, 0);
        assert_eq!(report.triples_written, 0);
    }
}
