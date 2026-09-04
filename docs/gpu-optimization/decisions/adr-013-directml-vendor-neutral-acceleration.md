---
title: "ADR 013: DirectML Vendor-Neutral Hardware Acceleration over CUDA"
category: "gpu-optimization"
status: "accepted"
tags: ["adr", "directml", "gpu", "cuda", "hardware", "decision"]
related:
  - "[[docs/gpu-optimization/index]]"
  - "[[docs/gpu-optimization/directml-vendor-neutrality]]"
---

# ADR 013: DirectML Vendor-Neutral Hardware Acceleration over CUDA

## Status
Accepted / Implemented

## Context
Standard neural network libraries often assume a dedicated NVIDIA GPU with CUDA Toolkit installations. In developer tooling, requiring CUDA locks out developers using AMD Radeon graphics, Intel Iris Xe/Arc GPUs, and Qualcomm Snapdragon laptops. Requiring users to install multi-gigabyte proprietary toolchains creates unacceptable setup friction.

## Decision
We selected **Microsoft DirectML over DirectX 12 Compute Shaders** (via ONNX Runtime `ort` v2) as our primary Windows acceleration provider.

### Rationale:
1. **Universal Hardware Neutrality**: Runs natively on NVIDIA, AMD, Intel, and Qualcomm graphics hardware without specialized driver modifications.
2. **Zero Setup Friction**: DirectML uses the operating system's native DirectX 12 graphics driver; users do not need to install CUDA or ROCm toolkits.
3. **Safe Rust Integration**: Cleanly supported via the official Microsoft `ort` crate under pure safe Rust (`#![forbid(unsafe_code)]`).

## Consequences

### Positive
- Works out-of-the-box on virtually 100% of modern Windows PCs.
- Supports heterogeneous architectures (integrated APUs, discrete cards, laptop eGPUs).

### Trade-offs
- Peak raw FLOPS throughput on high-end NVIDIA server GPUs (e.g. H100) is slightly lower (~10–15%) than vendor-tuned TensorRT/CUDA kernels; however, on developer client workstations, portability far outweighs this minor delta.
