---
title: "Hardware Acceleration & GPU Indexing Gateway"
category: "gpu-optimization"
status: "active"
tags: ["gpu-optimization", "directml", "vram", "hardware", "tdr", "quantization"]
related:
  - "[[docs/index]]"
  - "[[docs/gpu-optimization/directml-vendor-neutrality]]"
  - "[[docs/gpu-optimization/dynamic-hardware-governor]]"
  - "[[docs/gpu-optimization/double-buffered-dispatch]]"
  - "[[docs/gpu-optimization/tdr-watchdog-resilience]]"
  - "[[docs/gpu-optimization/quantization-fast-mode]]"
  - "[[docs/gpu-optimization/decisions/adr-013-directml-vendor-neutral-acceleration]]"
  - "[[docs/gpu-optimization/decisions/adr-014-wmi-dedicated-gpu-adapter-selection]]"
  - "[[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]]"
---

# Hardware Acceleration & GPU Indexing Hub

Welcome to the **Hardware Acceleration & GPU Indexing** module of `ctxvault`. This cluster covers the low-level hardware performance engineering, cross-platform acceleration layers, dynamic VRAM scheduling, and stability mechanisms that power ctxvault's embedding pipeline.

---

## 1. Cross-Platform Execution Stack

```
                               ┌───────────────────────────────────┐
                               │         ctxvault Embedder         │
                               └─────────────────┬─────────────────┘
                                                 │
                   ┌─────────────────────────────┼─────────────────────────────┐
                   ▼                             ▼                             ▼
        ┌─────────────────────┐       ┌─────────────────────┐       ┌─────────────────────┐
        │   Windows DirectML  │       │    Apple CoreML     │       │   Linux / CPU SIMD  │
        │ DirectX 12 Compute  │       │ Metal / Neural Eng. │       │ AVX2/AVX-512 / NEON │
        └──────────┬──────────┘       └──────────┬──────────┘       └──────────┬──────────┘
                   │                             │                             │
         NVIDIA / AMD / Intel         Apple Silicon M1–M4            x86_64 / aarch64
         APUs & Dedicated GPUs        Unified Memory Architecture     Compute Nodes
```

---

## 2. Core Architectural Articles

1. **[[docs/gpu-optimization/directml-vendor-neutrality]]**
   * *Vendor-Neutral Graphics Compute*: Why DirectML over DirectX 12 Compute delivers universal acceleration across NVIDIA GeForce, AMD Radeon, Intel Arc, and Qualcomm Snapdragon without proprietary CUDA dependencies.
2. **[[docs/gpu-optimization/dynamic-hardware-governor]]**
   * *AIMD Memory Governor*: Additive Increase / Multiplicative Decrease dynamic batch controller maintaining a strict 70% VRAM ceiling across heterogeneous hardware tiers.
3. **[[docs/gpu-optimization/double-buffered-dispatch]]**
   * *Zero Inter-Batch Gaps*: Double-buffered asynchronous GPU dispatch overlapping CPU tensor padding and mean-pooling with GPU compute kernel execution, driving hardware utilization from 15% to 80%+.
4. **[[docs/gpu-optimization/tdr-watchdog-resilience]]**
   * *400ms Watchdog Safety Ceiling*: Keeping per-dispatch execution latencies bounded under 400ms to eliminate OS Timeout Detection and Recovery crashes (`0x887A0006 Device Lost`).
5. **[[docs/gpu-optimization/quantization-fast-mode]]**
   * *INT8 & Instant Cold Indexing*: Dynamic 8-bit model weight quantization halving VRAM requirements, plus "Fast Mode" instant indexing bypassing neural passes entirely for BM25+Graph setups.

---

## 3. Architectural Decision Records (ADRs)

* **[[docs/gpu-optimization/decisions/adr-013-directml-vendor-neutral-acceleration]]**: Selecting Microsoft DirectML over proprietary NVIDIA CUDA SDKs to support universal developer hardware.
* **[[docs/gpu-optimization/decisions/adr-014-wmi-dedicated-gpu-adapter-selection]]**: Implementing automated CIM/WMI video controller telemetry to bind dedicated discrete GPUs over integrated APUs.
* **[[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]]**: Replacing static batch counts with dynamic sequence-length token budgets to prevent quadratic activation memory blowups.
