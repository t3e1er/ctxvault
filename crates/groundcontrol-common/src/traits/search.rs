//! Search service and algorithmic search index domain capability contracts.

use crate::types::{
    BinaryFingerprint, FingerprintRecord, Modality, SearchExplanation, SearchQuery, SearchResult,
};
use crate::Result;

/// Domain capability contract for high-throughput algorithmic semantic search and fingerprinting.
pub trait AlgorithmicSearchIndex: Send + Sync {
    /// Add or update binary fingerprints for extracted code symbols or doc chunks.
    fn index_fingerprints(&mut self, records: &[FingerprintRecord]) -> Result<()>;

    /// Perform a high-speed linear SIMD Hamming scan across all registered fingerprints.
    fn search_hamming(
        &self,
        query_bits: &BinaryFingerprint,
        limit: usize,
        modality: Modality,
    ) -> Result<Vec<(String, u32)>>;

    /// Project a text query into a 256-bit binary fingerprint via SIF.
    fn project_query(&self, query: &str) -> Result<BinaryFingerprint>;

    /// Clear all registered fingerprints.
    fn clear(&mut self);

    /// Total number of indexed fingerprints.
    fn len(&self) -> usize;

    /// Whether the index is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Domain capability contract for search-mode dispatch and signal fusion.
///
/// Defines the retrieval boundary for multimodal search execution (`bm25`,
/// `semantic`, `hybrid`, `graph`, `explain`, `fast`) and graph-based related
/// note exploration. Callers interact exclusively via [`SearchQuery`] and domain
/// result types ([`SearchResult`], [`SearchExplanation`]), keeping retrieval
/// orchestration decoupled from underlying storage engines.
///
/// # Two Methods, Two Result Shapes
///
/// The `explain` mode returns a rich breakdown ([`SearchExplanation`], with
/// per-signal scores) while other modes return ranked [`SearchResult`]s:
///
/// - [`SearchService::search`] handles `bm25`, `semantic`, `hybrid`, `graph`,
///   and `fast`, returning `Vec<SearchResult>`.
/// - [`SearchService::explain`] handles `explain`, returning
///   `Vec<SearchExplanation>`.
pub trait SearchService {
    /// Dispatch a `bm25` / `semantic` / `hybrid` / `graph` / `fast` search, returning
    /// ranked [`SearchResult`]s (before any detail/verbosity shaping).
    ///
    /// Returns an error if `query.mode` is `explain` (use
    /// [`SearchService::explain`]) or an unrecognized mode.
    fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;

    /// Dispatch an `explain` search, returning per-result score breakdowns as
    /// [`SearchExplanation`]s (before any detail/verbosity shaping).
    fn explain(&self, query: &SearchQuery) -> Result<Vec<SearchExplanation>>;

    /// Related search: given seed document paths, find the documents most
    /// related to them via a multi-source BFS approximation of Personalized
    /// PageRank over the knowledge graph.
    ///
    /// Traverses only the graph (never the lexical or vector signals),
    /// restricting results to the requested [`Modality`], and returns ranked
    /// [`SearchResult`]s (before any detail/verbosity shaping).
    fn search_related(
        &self,
        seeds: &[String],
        limit: usize,
        modality: Modality,
    ) -> Result<Vec<SearchResult>>;
}
