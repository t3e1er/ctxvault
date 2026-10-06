//! Shared types, traits, and error definitions for the groundcontrol engine.
//!
//! This crate contains no business logic — only the contracts that other crates
//! depend on. Keep it lean: adding a dependency here forces it on every consumer.
//!
//! Under the workspace's layered architecture, it hosts the domain capability traits in
//! [`traits`] — [`traits::MetadataCatalog`], [`traits::TextIndex`],
//! [`traits::VectorStore`], [`traits::GraphStore`], [`traits::EmbeddingProvider`],
//! [`traits::RetrievalAlgorithm`], [`traits::DocumentExtractor`], and
//! [`traits::SearchService`] — alongside the domain [`types`] their signatures speak.
//! Keeping this crate dependency-light preserves strict crate layering and ensures
//! backend storage engines remain fully encapsulated in `groundcontrol-core`.

pub mod client;
pub mod config;
pub mod error;
pub mod traits;
pub mod types;

// Root-level re-exports for streamlined ergonomics
pub use client::{ClientEntry, ClientsRegistry};
pub use error::{Error, Result};
pub use traits::{
    AlgorithmicSearchIndex, ContentExtractor, DocumentExtractor, EmbeddingProvider, GraphStore,
    MetadataCatalog, RetrievalAlgorithm, SearchService, TextIndex, VectorStore,
};
pub use types::*;

/// Convenient prelude re-exporting common capability traits, domain types, and error handling.
pub mod prelude {
    pub use super::traits::{
        AlgorithmicSearchIndex, ContentExtractor, DocumentExtractor, EmbeddingProvider, GraphStore,
        MetadataCatalog, RetrievalAlgorithm, SearchService, TextIndex, VectorStore,
    };
    pub use super::types::*;
    pub use super::{Error, Result};
}
