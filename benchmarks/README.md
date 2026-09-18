# ctxvault Benchmarking Suite & Evaluation Pipeline

The `benchmarks/` directory provides an automated, publication-grade evaluation harness for benchmarking `ctxvault`'s retrieval quality, latency, index size, and memory footprint across standard IR datasets:
- **SWE-bench Lite**: Issue-to-patch code localization across real-world Python repositories (`pallets/flask`, `psf/requests`, `astropy/astropy`).
- **CodeSearchNet-AdvTest**: Polyglot natural language code search across Go and Java repositories (`coreos/go-omaha`, `grokify/gotilla`, `tylertreat/BoomFilters`, `wcharczuk/go-chart`).
- **RepoBench-R**: Cross-file, multi-module repository retrieval tasks (`DLYuanGod/TinyGPT-V`, `Meituan-AutoML/MobileVLM`, `ali-vilab/dreamtalk`, `jianchang512/vocal-separate`).

All metrics are compiled into a unified master leaderboard in [`results/summary_report.md`](file:///c:/dev/ctx/ctxvault/benchmarks/results/summary_report.md) and [`results/summary_report.csv`](file:///c:/dev/ctx/ctxvault/benchmarks/results/summary_report.csv).

---

## 1. Directory Structure

```
benchmarks/
├── data/                                 # Committed ground-truth query datasets
│   ├── codesearchnet.json                # CodeSearchNet-AdvTest converted queries
│   ├── repobench.json                    # RepoBench-R converted queries
│   └── swe_bench.json                    # SWE-bench Lite converted queries
├── results/                              # Evaluation outputs and published metrics
│   ├── codesearchnet/                    # Per-repo reports (*_report.md, *_report.csv)
│   ├── repobench/                        # Per-repo reports (*_report.md, *_report.csv)
│   ├── swe_bench/                        # Per-repo reports (*_report.md, *_report.csv)
│   ├── summary_report.md                 # Unified master leaderboard (Markdown)
│   └── summary_report.csv                # Unified master data table (CSV)
├── workspace/                            # Staging workspace for clones & converted data
│   └── repos/                            # Cloned target evaluation repositories
├── run-benchmark-pipeline.ps1            # Primary Windows PowerShell pipeline runner
├── run-benchmark-pipeline.sh             # Linux/macOS Bash pipeline runner
└── README.md                             # This documentation
```

---

## 2. Pipeline Execution Modes

The pipeline runner script ([`run-benchmark-pipeline.ps1`](file:///c:/dev/ctx/ctxvault/benchmarks/run-benchmark-pipeline.ps1) on Windows, [`run-benchmark-pipeline.sh`](file:///c:/dev/ctx/ctxvault/benchmarks/run-benchmark-pipeline.sh) on Linux/macOS) automates binary compilation, repository cloning, indexing with resource profiling, multi-mode retrieval evaluation, and master leaderboard aggregation.

### Mode A: Standard Fast Evaluation (Existing Healthy Indices)
Evaluates all 11 repositories across all datasets without re-indexing repositories that already possess a valid `.index`:

```powershell
# Windows PowerShell
powershell -ExecutionPolicy Bypass -File .\benchmarks\run-benchmark-pipeline.ps1 -Release -SkipIndex
```

```bash
# Linux / macOS Bash
./benchmarks/run-benchmark-pipeline.sh
```

### Mode B: Clean Cold-Start Re-Indexing
Forces a complete, from-scratch rebuild of all Tantivy, SQLite, Petgraph, and HNSW indices with hardware resource profiling before running retrieval:

```powershell
powershell -ExecutionPolicy Bypass -File .\benchmarks\run-benchmark-pipeline.ps1 -Release -CleanIndex
```

```bash
CLEAN_INDEX=1 ./benchmarks/run-benchmark-pipeline.sh
```

### Mode C: Fast Deterministic Subsampling
For rapid sanity checking, deterministically subsamples queries per repository using a fixed seed:

```powershell
powershell -ExecutionPolicy Bypass -File .\benchmarks\run-benchmark-pipeline.ps1 `
  -Release `
  -SkipIndex `
  -FastSample `
  -SamplePerRepo 5 `
  -Seed 42
```

### Mode D: Dataset Filtering
Restrict evaluation to one or more specific benchmark suites:

```powershell
# Run CodeSearchNet only:
powershell -ExecutionPolicy Bypass -File .\benchmarks\run-benchmark-pipeline.ps1 `
  -Release `
  -SkipIndex `
  -Datasets "codesearchnet"

# Run SWE-bench and RepoBench:
powershell -ExecutionPolicy Bypass -File .\benchmarks\run-benchmark-pipeline.ps1 `
  -Release `
  -SkipIndex `
  -Datasets "swe_bench,repobench"
```

### Mode E: Retrieval Algorithm Ablation
Restrict evaluation to specific retrieval modes (default: `bm25,binary,ppr,fast`):

```powershell
# Compare BM25 against fast hybrid RRF only:
powershell -ExecutionPolicy Bypass -File .\benchmarks\run-benchmark-pipeline.ps1 `
  -Release `
  -SkipIndex `
  -Modes "bm25,fast"
```

Available retrieval modes:
- `bm25`: Tantivy Okapi BM25 lexical keyword index.
- `binary`: SIMD Hamming scan over 256-bit SIF Matryoshka binary embeddings.
- `ppr`: Personalized PageRank diffusion over the Petgraph code intelligence graph.
- `fast`: 3-way Reciprocal Rank Fusion (RRF) combining BM25, binary Hamming, and graph PPR.
- `semantic`: 768-dim dense ONNX embeddings via HNSW (`jina-embeddings-v2-base-code`).
- `full`: 4-modality hybrid fusion (BM25 + binary + dense HNSW + graph PPR).

---

## 3. Standalone CLI Operations (`ctxv-bench`)

You can run individual phases directly using the compiled benchmark CLI (`ctxv-bench`):

### Re-aggregating Master Leaderboard (< 1 second)
Recompiles [`summary_report.md`](file:///c:/dev/ctx/ctxvault/benchmarks/results/summary_report.md) and [`summary_report.csv`](file:///c:/dev/ctx/ctxvault/benchmarks/results/summary_report.csv) from all existing sub-report CSVs in `benchmarks/results/`:

```powershell
.\target\release\ctxv-bench.exe aggregate --results-dir benchmarks/results
```

### Evaluating a Single Repository
Evaluate a single target repository on specific queries without touching any other repo:

```powershell
.\target\release\ctxv-bench.exe eval `
  --corpus benchmarks/workspace/repos/codesearchnet/grokify__gotilla `
  --queries benchmarks/data/codesearchnet.json `
  --modes bm25,binary,ppr,fast `
  --k 10 `
  --output-dir benchmarks/results/codesearchnet `
  --output-prefix grokify__gotilla `
  --benchmark-name codesearchnet `
  --repository grokify/gotilla `
  --modality code
```

### Profiling / Indexing a Single Repository
Run hardware-accelerated indexing with memory, disk, and throughput profiling:

```powershell
.\target\release\ctxv-bench.exe index `
  --corpus benchmarks/workspace/repos/codesearchnet/grokify__gotilla `
  --clean `
  --output benchmarks/results/codesearchnet/index_profile_grokify__gotilla.json
```

### Importing / Converting an External Dataset
Convert an external JSON/JSONL dataset file into ctxvault's unified evaluation format:

```powershell
.\target\release\ctxv-bench.exe import `
  --input path/to/raw_dataset.json `
  --format codesearchnet `
  --output benchmarks/data/codesearchnet.json
```
Supported formats: `codesearchnet` (or `csn`), `repobench` (or `rb`), `swebench` (or `swe`).

---

## 4. Evaluation Metrics Reference

| Metric | Definition | Interpretation |
|---|---|---|
| **Recall@K** | $\frac{\|R \cap \text{Top-}K\|}{\|R\|}$ | Fraction of ground-truth relevant files retrieved in the top $K$ results. |
| **Precision@K** | $\frac{\|R \cap \text{Top-}K\|}{K}$ | Fraction of top-$K$ retrieved results that are relevant ground truth. |
| **MRR@K** | $\frac{1}{\text{rank}_1}$ | Mean Reciprocal Rank; reciprocal of the rank of the first relevant result (0 if not in top $K$). |
| **NDCG@K** | $\frac{\text{DCG}_K}{\text{IDCG}_K}$ | Normalized Discounted Cumulative Gain accounting for graded relevance positions. |
| **Sep Ratio** | $\frac{\text{score}_1}{\text{score}_K}$ | Ratio between the top hit score and the cutoff hit score; measures scoring confidence margin. |
| **Latency p50 / p95 / p99** | Milliseconds | Median, 95th, and 99th percentile end-to-end query retrieval latency. |
| **QPS** | $\frac{N}{\sum t}$ | Measured sustained throughput in queries per second. |

---

## 5. Script Parameters Cheat Sheet

### `run-benchmark-pipeline.ps1`

| Parameter | Type | Default | Description |
|---|---|---|---|
| `-WorkDir` | string | `.\benchmarks\workspace` | Staging workspace path for clones and data. |
| `-OutputDir` | string | `.\benchmarks\results` | Output directory for markdown and CSV reports. |
| `-Datasets` | string[] | `@("swe_bench", "codesearchnet", "repobench")` | Benchmark suites to execute. |
| `-Modes` | string[] | `@("bm25", "binary", "ppr", "fast")` | Retrieval algorithms to evaluate. |
| `-K` | int | `10` | Cutoff rank $K$ for IR metrics. |
| `-Release` | switch | `$false` | Build and invoke `ctxv-bench` with `--release`. |
| `-SkipIndex` | switch | `$false` | Skip indexing if healthy `.index/meta.db` exists. |
| `-CleanIndex` | switch | `$false` | Force fresh cold-start re-indexing. |
| `-FastSample` | switch | `$false` | Enable deterministic query subsampling. |
| `-SamplePerRepo` | int | `10` | Number of queries per repo when subsampling. |
| `-Seed` | uint32 | `42` | PRNG seed for deterministic query sampling. |
| `-IncludeJson` | switch | `$false` | Export raw JSON metrics alongside Markdown & CSV. |
| `-IncludeTex` | switch | `$false` | Export LaTeX tables for publication drafting. |
