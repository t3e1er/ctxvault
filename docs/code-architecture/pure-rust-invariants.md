---
title: "Pure Rust Invariants, Greenfield Discipline & Latency Budgets"
category: "code-architecture"
status: "active"
tags: ["rust", "safety", "msrv", "greenfield", "latency", "invariants"]
related:
  - "[[docs/code-architecture/index]]"
  - "[[docs/code-architecture/hexagonal-ports-adapters]]"
  - "[[docs/code-architecture/decisions/adr-009-greenfield-no-backwards-compat]]"
---

# Pure Rust Invariants, Greenfield Discipline & Latency Budgets

`ctxvault` is engineered for high-assurance, zero-compromise performance as an enterprise semantic infrastructure component. It is built in 100% pure safe Rust with strict greenfield engineering standards.

---

## 1. Non-Negotiable Invariants

```
┌──────────────────────────────┬────────────────────────────────────────────────────────────────────────┐
│ Invariant                    │ Architectural Enforcement Mechanism                                   │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ Zero Unsafe Code             │ `#![forbid(unsafe_code)]` enforced workspace-wide in all crates.      │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ Pinned Toolchain & MSRV      │ MSRV 1.80 pinned via `rust-toolchain.toml` and verified in CI.         │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ Zero Dead Code / Zero Debt   │ `cargo clippy` runs with `-D warnings`. Blanket `#[allow]` forbidden.  │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ Zero C-Runtime Dependencies  │ Pure Rust TLS (`rustls`), bundled SQLite (`rusqlite`), zero dynamic C.│
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ Sub-Millisecond Retrieval    │ p50 Lexical ~2.2ms, p50 Graph BFS ~1.8ms on developer workstations.   │
└──────────────────────────────┴────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Greenfield Engineering Discipline

Because `ctxvault` is an unencumbered greenfield system:
* **No Deprecated Shims**: When refactoring an API, data structure, or MCP tool signature, old code paths are deleted immediately.
* **No Backward-Compatibility Fallbacks**: On-disk indices (`.index/`) are derived, disposable, and 100% rebuildable. Schema updates trigger automated index rebuilds rather than carrying complex migration wrappers.
* **No Unwired TODO Stubs**: Unused functions, temporary structs, and commented-out code blocks fail CI and must be deleted within the same pull request.

---

## 3. Strict Latency Budgets

To ensure zero perceptible latency when AI agents query ctxvault during multi-step reasoning loops:

```
Operation                       Target Budget (p50)    Achieved (Empirical)
----------------------------------------------------------------------------
Tantivy BM25 Term Search        < 5.0 ms               ~ 2.2 ms
Petgraph BFS Graph Walk (3 hops)< 3.0 ms               ~ 1.8 ms
SQLite Symbol Scope Lookup      < 2.0 ms               ~ 0.8 ms
RRF 3-Way Fusion Math           < 0.5 ms               ~ 0.15 ms
HNSW ANN Cosine Search (Top-20) < 10.0 ms              ~ 4.5 ms
Tier-1 Progressive Handle Gen   < 5.0 ms               ~ 2.4 ms
```

See [[docs/code-architecture/decisions/adr-009-greenfield-no-backwards-compat]] for the architectural record.
