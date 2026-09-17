#!/usr/bin/env bash
# Forwarding shim to benchmarks/run-benchmark-pipeline.sh
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_SCRIPT="$SCRIPT_DIR/../benchmarks/run-benchmark-pipeline.sh"

exec "$TARGET_SCRIPT" "$@"
