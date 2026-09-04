//! Ports for the ctxvault hexagonal (ports-and-adapters) architecture.
//!
//! # Hexagonal intent
//!
//! ctxvault is organized as **ports and adapters**. A *port* is a contract —
//! a trait — that expresses a capability the domain needs (metadata catalog,
//! full-text index, vector store, graph store, embedding provider, search
//! dispatch). An *adapter* is a concrete backend that satisfies a port
//! (Tantivy behind the text-index port, HNSW behind the vector port, SQLite
//! behind the catalog port, and so on).
//!
//! Ports are defined **low** — as close to the shared domain as possible — so
//! that consumers depend on the contract, never on a concrete backend. Adapters
//! live in `ctxvault-core`, implement these ports, and keep their backend types
//! (`rusqlite::Connection`, `tantivy::*`, `hnsw_rs::*`, `petgraph::*`, `ort::*`)
//! encapsulated: a backend type must never leak across a port boundary. The
//! **composition root** in `ctxvault-cli` is the only place that names concrete
//! adapters and injects them.
//!
//! This module is the home for the dependency-light port traits. Keeping this
//! module free of heavy dependencies preserves the crate layering
//! (`common` ← `core` ← `mcp` ← `cli`): adding an infrastructure crate here
//! would force it on every consumer.
//!
//! # Generic-vs-trait-object decision (per port)
//!
//! How a port is *wired* (monomorphized generic bound vs. `dyn` trait object)
//! is a consumption decision recorded here so later wiring tasks (Phase 3) stay
//! consistent. It does **not** change how anything is stored today; this block
//! is a decision record, not a wiring change.
//!
//! Verified ownership: `Engine` owns each backend **by value**
//! (`graph: KnowledgeGraph`, `catalog: Store`, `text_index: BM25Index`,
//! `vectors: VectorIndex`, `embedder: Embedder`), reached only through `&` /
//! `&mut`. There is no `Arc<dyn …>`, and no runtime backend swap anywhere (the
//! only clone is internal to a `save`). Nothing requires dynamic dispatch.
//!
//! Therefore:
//!
//! - **Hot-path ports → generics with trait bounds.** `MetadataCatalog`,
//!   `TextIndex`, `VectorStore`, `GraphStore`, and `EmbeddingProvider` sit on
//!   the retrieval/index hot path and have exactly one concrete adapter each,
//!   owned by value. They will be wired as **generic type parameters with trait
//!   bounds** (monomorphized, zero-cost, statically dispatched) — never as
//!   `Arc<dyn _>` / `Box<dyn _>`.
//! - **Swap-point ports → trait objects.** `dyn` is reserved for a genuine
//!   runtime-swap seam (plugin-style backend chosen at run time). **No port in
//!   this refactor's scope needs `dyn` today** — there is no such seam.
//!   `SearchService` is the only candidate that might warrant evaluation (its
//!   mode dispatch could become a swap point), and that is deferred to Phase 2;
//!   until a real runtime swap exists, it too stays generic.
//!
//! In short: **hot-path = generic; swap-point = dyn; none currently need
//! `dyn`.**
//!
//! # Port-home decision
//!
//! Every consumed surface of the six ports is already **domain-typed** — the
//! methods that callers actually use return [`crate::types`] domain types
//! (`SearchResult`, `Edge`, `Document`, `CodeSymbol`, `Vec<f32>`, …), not the
//! backends' own types. The concrete infrastructure types stay private inside
//! their adapters. Because the contracts do not require any heavy crate, **all
//! six port traits will live here in `ctxvault-common::ports`**:
//!
//! - **`MetadataCatalog`** — in `ctxvault-common`. The SQLite `Store`'s public
//!   surface exchanges domain records (`FileRecord`, `ChunkRecord`,
//!   `EdgeTypeRecord`, `CodeSymbol`, config/state values); `rusqlite::Connection`
//!   never appears in a public signature.
//! - **`TextIndex`** — in `ctxvault-common`. The Tantivy `BM25Index`'s used
//!   surface takes `Chunk`/query strings and returns
//!   [`crate::types::SearchResult`], threading a [`crate::types::Modality`]
//!   filter; `tantivy::*` types stay internal.
//! - **`VectorStore`** — in `ctxvault-common`. The HNSW index's used surface
//!   exchanges plain `Vec<f32>` vectors and a [`crate::types::Modality`] filter;
//!   `hnsw_rs::*` stays internal. Its result/metadata types
//!   [`crate::types::VectorSearchResult`] and [`crate::types::VectorMeta`] were
//!   relocated from `ctxvault-core/src/vector_index.rs` into
//!   [`crate::types`] (both are plain data, independent of `hnsw_rs`) so this
//!   port's signatures can live in common without leaking a backend type.
//! - **`GraphStore`** — in `ctxvault-common`. The Petgraph `KnowledgeGraph`'s
//!   used surface exchanges `Document`/`Edge`/path-and-title domain values and
//!   returns domain types; `petgraph::NodeIndex` and friends stay internal.
//! - **`EmbeddingProvider`** — in `ctxvault-common`. The `Embedder`'s used
//!   surface takes `&str`/`&[&str]` and returns `Vec<f32>` / `Vec<Vec<f32>>`;
//!   `ort::*` and tokenizer types stay internal.
//! - **`SearchService`** — in `ctxvault-common`. Search-mode dispatch and RRF
//!   fusion operate purely over [`crate::types::SearchResult`] and
//!   [`crate::types::Modality`]; the fusion helpers already speak only domain
//!   types.
//!
//! [`crate::types::Modality`] — the filter threaded through `TextIndex` and
//! `VectorStore` — already lives in `ctxvault-common::types`, so common can
//! reference it without any new dependency.
//!
//! **No port needs to live in `core::ports`.** No contract forces a heavy crate
//! or an unmovable backend type into `common`; the sole complication
//! (`VectorSearchResult` / `VectorMeta` location) is resolved by relocating
//! those domain-shaped return types into [`crate::types`], not by hosting the
//! trait in core.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::{EdgeClass, EdgeTypeConfig};
use crate::types::{
    BrokenLink, Chunk, ChunkRecord, CircularDependency, CodeSymbol, CommunityDensity,
    CommunityDetectionResult, Document, Edge, EdgeProvenance, EdgeTypeRecord, FileRecord,
    GraphStats, IndexingState, LineageAnnotation, LineageNode, Modality, OrphanAdr,
    ResolutionConfidence, SearchDepth, SearchExplanation, SearchResult, VectorSearchResult,
};
use crate::Result;

