//! Retrieval algorithm execution runners.

pub mod sanitizer;

use std::str::FromStr;
use std::time::Instant;

use ctxvault_common::ports::{SearchQuery, SearchService};
use ctxvault_common::types::{Modality, SearchResult};
use ctxvault_core::engine::Engine;
use serde::{Deserialize, Serialize};

use crate::dataset::schema::BenchmarkQuery;
use sanitizer::sanitize_lucene_query;

/// Individual retrieval modes supported for benchmarking and ablation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMode {
    /// Pure Tantivy Okapi BM25 lexical retrieval.
    Bm25,
    /// Isolated SIF projection + 256-bit MRL binary Hamming scan.
    Binary,
    /// Isolated HippoRAG 2-hop personalized PageRank diffusion on Petgraph.
    Ppr,
    /// Fast algorithmic hybrid (BM25 + Binary Hamming + PPR via 3-way RRF).
    Fast,
    /// Pure dense ONNX neural embeddings (cosine similarity).
    Semantic,
    /// Full 3-signal hybrid (BM25 + ONNX + BFS graph proximity).
    Full,
}

impl RetrievalMode {
    /// Canonical string identifier for this retrieval mode.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bm25 => "bm25",
            Self::Binary => "binary",
            Self::Ppr => "ppr",
            Self::Fast => "fast",
            Self::Semantic => "semantic",
            Self::Full => "full",
        }
    }

    /// List of all standard ablation modes.
    pub fn all() -> &'static [RetrievalMode] {
        &[Self::Bm25, Self::Binary, Self::Ppr, Self::Fast, Self::Semantic, Self::Full]
    }
}

impl FromStr for RetrievalMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "bm25" | "lexical" => Ok(Self::Bm25),
            "binary" | "sif" | "hamming" => Ok(Self::Binary),
            "ppr" | "pagerank" | "diffusion" => Ok(Self::Ppr),
            "fast" | "fast_hybrid" => Ok(Self::Fast),
            "semantic" | "vector" | "dense" => Ok(Self::Semantic),
            "full" | "hybrid" => Ok(Self::Full),
            unknown => Err(format!(
                "Unknown retrieval mode '{unknown}'. Valid modes: bm25, binary, ppr, fast, semantic, full"
            )),
        }
    }
}

/// Options configuring a query execution run.
#[derive(Debug, Clone)]
pub struct QueryRunnerOptions {
    /// Number of candidates to retrieve.
    pub limit: usize,
    /// Modality filter (`both`, `code`, or `docs`).
    pub modality: Modality,
    /// Whether to enable query decomposition.
    pub decompose: bool,
}

impl Default for QueryRunnerOptions {
    fn default() -> Self {
        Self { limit: 10, modality: Modality::Both, decompose: false }
    }
}

/// Query runner that executes searches against an `Engine` and measures elapsed time.
pub struct QueryRunner;

impl QueryRunner {
    /// Execute a benchmark query using the specified mode and return the results and elapsed milliseconds.
    pub fn execute(
        engine: &Engine,
        query: &BenchmarkQuery,
        mode: RetrievalMode,
        options: &QueryRunnerOptions,
    ) -> ctxvault_common::Result<(Vec<SearchResult>, f64)> {
        let t_start = Instant::now();

        let sanitized = sanitize_lucene_query(&query.query);
        let search_text = if sanitized.is_empty() { query.query.clone() } else { sanitized };

        let results = match mode {
            RetrievalMode::Bm25 => {
                let sq = SearchQuery {
                    query: search_text,
                    mode: Some("bm25".to_string()),
                    limit: Some(options.limit),
                    modality: options.modality,
                    decompose: Some(options.decompose),
                    ..Default::default()
                };
                engine.search_service().search(&sq)?
            }
            RetrievalMode::Binary => {
                // Isolated binary index search: project query and run Hamming scan
                let binary = engine.binary_index();
                let q_fp = binary.project_query(&search_text)?;
                let hits = binary.search_hamming(&q_fp, options.limit, options.modality)?;
                hits.into_iter()
                    .map(|(path, dist)| {
                        let sim = 1.0 - (dist as f32 / 256.0);
                        SearchResult::new(path, sim as f64)
                    })
                    .collect()
            }
            RetrievalMode::Ppr => {
                // Isolated PPR diffusion: seed with BM25 then diffuse on Petgraph
                let sq = SearchQuery {
                    query: search_text,
                    mode: Some("bm25".to_string()),
                    limit: Some(options.limit * 2),
                    modality: options.modality,
                    ..Default::default()
                };
                let bm25_hits = engine.search_service().search(&sq)?;
                let seeds: Vec<(String, f64)> =
                    bm25_hits.into_iter().map(|r| (r.path, r.score)).collect();
                let ppr_scores = ctxvault_core::graph::diffusion::personalized_pagerank(
                    engine.knowledge_graph(),
                    &seeds,
                    ctxvault_core::graph::diffusion::PPR_DEFAULT_ALPHA,
                    ctxvault_core::graph::diffusion::PPR_DEFAULT_ITERATIONS,
                    None,
                );
                ppr_scores
                    .into_iter()
                    .take(options.limit)
                    .map(|p| SearchResult::new(p.path, p.score))
                    .collect()
            }
            RetrievalMode::Fast => {
                let sq = SearchQuery {
                    query: search_text,
                    mode: Some("fast".to_string()),
                    limit: Some(options.limit),
                    modality: options.modality,
                    decompose: Some(options.decompose),
                    ..Default::default()
                };
                engine.search_service().search(&sq)?
            }
            RetrievalMode::Semantic => {
                let sq = SearchQuery {
                    query: search_text,
                    mode: Some("semantic".to_string()),
                    limit: Some(options.limit),
                    modality: options.modality,
                    decompose: Some(options.decompose),
                    ..Default::default()
                };
                engine.search_service().search(&sq)?
            }
            RetrievalMode::Full => {
                let sq = SearchQuery {
                    query: search_text,
                    mode: Some("hybrid".to_string()),
                    limit: Some(options.limit),
                    modality: options.modality,
                    decompose: Some(options.decompose),
                    ..Default::default()
                };
                engine.search_service().search(&sq)?
            }
        };

        let elapsed_ms = t_start.elapsed().as_secs_f64() * 1000.0;
        Ok((results, elapsed_ms))
    }
}
