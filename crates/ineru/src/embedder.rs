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
}
