//! Stub embedding model used when the `embeddings` feature is disabled.
//!
//! Returns deterministic zero vectors at a fixed dimension. This is intended
//! purely as a dev-build escape hatch so contributors working on non-memory
//! code paths can skip the ort-sys C++ toolchain build. Hybrid search
//! degrades to keyword-only (Tantivy FTS continues to work; vector-similarity
//! rank contributions become ties).
//!
//! Production/CI builds must enable the `embeddings` feature.

use crate::error::Result;
use std::path::Path;
use std::sync::Arc;

/// Dimension of the vector the real model produces (all-MiniLM-L6-v2). Kept
/// identical here so LanceDB schema and cosine-similarity code see the same
/// shape regardless of feature state.
const STUB_VECTOR_DIM: usize = 384;

/// Stub embedding model. Has the same public API shape as the real model
/// (see `embedding::real::EmbeddingModel`), but produces zero vectors.
pub struct EmbeddingModel;

impl EmbeddingModel {
    /// Construct a stub model. `_cache_dir` is accepted for API parity; no
    /// files are downloaded or written.
    pub fn new(_cache_dir: &Path) -> Result<Self> {
        tracing::warn!(
            "embeddings feature disabled: vector similarity ranking is stubbed (zero vectors). \
             Hybrid search falls back to keyword-only."
        );
        Ok(Self)
    }

    /// Return a zero vector per input.
    pub fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        Ok(texts.into_iter().map(|_| zero_vec()).collect())
    }

    /// Return a zero vector.
    pub fn embed_one_blocking(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(zero_vec())
    }

    /// Return a zero vector. Not gated on `spawn_blocking` because the stub
    /// does no work.
    pub async fn embed_one(self: &Arc<Self>, _text: &str) -> Result<Vec<f32>> {
        Ok(zero_vec())
    }
}

fn zero_vec() -> Vec<f32> {
    vec![0.0; STUB_VECTOR_DIM]
}

/// Async function to embed text using a shared model.
pub async fn embed_text(model: &Arc<EmbeddingModel>, text: &str) -> Result<Vec<f32>> {
    model.embed_one(text).await
}
