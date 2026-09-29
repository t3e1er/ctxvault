use std::path::{Path, PathBuf};

use groundcontrol_common::{Error, Result};

/// Supported embedding model names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelName {
    /// jinaai/jina-embeddings-v2-base-code (768 dimensions, 8192 token window, code + NL, INT8 dynamic quantization).
    JinaEmbeddingsV2BaseCode,
}

impl ModelName {
    /// Get output dimensions for this model.
    pub fn dimensions(&self) -> usize {
        768
    }

    /// Parse a model name string into a `ModelName`.
    pub fn from_str_name(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        let name = if let Some(idx) = lower.find('/') { &lower[idx + 1..] } else { &lower };
        match name {
            "jina-embeddings-v2-base-code"
            | "jina-embeddings-v2-base-code-int8"
            | "jina-code-int8"
            | "jina-code"
            | "jina" => Some(Self::JinaEmbeddingsV2BaseCode),
            _ if lower.contains("jina") => Some(Self::JinaEmbeddingsV2BaseCode),
            _ => None,
        }
    }

    /// Get the canonical version string for this model.
    pub fn version_string(&self) -> &'static str {
        "jina-embeddings-v2-base-code-int8"
    }

    /// Directory name for sidecar model storage.
    pub fn model_dir_name(&self) -> &'static str {
        "jina-embeddings-v2-base-code"
    }

    /// Candidate ONNX subpaths to probe within a model directory.
    pub fn onnx_candidate_subpaths(&self) -> &[&'static str] {
        &["onnx/model_quantized.onnx"]
    }

    /// Maximum context token sequence length.
    pub fn max_seq_len(&self) -> usize {
        1024
    }
}

impl Default for ModelName {
    fn default() -> Self {
        Self::JinaEmbeddingsV2BaseCode
    }
}

/// Helper to check whether a directory contains tokenizer.json and any of the candidate ONNX models.
pub(crate) fn check_directory_for_model(
    dir: &Path,
    candidate_subpaths: &[&'static str],
) -> Option<(PathBuf, PathBuf)> {
    let tokenizer_path = dir.join("tokenizer.json");
    if !tokenizer_path.exists() {
        return None;
    }
    for sub in candidate_subpaths {
        let onnx_path = dir.join(sub);
        if onnx_path.exists() {
            return Some((onnx_path, tokenizer_path));
        }
    }
    None
}

/// Locate the ONNX model + `tokenizer.json` using sidecar resolution.
pub(crate) fn resolve_model_files(model_name: &ModelName) -> Result<(PathBuf, PathBuf)> {
    let candidate_subpaths = model_name.onnx_candidate_subpaths();

    // Priority 1: Check GROUNDCONTROL_MODELS_DIR and CTX_MODELS_DIR
    let env_models_dir =
        std::env::var("GROUNDCONTROL_MODELS_DIR").or_else(|_| std::env::var("CTX_MODELS_DIR"));
    if let Ok(models_dir) = env_models_dir {
        let base = PathBuf::from(models_dir);
        let candidates = [base.join(model_name.model_dir_name()), base];
        for dir in candidates {
            if let Some((onnx, tok)) = check_directory_for_model(&dir, candidate_subpaths) {
                tracing::info!(
                    model = %model_name.version_string(),
                    onnx = %onnx.display(),
                    "found model in env models dir"
                );
                return Ok((onnx, tok));
            }
        }
    }

    // Priority 2: Check central cache directory (${GROUNDCONTROL_CACHE_DIR}/models/<model>/)
    let central_cache_dir = groundcontrol_common::config::get_models_cache_dir();
    let central_candidates =
        [central_cache_dir.join(model_name.model_dir_name()), central_cache_dir];
    for dir in central_candidates {
        if let Some((onnx, tok)) = check_directory_for_model(&dir, candidate_subpaths) {
            tracing::info!(
                model = %model_name.version_string(),
                onnx = %onnx.display(),
                "found model in central cache dir"
            );
            return Ok((onnx, tok));
        }
    }

    // Priority 3: Check sidecar directory relative to executable (<exe_dir>/models/<model>/)
    // and workspace ancestor directories (<exe_dir>/../../models/<model>/)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for ancestor in exe_dir.ancestors().take(5) {
                let cand = ancestor.join("models").join(model_name.model_dir_name());
                if let Some((onnx, tok)) = check_directory_for_model(&cand, candidate_subpaths) {
                    tracing::info!(
                        model = %model_name.version_string(),
                        onnx = %onnx.display(),
                        "found sidecar/workspace ONNX model and tokenizer"
                    );
                    return Ok((onnx, tok));
                }
            }
        }
    }

    // Priority 4: Check current working directory and its ancestors
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(5) {
            let cand = ancestor.join("models").join(model_name.model_dir_name());
            if let Some((onnx, tok)) = check_directory_for_model(&cand, candidate_subpaths) {
                tracing::info!(
                    model = %model_name.version_string(),
                    onnx = %onnx.display(),
                    "found model in cwd ancestor"
                );
                return Ok((onnx, tok));
            }
        }
    }

    Err(Error::Index(format!(
        "Embedding model '{}' not found. Mirror the Hugging Face repo layout: place \
         'onnx/model_quantized.onnx' and 'tokenizer.json' under '<exe_dir>/models/{}/' \
         (or set 'GROUNDCONTROL_MODELS_DIR' to the parent models directory). Run scripts/fetch-model.sh \
         (or scripts/fetch-model.ps1) to download them.",
        model_name.version_string(),
        model_name.model_dir_name()
    )))
}
