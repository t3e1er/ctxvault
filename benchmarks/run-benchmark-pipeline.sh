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
CLEAN_INDEX="${CLEAN_INDEX:-0}"

echo "=== Phase 1: Building Benchmark Harness (ctxv-bench) ==="
cargo build --release -p ctxvault-bench
BENCH_EXE="$ROOT_DIR/target/release/ctxv-bench"

mkdir -p \
  "$WORK_DIR/data/swe_bench" "$WORK_DIR/data/codesearchnet" "$WORK_DIR/data/repobench" \
  "$WORK_DIR/converted/swe_bench" "$WORK_DIR/converted/codesearchnet" "$WORK_DIR/converted/repobench" \
  "$WORK_DIR/repos/swe_bench" "$WORK_DIR/repos/codesearchnet" "$WORK_DIR/repos/repobench" \
  "$OUTPUT_DIR/swe_bench" "$OUTPUT_DIR/codesearchnet" "$OUTPUT_DIR/repobench"

# Stage committed reference queries if available
if [ -f "$SCRIPT_DIR/data/swe_bench.json" ] && [ ! -f "$WORK_DIR/converted/swe_bench/swebench_converted.json" ]; then
  cp "$SCRIPT_DIR/data/swe_bench.json" "$WORK_DIR/converted/swe_bench/swebench_converted.json"
fi

if [ -f "$SCRIPT_DIR/data/codesearchnet.json" ] && [ ! -f "$WORK_DIR/converted/codesearchnet/csn_converted.json" ]; then
  cp "$SCRIPT_DIR/data/codesearchnet.json" "$WORK_DIR/converted/codesearchnet/csn_converted.json"
fi

if [ -f "$SCRIPT_DIR/data/repobench.json" ] && [ ! -f "$WORK_DIR/converted/repobench/repobench_converted.json" ]; then
  cp "$SCRIPT_DIR/data/repobench.json" "$WORK_DIR/converted/repobench/repobench_converted.json"
fi

echo "=== Phase 2: Indexing and Evaluating ==="

eval_suite() {
  local suite="$1"
  local queries_file="$2"
  local repos_dir="$WORK_DIR/repos/$suite"
  local out_suite_dir="$OUTPUT_DIR/$suite"

  if [ -f "$queries_file" ] && [ -d "$repos_dir" ]; then
    for repo_dir in "$repos_dir"/*/; do
      if [ -d "$repo_dir" ]; then
        local repo_name
        repo_name="$(basename "$repo_dir" | tr '__' '/')"
        local prefix
        prefix="$(basename "$repo_dir")"

        echo "--> Processing [$suite] repo: $repo_name..."
        local idx_args=("index" "--corpus" "$repo_dir" "--output" "$out_suite_dir/index_profile_${prefix}.json")
        if [ "$CLEAN_INDEX" = "1" ] || [ ! -f "$repo_dir/.index/meta.db" ]; then
          idx_args+=("--clean")
        fi
        "$BENCH_EXE" "${idx_args[@]}"

        echo "--> Evaluating [$suite] on [$repo_name] (Modes: $MODES, K=$K)..."
        "$BENCH_EXE" eval \
          --corpus "$repo_dir" \
          --queries "$queries_file" \
          --modes "$MODES" \
          --k "$K" \
          --category "$repo_name" \
          --benchmark-name "$suite" \
          --repository "$repo_name" \
          --output-dir "$out_suite_dir" \
          --output-prefix "$prefix"
      fi
    done
  fi
}

eval_suite "swe_bench" "$WORK_DIR/converted/swe_bench/swebench_converted.json"
eval_suite "codesearchnet" "$WORK_DIR/converted/codesearchnet/csn_converted.json"
eval_suite "repobench" "$WORK_DIR/converted/repobench/repobench_converted.json"

echo "=== Phase 3: Master Leaderboard Aggregation ==="
"$BENCH_EXE" aggregate --results-dir "$OUTPUT_DIR" --output-dir "$OUTPUT_DIR"
echo "Master reports generated at $OUTPUT_DIR/summary_report.md and $OUTPUT_DIR/summary_report.csv"
