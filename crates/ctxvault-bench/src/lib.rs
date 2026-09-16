//! Dedicated data science benchmarking and resource profiling harness for ctxvault.
//!
//! Provides comprehensive indexing performance profiling (wall-clock stage timings,
//! throughput, peak RSS memory, disk footprint breakdown, expansion ratio) and granular
//! retrieval algorithm evaluation (BM25, SIF+MRL Binary Hamming, HippoRAG PPR diffusion,
//! Fast Hybrid, Dense ONNX, and Full Hybrid) against ground-truth corpora.

pub mod dataset;
pub mod metrics;
pub mod profile;
pub mod report;
pub mod runners;
pub mod sweep;

pub use dataset::{BenchmarkDataset, BenchmarkQuery, DatasetLoader, RelevanceJudgment};
pub use metrics::{IrEvaluator, LatencyStats, LatencyTracker, QueryEvaluationMetrics};
pub use profile::{
    DiskBreakdown, DiskProfiler, IndexProfiler, IndexProfilerOptions, IndexingProfileReport,
    MemoryMetrics, MemoryTracker,
};
pub use report::{CsvReporter, JsonReporter, MarkdownReporter};
pub use runners::{QueryRunner, QueryRunnerOptions, RetrievalMode};
pub use sweep::{BenchmarkSuite, BenchmarkSuiteReport, ModeEvaluationSummary};
