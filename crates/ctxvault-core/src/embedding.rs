//! Embedding generation via fastembed-rs.
//!
//! Provides local ONNX-based inference for generating text embeddings
//! using models like MiniLM-L6-v2, nomic-embed-text, or BGE-small.

use ctxvault_common::{Error, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

/// Supported embedding model names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelName {
    /// jinaai/jina-embeddings-v2-base-code (768 dimensions, 8192 token window, code + NL).
    JinaEmbeddingsV2BaseCode,
    /// BGE-small-en-v1.5 (384 dimensions, fast general-purpose).
    BgeSmallEnV15,
    /// all-MiniLM-L6-v2 (384 dimensions).
    AllMiniLmL6V2,
}

impl ModelName {
    /// Get the fastembed model enum variant.
    fn to_fastembed_model(&self) -> EmbeddingModel {
        match self {
            Self::JinaEmbeddingsV2BaseCode => EmbeddingModel::JinaEmbeddingsV2BaseCode,
            Self::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
            Self::AllMiniLmL6V2 => EmbeddingModel::AllMiniLML6V2,
        }
    }

    /// Get output dimensions for this model.
    pub fn dimensions(&self) -> usize {
        match self {
            Self::JinaEmbeddingsV2BaseCode => 768,
            Self::BgeSmallEnV15 => 384,
            Self::AllMiniLmL6V2 => 384,
        }
    }

    /// Parse a model name string into a `ModelName`.
    ///
    /// Accepts both short names and full identifiers:
    /// - "jinaai/jina-embeddings-v2-base-code", "jina-embeddings-v2-base-code", "jina-code", "jina"
    /// - "BAAI/bge-small-en-v1.5", "bge-small-en-v1.5", "bge-small", "bge"
    /// - "Qdrant/all-MiniLM-L6-v2", "all-minilm-l6-v2", "minilm", "all-minilm"
    pub fn from_str_name(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        // Strip org prefix if present (e.g., "jinaai/jina-embeddings-v2-base-code" -> "jina-embeddings-v2-base-code")
        let name = if let Some(idx) = lower.find('/') { &lower[idx + 1..] } else { &lower };
        match name {
            "jina-embeddings-v2-base-code" | "jina-code" | "jina" => {
                Some(Self::JinaEmbeddingsV2BaseCode)
            }
            "all-minilm-l6-v2" | "minilm" | "all-minilm" => Some(Self::AllMiniLmL6V2),
            "bge-small-en-v1.5" | "bge-small" | "bge" => Some(Self::BgeSmallEnV15),
            _ => {
                // Try matching against the full lowercased string as well
                match lower.as_str() {
                    s if s.contains("jina") => Some(Self::JinaEmbeddingsV2BaseCode),
                    s if s.contains("minilm") => Some(Self::AllMiniLmL6V2),
                    s if s.contains("bge-small") => Some(Self::BgeSmallEnV15),
                    _ => None,
                }
            }
        }
    }

    /// Get the canonical version string for this model.
    pub fn version_string(&self) -> &'static str {
        match self {
            Self::JinaEmbeddingsV2BaseCode => "jina-embeddings-v2-base-code",
            Self::BgeSmallEnV15 => "bge-small-en-v1.5",
            Self::AllMiniLmL6V2 => "all-minilm-l6-v2",
        }
    }
}

impl Default for ModelName {
    fn default() -> Self {
        Self::JinaEmbeddingsV2BaseCode
    }
}

/// Embedder wraps fastembed's `TextEmbedding` for batch embedding generation.
pub struct Embedder {
    model: TextEmbedding,
    model_name: ModelName,
}

impl Embedder {
    /// Create a new embedder with the specified model.
    ///
    /// This downloads the model on first use (cached in the fastembed cache dir).
    pub fn new(model_name: ModelName) -> Result<Self> {
        let options =
            InitOptions::new(model_name.to_fastembed_model()).with_show_download_progress(false);

        let model = TextEmbedding::try_new(options)
            .map_err(|e| Error::Index(format!("failed to initialize embedding model: {}", e)))?;

        Ok(Self { model, model_name })
    }

    /// Create an embedder from a config model string (e.g., "BAAI/bge-small-en-v1.5").
    ///
    /// Falls back to the default model if the string is not recognized.
    pub fn from_config(model_str: &str) -> Result<Self> {
        let model_name = ModelName::from_str_name(model_str).unwrap_or_default();
        Self::new(model_name)
    }

    /// Create an embedder with the default model (BGE-small-en-v1.5).
    pub fn new_default() -> Result<Self> {
        Self::new(ModelName::default())
    }

    /// Get the output dimensions of this embedder.
    pub fn dimensions(&self) -> usize {
        self.model_name.dimensions()
    }

