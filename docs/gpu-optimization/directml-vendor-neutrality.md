---
title: "DirectML Hardware Acceleration & Vendor Neutrality"
category: "gpu-optimization"
status: "active"
tags: ["directml", "gpu", "directx12", "onnx", "ort", "hardware", "performance"]
related:
  - "[[docs/gpu-optimization/index]]"
  - "[[docs/gpu-optimization/dynamic-hardware-governor]]"
  - "[[docs/gpu-optimization/decisions/adr-013-directml-vendor-neutral-acceleration]]"
  - "[[docs/gpu-optimization/decisions/adr-014-wmi-dedicated-gpu-adapter-selection]]"
---

# DirectML Hardware Acceleration & Vendor Neutrality

In developer desktop tools, hard dependencies on proprietary GPU toolchains (such as NVIDIA CUDA or AMD ROCm) severely restrict user adoption. Developers work on laptops with integrated Intel/AMD graphics, workstations with discrete NVIDIA GeForce GPUs, or mobile setups with Qualcomm Snapdragon processors.

`ctxvault` achieves universal Windows hardware acceleration via **Microsoft DirectML over DirectX 12 Compute Shaders**.

---

## 1. The Vendor-Neutral Compute Layer

DirectML sits directly on top of the Windows DirectX 12 driver architecture, abstracting compute hardware behind a standardized OS API:

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                        ctxvault ONNX Runtime Engine (`ort` v2)                         │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            │
                                  Microsoft DirectML API
                                            │
┌───────────────────────────────────────────┴────────────────────────────────────────────┐
│                             DirectX 12 Compute Driver Layer                            │
└───────┬───────────────────────────┬───────────────────────────┬────────────────────────┘
        ▼                           ▼                           ▼
┌───────────────┐           ┌───────────────┐           ┌───────────────┐
│ NVIDIA GPU    │           │ AMD Radeon    │           │ Intel Arc /   │
│ GeForce / RTX │           │ RX / Ryzen APU│           │ Iris Xe APU   │
└───────────────┘           └───────────────┘           └───────────────┘
```

### Key Advantages:
1. **Zero Proprietary Drivers**: No CUDA Toolkit, cuDNN, or ROCm installations required. If the Windows OS has a standard DirectX 12 graphics driver, DirectML functions out of the box.
2. **Universal Hardware Coverage**: Runs seamlessly on NVIDIA, AMD (Radeon discrete and Ryzen APUs), Intel (discrete Arc and integrated Iris Xe / UHD), and Qualcomm Snapdragon X Elite (Adreno X1).
3. **Pure Safe Rust Integration**: Bound via the official Microsoft `ort` v2 ONNX Runtime execution provider without unsafe custom C bindings.

---

## 2. Dynamic Execution Provider Fallback

If DirectML initialization fails (e.g. running in a headless Linux container or bare VM), `ctxvault` automatically falls back through tiered execution providers:
1. **Tier 1 (Windows)**: Microsoft DirectML (DirectX 12).
2. **Tier 2 (macOS)**: Apple CoreML / Metal Execution Provider.
3. **Tier 3 (Universal Fallback)**: Multi-threaded CPU SIMD (AVX2, AVX-512, NEON) with multi-chunk batching.

See [[docs/gpu-optimization/decisions/adr-013-directml-vendor-neutral-acceleration]] for the architectural record.