/// Metadata catalog port: the durable record-keeping contract for a corpus.
///
/// This is the domain-facing contract for the SQLite-backed metadata store. It
/// covers file tracking, text chunks, code symbols, edge-type configuration,
/// key/value corpus config, and resumable indexing state. Every signature
/// speaks only [`crate::types`] domain records and standard-library types — no
/// backend type (`rusqlite::Connection`, statements, rows) ever crosses this
/// boundary, so consumers depend on the contract rather than on SQLite.
///
/// Construction (opening or creating the underlying database) is deliberately
/// **not** part of this port: it is an adapter/composition-root concern. The
/// port describes only the runtime behaviour a catalog must provide.
///
/// The `templates` and `validation_issues` tables exist in the schema but have
/// no accessor methods on the store today, so they are intentionally absent
/// from this contract; the port mirrors exactly the surface that is used.
pub trait MetadataCatalog {
    // ------------------------------------------------------------------
    // File tracking
    // ------------------------------------------------------------------

    /// Insert or replace a file record, stamping it with the current index time.
    fn insert_file(
        &self,
        path: &str,
        content_hash: &str,
        modified_at: i64,
        template: Option<&str>,
        title: Option<&str>,
    ) -> Result<()>;

    /// Retrieve a single file record by its corpus-relative path.
    fn get_file(&self, path: &str) -> Result<Option<FileRecord>>;

    /// Delete a file record and its associated chunks/issues (via cascade).
    fn delete_file(&self, path: &str) -> Result<()>;

    /// List all tracked files.
    fn list_files(&self) -> Result<Vec<FileRecord>>;

    // ------------------------------------------------------------------
    // Chunks
    // ------------------------------------------------------------------