    /// Get the model name.
    pub fn model_name(&self) -> &ModelName {
        &self.model_name
    }

    /// Embed a batch of text strings.
    ///
    /// Returns one embedding vector per input string.
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        let embeddings = self
            .model
            .embed(texts_owned, None)
            .map_err(|e| Error::Index(format!("embedding failed: {}", e)))?;

        Ok(embeddings)
    }

    /// Embed a single text string.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| Error::Index("embedding returned no results".to_string()))
    }

    /// Embed a search query string, prepending the model-specific instruction prompt if required.
    ///
    /// BGE models (`BAAI/bge-small-en-v1.5`) require an asymmetric query prompt prefix:
    /// `"Represent this sentence for searching relevant passages: <query>"`
    /// Passages and document chunks continue to be embedded without this prefix.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let text = match self.model_name {
            ModelName::BgeSmallEnV15 => {
                format!("Represent this sentence for searching relevant passages: {}", query)
            }
            _ => query.to_string(),
        };
        self.embed(&text)
    }

    /// Compute a document-level embedding by averaging chunk embeddings.
    ///
    /// Takes pre-computed chunk embeddings and returns their L2-normalized mean.
    pub fn average_embeddings(embeddings: &[Vec<f32>]) -> Option<Vec<f32>> {
        if embeddings.is_empty() {
            return None;
        }

        let dims = embeddings[0].len();
        let count = embeddings.len() as f32;

        let mut avg = vec![0.0f32; dims];
        for emb in embeddings {
            for (i, &val) in emb.iter().enumerate() {
                avg[i] += val;
            }
        }
        for val in &mut avg {
            *val /= count;
        }

        // L2 normalize the averaged vector.
        let norm: f32 = avg.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut avg {
                *val /= norm;
            }
        }

        Some(avg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_name_parsing() {
        assert_eq!(
            ModelName::from_str_name("jina-embeddings-v2-base-code"),
            Some(ModelName::JinaEmbeddingsV2BaseCode)
        );
        assert_eq!(
            ModelName::from_str_name("jinaai/jina-embeddings-v2-base-code"),
            Some(ModelName::JinaEmbeddingsV2BaseCode)
        );
        assert_eq!(ModelName::from_str_name("jina-code"), Some(ModelName::JinaEmbeddingsV2BaseCode));
        assert_eq!(ModelName::from_str_name("minilm"), Some(ModelName::AllMiniLmL6V2));
        assert_eq!(ModelName::from_str_name("all-MiniLM-L6-v2"), Some(ModelName::AllMiniLmL6V2));
        assert_eq!(
            ModelName::from_str_name("Qdrant/all-MiniLM-L6-v2"),
            Some(ModelName::AllMiniLmL6V2)
        );
        assert_eq!(ModelName::from_str_name("bge-small"), Some(ModelName::BgeSmallEnV15));
        assert_eq!(
            ModelName::from_str_name("BAAI/bge-small-en-v1.5"),
            Some(ModelName::BgeSmallEnV15)
        );
        assert_eq!(ModelName::from_str_name("unknown-model"), None);
    }

    #[test]
    fn test_default_model_is_jina() {
        assert_eq!(ModelName::default(), ModelName::JinaEmbeddingsV2BaseCode);
    }

    #[test]
    fn test_dimensions() {
        assert_eq!(ModelName::JinaEmbeddingsV2BaseCode.dimensions(), 768);
        assert_eq!(ModelName::AllMiniLmL6V2.dimensions(), 384);
        assert_eq!(ModelName::BgeSmallEnV15.dimensions(), 384);
    }

    #[test]
    fn test_average_embeddings_empty() {
        assert_eq!(Embedder::average_embeddings(&[]), None);
    }

    #[test]
    fn test_average_embeddings_single() {
        let emb = vec![1.0, 0.0, 0.0];
        let result = Embedder::average_embeddings(&[emb]).unwrap();
        // Single vector normalized should be [1, 0, 0].
        assert!((result[0] - 1.0).abs() < 1e-5);
        assert!((result[1] - 0.0).abs() < 1e-5);
        assert!((result[2] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_average_embeddings_multiple() {
        let embs = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let result = Embedder::average_embeddings(&embs).unwrap();
        // Average of [1,0,0] and [0,1,0] = [0.5, 0.5, 0] normalized = [0.707, 0.707, 0].
        let expected_val = 1.0 / 2.0_f32.sqrt();
        assert!((result[0] - expected_val).abs() < 1e-4);
        assert!((result[1] - expected_val).abs() < 1e-4);
        assert!((result[2] - 0.0).abs() < 1e-5);
    }

    // NOTE: The full integration test (Embedder::new_default + embed) requires
    // downloading the ONNX model. It's tested separately in integration tests to
    // avoid slowing down unit test runs.
}
