---
title: "Dynamic Hardware Governor & AIMD VRAM Scheduling"
category: "gpu-optimization"
status: "active"
tags: ["hardware-governor", "aimd", "vram", "batching", "memory-governor"]
related:
  - "[[docs/gpu-optimization/index]]"
  - "[[docs/data-science/transformer-memory-physics]]"
  - "[[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]]"
---

# Dynamic Hardware Governor & AIMD VRAM Scheduling

Hardcoded static activation budgets (such as fixing a 1 GB VRAM limit) fail across heterogeneous environments: on an 8 GB or 16 GB dedicated GPU, 70%+ of available hardware capacity sits completely idle; on an integrated 2 GB APU, that same 1 GB limit triggers fatal Out-of-Memory crashes.

`ctxvault` implements the **`HardwareGovernor`** trait paired with an **AIMD (Additive Increase / Multiplicative Decrease)** batch controller.

---

## 1. The HardwareGovernor Abstraction

`HardwareGovernor` queries real-time hardware telemetry before every batch dispatch:

```rust
pub trait HardwareGovernor: Send + Sync {
    /// Query real-time available memory headroom on the compute device (bytes).
    fn available_memory_bytes(&self) -> usize;

    /// Total device memory capacity (bytes).
    fn total_memory_bytes(&self) -> usize;

    /// Recommended activation budget under the Golden 70% headroom policy.
    fn activation_budget_bytes(&self) -> usize {
        let available = self.available_memory_bytes();
        (available as f64 * 0.70) as usize
    }
}
```

### The Golden 70% Policy
`ctxvault` caps peak dynamic memory consumption at **70% of available VRAM headroom**. The remaining 30% buffer protects against background operating system desktop compositing, external application spikes, and display driver frame buffers.

---

## 2. The AIMD Controller

To converge on optimal batch sizes dynamically without risking OOM or TDR timeouts:

```
                                  [Dispatch Batch N]
                                          │
                        ┌─────────────────┴─────────────────┐
                        ▼                                   ▼
              Latency < 350ms                     Latency > 400ms OR
              Memory Headroom > 30%               VRAM Headroom < 20%
                        │                                   │
                        ▼                                   ▼
               Additive Increase                  Multiplicative Decrease
             BatchSize += StepSize                 BatchSize = BatchSize / 2
```

1. **Additive Increase**: As long as GPU dispatch latency remains under 350ms and device memory headroom exceeds 30%, batch token budgets scale up linearly, saturating GPU compute cores.
2. **Multiplicative Decrease**: If measured dispatch latency exceeds 400ms or available VRAM dips below the 20% safety floor, the controller cuts the batch token budget in half immediately, preventing OS driver intervention.

See [[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]] for the decision context.
