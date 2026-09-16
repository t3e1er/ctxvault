//! Dataset schemas and loader utilities.

pub mod loader;
pub mod schema;

pub use loader::DatasetLoader;
pub use schema::{BenchmarkDataset, BenchmarkQuery, RelevanceJudgment};
