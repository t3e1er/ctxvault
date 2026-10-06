//! Full-text index domain capability contract.

use crate::types::{Chunk, Modality, SearchResult};
use crate::Result;

/// Domain capability contract for BM25 full-text indexing and lexical retrieval.
///
/// This contract defines data boundaries for document chunk ingestion, writer
/// lifecycle management, commit checkpoints, and ranked lexical querying
/// across modalities ([`Modality::Docs`], [`Modality::Code`], [`Modality::Both`]).
/// Every signature operates strictly on [`crate::types`] domain models and
/// standard-library types. Raw Tantivy index handles (`tantivy::Index`,
/// schemas, readers, writers) remain strictly encapsulated within
/// `groundcontrol-core`.
///
/// Index creation, disk directory initialization, and lockfile healing are
/// managed directly by the concrete index implementation in `groundcontrol-core`.
pub trait TextIndex {
    /// Release the underlying writer, dropping any exclusive index lock.
    ///
    /// Call after a commit to allow other processes to access the index.
    fn release_writer(&mut self);

    /// Add all chunks for a document to the index. Does NOT auto-commit.
    fn add_document(
        &mut self,
        doc_path: &str,
        title: Option<&str>,
        tags: &[String],
        chunks: &[Chunk],
    ) -> Result<()>;

    /// Remove all indexed chunks for a given document path. Does NOT auto-commit.
    fn remove_document(&mut self, doc_path: &str) -> Result<()>;

    /// Commit pending changes to disk.
    fn commit(&mut self) -> Result<()>;

    /// Search the index with a text query (no modality restriction).
    ///
    /// Thin wrapper over [`TextIndex::search_with_modality`] with
    /// [`Modality::Both`]. Returns ranked results with scores and snippets.
    fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Search the index, restricting results to the requested [`Modality`].
    fn search_with_modality(
        &self,
        query: &str,
        limit: usize,
        modality: Modality,
    ) -> Result<Vec<SearchResult>>;
}
