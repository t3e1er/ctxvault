#!/usr/bin/env bash
#
# fetch-model.sh — download the ctxvault embedding model into a sidecar layout
# that mirrors the upstream Hugging Face repo 1:1 (no renaming).
#
# Downloads the INT8-quantized ONNX weights + tokenizer for
# `jinaai/jina-embeddings-v2-base-code` into the exact upstream paths, so the
# ctxvault embedder (see `resolve_model_files` in
# crates/ctxvault-core/src/embedding.rs) finds them as-is:
#
#   <MODELS_DIR>/jina-embeddings-v2-base-code/onnx/model_quantized.onnx
#   <MODELS_DIR>/jina-embeddings-v2-base-code/tokenizer.json
#
# The embedder resolves this via any of (in priority order):
#   1. CTX_MODELS_DIR                         (set to <MODELS_DIR>)
#   2. <exe_dir>/models/<model>/              (production sidecar next to binary)
#   3. <exe_dir>/../models/<model>/           (cargo test/deps: target/<profile>/models/)
#
# Usage:
#   scripts/fetch-model.sh [MODELS_DIR]
#
# MODELS_DIR defaults to $CTX_MODELS_DIR, then ./models (repo-local; gitignored).
# The simplest, build-profile-independent approach is to export
# CTX_MODELS_DIR=<MODELS_DIR> so `cargo test` finds the model.
#
# Idempotent: skips download when files already exist with the expected size.
set -euo pipefail

# Upstream model (Apache-2.0). Revision pinned for reproducibility.
HF_REPO="jinaai/jina-embeddings-v2-base-code"
HF_REVISION="516f4baf13dec4ddddda8631e019b5737c8bc250"
MODEL_DIR_NAME="jina-embeddings-v2-base-code"

# Files mirrored verbatim from the HF repo: "<relative path> <expected bytes>".
# model_quantized.onnx = INT8 dynamic quantization (preferred, smallest).
FILES=(
  "onnx/model_quantized.onnx 161895621"
  "tokenizer.json 2561316"
)

MODELS_DIR="${1:-${CTX_MODELS_DIR:-./models}}"
DEST_DIR="${MODELS_DIR}/${MODEL_DIR_NAME}"
BASE_URL="https://huggingface.co/${HF_REPO}/resolve/${HF_REVISION}"

file_size() {
  # Portable byte size (Linux `stat -c`, macOS/BSD `stat -f`).
  stat -c%s "$1" 2>/dev/null || stat -f%z "$1" 2>/dev/null || echo 0
}

download() {
  local rel="$1" want_bytes="$2"
  local dest="${DEST_DIR}/${rel}"
  mkdir -p "$(dirname "$dest")"
  if [ -f "$dest" ] && [ "$(file_size "$dest")" = "$want_bytes" ]; then
    echo "[=] ${rel} already present (${want_bytes} bytes), skipping."
    return 0
  fi
  echo "[*] Downloading ${rel} (${want_bytes} bytes)..."
  curl -fSL --retry 3 --retry-delay 2 "${BASE_URL}/${rel}" -o "${dest}.part"
  local got_bytes
  got_bytes="$(file_size "${dest}.part")"
  if [ "$got_bytes" != "$want_bytes" ]; then
    rm -f "${dest}.part"
    echo "[ERROR] size mismatch for ${rel}: got ${got_bytes}, expected ${want_bytes}" >&2
    exit 1
  fi
  mv "${dest}.part" "$dest"
  echo "[+] ${rel} ok."
}

mkdir -p "$DEST_DIR"
for entry in "${FILES[@]}"; do
  download $entry
done

echo ""
echo "[+] Model ready at: ${DEST_DIR} (mirrors the Hugging Face repo layout)"
echo "    Point ctxvault at it with:  export CTX_MODELS_DIR=\"$(cd "$MODELS_DIR" && pwd)\""
