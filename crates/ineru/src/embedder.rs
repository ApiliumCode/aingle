//! Text-to-embedding strategies.
//!
//! [`Embedder`] is the unit callers own and inject. Implementations may hold a
//! loaded model (stateful) and may block, so embedding is *not* baked into data
//! structures like `MemoryQuery`.

use crate::types::Embedding;

/// Produces semantic embeddings for text.
///
/// `embed_passage` is for documents/chunks that get stored and searched against;
/// `embed_query` is for search queries. They are distinct because some models
/// (e.g. the E5 family) are trained with asymmetric prefixes, so the right one
/// must be applied at each call site.
pub trait Embedder: Send + Sync {
    /// Embed a document/chunk to be stored and searched against.
    fn embed_passage(&self, text: &str) -> Embedding;
    /// Embed a search query.
    fn embed_query(&self, text: &str) -> Embedding;
    /// Embed many passages at once, returning one embedding per input in order.
    ///
    /// The default loops over [`embed_passage`], which is correct for any
    /// implementation. Model-based embedders (ONNX) override this to run a SINGLE
    /// batched inference instead of one call per passage — the dominant cost when
    /// indexing, since per-call model overhead otherwise repeats for every chunk.
    fn embed_passages(&self, texts: &[String]) -> Vec<Embedding> {
        texts.iter().map(|t| self.embed_passage(t)).collect()
    }
    /// Dimensionality of the vectors this embedder produces.
    fn dimensions(&self) -> usize;
    /// `(strong, weak)` cosine-similarity cutoffs for this embedder's score
    /// distribution: at/above `strong` a match corroborates; below `weak` it is
    /// noise. The default suits the lexical-hash scale; model-based embedders
    /// override it.
    fn relevance_thresholds(&self) -> (f32, f32) {
        (0.55, 0.30)
    }

    /// A stable fingerprint of this embedder's vector space: the model identity
    /// AND its dimensionality. Two embedders share an identity **iff** vectors
    /// one produced are directly comparable (same cosine geometry) with the
    /// other's — so a persisted index is only valid while the active embedder's
    /// identity is unchanged.
    ///
    /// Dimensionality alone is not enough: two different models (or a
    /// not-yet-loaded placeholder and the real model) can share a dimension yet
    /// live in different vector spaces, which silently poisons retrieval. Callers
    /// persist this string and re-embed when it changes. The default keys on the
    /// dimension only; every real embedder overrides it with a model-specific tag.
    fn identity(&self) -> String {
        format!("emb-dim-{}", self.dimensions())
    }
}

/// Lexical hash embedder (`Embedding::from_text_simple*`). Always available;
/// captures lexical overlap, not meaning. The hash scheme is symmetric, so
/// passage and query embeddings are identical and no prefixes are applied.
///
/// Defaults to 64 dimensions (the historical scheme, byte-compatible with every
/// existing hash index). [`Self::with_dimensions`] builds one at any other
/// dimension — the degraded-mode stand-in for a neural model that failed to
/// load, matching the index shape so the engine keeps working lexically.
#[derive(Debug, Clone, Copy)]
pub struct HashEmbedder {
    dims: usize,
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl HashEmbedder {
    /// Creates the historical 64-dimensional `HashEmbedder`.
    pub fn new() -> Self {
        Self { dims: 64 }
    }

    /// Creates a `HashEmbedder` at an arbitrary dimension (e.g. 384 to stand in
    /// for a neural model at the same index shape). `64` is byte-compatible
    /// with [`Self::new`].
    pub fn with_dimensions(dims: usize) -> Self {
        Self { dims }
    }
}

impl Embedder for HashEmbedder {
    fn embed_passage(&self, text: &str) -> Embedding {
        Embedding::from_text_simple_dims(text, self.dims)
    }

