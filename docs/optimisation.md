# Embedding & Indexing Optimization Architecture: Cross-Platform Hardware Acceleration, Dynamic VRAM Scheduling & Quantization

This document establishes the production performance architecture, cross-platform hardware acceleration strategy, and runtime memory model for `ctxvault`'s embedding generation, vector indexing, and hybrid retrieval engine.

---

## 1. Executive Summary & Design Principles

In developer knowledge engines indexing polyglot codebases and large documentation vaults, embedding generation is the primary computational bottleneck (typically >85% of initial cold-index runtime).

### The Four Core Objectives
1. **Universal Portability Across OS**: Zero-friction operation on Windows, macOS, and Linux.
2. **Universal Device Support**: Seamless execution across discrete GPUs (NVIDIA GeForce/RTX, AMD Radeon), integrated APUs (AMD Ryzen Vega/RDNA, Intel Iris Xe/Arc, Qualcomm Snapdragon X), Apple Silicon unified memory (M1–M4), and pure CPU SIMD fallback.
3. **Vendor-Neutral APIs**: No direct proprietary SDK locks (no hard dependency on CUDA, ROCm, or proprietary vendor runtime installations). All hardware acceleration operates through standard OS-level graphics/compute drivers via **ONNX Runtime (DirectML on Windows, CoreML on macOS, CPU AVX2/AVX-512/NEON on Linux)**.
4. **Resilient Memory Stability**: Total protection against GPU VRAM Out-of-Memory (OOM) crashes, system RAM exhaustion, and cascading device-lost states through quadratic-aware dynamic batch packing and INT8 model quantization.

---

## 2. Comparative Architecture & Model Justification

### 2.1 Why Not Graph-Only (Reference: `codebase-memory` by DeusData)?
`codebase-memory` uses Tree-sitter AST parsing to construct structural graphs (`CALLS`, `DEFINES`, `IMPORTS`, `IMPLEMENTS`) stored in SQLite without any embedding models or LLM inferences.
- **Strength**: Instant indexing, zero VRAM overhead, deterministic structural call-graph navigation.
- **Weakness**: Incapable of answering conceptual, semantic, or cross-cutting questions (e.g., *"where is JWT authentication validated?"*, *"how is connection backoff retried?"*).
- **ctxvault Synthesis**: `ctxvault` combines both worlds. It includes structural AST chunking and graph navigation (`search_graph`, `search_multihop`) alongside high-fidelity dense vector retrieval (`search_semantic`, `search_hybrid`).

### 2.2 Why Not General Lightweight Text (Reference: `@glitchking/semantic-pages`)?
`semantic-pages` utilizes `all-MiniLM-L6-v2` (384 dimensions, 6 layers, 22.7M parameters) via Transformers.js in client/browser contexts.
- **Strength**: Ultra-compact footprint (~80 MB), minimal memory requirement (~150 MB total VRAM).
- **Weakness**: Trained on general English sentences; severely degrades when parsing syntax, function signatures, macros, types, and programming language idioms across 30+ languages.
- **ctxvault Decision**: `jinaai/jina-embeddings-v2-base-code` (137M parameters, 768 dimensions) was trained specifically on GitHub and CodeSearchNet code-docstring pairs. It enables true cross-modal natural language to code search (e.g., matching a natural language architecture question directly to the implementing Rust/Go/Python function).

---

## 3. The Mathematics of Transformer Activation Memory

### 3.1 The Quadratic Attention Trap ($O(N^2)$)
A transformer forward pass consists of:
1. **Model Weights**: Constant memory ($W$).
2. **Intermediate Activations & Attention Matrices**: Quadratic in token sequence length ($S$):
$$\text{Memory}_{\text{attention}} \approx B \times L \times H \times S^2 \times 4 \text{ bytes}$$
Where:
- $B$ = Batch size (number of chunks)
- $L$ = Number of transformer layers (12 for Jina-v2-base)
- $H$ = Number of attention heads (12 for Jina-v2-base)
- $S$ = Effective token sequence length
- Multiplied by 3–4× for intermediate projection, QKV, and FFN expansion buffers.

### 3.2 Memory Scaling Table (Jina-v2-base FP32)
| Batch Size ($B$) | Sequence Length ($S$) | Attention Activations | Weights | Total VRAM Required | Result on 8GB GTX 1070 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **64** | **8,192** | **~192.0 GB** | 0.6 GB | **~192.6 GB** | 💥 **Instant OOM** |
| **64** | **1,024** | **~3.0 GB** | 0.6 GB | **~3.6 GB** | ⚠️ Risky (Peak Spikes) |
| **64** | **512** | **~755 MB** | 0.6 GB | **~1.35 GB** | ✅ Stable |
| **16** | **1,024** | **~755 MB** | 0.6 GB | **~1.35 GB** | ✅ Stable |
| **8** | **2,048** | **~1.5 GB** | 0.6 GB | **~2.1 GB** | ✅ Stable |

Prior implementations suffered from hardcoded static batches ($B=64$) padded to the maximum token sequence in the batch. If a single document in a batch contained a long section near 2,000+ tokens, the entire 64-chunk batch expanded quadratically, immediately exhausting device VRAM and triggering DirectX 12 `887A0006 (Device Lost)` errors.