    /// Insert the given chunks for a file within a single transaction.
    fn insert_chunks(&self, file_path: &str, chunks: &[ChunkRecord]) -> Result<()>;

    /// Retrieve all chunks for a file, ordered by chunk index.
    fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<ChunkRecord>>;

    /// Delete all chunks for a file.
    fn delete_chunks_for_file(&self, file_path: &str) -> Result<()>;

    // ------------------------------------------------------------------
    // Edge types
    // ------------------------------------------------------------------

    /// Insert or replace edge-type configuration records within a transaction.
    fn insert_edge_types(&self, edge_types: &[EdgeTypeRecord]) -> Result<()>;

    /// List all registered edge types.
    fn list_edge_types(&self) -> Result<Vec<EdgeTypeRecord>>;

    // ------------------------------------------------------------------
    // Corpus config (key/value store)
    // ------------------------------------------------------------------

    /// Set a corpus configuration value for the given key.
    fn set_config(&self, key: &str, value: &str) -> Result<()>;

    /// Get a corpus configuration value by key, if present.
    fn get_config(&self, key: &str) -> Result<Option<String>>;

    // ------------------------------------------------------------------
    // Indexing state (resumable paginated indexing)
    // ------------------------------------------------------------------

    /// Retrieve the current indexing state for a corpus, if any.
    fn get_indexing_state(&self, corpus_id: &str) -> Result<Option<IndexingState>>;

    /// Insert or update the indexing state for a corpus.
    fn update_indexing_state(&self, state: &IndexingState) -> Result<()>;

    /// Reset (delete) the indexing state for a corpus.
    fn reset_indexing_state(&self, corpus_id: &str) -> Result<()>;

    // ------------------------------------------------------------------
    // Code symbols
    // ------------------------------------------------------------------

    /// Save the code symbols extracted from a file, replacing any existing ones.
    fn save_code_symbols(&self, file_path: &str, symbols: &[CodeSymbol]) -> Result<()>;

    /// Retrieve all code symbols defined in a file.
    fn get_code_symbols_for_file(&self, file_path: &str) -> Result<Vec<CodeSymbol>>;

    /// Find code symbols matching a name pattern (fuzzy match).
    fn find_symbols_by_name(&self, name_pattern: &str) -> Result<Vec<CodeSymbol>>;

    /// Find code symbols whose fully qualified scope path matches exactly.
    fn find_symbols_by_qualified_name(&self, scope_path: &str) -> Result<Vec<CodeSymbol>>;

    /// Retrieve all code symbols in the entire catalog.
    fn get_all_code_symbols(&self) -> Result<Vec<CodeSymbol>>;
}

/// Full-text index port: the BM25 lexical-retrieval contract for a corpus.
///
/// This is the domain-facing contract for the Tantivy-backed full-text index.
/// It covers document ingestion (add/remove), commit/writer-lifecycle, and
/// ranked lexical search — optionally restricted to a [`Modality`]. Every
/// signature speaks only [`crate::types`] domain types (`Chunk`,
/// `SearchResult`, `Modality`) and standard-library types — no backend type
/// (`tantivy::*`, schemas, writers, readers) ever crosses this boundary, so
/// consumers depend on the contract rather than on Tantivy.
///
/// Construction (opening or creating the underlying index) and lockfile
/// healing are deliberately **not** part of this port: they are
/// adapter/composition-root concerns. The port describes only the runtime
/// behaviour a full-text index must provide.
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

/// Vector store port: the dense approximate-nearest-neighbor contract for a corpus.
///
/// This is the domain-facing contract for the HNSW-backed vector index. It
/// covers vector ingestion (single/batch add and per-document removal),
/// similarity search restricted to a [`Modality`], persistence to disk, and the
/// dimension/model-version/stale/dirty bookkeeping the engine relies on. Every
/// signature speaks only plain `Vec<f32>` / `&[f32]` vectors, standard-library
/// types, and [`crate::types`] domain types
/// ([`Modality`], [`VectorSearchResult`]) — no backend type (`hnsw_rs::*`) ever
/// crosses this boundary, so consumers depend on the contract rather than on
/// HNSW.
///
/// # Construction vs. persistence
///
/// Persistence is split by object-safety and ownership:
///
/// - [`VectorStore::save`] is an **instance** method (`&self`) and therefore
///   part of the port — persisting the current state is runtime behaviour a
///   store must provide.
/// - Loading is deliberately **not** on the port. The load-equivalent operation
///   (and the `new` / `new_default` constructors) return `Self`, which a
///   `&dyn`-object-safe trait cannot express, and constructing a store — reading
///   a `vectors.json` off disk or building an empty index — is an
///   adapter/composition-root concern, not a runtime behaviour of an existing
///   store. The composition root constructs the concrete adapter (loading from
///   disk when present) and injects it behind this port.
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

