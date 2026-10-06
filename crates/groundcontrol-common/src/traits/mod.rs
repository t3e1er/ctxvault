//! Domain capability traits and storage abstraction boundaries for groundcontrol.
//!
//! # Layered Architecture & Storage Encapsulation
//!
//! groundcontrol follows a strict **Layered Architecture with Storage Encapsulation**
//! across four primary crates:
//!
//! ```text
//! groundcontrol-common ← groundcontrol-core ← groundcontrol-mcp ← groundcontrol-cli
//! ```
//!
//! Rather than introducing runtime dependency-injection indirection (`Arc<dyn Port>`)
//! or theoretical plugin swapping, `groundcontrol` embraces pragmatic Rust idioms:
//!
//! 1. **Crate Layering & Dependency Flow**: `groundcontrol-common` defines authoritative domain
//!    types ([`crate::types`]), configurations ([`crate::config`]), errors ([`crate::error`]),
//!    and capability contracts. It carries zero infrastructure dependencies (no SQLite, Tantivy,
//!    ONNX Runtime, or Petgraph), preserving clean compilation times and lightweight consumption.
//! 2. **Strict Storage Encapsulation Barrier**: Purpose-built storage engines implemented in
//!    `groundcontrol-core` encapsulate all third-party handles (`rusqlite::Connection`,
//!    `tantivy::*`, `hnsw_rs::*`, `petgraph::*`, `ort::*`) as private implementation details.
//!    Raw backend handles never leak across crate boundaries or into the MCP transport layer.
//! 3. **Composition by Value**: The orchestrating `Engine` in `groundcontrol-core` composes its
//!    concrete storage subsystems directly by value (`Store`, `BM25Index`, `KnowledgeGraph`,
//!    `VectorIndex`, `Embedder`). Internal execution is monomorphized, statically dispatched,
//!    and free of runtime dynamic dispatch (`Arc<dyn ...>`) overhead.
//!
//! # Purpose of Trait Contracts
//!
//! The trait contracts in this module serve three concrete architectural goals:
//!
//! - **Domain Capability Boundaries**: They formalize the exact operational contracts that
//!   higher-level services and MCP handlers require (e.g. metadata queries, chunk ingestion,
//!   graph traversal, vector similarity search).
//! - **Testability & Subsystem Isolation**: They enable clear mock/stub implementations for
//!   isolated subsystem testing and verification without instantiating heavy native storage backends.
//! - **Algorithmic Variations**: They provide uniform interfaces for interchangeable retrieval
//!   mechanisms (e.g., [`RetrievalAlgorithm`] in evaluation ablation suites, format-specific
//!   [`DocumentExtractor`] implementations, and multimodal [`SearchService`] routing).
//!
//! # Domain Capabilities
//!
//! All traits in this module speak exclusively in terms of [`crate::types`] domain records,
//! standard library types, and [`crate::Result`]:
//!
//! - **[`MetadataCatalog`]** ([`catalog`]) — Durable SQLite-backed catalog for tracking files,
//!   text chunks, code symbols, edge types, corpus configuration, and resumable indexing states.
//! - **[`TextIndex`]** ([`text_index`]) — Lexical BM25 full-text indexing, document removal,
//!   checkpoint commits, and modality-filtered keyword retrieval.
//! - **[`VectorStore`]** ([`vector_store`]) — Dense vector indexing, batch embedding ingestion,
//!   modality-filtered approximate nearest neighbor (ANN) search, and persistence.
//! - **[`GraphStore`]** ([`graph_store`]) — Petgraph-backed typed knowledge graph, multi-hop
//!   traversals, structural lineage tracing, backlink/forwardlink resolution, taxonomy checks,
//!   and community detection.
//! - **[`EmbeddingProvider`]** ([`embedding`]) — Dense vector generation for queries and batch
//!   text payloads, encapsulating hardware execution providers and tokenizer details.
//! - **[`RetrievalAlgorithm`]** ([`algorithm`]) — Uniform lifecycle and query interface for
//!   modular search algorithms used in evaluation and ablation harnesses.
//! - **[`DocumentExtractor`]** / **[`ContentExtractor`]** ([`extractor`]) — 100% pure Rust
//!   extraction of normalized text, headings, and cross-reference links from rich container
//!   formats (.docx, .pdf, .html).
//! - **[`SearchService`]** & **[`AlgorithmicSearchIndex`]** ([`search`]) — Search-mode dispatch,
//!   reciprocal rank fusion (RRF), fast binary Hamming scans, and personalized PageRank traversal.

pub mod algorithm;
pub mod catalog;
pub mod embedding;
pub mod extractor;
pub mod graph_store;
pub mod search;
pub mod text_index;
pub mod vector_store;

pub use algorithm::*;
pub use catalog::*;
pub use embedding::*;
pub use extractor::*;
pub use graph_store::*;
pub use search::*;
pub use text_index::*;
pub use vector_store::*;
