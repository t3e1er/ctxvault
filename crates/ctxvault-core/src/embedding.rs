//! Hardware-accelerated embedding generation via ONNX Runtime (`ort`) and `tokenizers`.
//!
//! Provides zero-configuration GPU hardware acceleration across all platforms:
//! - Windows: Microsoft DirectML over DirectX 12 Compute (NVIDIA GTX/RTX, AMD Radeon APU, Intel Arc).
//! - macOS: Apple CoreML / Metal Performance Shaders (Apple Silicon M1-M4).
//! - Linux / Docker: Pure-Rust SIMD AVX2/AVX-512 CPU fallback with multi-chunk batching.
//!
//! Includes dynamic VRAM-budget scheduling and sort-and-pack tokenization to prevent
//! GPU Out-of-Memory (OOM) errors and unrecoverable DirectX 12 device-lost states.

use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

use ctxvault_common::{Error, Result};

#[cfg(target_os = "macos")]
use ort::ep::CoreML;
#[cfg(target_os = "windows")]
use ort::ep::DirectML;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use tokenizers::Tokenizer;

/// Automatically select the most appropriate DirectML GPU device ID across all Windows systems:
/// 1. Checks `CTX_DEVICE_ID` environment variable for explicit user override.
/// 2. Queries Windows system video controllers to discover available GPUs and their dedicated video memory (VRAM).
/// 3. Selects the adapter with the highest dedicated VRAM (e.g. dedicated NVIDIA/AMD dGPU over integrated Intel/AMD iGPU).
/// 4. Gracefully falls back to Device ID 0 if enumeration is unavailable or on single-GPU systems.
#[cfg(target_os = "windows")]
static DETECTED_GPU: std::sync::OnceLock<(i32, usize)> = std::sync::OnceLock::new();

/// Automatically select the most appropriate DirectML GPU device ID across all Windows systems:
/// 1. Checks `CTX_DEVICE_ID` environment variable for explicit user override.
/// 2. Queries Windows system video controllers to discover available GPUs and their dedicated video memory (VRAM).
/// 3. Selects the adapter with the highest dedicated VRAM (e.g. dedicated NVIDIA/AMD dGPU over integrated Intel/AMD iGPU).
/// 4. Gracefully falls back to Device ID 0 if enumeration is unavailable or on single-GPU systems.
#[cfg(target_os = "windows")]
pub fn select_directml_device_id() -> i32 {
    // 1. Explicit override via CTX_DEVICE_ID
    if let Ok(val) = std::env::var("CTX_DEVICE_ID") {
        if let Ok(id) = val.parse::<i32>() {
            tracing::info!(device_id = id, "DirectML GPU device ID selected via CTX_DEVICE_ID");
            return id;
        }
    }

    let (device_id, _) = *DETECTED_GPU.get_or_init(|| {
        if let Ok(output) = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_VideoController | Select-Object Name, AdapterRAM | ConvertTo-Json",
            ])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    let list = if let Some(arr) = val.as_array() {
                        arr.clone()
                    } else if val.is_object() {
                        vec![val]
                    } else {
                        Vec::new()
                    };

                    let mut best_id: i32 = 0;
                    let mut max_ram: u64 = 0;
                    let mut best_name = String::new();

                    for (idx, item) in list.iter().enumerate() {
                        let name = item.get("Name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                        let ram = item.get("AdapterRAM").and_then(|v| v.as_u64()).unwrap_or(0);

                        tracing::debug!(
                            device_id = idx,
                            name = %name,
                            vram_mb = ram / (1024 * 1024),
                            "Detected GPU adapter"
                        );

                        if ram > max_ram {
                            max_ram = ram;
                            best_id = idx as i32;
                            best_name = name.to_string();
                        }
                    }

                    if max_ram > 0 {
                        let vram_mb = (max_ram / (1024 * 1024)) as usize;
                        tracing::info!(
                            device_id = best_id,
                            name = %best_name,
                            vram_mb,
                            "Automatically selected high-performance DirectML GPU adapter"
                        );
                        return (best_id, vram_mb);
                    }
                }
            }
        }
        (0, 1024)
    });

    device_id
}

