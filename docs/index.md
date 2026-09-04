---
title: "ctxvault Knowledge Corpus Hub"
category: "root"
status: "active"
tags: ["architecture", "search", "cast", "graph", "retrieval", "mcp", "polyglot", "karpathy", "wiki"]
related:
  - "[[docs/data-science/index]]"
  - "[[docs/agentic-strategy/index]]"
  - "[[docs/code-architecture/index]]"
  - "[[docs/mcp-modes/index]]"
  - "[[docs/gpu-optimization/index]]"
---

# ctxvault Knowledge Corpus Hub

Welcome to the central architectural documentation for **ctxvault**, an enterprise semantic Model Context Protocol (MCP) server and codebase intelligence system built in 100% pure safe Rust (`#![forbid(unsafe_code)]`).

This documentation is organized as a **compiled, structured knowledge corpus** following **Andrej Karpathy's "LLM Wiki" / Continuous Knowledge Crystallization (Principle 3)** paradigm. Rather than leaving knowledge in fragmented episodic session traces, architectural decisions, mathematical proofs, and systems engineering designs are pre-synthesized into dense, cross-linked, schema-validated notes.

---

## 1. Global Corpus Topology Graph

```
                               ┌────────────────────────────────────────────────────────┐
                               │                     docs/index.md                      │
                               │                Central Knowledge Hub                   │
                               └───────────────────────────┬────────────────────────────┘
                                                           │
        ┌──────────────────┬───────────────────────────────┼───────────────────────────────┬──────────────────┐
        ▼                  ▼                               ▼                               ▼                  ▼
┌───────────────┐  ┌───────────────┐               ┌───────────────┐               ┌───────────────┐  ┌───────────────┐
│ data-science/ │  │agentic-strat/ │               │code-architect/│               │  mcp-modes/   │  │gpu-optimizat/ │
│  (Theory &    │  │  (Progressive │               │  (Hexagonal   │               │ (39 Tools &   │  │  (DirectML &  │
│   Math RRF)   │  │   & Swarms)   │               │   & cAST)     │               │  Transports)  │  │   AIMD VRAM)  │
└───────┬───────┘  └───────┬───────┘               └───────┬───────┘               └───────┬───────┘  └───────┬───────┘
        │                  │                               │                               │                  │
        ▼                  ▼                               ▼                               ▼                  ▼
┌───────────────┐  ┌───────────────┐               ┌───────────────┐               ┌───────────────┐  ┌───────────────┐
│  decisions/   │  │  decisions/   │               │  decisions/   │               │  decisions/   │  │  decisions/   │
│ ADR 001–003   │  │ ADR 004–006   │               │ ADR 007–009,16│               │ ADR 010–012   │  │ ADR 013–015   │
└───────────────┘  └───────────────┘               └───────────────┘               └───────────────┘  └───────────────┘
```

---

## 2. Core Knowledge Clusters (Topic Gateways)

### 1. [[docs/data-science/index]] (Mathematical & Retrieval Theory)
* **Mathematical Foundations**: 4-modality hybrid search dynamics and failure modes of single-modality systems via [[docs/data-science/hybrid-retrieval-theory]].
* **Ranking Mechanics**: Reciprocal Rank Fusion (RRF, $k=60$) mathematical proof and ordinal rank combination via [[docs/data-science/rrf-mathematics]].
* **Polyglot Vector Spaces**: Asymmetric code-docstring representation using 768-dimensional Jina Code v2 via [[docs/data-science/code-embeddings-landscape]].
* **Graph Modularity**: Newman-Girvan modularity ($Q$) and Leiden connectivity-refined community detection via [[docs/data-science/community-detection-modularity]].
* **Hardware Physics**: Transformer attention activation scaling $\mathcal{O}(S^2)$ and sequence bucketing via [[docs/data-science/transformer-memory-physics]].