    fn embed_query(&self, text: &str) -> Embedding {
        Embedding::from_text_simple_dims(text, self.dims)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn identity(&self) -> String {
        format!("hash-lexical-{}", self.dims)
    }
}

#[cfg(feature = "neural-embeddings")]
use std::path::Path;
#[cfg(feature = "neural-embeddings")]
use std::sync::Mutex;

#[cfg(feature = "neural-embeddings")]
use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};

/// Real neural embedder: multilingual-e5-small (384-dim) via fastembed/ONNX,
/// loaded entirely from a local directory (no network). E5 is trained with
/// asymmetric prefixes, so `embed_query` prepends `"query: "` and
/// `embed_passage` prepends `"passage: "`.
///
/// fastembed's `embed` takes `&mut self`, so the model is held behind a `Mutex`
/// to satisfy the `&self` trait methods while staying `Send + Sync`.
/// Concurrent callers serialize through this lock for the duration of inference.
#[cfg(feature = "neural-embeddings")]
pub struct NeuralEmbedder {
    model: Mutex<TextEmbedding>,
}

#[cfg(feature = "neural-embeddings")]
impl NeuralEmbedder {
    /// Output dimensionality of multilingual-e5-small.
    const DIM: usize = 384;

    /// Loads the model from a directory containing `onnx/model.onnx`,
    /// `tokenizer.json`, `config.json`, `special_tokens_map.json`, and
    /// `tokenizer_config.json`. Returns an error (never panics) if any file is
    /// missing or the model fails to initialize, so callers can fall back.
    pub fn from_path(dir: &Path) -> crate::Result<Self> {
        let read = |name: &str| -> crate::Result<Vec<u8>> {
            std::fs::read(dir.join(name))
                .map_err(|e| crate::Error::Storage(format!("reading {name}: {e}")))
        };

        let onnx = read("onnx/model.onnx")?;
        let tokenizer_files = TokenizerFiles {
            tokenizer_file: read("tokenizer.json")?,
            config_file: read("config.json")?,
            special_tokens_map_file: read("special_tokens_map.json")?,
            tokenizer_config_file: read("tokenizer_config.json")?,
        };

        // E5 REQUIRES mean pooling; the fastembed default is Cls.
        let model =
            UserDefinedEmbeddingModel::new(onnx, tokenizer_files).with_pooling(Pooling::Mean);
        // Cap ONNX intra-op threads at half the logical cores (min 2, max 8).
        // The default saturates EVERY core, which turns a first-run bulk index
        // into a machine-freezing event on an average laptop; half the logical
        // cores approximates the physical-core count, so throughput stays close
        // to peak while the host keeps enough headroom to remain responsive.
        // Beyond 8 threads this model's batches gain little anyway.
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() / 2)
            .unwrap_or(2)
            .clamp(2, 8);
        let options = InitOptionsUserDefined::new()
            .with_max_length(512)
            .with_intra_threads(threads);

        let embedding = TextEmbedding::try_new_from_user_defined(model, options)
            .map_err(|e| crate::Error::Internal(format!("init e5: {e}")))?;

        Ok(Self {
            model: Mutex::new(embedding),
        })
    }

    /// Locks the model, recovering from a poisoned mutex instead of panicking.
    ///
    /// A prior embed that panicked (an ONNX runtime fault, a pathological input)
    /// would poison the mutex; a plain `.lock().unwrap()` then panics on EVERY
    /// subsequent call, permanently bricking the embedder for the whole process
    /// and, at startup, silently aborting the background boot task so the splash
    /// hangs forever. The `TextEmbedding` behind the lock has no cross-call
    /// invariant a panic could leave half-updated (each `embed` is standalone), so
    /// taking the guard via `into_inner` is safe and keeps the engine alive.
    fn lock_model(&self) -> std::sync::MutexGuard<'_, TextEmbedding> {
        self.model.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Embed a single prefixed string. NEVER panics: an embed failure or an empty
    /// result yields a zero vector (which scores ~0 cosine, so that one query just
    /// returns nothing) rather than taking the process — or the whole session's
    /// embedder — down. This is the single-item sibling of the batch path, which
    /// is already fault-tolerant; the two must fail the same soft way.
    fn embed_one(&self, prefixed: String) -> Embedding {
        let mut guard = self.lock_model();
        match guard.embed(vec![prefixed], None) {
            Ok(mut out) if !out.is_empty() => Embedding::new(out.remove(0)),
            Ok(_) => {
                log::warn!("e5 returned empty batch for single-item input; using zero vector");
                Embedding::new(vec![0.0; Self::DIM])
            }
            Err(e) => {
                log::warn!("e5 embed failed ({e}); using zero vector");
                Embedding::new(vec![0.0; Self::DIM])
            }
        }
    }

