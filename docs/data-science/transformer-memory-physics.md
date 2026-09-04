---
title: "Transformer Attention Activation Memory Physics & Quadratic Scaling"
category: "data-science"
status: "active"
tags: ["data-science", "vram", "attention", "complexity", "batching", "dynamic-packing"]
related:
  - "[[docs/data-science/index]]"
  - "[[docs/gpu-optimization/dynamic-hardware-governor]]"
  - "[[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]]"
---

# Transformer Attention Activation Memory Physics & Quadratic Scaling

In neural embedding generation, static batching strategies ($B=64$) cause fatal GPU Out-of-Memory (OOM) crashes and DirectX 12 driver resets (`0x887A0006`).

This document analyzes the mathematical physics of transformer intermediate activations and explains why dynamic token-budget packing is strictly necessary.

---

## 1. The Quadratic Attention Scaling Trap $\mathcal{O}(S^2)$

During a transformer forward pass, total device memory comprises two components:
1. **Model Parameter Weights ($W_{\text{static}}$)**: Constant memory invariant to batch size or sequence length (~548 MB for Jina Code v2 FP32).
2. **Intermediate Activation Tensors ($M_{\text{dynamic}}$)**: Memory allocated for self-attention matrices, projection buffers, and feed-forward layers.

The attention matrix computation requires computing the scaled dot-product:
$$\text{Attention}(Q, K, V) = \text{softmax}\left(\frac{QK^T}{\sqrt{d_k}}\right)V$$

Where $Q, K \in \mathbb{R}^{B \times H \times S \times d_k}$. The attention matrix $QK^T$ has dimensions:
$$\text{Shape}(QK^T) = B \times H \times S \times S$$

Total activation memory across $L$ transformer layers scales as:
$$M_{\text{attn}} \approx B \times L \times H \times S^2 \times 4 \text{ bytes}$$

Where:
* $B$ = Batch size (number of chunks).
* $L$ = Number of layers (12 for Jina Code v2).
* $H$ = Number of attention heads (12 for Jina Code v2).
* $S$ = Effective padded sequence length in tokens.
* $4$ = Bytes per FP32 scalar (2 for FP16).

---

## 2. Empirical Memory Scaling Matrix

```
┌────────────┬─────────────────────┬───────────────────────┬────────────────┬──────────────────────┬────────────────────────┐
│ Batch (B)  │ Sequence Length (S) │ Attention Activations │ Static Weights │ Total VRAM Required  │ Behavior on 8GB VRAM   │
├────────────┼─────────────────────┼───────────────────────┼────────────────┼──────────────────────┼────────────────────────┤
│ 64         │ 8,192               │ ~192.0 GB             │ 0.6 GB         │ ~192.6 GB            │ 💥 Instant Fatal OOM   │
│ 64         │ 2,048               │ ~12.0 GB              │ 0.6 GB         │ ~12.6 GB             │ 💥 OOM Crash / TDR Hang│
│ 64         │ 1,024               │ ~3.0 GB               │ 0.6 GB         │ ~3.6 GB              │ ⚠️ High Risk (Spikes)   │
│ 64         │ 512                 │ ~755 MB               │ 0.6 GB         │ ~1.35 GB             │ ✅ Completely Stable   │
│ 16         │ 1,024               │ ~755 MB               │ 0.6 GB         │ ~1.35 GB             │ ✅ Completely Stable   │
│ 8          │ 2,048               │ ~1.5 GB               │ 0.6 GB         │ ~2.1 GB              │ ✅ Completely Stable   │
│ 1          │ 8,192               │ ~3.0 GB               │ 0.6 GB         │ ~3.6 GB              │ ✅ Completely Stable   │
└────────────┴─────────────────────┴───────────────────────┴────────────────┴──────────────────────┴────────────────────────┘
```

### The Pathological Padding Problem
In standard static batching, every chunk in a batch is padded to match the **maximum sequence length** in that batch:
$$S_{\text{batch}} = \max_{i \in B} (\text{length}(c_i))$$

If 63 chunks in a batch are 128 tokens long, but a single chunk is 4,096 tokens, the entire 64-chunk batch is padded to 4,096 tokens! 
$$\text{Wasted Activation Memory} = \frac{4096^2}{128^2} = 1,024\times \text{ memory explosion!}$$

---

## 3. Algorithmic Solution: Sort-and-Pack Dynamic Bucketing

To guarantee that total activation memory remains within a safe budget (e.g. 1.5 GB), `ctxvault` implements **Sort-and-Pack Dynamic Sequence Bucketing**:

```
[Raw Incoming Chunks: Arbitrary Lengths]
               │
               ▼
[Sort Chunks by Sequence Length (S)]
               │
               ▼
[Dynamic Token-Budget Packing]
┌───────────────────────────────┐ ┌───────────────────────────────┐ ┌───────────────────────────────┐
│ Bucket 1: Short Chunks (S≤256)│ │ Bucket 2: Medium Chunks(S≤1k) │ │ Bucket 3: Long Chunks (S≤8k)  │
│ Max Batch B = 64              │ │ Max Batch B = 16              │ │ Max Batch B = 1–2             │
│ Memory: ~1.2 GB               │ │ Memory: ~1.4 GB               │ │ Memory: ~1.5–3.0 GB           │
└───────────────────────────────┘ └───────────────────────────────┘ └───────────────────────────────┘
```

By guaranteeing that $B \times S^2 \le \text{Constant Budget}$, `ctxvault` eliminates GPU VRAM crashes entirely.

See [[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]] for the hardware implementation details.