/// Graph store port: the typed knowledge-graph contract for a corpus.
///
/// This is the domain-facing contract for the Petgraph-backed `KnowledgeGraph`.
/// It covers node/edge mutation, edge construction from parsed documents,
/// traversal (BFS, shortest path, lineage), backlink/forwardlink queries,
/// taxonomy validation (broken links, cycles, orphan ADRs), community
/// detection, statistics, and persistence. Every signature speaks only
/// [`crate::types`] / [`crate::config`] domain types and standard-library
/// types — no backend type (`petgraph::NodeIndex`, `DiGraph`, …) ever crosses
/// this boundary, so consumers depend on the contract rather than on Petgraph.
///
/// # Exclusions (surface deliberately not on the port)
///
/// - **Construction and loading (`new`, `load`).** Both return `Self` — a
///   `&dyn`-object-safe trait cannot express that, and building a graph (empty
///   or deserialized from `graph.bin`) is an adapter/composition-root concern,
///   not runtime behaviour of an existing store. `save` (`&self`) *is* on the
///   port because persisting current state is runtime behaviour; loading stays
///   an inherent method on the concrete adapter.
/// - **`get_node`.** It returns a backend `NodeIndex` and has no non-test
///   callers; every existence check it served is covered by
///   [`GraphStore::contains_node`], so it is excluded entirely (a backend type
///   must never leak across the port).
/// - **`add_node` returns `()` here, not the backend `NodeIndex`.** The sole
///   external caller discards the index, and the internal consumers of that
///   index live inside the adapter's own edge-construction methods — off the
///   port.
///
/// Some inherent methods on the adapter (e.g. `traverse_dfs`, `orphan_paths`,
/// `node_degree_list`, `ensure_node`) have no non-test callers today and are
/// intentionally omitted from this contract; the port mirrors exactly the
/// surface consumers use.
pub trait GraphStore {
    // ------------------------------------------------------------------
    // Node / edge mutation
    // ------------------------------------------------------------------

    /// Add or update a node for the given path (with an optional title).
    ///
    /// Idempotent: re-adding an existing path updates its title in place.
    fn add_node(&mut self, path: &str, title: Option<&str>);

    /// Add a directed intra-corpus edge between two nodes.
    ///
    /// Creates the target node if it is missing. Thin wrapper over
    /// [`GraphStore::add_edge_full`] with no cross-corpus metadata.
    fn add_edge(
        &mut self,
        source: &str,
        target: &str,
        edge_type: &str,
        weight: f32,
        provenance: EdgeProvenance,
        class: EdgeClass,
    );

    /// Add a directed edge carrying optional cross-corpus resolution metadata.
    ///
    /// `target_corpus` / `confidence` are `None` for ordinary intra-corpus edges
    /// and `Some(_)` when the target was resolved in another corpus. Parallel
    /// edges of the same `edge_type` between the same nodes are de-duplicated
    /// (updated in place), keeping this operation idempotent.
    fn add_edge_full(
        &mut self,
        source: &str,
        target: &str,
        edge_type: &str,
        weight: f32,
        provenance: EdgeProvenance,
        class: EdgeClass,
        target_corpus: Option<String>,
        confidence: Option<ResolutionConfidence>,
    );

    /// Add a code edge into the graph with a structural edge class.
    fn add_code_edge(&mut self, edge: &Edge);

    /// Remove a node and all of its edges.
    fn remove_node(&mut self, path: &str) -> Result<()>;