    /// Number of passages embedded per ONNX invocation, and per lock hold.
    ///
    /// The batch size bounds BOTH peak memory and lock latency, and it must:
    /// an unbounded batch sizes the ONNX tensors (and the runtime's retained
    /// memory arena) by the largest document ever ingested — a single
    /// multi-megabyte note produced multi-gigabyte arenas and one model-lock
    /// hold of many minutes, freezing interactive queries for the duration.
    /// Sub-batching keeps the arena flat, reduces per-batch padding waste
    /// (fastembed pads every text to the longest in its batch), and releases
    /// the lock between sub-batches so queries interleave with a long index.
    const EMBED_BATCH: usize = 32;

    /// Embed a batch in bounded sub-batches ([`Self::EMBED_BATCH`]), falling
    /// back to per-item embedding for any sub-batch that fails.
    ///
    /// The fallback matters most on a first-run full index: a batched ONNX call
    /// can fail on some runtimes/hardware (or a pathological input), and that must
    /// NEVER sink the whole index — nor the rest of the batch. Worst case a failing
    /// sub-batch is exactly as correct, and as slow, as the proven per-item path.
    /// Never panics on an embed error.
    fn embed_prefixed_batch(&self, prefixed: Vec<String>) -> Vec<Embedding> {
        if prefixed.is_empty() {
            return Vec::new();
        }
        let mut out_all = Vec::with_capacity(prefixed.len());
        for chunk in prefixed.chunks(Self::EMBED_BATCH) {
            let batched = {
                let mut guard = self.lock_model();
                match guard.embed(chunk.to_vec(), Some(Self::EMBED_BATCH)) {
                    Ok(out) if out.len() == chunk.len() => {
                        out_all.extend(out.into_iter().map(Embedding::new));
                        true
                    }
                    Ok(out) => {
                        log::warn!(
                            "batch embed returned {} vectors for {} inputs; using per-item fallback",
                            out.len(),
                            chunk.len()
                        );
                        false
                    }
                    Err(e) => {
                        log::warn!("batch embed failed ({e}); using per-item fallback");
                        false
                    }
                }
            }; // drop the model lock between sub-batches (and before the fallback)
            if !batched {
                out_all.extend(chunk.iter().map(|p| self.embed_one_fallback(p)));
            }
        }
        out_all
    }

    /// Per-item fallback for a failed sub-batch: embeds one passage, yielding a
    /// zero vector on error so one bad input can't lose the rest. Re-locks per
    /// call so interactive queries interleave.
    fn embed_one_fallback(&self, p: &str) -> Embedding {
        let mut guard = self.lock_model();
        match guard.embed(vec![p.to_string()], None) {
            Ok(mut out) if !out.is_empty() => Embedding::new(out.remove(0)),
            Ok(_) => Embedding::new(vec![0.0; Self::DIM]),
            Err(e) => {
                log::warn!("per-item embed failed ({e}); storing zero vector");
                Embedding::new(vec![0.0; Self::DIM])
            }
        }
    }
}

#[cfg(feature = "neural-embeddings")]
impl Embedder for NeuralEmbedder {
    fn embed_passage(&self, text: &str) -> Embedding {
        self.embed_one(format!("passage: {text}"))
    }

    fn embed_query(&self, text: &str) -> Embedding {
        self.embed_one(format!("query: {text}"))
    }

    fn embed_passages(&self, texts: &[String]) -> Vec<Embedding> {
        let prefixed = texts.iter().map(|t| format!("passage: {t}")).collect();
        self.embed_prefixed_batch(prefixed)
    }

    fn dimensions(&self) -> usize {
        Self::DIM
    }

    fn relevance_thresholds(&self) -> (f32, f32) {
        (0.80, 0.77)
    }

