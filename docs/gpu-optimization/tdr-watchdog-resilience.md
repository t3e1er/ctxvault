---
title: "TDR Watchdog Resilience & 400ms Execution Ceilings"
category: "gpu-optimization"
status: "active"
tags: ["tdr", "directml", "directx12", "stability", "gpu-hang", "watchdog"]
related:
  - "[[docs/gpu-optimization/index]]"
  - "[[docs/gpu-optimization/dynamic-hardware-governor]]"
  - "[[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]]"
---

# TDR Watchdog Resilience & 400ms Execution Ceilings

On Windows workstations, long-running GPU compute kernels trigger a severe failure mode known as **TDR (Timeout Detection and Recovery)**.

If the Windows OS graphics scheduler detects that a single GPU dispatch has held the device command queue for longer than 2.0 seconds without returning control to the desktop window manager, the OS assumes the GPU has hung. The graphics driver is forcefully reset, terminating the application with DirectX 12 error **`0x887A0006 (DXGI_ERROR_DEVICE_HUNG / DEVICE_LOST)`**.

`ctxvault` eliminates TDR crashes by enforcing a strict **400ms Per-Dispatch Safety Ceiling**.

---

## 1. The Mechanics of Windows TDR

```
Time Elapsed:
0.0s ────────────────────────► 0.4s ────────────────────────► 2.0s
  ▲                              ▲                              ▲
  │                              │                              │
Batch Dispatched           ctxvault Target Ceiling        Windows OS Force Reset
to DirectML                (Kernel Finishes Here)         (0x887A0006 Device Lost)
```

By guaranteeing that every GPU forward pass finishes in $\le 400\text{ms}$, `ctxvault` maintains a **5x safety margin** beneath the Windows 2.0-second TDR threshold, even when other desktop applications (browsers, IDEs, 3D compositors) share the GPU simultaneously.

---

## 2. Dynamic Token-Budgeting Enforcement

To ensure execution latency never exceeds 400ms, batch sizes are computed as a function of **sequence length** and **measured hardware FLOPs**:

```
Sequence Length Range (S)    Max Safe Batch Size (B)    Measured Dispatch Time
--------------------------------------------------------------------------------
S ≤ 256 tokens               B = 64 chunks              ~ 120 ms
256 < S ≤ 512 tokens         B = 32 chunks              ~ 180 ms
512 < S ≤ 1,024 tokens       B = 16 chunks              ~ 260 ms
1,024 < S ≤ 2,048 tokens     B = 8 chunks               ~ 340 ms
2,048 < S ≤ 8,192 tokens     B = 1–2 chunks             ~ 380 ms
```

If the runtime measured dispatch latency approaches 380ms, the AIMD hardware governor immediately throttles the batch budget down, preserving complete OS driver stability.

See [[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]] for the architectural record.
