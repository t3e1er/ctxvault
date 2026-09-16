//! `ctxv-bench`: Dedicated data science benchmarking CLI for ctxvault.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::{Parser, Subcommand};
use ctxvault_bench::dataset::DatasetLoader;
use ctxvault_bench::profile::{IndexProfiler, IndexProfilerOptions};
use ctxvault_bench::report::{CsvReporter, JsonReporter, MarkdownReporter};
use ctxvault_bench::runners::RetrievalMode;
use ctxvault_bench::sweep::{BenchmarkSuite, BenchmarkSuiteReport};
use ctxvault_common::config::CorpusConfig;
use ctxvault_common::types::Modality;
use ctxvault_core::engine::Engine;

#[derive(Parser)]
#[command(
    name = "ctxv-bench",
    about = "Data science benchmark harness: Indexing resource profiling & retrieval algorithm ablation",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Profile the indexing pipeline (wall-clock stage timings, throughput, peak RSS, disk breakdown)
    Index {
        /// Path to the corpus directory
        #[arg(short, long)]
        corpus: PathBuf,

        /// Perform a clean cold-start build by removing existing .index directory
        #[arg(long, default_value_t = false)]
        clean: bool,

        /// Include dense ONNX neural re-embedding stage
        #[arg(long, default_value_t = false)]
        reembed: bool,

        /// Path to output JSON profiling report
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Run retrieval quality evaluation and latency benchmarks across specified modes
    Eval {
        /// Path to the indexed corpus directory
        #[arg(short, long)]
        corpus: PathBuf,

        /// Path to the queries and ground-truth judgments JSON file
        #[arg(short, long)]
        queries: PathBuf,

        /// Comma-separated list of retrieval modes to ablate (bm25, binary, ppr, fast, semantic, full, or all)
        #[arg(short, long, default_value = "all")]
        modes: String,

        /// Cutoff rank K for Recall@K, MRR@K, NDCG@K
        #[arg(short, long, default_value_t = 5)]
        k: usize,

        /// Modality filter: both, code, or docs
        #[arg(long, default_value = "both")]
        modality: String,

        /// Optional output file path (.md, .json, or .csv)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Run full benchmark: clean reindex with resource profiling followed by retrieval evaluation
    All {
        /// Path to the corpus directory
        #[arg(short, long)]
        corpus: PathBuf,

        /// Path to the queries and ground-truth judgments JSON file
        #[arg(short, long)]
        queries: PathBuf,

        /// Comma-separated list of retrieval modes (default: all)
        #[arg(short, long, default_value = "all")]
        modes: String,

        /// Cutoff rank K for Recall@K, MRR@K, NDCG@K
        #[arg(short, long, default_value_t = 5)]
        k: usize,

        /// Perform a clean cold-start build
        #[arg(long, default_value_t = false)]
        clean: bool,

        /// Include dense ONNX neural re-embedding stage
        #[arg(long, default_value_t = false)]
        reembed: bool,

        /// Output directory to write report.md, report.json, and report.csv
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
    },
}

fn parse_modes(modes_str: &str) -> Result<Vec<RetrievalMode>, String> {
    if modes_str.trim().eq_ignore_ascii_case("all") {
        return Ok(RetrievalMode::all().to_vec());
    }
    let mut modes = Vec::new();
    for part in modes_str.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            modes.push(RetrievalMode::from_str(trimmed)?);
        }
    }
    if modes.is_empty() {
        return Err("No retrieval modes specified".to_string());
    }
    Ok(modes)
}

