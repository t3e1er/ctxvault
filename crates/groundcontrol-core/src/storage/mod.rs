//! Storage implementations providing groundcontrol domain capabilities:
//! - `sqlite`: [`MetadataCatalog`](groundcontrol_common::traits::MetadataCatalog) implementation (`Store`)
//! - `tantivy`: [`TextIndex`](groundcontrol_common::traits::TextIndex) implementation (`BM25Index`)
//! - `hnsw`: [`VectorStore`](groundcontrol_common::traits::VectorStore) implementation (`VectorIndex`)
//! - `binary`: [`AlgorithmicSearchIndex`](groundcontrol_common::traits::AlgorithmicSearchIndex) implementation (`BinarySearchIndex`)

pub mod hnsw;
pub mod sqlite;
pub mod tantivy;

pub use hnsw::VectorIndex;
pub use sqlite::Store;
pub use tantivy::BM25Index;
