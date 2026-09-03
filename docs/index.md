---
title: ctxvault Architecture & Documentation Hub
tags: [architecture, search, cast, graph, retrieval, mcp, polyglot]
status: active
---

# ctxvault Knowledge Hub

Welcome to the central architectural documentation for **ctxvault**, a high-performance cross-modal knowledge engine and codebase intelligence system built in Rust.

---

## Navigation & Knowledge Graph

```
                   ┌──────────────────────────────────────┐
                   │           docs/index.md              │
                   └──────────────────┬───────────────────┘
                                      │
          ┌───────────────────────────┼───────────────────────────┐
          ▼                           ▼                           ▼
┌───────────────────┐       ┌───────────────────┐       ┌───────────────────┐
│ [[what-is-ctxvault│       │[[why-hybrid-retrie│       │[[how-cast-chunking│
│       ]]          │       │      val]]        │       │     -works]]      │
└─────────┬─────────┘       └─────────┬─────────┘       └─────────┬─────────┘
          │                           │                           │
          └───────────────────────────┼───────────────────────────┘
                                      │
                   ┌──────────────────┴──────────────────┐
                   ▼                                     ▼
        ┌─────────────────────┐               ┌─────────────────────┐
        │[[how-search-pipeline│               │[[how-architecture-de│
        │      -works]]       │               │   tection-works]]   │
        └─────────────────────┘               └─────────────────────┘
```

---

## Core Documentation Topics

### 1. [[what-is-ctxvault]]
* **The WHAT**: Comprehensive system architecture overview of ctxvault.
* **Capabilities**: Dual-layer cross-modal indexing, 16-language polyglot AST chunking via [[how-cast-chunking-works]], Petgraph knowledge graph, and 41 registered Model Context Protocol (MCP) tools.

### 2. [[why-hybrid-retrieval]]
* **The WHY**: The mathematical and empirical rationale behind our retrieval pipeline.
* **Principles**: Why pure vector search and pure BM25 both fail on real-world software engineering tasks, why unified metric spaces matter for cross-modal queries, and how Reciprocal Rank Fusion (RRF) eliminates brittle score calibration.

### 3. [[how-cast-chunking-works]]
* **The HOW (Chunking & AST)**: Concrete syntax tree traversal using Tree-sitter across 16 modern languages.
* **Mechanism**: Active scope stack tracking (`// Scope: Class > method`), leading docstring binding, and SQLite `code_symbols` persistence.

### 4. [[how-search-pipeline-works]]
* **The HOW (Retrieval Pipeline)**: End-to-end execution flow of `search_hybrid`.
* **Engines**: Tantivy BM25 lexical engine, Jina v2 Base Code 768-dimensional dense vectors via ONNX runtime, and Petgraph graph hops fused into an optimal ranked result list.

### 5. [[how-architecture-detection-works]]
* **The HOW (Code Intelligence & Graph)**: High-level architectural clustering via the Louvain modularity algorithm.
* **Analysis**: Calculating cross-modal modularity scores ($Q$), detecting interface bridge hub nodes, and computing filesystem delta blast radii via `detect_changes`.

### 6. [[optimisation]]
* **Hardware & Runtime Strategies**: Comprehensive performance engineering for embedding generation, batching physics, and cross-platform acceleration.
* **Analysis**: Algorithmic multi-chunk batching (3x-5x gain), pure-Rust ML runtime deep-dive (Hugging Face `Candle` vs Tracel AI `Burn`/`wgpu`), device-tier batch heuristics, and developer machine portability (Apple Silicon Metal, Windows DirectML, Linux CUDA, APUs).

---

## Semantic Graph Tags

* `#architecture`: System design, module boundaries, and high-level architectural decisions.
* `#search`: Hybrid retrieval, RRF ranking, BM25 scoring, and vector indexing.
* `#cast`: Context-Aware AST chunking, scope breadcrumb injection, and grammar traversal.
* `#graph`: Petgraph multi-hop graph walks, wikilinks, and caller-callee hierarchies.
* `#mcp`: Model Context Protocol server, client, proxy, and stdio/HTTP transports.
* `#polyglot`: Multi-language support (Rust, TS, JS, Python, Go, C/C++, Java, C#, Ruby, PHP, Swift, Elixir, Lua, Bash).
