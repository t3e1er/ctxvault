//! Core engine: indexing, search, graph, embeddings, and persistence.
//!
//! This crate contains all domain logic. It has no knowledge of MCP protocol
//! or CLI concerns — those belong in `groundcontrol-mcp` and `groundcontrol-cli` respectively.
//!
//! It provides the **adapters** implementing the `groundcontrol-common` ports —
//! `Store` (SQLite), `BM25Index` (Tantivy), `VectorIndex` (HNSW),
//! `KnowledgeGraph` (Petgraph), and `Embedder` (ONNX) — plus `CoreSearchService`,
//! keeping each backend crate encapsulated so no backend type crosses a port. The
//! [`engine::Engine`] domain orchestrator is a single concrete type that owns
//! these adapters and exposes them port-typed; [`engine::builder::EngineBuilder`]
//! is the construction seam that builds and injects the concrete adapters.

pub mod algorithm;
pub mod analytics;
pub mod classifier;
pub mod corpus_manager;
pub mod embedding;
pub mod engine;
pub mod graph;
pub mod index;
pub mod parser;
pub mod search;
pub mod storage;
pub mod template;
pub mod watcher;

pub use engine::builder::EngineBuilder;
pub use index::bundle;
pub use parser::code::normalize_scope_path;
pub use search::service::CoreSearchService;
