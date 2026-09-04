---
title: "INT8 Quantization & Instant Fast-Mode Cold Indexing"
category: "gpu-optimization"
status: "active"
tags: ["quantization", "int8", "fast-mode", "cold-index", "performance"]
related:
  - "[[docs/gpu-optimization/index]]"
  - "[[docs/data-science/code-embeddings-landscape]]"
  - "[[docs/code-architecture/decisions/adr-008-anchor-embedding-paradigm]]"
---

# INT8 Quantization & Instant Fast-Mode Cold Indexing

Developer machines vary from high-end multi-GPU workstations to battery-constrained ultrabooks. `ctxvault` provides two specialized performance modes to accommodate resource-constrained environments: **INT8 Model Quantization** and **Instant Fast-Mode Indexing**.

---

## 1. INT8 Dynamic Quantization

For machines with limited VRAM (e.g. 2 GB–4 GB integrated APUs), `ctxvault` supports dynamically quantized INT8 ONNX models:

```
┌─────────────────────────────────────┬───────────────────┬───────────────────┐
│ Metric                              │ Standard FP32     │ Quantized INT8    │
├─────────────────────────────────────┼───────────────────┼───────────────────┤
│ Model Disk Footprint                │ ~ 548 MB          │ ~ 137 MB (75% ↓)  │
│ Base VRAM Consumption               │ ~ 620 MB          │ ~ 210 MB (66% ↓)  │
│ Forward Pass Latency (CPU SIMD)     │ ~ 45 ms / batch   │ ~ 21 ms / batch   │
│ Cross-Modal Code MRR@10 Retrieval   │ 0.842             │ 0.836 (<0.8% drop)│
└─────────────────────────────────────┴───────────────────┴───────────────────┘
```

INT8 dynamic quantization preserves 32-bit floating-point precision for activation layers while quantizing transformer projection and feed-forward weights into 8-bit integers, halving memory bandwidth pressure.

---

## 2. Instant Fast-Mode Cold Indexing (`--fast`)

When a developer opens a massive repository for the first time (e.g. the 20,000-file `kubernetes` repo), they frequently need **immediate jump-to-definition, caller graphs, and exact identifier lookup**, without waiting for neural embedding passes.

By passing the `--fast` flag during indexing:
1. **Neural Embeddings Skipped**: All forward passes, ONNX session initialization, and HNSW vector writes are completely bypassed.
2. **100% Lexical Inverted Index (Tantivy BM25)**: Generated immediately in seconds.
3. **100% Structural Code Graph (Petgraph)**: AST functions, methods, imports, and calls are fully wired.
4. **Instant Operational Readiness**: A 20,000-file repository is indexed and fully queryable via `get_symbol_definition`, `find_callers`, and `search(mode="bm25")` in **$<10$ seconds**.

Dense vectors can then be generated lazily in the background or triggered via `reembed_corpus` when hardware is idle.