    fn identity(&self) -> String {
        // Model-specific tag: bump this whenever the model (or its vector space)
        // changes so persisted indices re-embed. multilingual-e5-small, 384-dim.
        format!("neural-e5-small-{}", Self::DIM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_embedder_has_64_dimensions() {
        let e = HashEmbedder::new();
        assert_eq!(e.dimensions(), 64);
    }

    #[test]
    fn hash_embedder_produces_64_dim_vectors() {
        let e = HashEmbedder::new();
        let p = e.embed_passage("the quick brown fox");
        let q = e.embed_query("the quick brown fox");
        assert_eq!(p.0.len(), 64);
        assert_eq!(q.0.len(), 64);
    }

    #[test]
    fn hash_embedder_is_deterministic() {
        let e = HashEmbedder::new();
        let a = e.embed_passage("hello world");
        let b = e.embed_passage("hello world");
        assert_eq!(a.0, b.0);
    }

    #[test]
    fn hash_embedder_passage_and_query_are_identical() {
        let e = HashEmbedder::new();
        let p = e.embed_passage("test input");
        let q = e.embed_query("test input");
        assert_eq!(p.0, q.0);
    }

    #[test]
    fn hash_embedder_relevance_thresholds() {
        let e = HashEmbedder::new();
        assert_eq!(e.relevance_thresholds(), (0.55, 0.30));
    }

    #[test]
    fn embed_passages_default_matches_per_item_and_preserves_order() {
        let e = HashEmbedder::new();
        let texts = vec![
            "first passage".to_string(),
            "second passage".to_string(),
            "third passage".to_string(),
        ];
        let batch = e.embed_passages(&texts);
        assert_eq!(batch.len(), texts.len(), "one embedding per input");
        for (i, t) in texts.iter().enumerate() {
            assert_eq!(batch[i].0, e.embed_passage(t).0, "batch[{i}] must equal the per-item embedding");
        }
    }

    #[test]
    fn embed_passages_empty_is_empty() {
        let e = HashEmbedder::new();
        assert!(e.embed_passages(&[]).is_empty());
    }

    #[test]
    fn hash_embedder_with_dimensions_matches_requested_shape() {
        // Degraded-mode contract: a hash embedder can stand in for a neural
        // model at the SAME index dimension (384), deterministically, with a
        // distinct identity so the identity-reconcile re-embeds when the real
        // model comes back.
        let e = HashEmbedder::with_dimensions(384);
        assert_eq!(e.dimensions(), 384);
        assert_eq!(e.identity(), "hash-lexical-384");
        let v = e.embed_passage("semantic fallback at neural shape");
        assert_eq!(v.0.len(), 384, "vector must match the requested dims");
        let v2 = e.embed_passage("semantic fallback at neural shape");
        assert_eq!(v.0, v2.0, "must be deterministic");
        // Not all-zero (a zero vector would silently ground nothing).
        assert!(v.0.iter().any(|x| *x != 0.0), "vector must carry signal");
    }

    #[test]
    fn hash_embedder_64_stays_byte_compatible() {
        // The historical 64-dim scheme must not change: existing persisted hash
        // indexes were built with it, and identity "hash-lexical-64" stays the
        // same, so different vector values would poison cosine scores silently.
        let legacy = HashEmbedder::new().embed_passage("compatibility check");
        let via_dims = HashEmbedder::with_dimensions(64).embed_passage("compatibility check");
        assert_eq!(legacy.0, via_dims.0, "64-dim path must stay byte-identical");
        assert_eq!(
            HashEmbedder::with_dimensions(64).identity(),
            HashEmbedder::new().identity()
        );
    }

    #[test]
    fn poisoned_mutex_recovers_instead_of_panicking() {
        // Regression: a prior embed that panicked used to poison the model mutex,
        // so every later `.lock().unwrap()` panicked forever — bricking the
        // embedder and, at startup, silently killing the boot task so the splash
        // hung. `lock_model` uses exactly this `into_inner` recovery; prove the
        // pattern actually recovers a poisoned lock and yields the value.
        use std::sync::{Arc, Mutex};
        let m = Arc::new(Mutex::new(7));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("poison the lock");
        })
        .join();
        assert!(m.lock().is_err(), "lock should be poisoned after the panic");
        let guard = m.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(*guard, 7, "recovered guard must expose the protected value");
    }

