//! File classification and exclusion pattern matching.

pub mod engine;
pub mod exclude;

pub use engine::{FileClassification, FileClassifier};
pub use exclude::ExcludeMatcher;
