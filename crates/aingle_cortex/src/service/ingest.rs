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
use ineru::{Embedding, MemoryEntry, MemoryMetadata};

// Bring the graph error type into scope for duplicate-matching in ingest logic.
use aingle_graph::Error as GraphError;

/// The predicate used to anchor the per-file content-hash registry triple.
pub const PRED_SOURCE_HASH: &str = "aingle:source_hash";

/// Ineru `entry_type` used for ingested text chunks. Grounding filters on this.
pub const CHUNK_ENTRY_TYPE: &str = "doc_chunk";

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
    let mut report = IngestReport::default();

    // Build a walk that respects .gitignore / .ignore files
    let walker = ignore::WalkBuilder::new(root_path)
        .hidden(false)
        .git_ignore(true)
        .build();

    let mut files: Vec<(String, String)> = Vec::new(); // (rel_path, content)

    for entry in walker {
        let entry = entry.map_err(|e| Error::Internal(format!("walk error: {e}")))?;
        let path = entry.path();

        // Skip directories
        if !path.is_file() {
            continue;
        }

        // Filter to supported extensions: .md, .markdown, .txt, .rs, .py, .ts, .js
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !matches!(
            ext.as_str(),
            "md" | "markdown" | "txt" | "rs" | "py" | "ts" | "js" | "toml" | "json"
        ) {
            continue;
        }

        report.files_seen += 1;

        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Internal(format!("read {}: {e}", path.display())))?;

        // Compute relative path from root_path for use as the note subject
        let rel_path = path
            .strip_prefix(root_path)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        files.push((rel_path, content));
    }

    for (rel_path, content) in files {
        let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();

        // Check registry: does a triple (rel_path, aingle:source_hash, <hash>) already exist?
        let existing_hash = {
            let graph = state.graph.read().await;
            let pattern = TriplePattern::any()
                .with_subject(NodeId::named(&rel_path))
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
                continue;
            }
        }

        report.files_ingested += 1;

        // Remove old registry triple if the hash changed
        if existing_hash.is_some() {
            // Delete by finding the triple's hex id
            let old_triple_id = {
                let graph = state.graph.read().await;
                let pattern = TriplePattern::any()
                    .with_subject(NodeId::named(&rel_path))
                    .with_predicate(Predicate::named(PRED_SOURCE_HASH));
                graph
                    .find(pattern)
                    .ok()
                    .and_then(|v| v.into_iter().next())
                    .map(|t| t.id().to_hex())
            };
            if let Some(hex_id) = old_triple_id {
                // Best-effort: ignore NotFound
                let _ = delete_triple(state, &hex_id, namespace.clone()).await;
            }
        }

        // Extract triples and chunks from the file
        let extraction = extract(&rel_path, &content);

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

        // Write text chunks to Ineru memory
        for chunk in &extraction.chunks {
            let embedding = Embedding::from_text_simple(&chunk.text);
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

            let mut mem = state.memory.write().await;
            mem.remember(entry)
                .map_err(|e| Error::Internal(format!("memory write error: {e}")))?;
            report.chunks_written += 1;
        }

        // Write/update the source-hash registry triple
        #[cfg(feature = "dag")]
        let registry_prov = Some(aingle_graph::dag::Provenance {
            source_path: rel_path.clone(),
            line_start: 0,
            line_end: 0,
            content_hash: content_hash.clone(),
        });
        #[cfg(not(feature = "dag"))]
        let _registry_prov = ();

        insert_triple_inner(
            state,
            ValueDto::String(content_hash.clone()),
            &rel_path,
            PRED_SOURCE_HASH,
            #[cfg(feature = "dag")]
            registry_prov,
            #[cfg(not(feature = "dag"))]
            None,
            namespace.clone(),
        )
        .await
        .map_err(|e| Error::Internal(format!("registry triple insert error: {e}")))?;

        report.triples_written += 1;

        report.sources.push(SourceRecord {
            path: rel_path.clone(),
            content_hash: content_hash.clone(),
        });
    }

    Ok(report)
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
            t.object_string().map(|h| SourceRecord {
                path: t.subject.to_string(),
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
        write(dir.path(), "note.md", "# Title\n\nWe use [[sled]] for storage. #durability\n");
        let state = enabled_state().await;

        let report = ingest_path(&state, dir.path().to_str().unwrap(), None).await.unwrap();

        assert_eq!(report.files_seen, 1);
        assert_eq!(report.files_ingested, 1);
        assert!(report.triples_written >= 3); // heading + links_to + tagged + registry
        assert!(report.chunks_written >= 1);

        let mem = state.memory.read().await;
        let hits = mem.recall_text("sled storage").unwrap();
        assert!(!hits.is_empty());
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
        assert_eq!(actions_after_first, actions_after_second,
            "re-ingesting unchanged files must write zero new DAG actions");
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
}