    /// Remove all edges where the given path is source or target.
    fn remove_edges_for_node(&mut self, path: &str);

    // ------------------------------------------------------------------
    // Node / edge queries
    // ------------------------------------------------------------------

    /// Whether a node with the given path exists in the graph.
    fn contains_node(&self, path: &str) -> bool;

    /// Enumerate a node's outgoing frontmatter-provenance edges as
    /// `(edge_type, raw_target)` pairs (used by the cross-corpus resolver).
    fn outgoing_frontmatter_targets(&self, path: &str) -> Vec<(String, String)>;

    /// Enumerate all node paths currently in the graph.
    fn node_paths(&self) -> Vec<String>;

    /// Number of nodes.
    fn node_count(&self) -> usize;

    /// Number of edges.
    fn edge_count(&self) -> usize;

    /// Retrieve all edges currently in the knowledge graph.
    fn get_all_edges(&self) -> Vec<Edge>;

    // ------------------------------------------------------------------
    // Edge construction from documents
    // ------------------------------------------------------------------

    /// Build edges for a parsed document from the given edge-type configs.
    ///
    /// Processes wikilinks, shared tags, and frontmatter fields.
    fn build_edges_for_document(
        &mut self,
        doc: &Document,
        edge_configs: &[EdgeTypeConfig],
        all_docs: &[Document],
    );

    /// Build tag edges across all documents using an inverted tag index.
    fn build_all_tag_edges(&mut self, configs: &[EdgeTypeConfig], all_docs: &[Document]);

    // ------------------------------------------------------------------
    // Traversal & queries
    // ------------------------------------------------------------------

    /// BFS from a starting node up to `max_depth` hops.
    ///
    /// Optionally filtered by edge type and/or edge class. Returns
    /// `(path, hops_from_start)` pairs.
    fn traverse_bfs(
        &self,
        start: &str,
        max_depth: usize,
        edge_type_filter: Option<&[String]>,
        edge_class_filter: Option<EdgeClass>,
    ) -> Vec<(String, usize)>;

    /// All notes that link TO this note, grouped by edge type.
    fn backlinks(
        &self,
        path: &str,
        edge_class_filter: Option<EdgeClass>,
    ) -> HashMap<String, Vec<String>>;

    /// All notes this note links TO, grouped by edge type.
    fn forwardlinks(
        &self,
        path: &str,
        edge_class_filter: Option<EdgeClass>,
    ) -> HashMap<String, Vec<String>>;

    /// Shortest path between two nodes, optionally filtered by edge type/class.
    ///
    /// Returns the path as a list of document paths, or `None` if unreachable.
    fn shortest_path(
        &self,
        from: &str,
        to: &str,
        edge_type_filter: Option<&[String]>,
        edge_class_filter: Option<EdgeClass>,
    ) -> Option<Vec<String>>;

    /// Graph statistics (node/edge counts, orphans, most-connected, distribution).
    fn stats(&self) -> GraphStats;

    // ------------------------------------------------------------------
    // Structural lineage & taxonomy
    // ------------------------------------------------------------------

    /// Deterministically traverse the graph along a structural edge type.
    ///
    /// `direction` is `"outgoing"`, `"incoming"`, or `"both"`. Returns the
    /// ordered lineage chain, starting with the start node at depth 0.
    fn traverse_lineage(
        &self,
        start: &str,
        edge_type: &str,
        direction: &str,
        max_depth: usize,
    ) -> Vec<LineageNode>;

    /// Extract active structural lineage metadata for a node, if any.
    fn extract_lineage_for_node(&self, path: &str) -> Option<LineageAnnotation>;

    /// Detect broken structural links (targets absent from `existing_paths`).
    fn detect_broken_links(&self, existing_paths: &HashSet<String>) -> Vec<BrokenLink>;

    /// Detect circular dependencies within the given directed-acyclic relations.
    fn detect_circular_dependencies(&self, edge_types: &[&str]) -> Vec<CircularDependency>;

    /// Detect ADR notes with no inbound or outbound structural links.
    fn detect_orphan_adrs(&self, adr_paths: &[String]) -> Vec<OrphanAdr>;

    // ------------------------------------------------------------------
    // Community detection
    // ------------------------------------------------------------------

