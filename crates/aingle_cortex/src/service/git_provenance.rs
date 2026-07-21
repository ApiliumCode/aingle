// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Git provenance for ingested state.
//!
//! When the ingest root is a git working tree, each (re-)ingest stamps the
//! branch + commit it ran against into the signed DAG as a `Custom` action.
//! The graph then records *which git state it reflects* — so a caller can answer
//! "what commit is this graph built from?" and, per recorded run, reconstruct
//! the state as of a past commit via ordinary DAG time-travel. Reads `.git`
//! directly; no dependency on the `git` CLI.

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// DAG `Custom` payload type tag for a git-provenance marker.
pub const GIT_PROVENANCE_PAYLOAD_TYPE: &str = "git-provenance";

/// The git ref an ingest ran against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRef {
    /// Branch name (e.g. `main`), or `None` when HEAD is detached.
    pub branch: Option<String>,
    /// Full commit SHA that HEAD resolved to.
    pub commit: String,
}

/// Resolve the git `.git` directory for a working tree at `root`. A regular
/// checkout has a `.git` directory; a linked worktree or submodule has a `.git`
/// *file* containing `gitdir: <path>`.
fn git_dir(root: &str) -> Option<PathBuf> {
    let dot_git = Path::new(root).join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    if dot_git.is_file() {
        let content = std::fs::read_to_string(&dot_git).ok()?;
        let rest = content.lines().next()?.strip_prefix("gitdir:")?.trim();
        let p = Path::new(rest);
        return Some(if p.is_absolute() {
            p.to_path_buf()
        } else {
            Path::new(root).join(rest)
        });
    }
    None
}

fn resolve_packed_ref(git_dir: &Path, ref_path: &str) -> Option<String> {
    let packed = std::fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    for line in packed.lines() {
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        if let Some((sha, name)) = line.split_once(' ') {
            if name.trim() == ref_path {
                return Some(sha.trim().to_string());
            }
        }
    }
    None
}

/// Read the git ref of the working tree at `root` by reading `.git` directly.
///
/// Returns `None` when `root` is not a git working tree or HEAD cannot be
/// resolved. Handles symbolic HEAD (loose ref file, then `packed-refs`) and a
/// detached HEAD (a bare commit SHA).
pub fn read_git_ref(root: &str) -> Option<GitRef> {
    let dir = git_dir(root)?;
    let head = std::fs::read_to_string(dir.join("HEAD")).ok()?;
    let head = head.trim();

    if let Some(ref_path) = head.strip_prefix("ref:") {
        let ref_path = ref_path.trim(); // e.g. "refs/heads/main"
        let branch = ref_path.rsplit('/').next().map(str::to_string);
        let commit = std::fs::read_to_string(dir.join(ref_path))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| resolve_packed_ref(&dir, ref_path))?;
        Some(GitRef { branch, commit })
    } else if head.len() >= 40 && head.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(GitRef {
            branch: None,
            commit: head.to_string(),
        })
    } else {
        None
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
}

/// Stamp a git-provenance marker for an ingest run into the signed DAG.
///
/// No-op (returns `None`) when `root` is not a git working tree or the DAG is
/// unavailable. On success returns the hex hash of the recorded action.
pub async fn record_git_provenance(
    state: &AppState,
    root: &str,
    files_ingested: usize,
) -> Option<String> {
    let git_ref = read_git_ref(root)?;
    let graph = state.graph.read().await;
    let dag_store = graph.dag_store()?;
    let author = state
        .dag_author
        .clone()
        .unwrap_or_else(|| aingle_graph::NodeId::named("node:local"));
    let seq = state
        .dag_seq_counter
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let parents = dag_store.tips().unwrap_or_default();
    let at = chrono::Utc::now();
    let summary = match &git_ref.branch {
        Some(b) => format!("ingest at {b}@{}", short(&git_ref.commit)),
        None => format!("ingest at detached {}", short(&git_ref.commit)),
    };

    let mut action = aingle_graph::dag::DagAction {
        parents,
        author,
        seq,
        timestamp: at,
        payload: aingle_graph::dag::DagPayload::Custom {
            payload_type: GIT_PROVENANCE_PAYLOAD_TYPE.to_string(),
            payload_summary: summary,
            payload: Some(serde_json::json!({
                "branch": git_ref.branch,
                "commit": git_ref.commit,
                "files_ingested": files_ingested,
                "at": at.to_rfc3339(),
            })),
            subject: Some(git_ref.commit.clone()),
        },
        signature: None,
    };

    let sign = |a: &mut aingle_graph::dag::DagAction| {
        if let Some(ref key) = state.dag_signing_key {
            key.sign(a);
        }
    };
    sign(&mut action);

    // Never lose the marker to a transient stale-tip issue: retry parentless.
    if let Err(e) = dag_store.put(&action) {
        tracing::warn!("git-provenance put failed ({e}); retrying without parents");
        action.parents = Vec::new();
        action.signature = None;
        sign(&mut action);
        dag_store.put(&action).ok()?;
    }
    Some(action.compute_hash().to_hex())
}

