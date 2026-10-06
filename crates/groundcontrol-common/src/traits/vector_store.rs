//! Vector store domain capability contract.

use std::path::Path;

use crate::types::{Modality, VectorSearchResult};
use crate::Result;

/// Domain capability contract for dense approximate-nearest-neighbor (ANN) vector indexing.
///
/// This contract defines storage boundaries for vector ingestion (individual
/// and batched additions, document-level and chunk-level embeddings), path-based
/// vector pruning, similarity search restricted to a [`Modality`], index
/// persistence, and model-version metadata bookkeeping. Every signature
/// speaks only plain `Vec<f32>` / `&[f32]` vectors, standard-library types, and
/// [`crate::types`] domain types ([`Modality`], [`VectorSearchResult`]). Raw
/// backend types (`hnsw_rs::*`) remain strictly encapsulated within
/// `groundcontrol-core`.
///
/// Index instantiation and disk loading are owned directly by the concrete
/// `VectorIndex` lifecycle in `groundcontrol-core`.
pub trait VectorStore {
    /// Add a single vector to the index.
    ///
    /// `modality` is the coarse modality tag ("code" / "docs") used for
    /// modality-filtered search. Returns the internal ID assigned to the vector.
    fn add(
        &mut self,
        vector: &[f32],
        doc_path: &str,
        chunk_index: Option<usize>,
        is_doc_level: bool,
        modality: &str,
    ) -> Result<usize>;

    /// Add multiple vectors in batch (more efficient than individual adds).
    ///
    /// Returns the internal IDs assigned, in input order.
    fn add_batch(
        &mut self,
        vectors: &[Vec<f32>],
        doc_path: &str,
        chunk_indices: &[Option<usize>],
        is_doc_level: bool,
        modality: &str,
    ) -> Result<Vec<usize>>;

    /// Remove all vectors associated with a given document path.
    fn remove_document(&mut self, doc_path: &str);

    /// Search for the `k` nearest neighbors to a query vector.
    ///
    /// - `doc_level_only`: if true, only return document-level embeddings.
    /// - `modality`: restrict results to the given [`Modality`] (post-filter on
    ///   each vector's coarse modality tag).
    ///
    /// Returns results sorted by descending similarity score.
    fn search(
        &self,
        query: &[f32],
        k: usize,
        doc_level_only: bool,
        modality: Modality,
    ) -> Result<Vec<VectorSearchResult>>;

    /// Persist the index to disk (vectors + metadata) at the given path.
    fn save(&self, path: &Path) -> Result<()>;

    /// Get the dimensionality of vectors in this index.
    fn dimensions(&self) -> usize;

    /// Get the number of vectors currently in the index.
    fn len(&self) -> usize;

    /// Check whether the index is empty.
    fn is_empty(&self) -> bool;

    /// Get the model version stored with this index, if any.
    fn model_version(&self) -> Option<&str>;

    /// Set the model version for this index.
    fn set_model_version(&mut self, version: &str);

    /// Check whether vectors are marked as stale (model version mismatch).
    fn is_stale(&self) -> bool;

    /// Mark vectors as stale (model version mismatch detected).
    fn mark_stale(&mut self);

    /// Clear the stale flag (after re-embedding completes).
    fn clear_stale(&mut self);

    /// Check whether the index has unpersisted changes.
    fn is_dirty(&self) -> bool;

    /// Mark the index as having unpersisted changes.
    fn mark_dirty(&self);

    /// Clear the dirty flag.
    fn clear_dirty(&self);
}