    /// Detect communities using the Louvain modularity-based algorithm.
    fn detect_communities(&self) -> CommunityDetectionResult;

    /// Detect communities with a Leiden-style connectivity-refinement pass.
    fn detect_communities_leiden(&self) -> CommunityDetectionResult;

    /// Per-community density statistics for the current partition.
    fn community_densities(&self) -> Vec<CommunityDensity>;

    // ------------------------------------------------------------------
    // Persistence
    // ------------------------------------------------------------------

    /// Serialize the graph to a file at the given path.
    fn save(&self, path: &Path) -> Result<()>;
}

/// Embedding provider port: the dense-embedding contract for a corpus.
///
/// This is the domain-facing contract for the ONNX-backed `Embedder`. It covers
/// the embedding capabilities consumers actually reach for across the port
/// boundary: encoding a single search query, encoding a batch of texts, and
/// reporting the output dimensionality. Every signature speaks only `&str` /
/// `&[&str]` inputs and plain `Vec<f32>` / `Vec<Vec<f32>>` / `usize` outputs —
/// no backend type (`ort::*`, `tokenizers::Tokenizer`, the core-local
/// `ModelName` enum, or `HardwareGovernor`) ever crosses this boundary, so
/// consumers depend on the contract rather than on ONNX Runtime.
///
/// Signatures mirror the concrete adapter's inherent methods exactly so the
/// adapter forwards trivially, and the surface is deliberately **object-safe**
/// (no generic methods, no `Self`-returning methods, no associated consts): it
/// is usable both as a generic trait bound — the intended hot-path wiring per
/// the generic-vs-`dyn` decision above — and, if ever needed, behind
/// `Arc<dyn EmbeddingProvider>`.
///
/// # Exclusions (surface deliberately not on the port)
///
/// - **`model_name()` / model-version / max-seq-len.** `model_name()` returns a
///   core-local `ModelName` enum that wraps ONNX/tokenizer backend knowledge;
///   returning it would leak a backend-coupled type across the port, and
///   relocating `ModelName` into `common` would drag model/backend knowledge
///   into the dependency-light crate. Its only two uses are inside the adapter's
///   own crate — `version_string()` (engine.rs) and `max_seq_len()`
///   (pipeline.rs) — where the caller holds a *concrete* `Embedder` /
///   `Arc<Embedder>`, not a port-typed value. No consumer needs a model version
///   *through* the port today, so none of these are on the contract; they stay
///   inherent, backend-coupled methods on the adapter.
/// - **`average_embeddings`.** It is a pure, stateless *associated* (static)
///   function (`Embedder::average_embeddings(&[Vec<f32>]) -> Option<Vec<f32>>`)
///   with no `&self`, called as `Embedder::average_embeddings(...)`. It is not a
///   per-instance provider capability, and a static method on a trait would
///   break object-safety. It stays an inherent associated fn on the adapter.
/// - **`tokenizer()` / `governor()` / `session_count()` /
///   `has_token_type_ids()` / `is_gpu_disabled()` / `reset_gpu_disabled()`.**
///   These expose backend or backend-adjacent types (`tokenizers::Tokenizer`,
///   `Arc<dyn HardwareGovernor>`) or internal accelerator state. The async
///   indexing pipeline that the task text alludes to holds the *concrete*
///   `Arc<Embedder>` (not the port) and reaches these inherent accessors
///   directly; no consumer accesses them through a port-typed value, so they are
///   excluded to keep every port signature backend-free.
///
/// `embed()` (embed a single text) is likewise **not** on the port: it is the
/// internal sibling that [`EmbeddingProvider::embed_query`] wraps, has no
/// cross-boundary caller, and is trivially `embed_batch`-of-one. Consumers cross
/// the boundary via `embed_query`; `embed` stays inherent.
///
/// Construction (loading the ONNX model + tokenizer) is deliberately **not**
/// part of this port: it is an adapter/composition-root concern. The port
/// describes only the runtime behaviour an embedding provider must offer.
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

