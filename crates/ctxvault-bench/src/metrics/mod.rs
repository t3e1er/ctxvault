//! Evaluation metrics for ranking quality and latency.

pub mod ir;
pub mod latency;

pub use ir::{IrEvaluator, QueryEvaluationMetrics};
pub use latency::{LatencyStats, LatencyTracker};