/// A recorded ingest-run git ref.
#[derive(Debug, Clone, Serialize)]
pub struct GitProvenanceRecord {
    pub branch: Option<String>,
    pub commit: String,
    pub files_ingested: usize,
    /// RFC-3339 timestamp of the ingest run.
    pub at: String,
}

/// List recorded git-provenance markers, newest first — the graph's git history.
pub async fn list_git_provenance(state: &AppState, limit: usize) -> Vec<GitProvenanceRecord> {
    let graph = state.graph.read().await;
    let Some(dag_store) = graph.dag_store() else {
        return vec![];
    };
    let author = state
        .dag_author
        .clone()
        .unwrap_or_else(|| aingle_graph::NodeId::named("node:local"));
    // Over-fetch: the author's chain also holds triple-insert actions.
    let chain = dag_store
        .chain(&author, limit.saturating_mul(8).max(64))
        .unwrap_or_default();
    let mut out: Vec<GitProvenanceRecord> = Vec::new();
    for action in chain {
        let aingle_graph::dag::DagPayload::Custom {
            payload_type,
            payload,
            ..
        } = &action.payload
        else {
            continue;
        };
        if payload_type != GIT_PROVENANCE_PAYLOAD_TYPE {
            continue;
        }
        let p = payload.as_ref();
        let get_str = |k: &str| {
            p.and_then(|v| v.get(k))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        out.push(GitProvenanceRecord {
            branch: get_str("branch"),
            commit: get_str("commit").unwrap_or_default(),
            files_ingested: p
                .and_then(|v| v.get("files_ingested"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize,
            at: get_str("at").unwrap_or_default(),
        });
    }
    // Newest first by ingest timestamp.
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn reads_symbolic_head_from_loose_ref() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join(".git/HEAD"), "ref: refs/heads/dev\n");
        write(
            &root.join(".git/refs/heads/dev"),
            "1234567890abcdef1234567890abcdef12345678\n",
        );
        let r = read_git_ref(root.to_str().unwrap()).unwrap();
        assert_eq!(r.branch.as_deref(), Some("dev"));
        assert_eq!(r.commit, "1234567890abcdef1234567890abcdef12345678");
    }

    #[test]
    fn reads_symbolic_head_from_packed_refs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join(".git/HEAD"), "ref: refs/heads/main\n");
        write(
            &root.join(".git/packed-refs"),
            "# pack-refs with: peeled fully-peeled sorted\nabc1230000000000000000000000000000000000 refs/heads/main\n",
        );
        let r = read_git_ref(root.to_str().unwrap()).unwrap();
        assert_eq!(r.branch.as_deref(), Some("main"));
        assert_eq!(r.commit, "abc1230000000000000000000000000000000000");
    }

    #[test]
    fn reads_detached_head() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join(".git/HEAD"),
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n",
        );
        let r = read_git_ref(root.to_str().unwrap()).unwrap();
        assert_eq!(r.branch, None);
        assert_eq!(r.commit, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    }

    #[test]
    fn non_git_dir_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_git_ref(dir.path().to_str().unwrap()).is_none());
    }

    #[tokio::test]
    async fn records_and_lists_a_git_provenance_marker() {
        let state = crate::state::AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join(".git/HEAD"), "ref: refs/heads/feature-x\n");
        write(
            &root.join(".git/refs/heads/feature-x"),
            "aaaa111100000000000000000000000000000000\n",
        );

        let hash = record_git_provenance(&state, root.to_str().unwrap(), 7).await;
        assert!(hash.is_some(), "a git working tree should record a marker");

        let list = list_git_provenance(&state, 10).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].branch.as_deref(), Some("feature-x"));
        assert_eq!(list[0].commit, "aaaa111100000000000000000000000000000000");
        assert_eq!(list[0].files_ingested, 7);
    }
}
