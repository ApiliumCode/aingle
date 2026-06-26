// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Embedder selection and index-migration helpers for Cortex.
//!
//! Chooses a `NeuralEmbedder` when the `neural-embeddings` feature is on and a
//! model directory is available, else falls back to `HashEmbedder`. Also owns
//! the dimension-sidecar bookkeeping used to detect an embedder change and the
//! registry-clear that forces a re-ingest after one.

use ineru::{Embedder, HashEmbedder};
use std::sync::Arc;

/// Builds the active embedder. Returns a `NeuralEmbedder` only when cortex is
/// compiled with `neural-embeddings` AND `model_dir` is `Some` AND the model
/// loads; otherwise a `HashEmbedder`. Never panics — embedding must not be able
/// to take the server down.
pub fn build_embedder(model_dir: Option<&str>) -> Arc<dyn Embedder> {
    #[cfg(feature = "neural-embeddings")]
    if let Some(dir) = model_dir {
        match ineru::NeuralEmbedder::from_path(std::path::Path::new(dir)) {
            Ok(e) => {
                log::info!("Using neural embedder (multilingual-e5-small) from {dir}");
                return Arc::new(e);
            }
            Err(e) => {
                log::warn!("Failed to load neural embedder from {dir}: {e}. Using hash embedder.");
            }
        }
    }
    #[cfg(not(feature = "neural-embeddings"))]
    if model_dir.is_some() {
        log::warn!(
            "--embed-model was set but cortex was built without the `neural-embeddings` \
             feature; using the hash embedder."
        );
    }
    Arc::new(HashEmbedder::new())
}

/// Reads the persisted embedder dimensionality from `<dir>/embedder.dims`.
/// Returns `None` if the sidecar is absent or unparseable.
pub fn read_persisted_dims(dir: &std::path::Path) -> Option<usize> {
    let raw = std::fs::read_to_string(dir.join("embedder.dims")).ok()?;
    raw.trim().parse::<usize>().ok()
}

/// Writes the active embedder dimensionality to `<dir>/embedder.dims`.
pub fn write_dims(dir: &std::path::Path, dims: usize) {
    if let Err(e) = std::fs::write(dir.join("embedder.dims"), dims.to_string()) {
        log::warn!("Failed to write embedder.dims sidecar: {e}");
    }
}

/// Deletes every `aingle:source_hash` registry triple so the next ingest treats
/// all files as new and re-embeds them. Returns the number removed.
pub fn clear_source_registry(graph: &aingle_graph::GraphDB) -> usize {
    use aingle_graph::{Predicate, TriplePattern};
    let pattern =
        TriplePattern::any().with_predicate(Predicate::named(crate::service::ingest::PRED_SOURCE_HASH));
    let ids: Vec<_> = match graph.find(pattern) {
        Ok(ts) => ts.into_iter().map(|t| t.id()).collect(),
        Err(e) => {
            log::warn!("clear_source_registry: graph find failed: {e}");
            return 0;
        }
    };
    let mut removed = 0;
    for id in &ids {
        if matches!(graph.delete(id), Ok(true)) {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_embedder_without_model_is_hash_64d() {
        let e = build_embedder(None);
        assert_eq!(e.dimensions(), 64);
    }

    #[test]
    fn build_embedder_missing_dir_falls_back_to_hash() {
        let e = build_embedder(Some("/nonexistent/model/dir"));
        assert_eq!(e.dimensions(), 64);
    }
}