fn parse_modality(mod_str: &str) -> Modality {
    match mod_str.to_lowercase().trim() {
        "code" => Modality::Code,
        "docs" | "doc" => Modality::Docs,
        _ => Modality::Both,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Index { corpus, clean, reembed, output } => {
            println!("Profiling indexing pipeline for: {}", corpus.display());
            let opts =
                IndexProfilerOptions { include_dense_embedding: reembed, clean_cold_start: clean };
            let report = IndexProfiler::profile(&corpus, &opts)?;
            let json_str = serde_json::to_string_pretty(&report)?;

            if let Some(out_path) = output {
                fs::write(&out_path, &json_str)?;
                println!("Wrote indexing profile report to: {}", out_path.display());
            } else {
                println!("{json_str}");
            }
        }

        Commands::Eval { corpus, queries, modes, k, modality, output } => {
            let modes_list = parse_modes(&modes)?;
            let mod_enum = parse_modality(&modality);
            let dataset = DatasetLoader::load_from_file(&queries)?;

            println!(
                "Loaded {} queries. Evaluating modes: {:?} at K={}",
                dataset.queries.len(),
                modes_list,
                k
            );

            let index_dir = corpus.join(".index");
            let config_path = corpus.join("corpus.toml");
            let config: CorpusConfig = if config_path.exists() {
                let s = fs::read_to_string(&config_path)?;
                toml::from_str(&s)?
            } else {
                CorpusConfig { path: corpus.to_string_lossy().to_string(), ..Default::default() }
            };

            let engine = Engine::open(config, &index_dir)?;
            let summaries =
                BenchmarkSuite::evaluate_modes(&engine, &dataset, &modes_list, k, mod_enum)?;

            let report = BenchmarkSuiteReport {
                timestamp_unix: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                query_count: dataset.queries.len(),
                k,
                modes: summaries,
                indexing: None,
            };

            output_report(&report, output.as_deref())?;
        }

        Commands::All { corpus, queries, modes, k, clean, reembed, output_dir } => {
            let modes_list = parse_modes(&modes)?;
            let dataset = DatasetLoader::load_from_file(&queries)?;

            println!("=== 1. Profiling Indexing Pipeline ===");
            let opts =
                IndexProfilerOptions { include_dense_embedding: reembed, clean_cold_start: clean };
            let idx_report = IndexProfiler::profile(&corpus, &opts)?;
            println!(
                "Indexed {} documents in {:.2}ms ({:.1} files/s, peak RSS: {:.1}MB)",
                idx_report.documents_indexed,
                idx_report.total_elapsed_ms,
                idx_report.docs_per_second,
                idx_report.memory.peak_mb()
            );

            println!("\n=== 2. Running Retrieval Ablation ===");
            let index_dir = corpus.join(".index");
            let config_path = corpus.join("corpus.toml");
            let config: CorpusConfig = if config_path.exists() {
                let s = fs::read_to_string(&config_path)?;
                toml::from_str(&s)?
            } else {
                CorpusConfig { path: corpus.to_string_lossy().to_string(), ..Default::default() }
            };

            let engine = Engine::open(config, &index_dir)?;
            let summaries =
                BenchmarkSuite::evaluate_modes(&engine, &dataset, &modes_list, k, Modality::Both)?;

            let suite_report = BenchmarkSuiteReport {
                timestamp_unix: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                query_count: dataset.queries.len(),
                k,
                modes: summaries,
                indexing: Some(idx_report),
            };

            if let Some(dir) = output_dir {
                fs::create_dir_all(&dir)?;
                let md_str = MarkdownReporter::render(&suite_report);
                let json_str = JsonReporter::to_string(&suite_report)?;
                let csv_str = CsvReporter::render_retrieval_csv(&suite_report);

                fs::write(dir.join("report.md"), md_str)?;
                fs::write(dir.join("report.json"), json_str)?;
                fs::write(dir.join("report.csv"), csv_str)?;
                println!(
                    "Saved reports (report.md, report.json, report.csv) to: {}",
                    dir.display()
                );
            } else {
                let md = MarkdownReporter::render(&suite_report);
                println!("\n{md}");
            }
        }
    }

    Ok(())
}

fn output_report(
    report: &BenchmarkSuiteReport,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let md = MarkdownReporter::render(report);
    if let Some(path) = output {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("md");
        match ext {
            "json" => {
                let json_str = JsonReporter::to_string(report)?;
                fs::write(path, json_str)?;
            }
            "csv" => {
                let csv_str = CsvReporter::render_retrieval_csv(report);
                fs::write(path, csv_str)?;
            }
            _ => {
                fs::write(path, &md)?;
            }
        }
        println!("Report saved to: {}", path.display());
    } else {
        println!("{md}");
    }
    Ok(())
}
