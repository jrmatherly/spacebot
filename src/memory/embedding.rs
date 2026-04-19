//! Embedding generation via fastembed.
//!
//! Gated on the `embeddings` feature (on by default). When the feature is
//! disabled, `EmbeddingModel` is a stub that returns zero vectors — useful for
//! dev iteration on code paths that don't exercise vector similarity, but
//! degrades hybrid search to keyword-only (Tantivy FTS continues to work).
//! Release builds and CI must keep the feature enabled.

#[cfg(feature = "embeddings")]
mod real;
#[cfg(not(feature = "embeddings"))]
mod stub;

#[cfg(feature = "embeddings")]
pub use real::{EmbeddingModel, embed_text};
#[cfg(not(feature = "embeddings"))]
pub use stub::{EmbeddingModel, embed_text};
