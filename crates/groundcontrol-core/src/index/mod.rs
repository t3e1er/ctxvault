//! Index management: orchestrates Tantivy (BM25), HNSW (vector), and ingestion pipeline.

pub mod bundle;
pub mod pipeline;

pub use crate::classifier::{exclude, FileClassification, FileClassifier};
pub use crate::storage::tantivy::{heal_stale_lockfiles, BM25Index};