    #[cfg(feature = "neural-embeddings")]
    #[test]
    fn neural_batch_crossing_subbatch_boundary_preserves_order_and_count() {
        // Regression for the bounded sub-batching: a batch larger than
        // EMBED_BATCH must come back complete and in order across sub-batch
        // boundaries (an unbounded batch used to size the ONNX arena by the
        // largest document — multi-GB for a multi-MB note). Skips without a model.
        use std::path::PathBuf;
        let dir = std::env::var("INERU_E5_MODEL_DIR").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/test-models/multilingual-e5-small").to_string()
        });
        let p = PathBuf::from(dir);
        if !p.join("onnx/model.onnx").exists() {
            eprintln!("skipping: no e5 model at {}", p.display());
            return;
        }
        let e = NeuralEmbedder::from_path(&p).expect("load model");
        let n = NeuralEmbedder::EMBED_BATCH * 2 + 5; // forces 3 sub-batches
        let texts: Vec<String> = (0..n).map(|i| format!("passage number {i}")).collect();
        let batch = e.embed_passages(&texts);
        assert_eq!(batch.len(), n, "one embedding per input across sub-batches");
        // Order pinned: items straddling the sub-batch boundaries must equal
        // their individually-embedded versions.
        for &i in &[0usize, NeuralEmbedder::EMBED_BATCH - 1, NeuralEmbedder::EMBED_BATCH, n - 1] {
            let single = e.embed_passage(&texts[i]);
            assert_eq!(batch[i].0, single.0, "batch[{i}] must match per-item embedding");
        }
    }

    #[cfg(feature = "neural-embeddings")]
    #[test]
    fn neural_embed_query_never_panics_after_mutex_poison() {
        // A faulting query must not brick the model for the rest of the session.
        // Poison the model mutex, then confirm `embed_query` still returns a
        // (dimension-correct) vector via `lock_model`'s recovery instead of
        // panicking. Skips when no model is present (CI without the ONNX asset).
        use std::path::PathBuf;
        let dir = std::env::var("INERU_E5_MODEL_DIR").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/test-models/multilingual-e5-small").to_string()
        });
        let p = PathBuf::from(dir);
        if !p.join("onnx/model.onnx").exists() {
            eprintln!("skipping: no e5 model at {}", p.display());
            return;
        }
        let e = NeuralEmbedder::from_path(&p).expect("load model");
        // Poison the model mutex the same way a panicking embed would.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = e.model.lock().unwrap();
            panic!("simulate an embed panic under the lock");
        }));
        assert!(e.model.lock().is_err(), "model mutex should be poisoned");
        // The whole point: no panic here, and a usable vector comes back.
        let v = e.embed_query("does this still work after a poison?");
        assert_eq!(v.0.len(), NeuralEmbedder::DIM, "query vector keeps model dim");
    }

    #[cfg(feature = "neural-embeddings")]
    #[test]
    fn diag_raw_model_supports_multi_item_batch() {
        // Diagnostic: does the bundled ONNX model actually accept a batch > 1?
        // Bypasses the resilient fallback and calls the raw model directly, so a
        // panic/Err here means the fast path is unavailable and indexing runs on
        // the per-item fallback. Skips when no model is present.
        use std::path::PathBuf;
        let dir = std::env::var("INERU_E5_MODEL_DIR").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/test-models/multilingual-e5-small").to_string()
        });
        let p = PathBuf::from(dir);
        if !p.join("onnx/model.onnx").exists() {
            eprintln!("skipping: model files not found at {}", p.display());
            return;
        }
        let e = NeuralEmbedder::from_path(&p).expect("load model");
        let inputs = vec![
            "passage: primero".to_string(),
            "passage: segundo".to_string(),
            "passage: tercero".to_string(),
        ];
        let mut guard = e.model.lock().expect("mutex");
        let out = guard
            .embed(inputs.clone(), None)
            .expect("RAW BATCH FAILED — model does not support batch>1; fast path unavailable");
        assert_eq!(
            out.len(),
            inputs.len(),
            "raw batch returned {} vectors for {} inputs",
            out.len(),
            inputs.len()
        );
        eprintln!("DIAG: raw multi-item batch OK — fast path available");
    }
}

