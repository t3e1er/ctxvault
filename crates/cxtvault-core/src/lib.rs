//! Core engine: indexing, search, graph, embeddings, and persistence.
//!
//! This crate contains all domain logic. It has no knowledge of MCP protocol
//! or CLI concerns — those belong in `cxtvault-mcp` and `cxtvault-cli` respectively.

pub mod analytics;
pub mod corpus_manager;
pub mod embedding;
pub mod engine;
pub mod graph;
pub mod index;
pub mod parser;
pub mod persistence;
pub mod search;
pub mod template;
pub mod vector_index;
pub mod watcher;