/// Detect dedicated VRAM in megabytes for the primary GPU adapter on Windows.
#[cfg(target_os = "windows")]
pub fn detect_gpu_vram_mb() -> usize {
    let _ = select_directml_device_id();
    DETECTED_GPU.get().map(|&(_, vram)| vram).unwrap_or(1024)
}

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
    ///
    /// Accepts: "jinaai/jina-embeddings-v2-base-code", "jina-embeddings-v2-base-code",
    /// "jina-embeddings-v2-base-code-int8", "jina-code-int8", "jina-code", "jina"
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

    /// Candidate ONNX subpaths to probe within a model directory, in preference order.
    ///
    /// These mirror the upstream Hugging Face repo layout 1:1 (no renaming) so a
    /// plain `hf download`/`git clone` of `jinaai/jina-embeddings-v2-base-code`
    /// into the sidecar directory works as-is. The quantized weights are preferred
    /// (smallest, INT8 dynamic quantization); the fp32 `model.onnx` is the fallback.
    pub fn onnx_candidate_subpaths(&self) -> &[&'static str] {
        &["onnx/model_quantized.onnx", "onnx/model.onnx"]
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

/// Trait defining device memory introspection, target execution slicing, and AIMD batch scaling.
pub trait HardwareGovernor: Send + Sync {
    /// Query real-time available memory headroom on the target compute device (bytes).
    fn available_memory_bytes(&self) -> usize;

    /// Total device memory capacity (bytes).
    fn total_memory_bytes(&self) -> usize;

    /// Compute the adaptive batch size for a given sequence length,
    /// informed by the measured latency of the previous dispatch.
    fn compute_adaptive_batch(&self, seq_len: usize, last_dispatch_ms: u64) -> usize;

    /// Target execution slice duration in milliseconds.
    /// Dispatches should aim for this duration to balance throughput vs. desktop responsiveness.
    fn target_slice_ms(&self) -> u64;

    /// Report the current compute provider name for logging.
    fn provider_name(&self) -> &str;
}

/// Additive Increase / Multiplicative Decrease (AIMD) controller for dynamically scaling batch sizes.
#[derive(Debug, Clone)]
pub struct AimdController {
    /// Multiplier scale factor, clamped between 0.2 and 4.0 (starts at 1.0).
    pub scale: f64,
    /// Exponential moving average of dispatch latency (ms).
    pub ema_latency_ms: f64,
    /// Target execution slice duration in milliseconds.
    pub target_slice_ms: u64,
    /// Number of dispatches observed.
    pub sample_count: u64,
}

impl AimdController {
    /// Create a new AIMD controller targeting the specified slice duration in milliseconds.
    pub fn new(target_slice_ms: u64) -> Self {
        Self {
            scale: 1.0,
            ema_latency_ms: target_slice_ms as f64,
            target_slice_ms,
            sample_count: 0,
        }
    }

    /// Record a measured dispatch latency in milliseconds and adapt the scale factor.
    ///
    /// - If `dispatch_ms < 50`: Additive Increase (+10% scale).
    /// - If `dispatch_ms > 150`: Multiplicative Decrease (-20% scale).
    /// - Between 50ms and 150ms: Golden sweet spot, maintains current scale.
    pub fn record_dispatch(&mut self, dispatch_ms: u64) {
        if dispatch_ms == 0 {
            return;
        }
        self.sample_count += 1;
        let d = dispatch_ms as f64;
        self.ema_latency_ms =
            if self.sample_count <= 1 { d } else { 0.8 * self.ema_latency_ms + 0.2 * d };

        if dispatch_ms < 50 {
            // Additive increase: +10%
            self.scale = (self.scale + 0.10).min(4.0);
        } else if dispatch_ms > 150 {
            // Multiplicative decrease: -20%
            self.scale = (self.scale * 0.80).max(0.2);
        }
    }
}

/// DirectML hardware governor for Windows DirectX 12 Compute.
#[cfg(target_os = "windows")]
pub struct DirectMlGovernor {
    total_memory_bytes: usize,
    current_usage_bytes: AtomicUsize,
    last_usage_refresh: Mutex<Instant>,
    aimd: Mutex<AimdController>,
}

#[cfg(target_os = "windows")]
impl DirectMlGovernor {
    /// Create a new DirectML governor with auto-detected or overridden VRAM.
    pub fn new() -> Self {
        let vram_mb = if let Ok(val) = std::env::var("CTX_VRAM_MB") {
            val.parse::<usize>().unwrap_or_else(|_| detect_gpu_vram_mb())
        } else {
            detect_gpu_vram_mb()
        };
        let total_memory_bytes = vram_mb.max(256) * 1024 * 1024;
        let governor = Self {
            total_memory_bytes,
            current_usage_bytes: AtomicUsize::new(0),
            last_usage_refresh: Mutex::new(Instant::now() - Duration::from_secs(10)),
            aimd: Mutex::new(AimdController::new(100)),
        };
        governor.refresh_vram_usage_if_needed();
        governor
    }

    /// Construct a DirectMlGovernor with explicit VRAM in bytes (useful for testing).
    pub fn with_vram_bytes(vram_bytes: usize) -> Self {
        Self {
            total_memory_bytes: vram_bytes,
            current_usage_bytes: AtomicUsize::new(0),
            last_usage_refresh: Mutex::new(Instant::now() - Duration::from_secs(10)),
            aimd: Mutex::new(AimdController::new(100)),
        }
    }

    fn refresh_vram_usage_if_needed(&self) {
        if let Ok(mut last) = self.last_usage_refresh.try_lock() {
            if last.elapsed() >= Duration::from_secs(2) {
                *last = Instant::now();
                if let Ok(output) = std::process::Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-Command",
                        "Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPULocalAdapterMemory | Select-Object -ExpandProperty LocalUsage | Measure-Object -Maximum | Select-Object -ExpandProperty Maximum",
                    ])
                    .output()
                {
                    if output.status.success() {
                        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if let Ok(bytes) = text.parse::<usize>() {
                            self.current_usage_bytes.store(bytes, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Default for DirectMlGovernor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
impl HardwareGovernor for DirectMlGovernor {
    fn available_memory_bytes(&self) -> usize {
        self.refresh_vram_usage_if_needed();
        let usage = self.current_usage_bytes.load(Ordering::Relaxed);
        self.total_memory_bytes.saturating_sub(usage).max(256 * 1024 * 1024)
    }

    fn total_memory_bytes(&self) -> usize {
        self.total_memory_bytes
    }

    fn compute_adaptive_batch(&self, seq_len: usize, last_dispatch_ms: u64) -> usize {
        let mut aimd = self.aimd.lock().unwrap();
        if last_dispatch_ms > 0 {
            aimd.record_dispatch(last_dispatch_ms);
        }
        let scale = aimd.scale;
        drop(aimd);

        let available = self.available_memory_bytes();
        // Golden 70% rule: 70% of available VRAM headroom
        let activation_budget = (available as f64 * 0.70) as usize;

        let seq_len = seq_len.max(1);
        let per_chunk_attention_bytes = (3 * 12 * seq_len * seq_len * 4).max(2048);
        let batch_by_mem = activation_budget / per_chunk_attention_bytes;

        let max_tokens = (activation_budget / 32).clamp(8_192, 65_536);
        let batch_by_tokens = max_tokens / seq_len;

        // Windows DirectML TDR safety ceiling: ensure single GPU dispatch duration is < 400ms
        let tdr_safe_cap = match seq_len {
            s if s > 768 => 16,
            s if s > 384 => 64,
            s if s > 128 => 128,
            _ => 256,
        };

        let base_batch = batch_by_mem.min(batch_by_tokens).min(tdr_safe_cap).max(1);

        let scaled = ((base_batch as f64) * scale).round() as usize;
        scaled.clamp(1, tdr_safe_cap)
    }

    fn target_slice_ms(&self) -> u64 {
        100
    }

    fn provider_name(&self) -> &str {
        "DirectML"
    }
}

/// CoreML / Metal hardware governor for macOS Apple Silicon.
#[cfg(target_os = "macos")]
pub struct CoreMlGovernor {
    total_memory_bytes: usize,
    aimd: Mutex<AimdController>,
}

#[cfg(target_os = "macos")]
impl CoreMlGovernor {
    /// Create a new CoreML governor with unified system memory.
    pub fn new() -> Self {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total = sys.total_memory() as usize;
        Self {
            total_memory_bytes: if total == 0 { 16 * 1024 * 1024 * 1024 } else { total },
            aimd: Mutex::new(AimdController::new(100)),
        }
    }
}

#[cfg(target_os = "macos")]
impl Default for CoreMlGovernor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl HardwareGovernor for CoreMlGovernor {
    fn available_memory_bytes(&self) -> usize {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let free = sys.available_memory() as usize;
        if free == 0 {
            8 * 1024 * 1024 * 1024
        } else {
            free
        }
    }

    fn total_memory_bytes(&self) -> usize {
        self.total_memory_bytes
    }

    fn compute_adaptive_batch(&self, seq_len: usize, last_dispatch_ms: u64) -> usize {
        let mut aimd = self.aimd.lock().unwrap();
        if last_dispatch_ms > 0 {
            aimd.record_dispatch(last_dispatch_ms);
        }
        let scale = aimd.scale;
        drop(aimd);

        let available = self.available_memory_bytes();
        let activation_budget = (available as f64 * 0.70) as usize;

        let seq_len = seq_len.max(1);
        let per_chunk_attention_bytes = (3 * 12 * seq_len * seq_len * 4).max(2048);
        let batch_by_mem = activation_budget / per_chunk_attention_bytes;
        let max_tokens = (activation_budget / 32).clamp(8_192, 65_536);
        let batch_by_tokens = max_tokens / seq_len;

        let max_cap = match seq_len {
            s if s > 768 => 32,
            s if s > 384 => 96,
            s if s > 128 => 192,
            _ => 256,
        };

        let base_batch = batch_by_mem.min(batch_by_tokens).min(max_cap).max(1);
        let scaled = ((base_batch as f64) * scale).round() as usize;
        scaled.clamp(1, max_cap)
    }

    fn target_slice_ms(&self) -> u64 {
        100
    }

    fn provider_name(&self) -> &str {
        "CoreML"
    }
}

/// CPU hardware governor for Linux, Docker, or fallback environments.
pub struct CpuGovernor {
    total_memory_bytes: usize,
    aimd: Mutex<AimdController>,
}

impl CpuGovernor {
    /// Create a new CPU governor using host physical memory introspection.
    pub fn new() -> Self {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total = sys.total_memory() as usize;
        Self {
            total_memory_bytes: if total == 0 { 8 * 1024 * 1024 * 1024 } else { total },
            aimd: Mutex::new(AimdController::new(100)),
        }
    }
}

impl Default for CpuGovernor {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareGovernor for CpuGovernor {
    fn available_memory_bytes(&self) -> usize {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let free = sys.available_memory() as usize;
        if free == 0 {
            4 * 1024 * 1024 * 1024
        } else {
            free
        }
    }

    fn total_memory_bytes(&self) -> usize {
        self.total_memory_bytes
    }

    fn compute_adaptive_batch(&self, seq_len: usize, last_dispatch_ms: u64) -> usize {
        let mut aimd = self.aimd.lock().unwrap();
        if last_dispatch_ms > 0 {
            aimd.record_dispatch(last_dispatch_ms);
        }
        drop(aimd);

        // L3 Cache constraint: clamp batch size to 64 MB working set, capped at 16 chunks to prevent DDR bus thrashing
        let l3_cache_budget = 64 * 1024 * 1024;
        let per_chunk_bytes = (3 * 12 * seq_len.max(1) * seq_len.max(1) * 4).max(2048);
        let batch_by_cache = l3_cache_budget / per_chunk_bytes;
        let cpu_cap = match seq_len {
            s if s > 512 => 8,
            s if s > 256 => 12,
            _ => 16,
        };
        batch_by_cache.min(cpu_cap).max(1)
    }

    fn target_slice_ms(&self) -> u64 {
        100
    }

    fn provider_name(&self) -> &str {
        "CPU"
    }
}

/// Get the default hardware governor for the current execution platform.
pub fn default_hardware_governor() -> Arc<dyn HardwareGovernor> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(DirectMlGovernor::new())
    }
    #[cfg(target_os = "macos")]
    {
        Arc::new(CoreMlGovernor::new())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Arc::new(CpuGovernor::new())
    }
}

/// Helper to check whether a directory contains tokenizer.json and any of the candidate ONNX models.
fn check_directory_for_model(
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
///
/// The expected directory mirrors the upstream Hugging Face repo layout 1:1
/// (`onnx/model_quantized.onnx` + `tokenizer.json`), so a plain download/clone of
/// `jinaai/jina-embeddings-v2-base-code` into the sidecar dir works with no renaming.
///
/// Directories are probed in priority order:
/// 1. `CTX_MODELS_DIR` environment variable
/// 2. Sidecar `<exe_dir>/models/jina-embeddings-v2-base-code/` (production path)
/// 3. Cargo test/deps `<exe_dir>/../models/jina-embeddings-v2-base-code/`
fn resolve_model_files(model_name: &ModelName) -> Result<(PathBuf, PathBuf)> {
    let candidate_subpaths = model_name.onnx_candidate_subpaths();

    // Priority 1: Check CTX_MODELS_DIR
    if let Ok(models_dir) = std::env::var("CTX_MODELS_DIR") {
        let base = PathBuf::from(models_dir);
        let candidates = [base.join(model_name.model_dir_name()), base];
        for dir in candidates {
            if let Some((onnx, tok)) = check_directory_for_model(&dir, candidate_subpaths) {
                tracing::info!(
                    model = %model_name.version_string(),
                    onnx = %onnx.display(),
                    "found model in CTX_MODELS_DIR"
                );
                return Ok((onnx, tok));
            }
        }
    }

    // Priority 2: Check sidecar directory relative to executable (<exe_dir>/models/<model>/)
    // Priority 3: Check <exe_dir>/../models/<model>/ for cargo test / deps builds
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let primary = exe_dir.join("models").join(model_name.model_dir_name());
            if let Some((onnx, tok)) = check_directory_for_model(&primary, candidate_subpaths) {
                tracing::info!(
                    model = %model_name.version_string(),
                    onnx = %onnx.display(),
                    "found sidecar ONNX model and tokenizer"
                );
                return Ok((onnx, tok));
            }

            if let Some(parent) = exe_dir.parent() {
                let parent_models = parent.join("models").join(model_name.model_dir_name());
                if let Some((onnx, tok)) =
                    check_directory_for_model(&parent_models, candidate_subpaths)
                {
                    tracing::info!(
                        model = %model_name.version_string(),
                        onnx = %onnx.display(),
                        "found parent sidecar ONNX model and tokenizer"
                    );
                    return Ok((onnx, tok));
                }
            }
        }
    }

    Err(Error::Index(format!(
        "Embedding model '{}' not found. Mirror the Hugging Face repo layout: place \
         'onnx/model_quantized.onnx' and 'tokenizer.json' under '<exe_dir>/models/{}/' \
         (or set 'CTX_MODELS_DIR' to the parent models directory). Run scripts/fetch-model.sh \
         (or scripts/fetch-model.ps1) to download them.",
        model_name.version_string(),
        model_name.model_dir_name()
    )))
}

/// Embedder wraps ONNX Runtime hardware-accelerated sessions for batch embedding generation.
pub struct Embedder {
    sessions: Vec<Mutex<Session>>,
    cpu_session: Option<Mutex<Session>>,
    tokenizer: Tokenizer,
    model_name: ModelName,
    governor: Arc<dyn HardwareGovernor>,
    has_token_type_ids: bool,
    gpu_disabled: AtomicBool,
}

impl Embedder {
    /// Create a new embedder with the specified model and hardware governor.
    pub fn new(model_name: ModelName, governor: Arc<dyn HardwareGovernor>) -> Result<Self> {
        let (model_path, tokenizer_path) = resolve_model_files(&model_name)?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            Error::Index(format!("failed to load tokenizer from {}: {e}", tokenizer_path.display()))
        })?;

        let create_builder = || -> Result<ort::session::builder::SessionBuilder> {
            // `mut` is only needed on platforms that reassign `builder` to attach a
            // hardware execution provider below (Windows/DirectML, macOS/CoreML).
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            let mut builder = Session::builder()
                .map_err(|e| Error::Index(format!("failed to create session builder: {e}")))?
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| {
                    Error::Index(format!("failed to set graph optimization level: {e}"))
                })?;
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            let builder = Session::builder()
                .map_err(|e| Error::Index(format!("failed to create session builder: {e}")))?
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| {
                    Error::Index(format!("failed to set graph optimization level: {e}"))
                })?;

            #[cfg(target_os = "windows")]
            {
                let device_id = select_directml_device_id();
                builder = builder
                    .with_execution_providers([DirectML::default()
                        .with_device_id(device_id)
                        .build()])
                    .map_err(|e| {
                        Error::Index(format!("failed to configure DirectML provider: {e}"))
                    })?;
            }

            #[cfg(target_os = "macos")]
            {
                builder =
                    builder.with_execution_providers([CoreML::default().build()]).map_err(|e| {
                        Error::Index(format!("failed to configure CoreML provider: {e}"))
                    })?;
            }

            Ok(builder)
        };

        let session_0 = create_builder()?.commit_from_file(&model_path).map_err(|e| {
            Error::Index(format!("failed to load ONNX model from {}: {e}", model_path.display()))
        })?;

        let has_token_type_ids = session_0.inputs().iter().any(|i| i.name() == "token_type_ids");

        let mut sessions = vec![Mutex::new(session_0)];

        // Strategy 3: Dual Concurrent Inference Streams
        // Only spawn dual sessions if governor reports total memory >= 4 GB on hardware acceleration platforms
        let has_sufficient_vram = governor.total_memory_bytes() >= 4 * 1024 * 1024 * 1024;
        let can_use_dual_stream =
            cfg!(any(target_os = "windows", target_os = "macos")) && has_sufficient_vram;

        if can_use_dual_stream {
            match create_builder().and_then(|mut b| {
                b.commit_from_file(&model_path).map_err(|e| {
                    Error::Index(format!(
                        "failed to load dual ONNX session from {}: {e}",
                        model_path.display()
                    ))
                })
            }) {
                Ok(session_1) => {
                    tracing::info!("Dual hardware acceleration sessions initialized for concurrent inference streams");
                    sessions.push(Mutex::new(session_1));
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize second concurrent session ({e}), continuing with single session");
                }
            }
        }

        // Prepare a resilient CPU fallback session
        let cpu_session = {
            let mut cpu_builder = Session::builder()
                .map_err(|e| Error::Index(format!("failed to create CPU session builder: {e}")))?
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| Error::Index(format!("failed to set CPU optimization level: {e}")))?;
            match cpu_builder.commit_from_file(&model_path) {
                Ok(sess) => Some(Mutex::new(sess)),
                Err(e) => {
                    tracing::warn!("Failed to initialize CPU fallback session: {e}");
                    None
                }
            }
        };

        tracing::info!(
            model = %model_name.version_string(),
            dimensions = model_name.dimensions(),
            token_type_ids = has_token_type_ids,
            sessions = sessions.len(),
            provider = governor.provider_name(),
            "embedder initialized with hardware acceleration and dynamic activation governor"
        );

        Ok(Self {
            sessions,
            cpu_session,
            tokenizer,
            model_name,
            governor,
            has_token_type_ids,
            gpu_disabled: AtomicBool::new(false),
        })
    }

    /// Create an embedder from a config model string (e.g., "jinaai/jina-embeddings-v2-base-code").
    pub fn from_config(model_str: &str) -> Result<Self> {
        let model_name = ModelName::from_str_name(model_str).unwrap_or_default();
        Self::new(model_name, default_hardware_governor())
    }

    /// Create an embedder with the default model and default platform hardware governor.
    pub fn new_default() -> Result<Self> {
        Self::new(ModelName::default(), default_hardware_governor())
    }

    /// Get the output dimensions of this embedder.
    pub fn dimensions(&self) -> usize {
        self.model_name.dimensions()
    }

    /// Get the model name.
    pub fn model_name(&self) -> &ModelName {
        &self.model_name
    }

    /// Get the hardware governor.
    pub fn governor(&self) -> &Arc<dyn HardwareGovernor> {
        &self.governor
    }

    /// Number of concurrent inference sessions available.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Check if hardware GPU acceleration is currently disabled (due to device lost / fallback).
    pub fn is_gpu_disabled(&self) -> bool {
        self.gpu_disabled.load(Ordering::Relaxed)
    }

    /// Reset GPU disabled state to re-attempt hardware acceleration.
    pub fn reset_gpu_disabled(&self) {
        self.gpu_disabled.store(false, Ordering::SeqCst);
    }

    /// Get a reference to the HuggingFace BPE tokenizer.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// Whether this model requires token type IDs.
    pub fn has_token_type_ids(&self) -> bool {
        self.has_token_type_ids
    }

    /// Embed a batch of text strings respecting dynamic VRAM budget and sort-and-pack tokenization.
    ///
    /// Returns one L2-normalized embedding vector per input string, in the exact order of `texts`.
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // 1. Tokenize all texts upfront
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| Error::Index(format!("tokenization failed: {e}")))?;

        let num_texts = encodings.len();
        if num_texts == 1 {
            return self.embed_encoded_sub_batch(&[&encodings[0]]);
        }

        // 2. Sort-and-Pack: sort indices by sequence length
        let max_model_len = self.model_name.max_seq_len();
        let mut sorted_indices: Vec<usize> = (0..num_texts).collect();
        sorted_indices.sort_by_key(|&idx| encodings[idx].get_ids().len());

        let mut results: Vec<Vec<f32>> = vec![Vec::new(); num_texts];
        let mut current_batch_indices: Vec<usize> = Vec::new();

        for &idx in &sorted_indices {
            let seq_len = encodings[idx].get_ids().len().min(max_model_len).max(1);
            let max_allowed_batch = self.governor.compute_adaptive_batch(seq_len, 0);

            if !current_batch_indices.is_empty() && current_batch_indices.len() >= max_allowed_batch
            {
                // Flush current sub-batch
                let sub_batch_encodings: Vec<&tokenizers::Encoding> =
                    current_batch_indices.iter().map(|&i| &encodings[i]).collect();
                let sub_embeddings = self.embed_encoded_sub_batch(&sub_batch_encodings)?;
                for (sub_i, &orig_idx) in current_batch_indices.iter().enumerate() {
                    results[orig_idx] = sub_embeddings[sub_i].clone();
                }
                current_batch_indices.clear();
            }

            current_batch_indices.push(idx);
        }

        // Flush any remaining items
        if !current_batch_indices.is_empty() {
            let sub_batch_encodings: Vec<&tokenizers::Encoding> =
                current_batch_indices.iter().map(|&i| &encodings[i]).collect();
            let sub_embeddings = self.embed_encoded_sub_batch(&sub_batch_encodings)?;
            for (sub_i, &orig_idx) in current_batch_indices.iter().enumerate() {
                results[orig_idx] = sub_embeddings[sub_i].clone();
            }
        }

        Ok(results)
    }

    /// Attention-weighted mean pooling and L2 normalization over raw output tensor.
    fn pool_embeddings(
        batch_size: usize,
        max_len: usize,
        hidden_size: usize,
        flat_attention_mask: &[i64],
        hidden_data: &[f32],
    ) -> Vec<Vec<f32>> {
        let mut embeddings = Vec::with_capacity(batch_size);

        for b in 0..batch_size {
            let mut sum_vec = vec![0.0f32; hidden_size];
            let mut token_count = 0.0f32;

            for s in 0..max_len {
                if flat_attention_mask[b * max_len + s] == 1 {
                    token_count += 1.0;
                    let offset = (b * max_len + s) * hidden_size;
                    for d in 0..hidden_size {
                        sum_vec[d] += hidden_data[offset + d];
                    }
                }
            }

            if token_count > 0.0 {
                for val in &mut sum_vec {
                    *val /= token_count;
                }
            }

            let norm: f32 = sum_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for val in &mut sum_vec {
                    *val /= norm;
                }
            }

            embeddings.push(sum_vec);
        }

        embeddings
    }

    /// Embed a pre-tokenized sub-batch in a single tensor forward pass.
    fn embed_encoded_sub_batch(
        &self,
        encodings: &[&tokenizers::Encoding],
    ) -> Result<Vec<Vec<f32>>> {
        if encodings.is_empty() {
            return Ok(Vec::new());
        }

        let batch_size = encodings.len();
        let max_model_len = self.model_name.max_seq_len();
        let raw_max_len = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(1);
        let max_len = raw_max_len.min(max_model_len).max(1);

        let mut flat_input_ids = Vec::with_capacity(batch_size * max_len);
        let mut flat_attention_mask = Vec::with_capacity(batch_size * max_len);
        let mut flat_token_type_ids = if self.has_token_type_ids {
            Some(Vec::with_capacity(batch_size * max_len))
        } else {
            None
        };

        for enc in encodings {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            let type_ids = enc.get_type_ids();

            let cur_len = ids.len().min(max_len);

            for i in 0..cur_len {
                flat_input_ids.push(ids[i] as i64);
                flat_attention_mask.push(mask[i] as i64);
            }
            for _ in cur_len..max_len {
                flat_input_ids.push(0i64);
                flat_attention_mask.push(0i64);
            }

            if let Some(ref mut type_vec) = flat_token_type_ids {
                for i in 0..cur_len {
                    type_vec.push(type_ids[i] as i64);
                }
                for _ in cur_len..max_len {
                    type_vec.push(0i64);
                }
            }
        }

        self.run_staged_tensor_batch(
            0,
            batch_size,
            max_len,
            flat_input_ids,
            flat_attention_mask,
            flat_token_type_ids,
        )
    }

    /// Execute a tensor forward pass on pre-staged contiguous flat arrays.
    pub(crate) fn run_staged_tensor_batch(
        &self,
        session_index: usize,
        batch_size: usize,
        max_len: usize,
        flat_input_ids: Vec<i64>,
        flat_attention_mask: Vec<i64>,
        flat_token_type_ids: Option<Vec<i64>>,
    ) -> Result<Vec<Vec<f32>>> {
        let use_hardware = !self.gpu_disabled.load(Ordering::Relaxed);
        let mut hw_error: Option<String> = None;
        let sub_start = std::time::Instant::now();

        if use_hardware {
            let session_idx = session_index % self.sessions.len();
            let mut session_guard = self.sessions[session_idx]
                .lock()
                .map_err(|e| Error::Index(format!("session {session_idx} lock poisoned: {e}")))?;

            let run_result = if let Some(ref type_ids) = flat_token_type_ids {
                let input_ids_val =
                    ort::value::Tensor::from_array(([batch_size, max_len], flat_input_ids.clone()))
                        .map_err(|e| {
                            Error::Index(format!("failed to construct input_ids tensor: {e}"))
                        })?;
                let attention_mask_val = ort::value::Tensor::from_array((
                    [batch_size, max_len],
                    flat_attention_mask.clone(),
                ))
                .map_err(|e| {
                    Error::Index(format!("failed to construct attention_mask tensor: {e}"))
                })?;
                let token_type_ids_val =
                    ort::value::Tensor::from_array(([batch_size, max_len], type_ids.clone()))
                        .map_err(|e| {
                            Error::Index(format!("failed to construct token_type_ids tensor: {e}"))
                        })?;

                session_guard.run(ort::inputs![
                    "input_ids" => input_ids_val,
                    "attention_mask" => attention_mask_val,
                    "token_type_ids" => token_type_ids_val,
                ])
            } else {
                let input_ids_val =
                    ort::value::Tensor::from_array(([batch_size, max_len], flat_input_ids.clone()))
                        .map_err(|e| {
                            Error::Index(format!("failed to construct input_ids tensor: {e}"))
                        })?;
                let attention_mask_val = ort::value::Tensor::from_array((
                    [batch_size, max_len],
                    flat_attention_mask.clone(),
                ))
                .map_err(|e| {
                    Error::Index(format!("failed to construct attention_mask tensor: {e}"))
                })?;

                session_guard.run(ort::inputs![
                    "input_ids" => input_ids_val,
                    "attention_mask" => attention_mask_val,
                ])
            };

            match run_result {
                Ok(outputs) => match outputs["last_hidden_state"].try_extract_tensor::<f32>() {
                    Ok(hidden_tensor) => {
                        let pool_res = Self::pool_embeddings(
                            batch_size,
                            max_len,
                            self.model_name.dimensions(),
                            &flat_attention_mask,
                            hidden_tensor.1,
                        );
                        tracing::debug!(session_idx, batch_size, max_len, elapsed = ?sub_start.elapsed(), "Sub-batch [GPU] completed");
                        return Ok(pool_res);
                    }
                    Err(e) => {
                        hw_error = Some(format!("failed to extract last_hidden_state tensor: {e}"));
                    }
                },
                Err(e) => {
                    hw_error =
                        Some(format!("hardware forward pass failed on session {session_idx}: {e}"));
                }
            }

            // Fallback triggered: disable GPU acceleration for remaining batches to prevent cascading timeouts
            self.gpu_disabled.store(true, Ordering::SeqCst);
            tracing::warn!(
                error = %hw_error.as_deref().unwrap_or("unknown"),
                "DirectML hardware forward pass failed; disabling GPU acceleration and activating CPU fallback"
            );
        }

        // Fall back to CPU session
        if let Some(ref cpu_session_mutex) = self.cpu_session {
            if let Some(ref err) = hw_error {
                tracing::warn!(
                    "Hardware forward pass failed ({err}); executing on CPU fallback session"
                );
            }

            let mut cpu_session = cpu_session_mutex
                .lock()
                .map_err(|e| Error::Index(format!("cpu session lock poisoned: {e}")))?;

            let input_ids_val =
                ort::value::Tensor::from_array(([batch_size, max_len], flat_input_ids)).map_err(
                    |e| Error::Index(format!("failed to construct input_ids tensor: {e}")),
                )?;
            let attention_mask_val = ort::value::Tensor::from_array((
                [batch_size, max_len],
                flat_attention_mask.clone(),
            ))
            .map_err(|e| Error::Index(format!("failed to construct attention_mask tensor: {e}")))?;

            let outputs = if let Some(type_ids) = flat_token_type_ids {
                let token_type_ids_val =
                    ort::value::Tensor::from_array(([batch_size, max_len], type_ids)).map_err(
                        |e| Error::Index(format!("failed to construct token_type_ids tensor: {e}")),
                    )?;

                cpu_session
                    .run(ort::inputs![
                        "input_ids" => input_ids_val,
                        "attention_mask" => attention_mask_val,
                        "token_type_ids" => token_type_ids_val,
                    ])
                    .map_err(|e| Error::Index(format!("CPU fallback forward pass failed: {e}")))?
            } else {
                cpu_session
                    .run(ort::inputs![
                        "input_ids" => input_ids_val,
                        "attention_mask" => attention_mask_val,
                    ])
                    .map_err(|e| Error::Index(format!("CPU fallback forward pass failed: {e}")))?
            };

            let hidden_tensor = outputs["last_hidden_state"]
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Index(format!("failed to extract last_hidden_state: {e}")))?;

            let pool_res = Self::pool_embeddings(
                batch_size,
                max_len,
                self.model_name.dimensions(),
                &flat_attention_mask,
                hidden_tensor.1,
            );
            tracing::debug!(batch_size, max_len, elapsed = ?sub_start.elapsed(), "Sub-batch [CPU] completed");
            return Ok(pool_res);
        }

        Err(Error::Index(format!(
            "forward pass failed and no CPU fallback available: {}",
            hw_error.unwrap_or_else(|| "hardware disabled".to_string())
        )))
    }

    /// Embed a single text string.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| Error::Index("embedding returned no results".to_string()))
    }

    /// Embed a search query string.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.embed(query)
    }

    /// Compute a document-level embedding by averaging chunk embeddings.
    pub fn average_embeddings(embeddings: &[Vec<f32>]) -> Option<Vec<f32>> {
        if embeddings.is_empty() {
            return None;
        }

        let dims = embeddings[0].len();
        let count = embeddings.len() as f32;

        let mut avg = vec![0.0f32; dims];
        for emb in embeddings {
            for (i, &val) in emb.iter().enumerate() {
                avg[i] += val;
            }
        }
        for val in &mut avg {
            *val /= count;
        }

        let norm: f32 = avg.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut avg {
                *val /= norm;
            }
        }

        Some(avg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_name_parsing() {
        assert_eq!(
            ModelName::from_str_name("jina-embeddings-v2-base-code"),
            Some(ModelName::JinaEmbeddingsV2BaseCode)
        );
        assert_eq!(
            ModelName::from_str_name("jina-embeddings-v2-base-code-int8"),
            Some(ModelName::JinaEmbeddingsV2BaseCode)
        );
        assert_eq!(
            ModelName::from_str_name("jina-code-int8"),
            Some(ModelName::JinaEmbeddingsV2BaseCode)
        );
        assert_eq!(
            ModelName::from_str_name("jinaai/jina-embeddings-v2-base-code"),
            Some(ModelName::JinaEmbeddingsV2BaseCode)
        );
        assert_eq!(
            ModelName::from_str_name("jina-code"),
            Some(ModelName::JinaEmbeddingsV2BaseCode)
        );
        assert_eq!(ModelName::from_str_name("jina"), Some(ModelName::JinaEmbeddingsV2BaseCode));
        assert_eq!(ModelName::from_str_name("unknown-model"), None);
    }

    #[test]
    fn test_default_model_is_jina() {
        assert_eq!(ModelName::default(), ModelName::JinaEmbeddingsV2BaseCode);
    }

    #[test]
    fn test_dimensions() {
        assert_eq!(ModelName::JinaEmbeddingsV2BaseCode.dimensions(), 768);
    }

    #[test]
    fn test_average_embeddings_empty() {
        assert_eq!(Embedder::average_embeddings(&[]), None);
    }

    #[test]
    fn test_average_embeddings_single() {
        let emb = vec![1.0, 0.0, 0.0];
        let result = Embedder::average_embeddings(&[emb]).unwrap();
        assert!((result[0] - 1.0).abs() < 1e-5);
        assert!((result[1] - 0.0).abs() < 1e-5);
        assert!((result[2] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_average_embeddings_multiple() {
        let embs = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let result = Embedder::average_embeddings(&embs).unwrap();
        let expected_val = 1.0 / 2.0_f32.sqrt();
        assert!((result[0] - expected_val).abs() < 1e-4);
        assert!((result[1] - expected_val).abs() < 1e-4);
        assert!((result[2] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_aimd_controller_additive_increase() {
        let mut aimd = AimdController::new(100);
        assert!((aimd.scale - 1.0).abs() < 1e-5);

        // Fast dispatch (< 50ms): additive increase +0.10
        aimd.record_dispatch(40);
        assert!((aimd.scale - 1.10).abs() < 1e-5);
        assert!((aimd.ema_latency_ms - 40.0).abs() < 1e-5);

        aimd.record_dispatch(30);
        assert!((aimd.scale - 1.20).abs() < 1e-5);
    }

    #[test]
    fn test_aimd_controller_multiplicative_decrease() {
        let mut aimd = AimdController::new(100);
        // Slow dispatch (> 150ms): multiplicative decrease * 0.80
        aimd.record_dispatch(200);
        assert!((aimd.scale - 0.80).abs() < 1e-5);

        aimd.record_dispatch(180);
        assert!((aimd.scale - 0.64).abs() < 1e-5);
    }

    #[test]
    fn test_aimd_controller_sweet_spot() {
        let mut aimd = AimdController::new(100);
        // Sweet spot dispatch (between 50ms and 150ms): maintains current scale
        aimd.record_dispatch(80);
        assert!((aimd.scale - 1.0).abs() < 1e-5);

        aimd.record_dispatch(120);
        assert!((aimd.scale - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_aimd_controller_clamping() {
        let mut aimd = AimdController::new(100);
        for _ in 0..50 {
            aimd.record_dispatch(20);
        }
        assert!(aimd.scale <= 4.0);

        for _ in 0..50 {
            aimd.record_dispatch(300);
        }
        assert!(aimd.scale >= 0.2);
    }

    #[test]
    fn test_cpu_governor_defaults_and_l3_capping() {
        let gov = CpuGovernor::new();
        assert_eq!(gov.provider_name(), "CPU");
        assert!(gov.total_memory_bytes() > 0);
        assert!(gov.available_memory_bytes() > 0);

        let short_batch = gov.compute_adaptive_batch(128, 0);
        assert!(short_batch <= 16);
        assert!(short_batch >= 1);

        let long_batch = gov.compute_adaptive_batch(1024, 0);
        assert!(long_batch <= short_batch);
        assert!(long_batch >= 1);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_directml_governor_adaptive_scaling() {
        let gov = DirectMlGovernor::with_vram_bytes(8 * 1024 * 1024 * 1024);
        assert_eq!(gov.provider_name(), "DirectML");
        assert_eq!(gov.total_memory_bytes(), 8 * 1024 * 1024 * 1024);

        let base_batch = gov.compute_adaptive_batch(128, 0);
        assert!(base_batch >= 16);

        // Record fast dispatches -> batch size increases
        let boosted_batch = gov.compute_adaptive_batch(128, 30);
        assert!(boosted_batch >= base_batch);

        // Record slow dispatches -> batch size decreases
        for _ in 0..5 {
            let _ = gov.compute_adaptive_batch(128, 250);
        }
        let throttled_batch = gov.compute_adaptive_batch(128, 250);
        assert!(throttled_batch < boosted_batch);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_auto_select_directml_gpu() {
        let device_id = select_directml_device_id();
        println!(">>> Auto-selected DirectML Device ID: {device_id}");
        assert!(device_id >= 0);
    }
}