### 2. [[docs/agentic-strategy/index]] (Agentic Memory & Swarms)
* **Karpathy LLM Wiki**: Transforming raw episodic exhaust into compounding semantic notes via [[docs/agentic-strategy/knowledge-crystallization]].
* **Progressive Disclosure**: The strict 3-tier retrieval contract (<150 token handles $\to$ AST symbol snippet $\to$ whole file) via [[docs/agentic-strategy/progressive-disclosure]].
* **Swarm Topologies**: Coordinating specialized Scout, Reader, Writer, and Crystallizer agents via [[docs/agentic-strategy/swarm-topologies]].
* **Tool Profiling**: Gating tool exposure via `--profile scout|analysis|all` via [[docs/agentic-strategy/tool-profiling]].

### 3. [[docs/code-architecture/index]] (Systems Engineering & Rust Core)
* **Pure Rust Invariants**: `#![forbid(unsafe_code)]`, pinned MSRV 1.80, and sub-millisecond retrieval budgets via [[docs/code-architecture/pure-rust-invariants]].
* **Hexagonal Architecture**: Complete encapsulation of storage backends (Tantivy, SQLite, HNSW, Petgraph) behind port traits via [[docs/code-architecture/hexagonal-ports-adapters]].
* **cAST Polyglot Chunking**: Tree-sitter AST parsing across 16 modern languages with scope breadcrumb injection via [[docs/code-architecture/cast-chunking-engine]].
* **Scope Normalization**: Resolving unspecialized generic queries (`Type > method`) without 404s via [[docs/code-architecture/generic-scope-normalization]].

### 4. [[docs/mcp-modes/index]] (Model Context Protocol Surface)
* **Dual Transports**: Stdio IPC framing for local subagents vs Axum Streamable HTTP with SSE for remote swarms via [[docs/mcp-modes/transport-architecture]].
* **39-Tool Registry**: Exhaustive inventory across 8 functional domains via [[docs/mcp-modes/tool-surface-catalog]].
* **Modal Search Parameterization**: Executing `mode="hybrid"|"bm25"|"semantic"|"graph"|"explain"` with bi-modal filters via [[docs/mcp-modes/search-modes-modalities]].
* **Multi-Corpus Serving**: Routing $N$ independent index roots concurrently via `CorpusManager` via [[docs/mcp-modes/multi-corpus-serving]].
* **Schema Integrity**: Template schemas (`.templates/`), taxonomy checks, and whole-corpus health audits via [[docs/mcp-modes/schema-taxonomy-enforcement]].

### 5. [[docs/gpu-optimization/index]] (Hardware Acceleration & Indexing Performance)
* **Vendor-Neutral DirectML**: DirectX 12 Compute acceleration across NVIDIA, AMD, Intel, and Qualcomm without CUDA via [[docs/gpu-optimization/directml-vendor-neutrality]].
* **AIMD Memory Governor**: Dynamic batch controller maintaining a 70% VRAM ceiling via [[docs/gpu-optimization/dynamic-hardware-governor]].
* **Double-Buffered Dispatch**: Overlapping CPU tensor packing with GPU compute to achieve 85%+ hardware saturation via [[docs/gpu-optimization/double-buffered-dispatch]].
* **TDR Watchdog Safety**: 400ms per-dispatch latency ceiling eliminating Windows driver hangs (`0x887A0006`) via [[docs/gpu-optimization/tdr-watchdog-resilience]].
* **Quantization & Fast Mode**: INT8 dynamic quantization and instant cold indexing via [[docs/gpu-optimization/quantization-fast-mode]].

---

## 3. Authoritative Architectural Decisions (ADR Catalog)

