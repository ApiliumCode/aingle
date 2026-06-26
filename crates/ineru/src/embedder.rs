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
    /// Dimensionality of the vectors this embedder produces.
    fn dimensions(&self) -> usize;
}

/// 64-dimensional fallback embedder built on the lexical hash scheme
/// (`Embedding::from_text_simple`). Always available; captures lexical overlap,
/// not meaning. The hash scheme is symmetric, so passage and query embeddings
/// are identical and no prefixes are applied.
#[derive(Debug, Default, Clone, Copy)]
pub struct HashEmbedder;

impl HashEmbedder {
    /// Creates a new `HashEmbedder`.
    pub fn new() -> Self {
        Self
    }
}

impl Embedder for HashEmbedder {
    fn embed_passage(&self, text: &str) -> Embedding {
        Embedding::from_text_simple(text)
    }

    fn embed_query(&self, text: &str) -> Embedding {
        Embedding::from_text_simple(text)
    }

    fn dimensions(&self) -> usize {
        64
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
        let model = UserDefinedEmbeddingModel::new(onnx, tokenizer_files)
            .with_pooling(Pooling::Mean);
        let options = InitOptionsUserDefined::new().with_max_length(512);

        let embedding = TextEmbedding::try_new_from_user_defined(model, options)
            .map_err(|e| crate::Error::Internal(format!("init e5: {e}")))?;

        Ok(Self {
            model: Mutex::new(embedding),
        })
    }

    fn embed_one(&self, prefixed: String) -> Embedding {
        let mut guard = self.model.lock().expect("embedder mutex poisoned");
        let out = guard
            .embed(vec![prefixed], None)
            .expect("e5 embed failed");
        let vector = out
            .into_iter()
            .next()
            .expect("e5 returned empty batch for single-item input");
        Embedding::new(vector)
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

    fn dimensions(&self) -> usize {
        Self::DIM
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
}

#[cfg(all(test, feature = "neural-embeddings"))]
mod neural_tests {
    use super::*;
    use std::path::PathBuf;

    /// Returns the model dir, or `None` (test skips) if it isn't present.
    fn model_dir() -> Option<PathBuf> {
        let dir = std::env::var("INERU_E5_MODEL_DIR").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/test-models/multilingual-e5-small").to_string()
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
        let related =
            e.embed_passage("Los perros necesitan paseos diarios, agua fresca y una dieta equilibrada.");
        let unrelated =
            e.embed_passage("La bolsa de valores cerró hoy con fuertes pérdidas para los inversores.");

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
}
