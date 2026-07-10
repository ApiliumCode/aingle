// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Embedder selection and index-migration helpers for Cortex.
//!
//! Chooses a `NeuralEmbedder` when the `neural-embeddings` feature is on and a
//! model directory is available, else falls back to `HashEmbedder`. Also owns
//! the dimension-sidecar bookkeeping used to detect an embedder change and the
//! registry-clear that forces a re-ingest after one.

use ineru::{Embedder, Embedding, HashEmbedder};
use std::sync::Arc;

/// Builds the active embedder. Returns a `NeuralEmbedder` only when cortex is
/// compiled with `neural-embeddings` AND `model_dir` is `Some` AND the model
/// loads; otherwise a `HashEmbedder`. Never panics — embedding must not be able
/// to take the server down.
pub fn build_embedder(model_dir: Option<&str>) -> Arc<dyn Embedder> {
    #[cfg(feature = "neural-embeddings")]
    if let Some(dir) = model_dir {
        // Retry the neural load: on Windows an app restart (e.g. after creating a
        // vault) can briefly leave the ONNX runtime DLL / model file locked by the
        // exiting process, so the first load fails transiently and the engine would
        // hard-fail with "engine unavailable". A few short backoff retries turn that
        // intermittent failure into a successful load. `build_embedder` runs on a
        // blocking thread, so sleeping here is fine.
        let path = std::path::Path::new(dir);
        let mut last_err = String::new();
        for attempt in 1..=5u32 {
            match ineru::NeuralEmbedder::from_path(path) {
                Ok(e) => {
                    log::info!("Using neural embedder from {dir} (attempt {attempt})");
                    return Arc::new(e);
                }
                Err(e) => {
                    last_err = e.to_string();
                    log::warn!("neural embedder load attempt {attempt}/5 failed: {last_err}");
                    if attempt < 5 {
                        std::thread::sleep(std::time::Duration::from_millis(400 * attempt as u64));
                    }
                }
            }
        }
        log::warn!(
            "Failed to load neural embedder from {dir} after 5 attempts: {last_err}. Using hash embedder."
        );
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
pub fn read_dims(dir: &std::path::Path) -> Option<usize> {
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
pub fn clear_source_registry(graph: &aingle_graph::GraphDB) -> Result<usize, aingle_graph::Error> {
    use aingle_graph::{Predicate, TriplePattern};
    let pattern = TriplePattern::any()
        .with_predicate(Predicate::named(crate::service::ingest::PRED_SOURCE_HASH));
    let ids: Vec<_> = graph.find(pattern)?.into_iter().map(|t| t.id()).collect();
    let mut removed = 0;
    for id in &ids {
        match graph.delete(id) {
            Ok(true) => removed += 1,
            Ok(false) => {} // already gone — fine
            Err(e) => log::warn!("clear_source_registry: delete failed for {id:?}: {e}"),
        }
    }
    Ok(removed)
}

/// An embedder whose inner delegate can be hot-swapped at runtime while its
/// reported dimensionality stays FIXED. Lets a UI start immediately with a
/// "pending" embedder and install the real (slow-to-load) model later WITHOUT
/// ever changing the vector dimension — so a dimension-keyed index (HNSW) stays
/// consistent. Stored vectors must only be produced AFTER a real delegate is
/// installed; the caller gates ingest on readiness.
pub struct SwappableEmbedder {
    inner: std::sync::RwLock<Arc<dyn Embedder>>,
    dims: usize,
}

/// Placeholder delegate before the real model is installed. Returns a zero vector
/// of the fixed dims — harmless for queries (cosine 0 → "ungrounded") and never
/// used for stored passages because ingest is gated on readiness.
struct PendingEmbedder {
    dims: usize,
}

impl Embedder for PendingEmbedder {
    fn embed_passage(&self, _text: &str) -> Embedding {
        Embedding::new(vec![0.0; self.dims])
    }
    fn embed_query(&self, _text: &str) -> Embedding {
        Embedding::new(vec![0.0; self.dims])
    }
    fn dimensions(&self) -> usize {
        self.dims
    }
}

impl SwappableEmbedder {
    /// Creates a swappable embedder in the pending state with a fixed dimension.
    pub fn new_pending(dims: usize) -> Self {
        Self {
            inner: std::sync::RwLock::new(Arc::new(PendingEmbedder { dims })),
            dims,
        }
    }

    /// Installs the real delegate. The delegate MUST report the same dimension
    /// this swappable was created with; a mismatch is logged and ignored so the
    /// index dimension can never change underneath stored vectors.
    pub fn install(&self, delegate: Arc<dyn Embedder>) {
        if delegate.dimensions() != self.dims {
            log::warn!(
                "SwappableEmbedder.install rejected: delegate dims {} != fixed {}",
                delegate.dimensions(),
                self.dims
            );
            return;
        }
        *self.inner.write().expect("swappable embedder poisoned") = delegate;
    }
}

impl Embedder for SwappableEmbedder {
    fn embed_passage(&self, text: &str) -> Embedding {
        let inner = self
            .inner
            .read()
            .expect("swappable embedder poisoned")
            .clone();
        inner.embed_passage(text)
    }
    fn embed_query(&self, text: &str) -> Embedding {
        let inner = self
            .inner
            .read()
            .expect("swappable embedder poisoned")
            .clone();
        inner.embed_query(text)
    }
    fn embed_passages(&self, texts: &[String]) -> Vec<Embedding> {
        // Delegate to the installed model so its batched inference is reached;
        // the default trait loop would re-serialize into per-passage calls.
        let inner = self
            .inner
            .read()
            .expect("swappable embedder poisoned")
            .clone();
        inner.embed_passages(texts)
    }
    fn dimensions(&self) -> usize {
        self.dims
    }
    fn relevance_thresholds(&self) -> (f32, f32) {
        let inner = self
            .inner
            .read()
            .expect("swappable embedder poisoned")
            .clone();
        inner.relevance_thresholds()
    }
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

    #[test]
    fn dims_sidecar_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        write_dims(dir.path(), 384);
        assert_eq!(read_dims(dir.path()), Some(384));
    }

    #[test]
    fn read_dims_absent_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_dims(dir.path()), None);
    }

    #[test]
    fn clear_source_registry_on_empty_graph_is_zero() {
        let graph = aingle_graph::GraphDB::memory().unwrap();
        assert_eq!(clear_source_registry(&graph).unwrap(), 0);
    }

    #[test]
    fn swappable_reports_fixed_dims_before_and_after_install() {
        let s = SwappableEmbedder::new_pending(384);
        assert_eq!(s.dimensions(), 384);
        let q = s.embed_query("hola");
        assert_eq!(q.0.len(), 384);
        assert!(q.0.iter().all(|x| *x == 0.0));
        s.install(std::sync::Arc::new(Fake384));
        assert_eq!(s.dimensions(), 384);
        let q2 = s.embed_query("hola");
        assert_eq!(q2.0.len(), 384);
        assert!(q2.0.iter().any(|x| *x != 0.0));
    }

    #[test]
    fn swappable_rejects_mismatched_dims_install() {
        let s = SwappableEmbedder::new_pending(384);
        s.install(std::sync::Arc::new(ineru::HashEmbedder::new())); // 64d → rejected
        let q = s.embed_query("x");
        assert_eq!(q.0.len(), 384);
        assert!(q.0.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn swappable_delegates_relevance_thresholds_after_install() {
        let s = SwappableEmbedder::new_pending(384);
        s.install(std::sync::Arc::new(Fake384));
        assert_eq!(s.relevance_thresholds(), (0.80, 0.77));
    }

    /// 384-dim test delegate with non-zero output and the e5 thresholds.
    struct Fake384;
    impl ineru::Embedder for Fake384 {
        fn embed_passage(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.5; 384])
        }
        fn embed_query(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.5; 384])
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn relevance_thresholds(&self) -> (f32, f32) {
            (0.80, 0.77)
        }
    }
}