```
┌─────────┬───────────────────────────────────────────────────────────────────┬──────────────────────────────────────────┐
│ ADR #   │ Title                                                             │ Topic Cluster                            │
├─────────┼───────────────────────────────────────────────────────────────────┼──────────────────────────────────────────┤
│ ADR 001 │ [[docs/data-science/decisions/adr-001-rrf-vs-learned-fusion]]     │ Data Science: Ranking Fusion             │
│ ADR 002 │ [[docs/data-science/decisions/adr-002-jina-code-768d-selection]]  │ Data Science: Vector Spaces              │
│ ADR 003 │ [[docs/data-science/decisions/adr-003-leiden-louvain-graph-clus...│ Data Science: Graph Community Detection  │
│ ADR 004 │ [[docs/agentic-strategy/decisions/adr-004-progressive-disclosu...│ Agentic Strategy: Token Contracts        │
│ ADR 005 │ [[docs/agentic-strategy/decisions/adr-005-deterministic-vs-llm...│ Agentic Strategy: Graph Derivation       │
│ ADR 006 │ [[docs/agentic-strategy/decisions/adr-006-role-based-tool-pro...│ Agentic Strategy: Tool Profiling         │
│ ADR 007 │ [[docs/code-architecture/decisions/adr-007-hexagonal-ports-ad...│ Code Architecture: Hexagonal Isolation   │
│ ADR 008 │ [[docs/code-architecture/decisions/adr-008-anchor-embedding-p...│ Code Architecture: Indexing Acceleration │
│ ADR 009 │ [[docs/code-architecture/decisions/adr-009-greenfield-no-back...│ Code Architecture: Greenfield Discipline │
│ ADR 010 │ [[docs/mcp-modes/decisions/adr-010-unified-modal-search-tool]]    │ MCP Surface: Unified Tool Ergonomics     │
│ ADR 011 │ [[docs/mcp-modes/decisions/adr-011-readonly-readwrite-handler...│ MCP Surface: Handler Concurrency         │
│ ADR 012 │ [[docs/mcp-modes/decisions/adr-012-in-process-multi-corpus-ma...│ MCP Surface: Multi-Corpus Serving        │
│ ADR 013 │ [[docs/gpu-optimization/decisions/adr-013-directml-vendor-neu...│ GPU Optimization: Vendor-Neutral Compute │
│ ADR 014 │ [[docs/gpu-optimization/decisions/adr-014-wmi-dedicated-gpu-a...│ GPU Optimization: Hardware Selection     │
│ ADR 015 │ [[docs/gpu-optimization/decisions/adr-015-dynamic-token-budge...│ GPU Optimization: TDR Safety Ceiling     │
│ ADR 016 │ [[docs/code-architecture/decisions/adr-016-generic-normalized...│ Code Architecture: Scope Resolution      │
└─────────┴───────────────────────────────────────────────────────────────────┴──────────────────────────────────────────┘
```

---

## 4. Semantic Tag Taxonomy

* `#architecture`: Core system invariants, module layering, and hexagonal port abstractions.
* `#search`: Modal search parameterization, hybrid retrieval, and bi-modal filtering.
* `#rrf`: Reciprocal Rank Fusion mathematical proofs, ordinal rank combination, and score calibration.
* `#vectors`: Dense neural vector representations, Jina Code v2, and HNSW approximate nearest neighbors.
* `#graph`: Petgraph property graph walks, wikilinks, caller-callee chains, and community detection.
* `#cast`: Context-Aware AST chunking, Tree-sitter grammar traversal, and scope breadcrumbs.
* `#progressive-disclosure`: 3-tier retrieval ergonomics, token contracts, and handle generation.
* `#agentic`: Multi-agent swarm topologies, memory substrates, and persona separation.
* `#crystallization`: Compiling episodic developer traces into permanent, schema-validated notes.
* `#mcp`: Model Context Protocol 39-tool registry, transports (stdio/HTTP), and handler locking.
* `#gpu`: Hardware acceleration, DirectML over DirectX 12 Compute, and WMI adapter selection.
* `#vram`: Dynamic activation memory governors, $O(S^2)$ attention physics, and AIMD batch controllers.
* `#tdr`: Windows Timeout Detection and Recovery resilience and 400ms safety ceilings.
* `#adr`: Authoritative Architecture Decision Records documenting concrete engineering trade-offs.
