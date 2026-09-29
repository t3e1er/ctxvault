//! Algorithm Substrate: modular, decomposed retrieval components.

pub mod binaryv3;
pub mod bm25;
pub mod composite;
pub mod dense;
pub mod eval;
pub mod graph;
pub mod registry;

pub use binaryv3::{BinaryV3Algorithm, BinaryV3Config};
/// Canonical binary retrieval algorithm backed by BinaryV3.
pub type BinaryAlgorithm = BinaryV3Algorithm;
/// Configuration for the canonical binary retrieval algorithm.
pub type BinaryConfig = BinaryV3Config;
pub use bm25::{Bm25Algorithm, Bm25Config};
pub use composite::{search_fast_composite, search_hybrid_composite, CompositeConfig};
pub use dense::{DenseAlgorithm, DenseConfig};
pub use eval::{AlgoConfig, AlgoHit, AlgorithmicIndex, IndexStats};
pub use graph::{GraphAlgorithm, GraphConfig};
pub use groundcontrol_common::types::BinaryProjectionKind;
pub use registry::AlgorithmRegistry;