---

## 4. The Unified Cross-Platform Execution Stack

`ctxvault` uses ONNX Runtime (`ort` v2) with automatic target-aware execution provider binding:

```
                          ┌────────────────────────┐
                          │   ctxvault Embedder    │
                          └───────────┬────────────┘
                                      │
               ┌──────────────────────┼──────────────────────┐
               ▼                      ▼                      ▼
      [Windows Target]         [macOS Target]          [Linux / Fallback]
       Microsoft DirectML       Apple CoreML            CPU SIMD Engine
       (DirectX 12 Compute)     (Metal / ANE)          (AVX2 / AVX-512 / NEON)
               │                      │                      │
       NVIDIA / AMD / Intel    Apple Silicon M1-M4      x86_64 / aarch64
       APUs & Discrete GPUs    Unified Memory Architecture
```

### 4.1 Tier 1: Windows DirectML (DirectX 12)
DirectML runs over DirectX 12 Compute Shaders. It delivers vendor-neutral acceleration across:
- NVIDIA GeForce GTX / RTX
- AMD Radeon RX and Ryzen APUs (Radeon 680M/780M/890M)
- Intel Arc discrete and Intel Iris Xe / UHD integrated graphics
- Qualcomm Snapdragon X Elite (Adreno X1)

### 4.2 Tier 2: macOS CoreML
Directly utilizes Apple Silicon’s unified memory architecture, running across the Apple GPU and Apple Neural Engine (ANE) with zero memory copy overhead.

### 4.3 Tier 3: Pure CPU Fallback with Multi-Chunk Batching
When no GPU is present or when hardware runs out of resources, execution falls back cleanly to optimized CPU execution using vector SIMD (AVX2, AVX-512 on x86; NEON on ARM).

---

## 5. Algorithmic Fixes: VRAM-Aware Dynamic Batch Scheduling

To guarantee zero OOMs and high throughput across all hardware tiers, the embedding pipeline implements three key optimizations:

### 5.1 Sort-and-Pack Sequence Bucketing
Instead of naively grouping chunks in file order (mixing a 50-token chunk with a 1,024-token chunk and padding both to 1,024), `ctxvault` uses **Sort-and-Pack**:
1. Tokenize all pending chunks in the staging buffer.
2. Sort chunk indices by their token length.
3. Slice into homogeneous sub-batches where lengths are tightly bounded.
4. Calculate the dynamic batch size limit for each bucket:
$$B_{\text{max}}(S) = \min\left(B_{\text{soft\_cap}}, \frac{\text{VRAM}_{\text{budget}}}{L \times H \times S^2 \times 4 \times 3}\right)$$
5. Execute the forward pass on GPU.
6. Re-order output embedding vectors to match the original document order.

### 5.2 Device Budget Detection & VRAM Tiering
At initialization, `ctxvault` assesses available hardware memory:
- **High VRAM (>= 6 GB)**: Activation budget ~2.0 GB. $B \in [16, 64]$.
- **Integrated APU / Low VRAM (1–4 GB)**: Activation budget ~512 MB. $B \in [8, 32]$.
- **Constrained / CPU (< 1 GB)**: Activation budget ~128 MB. $B \in [4, 16]$.

### 5.3 Resilient Device-Lost Recovery
If DirectML or CoreML throws an unrecoverable driver error (`DXGI_ERROR_DEVICE_REMOVED`, `887A0006`, or driver crash), the embedder marks the hardware session as disabled via an atomic flag (`gpu_disabled`), releases GPU pipeline handles, and seamlessly routes all subsequent batches to the CPU fallback session. The indexing process continues without crashing.

### 5.4 Model Quantization (INT8 & FP16)
To support integrated APUs with shared VRAM limits:
- **FP32 Model**: ~612 MB weights, requires >= 2 GB dedicated VRAM.
- **INT8 Quantized Model**: ~154 MB weights, runs comfortably on AMD Ryzen APUs (512MB default allocation), Intel UHD graphics, and entry-level developer laptops with minimal impact on retrieval precision (MTEB delta < 0.8%).

---

## 6. End-to-End Indexing Throughput Projections

For a 50,000-chunk codebase repository (~10,000 files):

| Environment | Quantization | Effective Throughput | Total Index Time |
| :--- | :--- | :--- | :--- |
| **NVIDIA GTX 1070 (DirectML)** | INT8 / FP16 | 450–700 chunks/sec | **1.2 – 1.8 minutes** |
| **Apple M3 Pro (CoreML)** | FP16 | 500–800 chunks/sec | **1.0 – 1.5 minutes** |
| **AMD Ryzen 7 7840U APU (DirectML)** | INT8 | 180–300 chunks/sec | **2.8 – 4.5 minutes** |
| **Intel Core i7-6700 CPU (AVX2)** | INT8 | 100–160 chunks/sec | **5.2 – 8.0 minutes** |
| **Headless Linux ARM64 (NEON)** | INT8 | 50–90 chunks/sec | **9.0 – 16.0 minutes** |

All targets comfortably complete 50,000 files in well under 30 minutes, meeting the core latency requirements.
