#!/usr/bin/env bash
# run-benchmark-pipeline.sh
# ArXiv-Grade Multi-Repo & Multi-Dataset Evaluation Pipeline for ctxvault.
set -euo pipefail

WORK_DIR="${1:-./benchmarks/workspace}"
OUTPUT_DIR="${2:-./benchmarks/publication_results}"
DATASETS="${3:-all}"
MODES="${4:-bm25,binary,ppr,fast,semantic}"
K="${5:-10}"

echo "=== Phase 1: Building Benchmark Harness (ctxv-bench) ==="
cargo build --release -p ctxvault-bench
BENCH_EXE="./target/release/ctxv-bench"

mkdir -p "$WORK_DIR/data" "$WORK_DIR/converted" "$WORK_DIR/repos" "$OUTPUT_DIR"

echo "=== Phase 2: Indexing Corpus & Profiling ==="
"$BENCH_EXE" index --corpus . --output "$OUTPUT_DIR/index_profiling_report.json"

echo "=== Phase 3: Evaluating Public Benchmarks (CodeSearchNet / RepoBench / SWE-bench) ==="
if [ -f "$WORK_DIR/converted/csn_converted.json" ]; then
  "$BENCH_EXE" eval \
    --corpus . \
    --queries "$WORK_DIR/converted/csn_converted.json" \
    --modes "$MODES" \
    --k "$K" \
    --output "$OUTPUT_DIR/csn_report.md"

  "$BENCH_EXE" eval \
    --corpus . \
    --queries "$WORK_DIR/converted/csn_converted.json" \
    --modes "$MODES" \
    --k "$K" \
    --output "$OUTPUT_DIR/csn_table.tex"
fi

echo "=== Benchmark Pipeline Complete ==="
echo "Results saved to $OUTPUT_DIR"