#[cfg(all(test, feature = "neural-embeddings"))]
mod neural_tests {
    use super::*;
    use std::path::PathBuf;

    /// Returns the model dir, or `None` (test skips) if it isn't present.
    fn model_dir() -> Option<PathBuf> {
        let dir = std::env::var("INERU_E5_MODEL_DIR").unwrap_or_else(|_| {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/test-models/multilingual-e5-small"
            )
            .to_string()
        });
        let p = PathBuf::from(dir);
        if p.join("onnx/model.onnx").exists() {
            Some(p)
        } else {
            eprintln!("skipping: model files not found at {}", p.display());
            None
        }
    }

    #[test]
    fn neural_embedder_reports_384_dimensions() {
        let Some(dir) = model_dir() else { return };
        let e = NeuralEmbedder::from_path(&dir).expect("load model");
        assert_eq!(e.dimensions(), 384);
    }

    #[test]
    fn neural_embedder_produces_384_dim_vectors() {
        let Some(dir) = model_dir() else { return };
        let e = NeuralEmbedder::from_path(&dir).expect("load model");
        let v = e.embed_passage("el perro corre en el parque");
        assert_eq!(v.0.len(), 384);
        assert!(v.0.iter().any(|x| *x != 0.0));
    }

    #[test]
    fn neural_embedder_captures_semantic_similarity() {
        let Some(dir) = model_dir() else { return };
        let e = NeuralEmbedder::from_path(&dir).expect("load model");

        // E5 is trained for sentence/passage retrieval, which is exactly how this
        // embedder is used (queries = questions, chunks = sentences). Isolated
        // single words cluster too tightly to test meaningfully; realistic
        // sentence-level inputs produce a clear semantic margin.
        let query = e.embed_query("¿Cómo debo cuidar a mi perro?");
        let related = e.embed_passage(
            "Los perros necesitan paseos diarios, agua fresca y una dieta equilibrada.",
        );
        let unrelated = e.embed_passage(
            "La bolsa de valores cerró hoy con fuertes pérdidas para los inversores.",
        );

        let near = query.cosine_similarity(&related);
        let far = query.cosine_similarity(&unrelated);

        // A real model ranks the dog-care passage above the stock-market one for a
        // dog-care question. The 64-dim hash embedder cannot.
        assert!(
            near > far,
            "expected sim(query,related)={near} > sim(query,unrelated)={far}"
        );
    }

    #[test]
    fn neural_embedder_applies_distinct_prefixes() {
        let Some(dir) = model_dir() else { return };
        let e = NeuralEmbedder::from_path(&dir).expect("load model");

        // Same raw text, different prefixes → different vectors.
        let as_query = e.embed_query("documento");
        let as_passage = e.embed_passage("documento");
        assert_ne!(as_query.0, as_passage.0);
    }

    #[test]
    fn neural_embedder_relevance_thresholds() {
        // Calibrated to multilingual-e5-small's anisotropic cosine scale:
        // unrelated sentence pairs ceil ~0.76, related floor ~0.81.
        let Some(dir) = model_dir() else { return };
        let e = NeuralEmbedder::from_path(&dir).expect("load model");
        assert_eq!(e.relevance_thresholds(), (0.80, 0.77));
    }

    #[test]
    fn neural_batch_embeddings_match_per_item() {
        // The batched path (one ONNX call for N passages) must produce the SAME
        // vectors as N single calls — otherwise indexing speed would come at the
        // cost of retrieval correctness.
        let Some(dir) = model_dir() else { return };
        let e = NeuralEmbedder::from_path(&dir).expect("load model");
        let texts = vec![
            "el perro corre en el parque".to_string(),
            "la bolsa cerró con pérdidas".to_string(),
            "los gatos duermen mucho".to_string(),
        ];
        let batch = e.embed_passages(&texts);
        assert_eq!(batch.len(), texts.len());
        for (i, t) in texts.iter().enumerate() {
            let single = e.embed_passage(t);
            let sim = batch[i].cosine_similarity(&single);
            assert!(
                sim > 0.9999,
                "batch[{i}] must match per-item embedding (cosine {sim})"
            );
        }
    }
}
