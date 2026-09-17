#!/usr/bin/env bash
# run-benchmark-pipeline.sh
# ArXiv-Grade Multi-Repo & Multi-Dataset Evaluation Pipeline for ctxvault.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

WORK_DIR="${1:-$SCRIPT_DIR/workspace}"
OUTPUT_DIR="${2:-$SCRIPT_DIR/results}"
MODES="${3:-bm25,binary,ppr,fast}"
K="${4:-10}"

echo "=== Phase 1: Building Benchmark Harness (ctxv-bench) ==="
cargo build --release -p ctxvault-bench
BENCH_EXE="$ROOT_DIR/target/release/ctxv-bench"

mkdir -p \
  "$WORK_DIR/data/swe_bench" "$WORK_DIR/data/codesearchnet" "$WORK_DIR/data/repobench" \
  "$WORK_DIR/converted/swe_bench" "$WORK_DIR/converted/codesearchnet" "$WORK_DIR/converted/repobench" \
  "$WORK_DIR/repos/swe_bench" "$WORK_DIR/repos/codesearchnet" "$WORK_DIR/repos/repobench" \
  "$OUTPUT_DIR/swe_bench" "$OUTPUT_DIR/codesearchnet" "$OUTPUT_DIR/repobench"

echo "=== Phase 2: Indexing and Evaluating ==="
# Index and evaluate SWE-bench converted queries if present
if [ -f "$WORK_DIR/converted/swe_bench/swebench_converted.json" ]; then
  for repo_dir in "$WORK_DIR/repos/swe_bench"/*/; do
    if [ -d "$repo_dir" ]; then
      repo_name="$(basename "$repo_dir" | tr '__' '/')"
      prefix="$(basename "$repo_dir")"
      echo "--> Evaluating SWE-bench on [$repo_name]..."
      "$BENCH_EXE" index --corpus "$repo_dir" --output "$OUTPUT_DIR/swe_bench/index_profile_${prefix}.json"
      "$BENCH_EXE" eval \
        --corpus "$repo_dir" \
        --queries "$WORK_DIR/converted/swe_bench/swebench_converted.json" \
        --modes "$MODES" \
        --k "$K" \
        --category "$repo_name" \
        --benchmark-name "swe_bench" \
        --repository "$repo_name" \
        --output-dir "$OUTPUT_DIR/swe_bench" \
        --output-prefix "$prefix"
    fi
  done
fi

echo "=== Phase 3: Master Leaderboard Aggregation ==="
"$BENCH_EXE" aggregate --results-dir "$OUTPUT_DIR" --output-dir "$OUTPUT_DIR"
echo "Master reports generated at $OUTPUT_DIR/summary_report.md and $OUTPUT_DIR/summary_report.csv"
