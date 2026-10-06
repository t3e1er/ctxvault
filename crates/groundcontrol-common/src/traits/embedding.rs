//! Embedding provider domain capability contract.

use crate::Result;

/// Domain capability contract for dense text embedding.
///
/// This contract abstracts query and batch text encoding while encapsulating
/// neural runtime details (`ort::*`, ONNX execution providers, HuggingFace
/// tokenizers, and hardware acceleration). Callers receive pure `Vec<f32>`
/// embeddings with known dimensionality, enabling clean testing and isolation
/// between the indexing pipeline, search services, and runtime inference.
///
/// # Inherent Adapter Capabilities
///
/// Methods tied directly to ONNX model configuration, tokenization internals,
/// or accelerator hardware (`tokenizer()`, `governor()`, `model_name()`,
/// `average_embeddings`) remain inherent methods on the concrete `Embedder`
/// in `groundcontrol-core` to prevent leaking accelerator or tokenizer types
/// into `groundcontrol-common`.
pub trait EmbeddingProvider {
    /// Embed a search query string into a single dense vector.
    ///
    /// Returns one L2-normalized embedding vector for the query.
    fn embed_query(&self, query: &str) -> Result<Vec<f32>>;

    /// Embed a batch of text strings into dense vectors.
    ///
    /// Returns one L2-normalized embedding vector per input string, in the exact
    /// order of `texts`.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Get the output dimensionality of the embeddings this provider produces.
    fn dimensions(&self) -> usize;
}