/// Query options for the [`SearchService`] port.
///
/// Carries every input the search-mode dispatch needs, mirroring the MCP
/// `search` tool's parameter semantics exactly. The consumer (the MCP adapter)
/// parses raw request JSON into this struct; the service owns the per-mode
/// defaults, fallbacks, and dispatch. Fields that are only meaningful for a
/// subset of modes are still carried unconditionally — the service applies the
/// mode-specific default (e.g. `graph_depth` defaults to 2 for `hybrid`/`explain`
/// and 3 for `graph`; `edge_class` defaults to `Semantic` for `hybrid`,
/// `Structural` for `graph`, and `None` for `explain`).
///
/// This opts type lives in `ctxvault-common` — alongside the port trait it feeds
/// — because it references only [`crate::types`] / [`crate::config`] domain types
/// (`Modality`, `SearchDepth`) and standard-library types, forcing no heavy
/// dependency onto the dependency-light crate. Verbosity/detail (`detail=ids`
/// snippet stripping) is deliberately absent: it shapes the outbound JSON and
/// stays an adapter concern, not a search-dispatch concern.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// The raw query text.
    pub query: String,
    /// Retrieval mode: `bm25`, `semantic`, `hybrid`, `graph`, or `explain`.
    ///
    /// `None` selects the default (`hybrid`). Any unrecognized value is an error,
    /// reproduced by the service.
    pub mode: Option<String>,
    /// Maximum number of results to return. `None` defaults to 10.
    pub limit: Option<usize>,
    /// Modality filter (docs | code | both), threaded through every mode.
    pub modality: Modality,
    /// Semantic-search depth (precise | broad | adaptive). Only used by `semantic`.
    pub depth: SearchDepth,
    /// Graph traversal depth. `None` takes the per-mode default (2 for
    /// `hybrid`/`explain`, 3 for `graph`).
    pub graph_depth: Option<usize>,
    /// Optional edge-type filter for graph-aware modes.
    pub edge_types: Option<Vec<String>>,
    /// Optional edge-class filter (`semantic` | `structural` | `hybrid`) as a raw
    /// string. The service applies the per-mode default when this is `None`.
    pub edge_class: Option<String>,
    /// Whether to run multi-hop query decomposition (`hybrid` mode only).
    pub decompose: Option<bool>,
}

/// Search service port: the search-mode dispatch + RRF fusion contract.
///
/// This is the domain-facing contract for the retrieval dispatch that selects a
/// mode (`bm25` | `semantic` | `hybrid` | `graph` | `explain`) and fuses signals
/// via RRF. Every signature speaks only [`SearchQuery`] and [`crate::types`]
/// domain results ([`SearchResult`], [`SearchExplanation`]) — no backend type
/// (`tantivy::*`, `hnsw_rs::*`, `petgraph::*`, `ort::*`) crosses this boundary,
/// so consumers dispatch a search without naming any concrete adapter.
///
/// # Two methods, two result shapes
///
/// The `explain` mode returns a different, richer shape
/// ([`SearchExplanation`], with per-signal score breakdown) than the other four
/// modes ([`SearchResult`]). Rather than fold the two into one type, the port
/// exposes them as separate methods:
///
/// - [`SearchService::search`] handles `bm25`, `semantic`, `hybrid`, and `graph`,
///   returning `Vec<SearchResult>`.
/// - [`SearchService::explain`] handles `explain`, returning
///   `Vec<SearchExplanation>`.
///
/// A caller routes on [`SearchQuery::mode`] and calls the matching method; the
/// implementation still validates the mode and reproduces the exact
/// invalid-mode error for a mode that does not belong to the method invoked.
///
/// # Generic-vs-`dyn` wiring
///
/// Consistent with the per-port decision recorded above: `SearchService` sits on
/// the retrieval hot path and there is **no runtime-swap seam** for the dispatch
/// today, so its implementation is a concrete struct wired as a
/// **generic type parameter with this trait bound** — monomorphized, zero-cost,
/// statically dispatched — never `Arc<dyn SearchService>`. The trait exists to
/// keep consumers (the MCP layer) depending on the contract rather than on the
/// core implementation's internals.
pub trait SearchService {
    /// Dispatch a `bm25` / `semantic` / `hybrid` / `graph` search, returning
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
