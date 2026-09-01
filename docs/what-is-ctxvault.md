---
title: What is ctxvault? System Overview
tags: [architecture, overview, mcp, polyglot]
status: active
---

# What is ctxvault?

**ctxvault** is a unified, high-performance cross-modal knowledge engine and codebase intelligence system written in Rust. It bridges conceptual human documentation (Architecture Decision Records, specs, wikis) with concrete polyglot source code implementations.

See also: [[docs/index]], [[why-hybrid-retrieval]], [[how-cast-chunking-works]], [[how-search-pipeline-works]].

---

## 🏗️ Architectural Layers

```
                     ┌──────────────────────────────────────────────┐
                     │            AI Agents / Developers            │
                     └──────────────────────┬───────────────────────┘
                                            │ MCP Protocol (Stdio / SSE / HTTP)
                                            ▼
                     ┌──────────────────────────────────────────────┐
                     │            ctxvault MCP Server               │
                     │            (41 Unified Tools)                │
                     └──────────────────────┬───────────────────────┘
                                            │
                     ┌──────────────────────┴──────────────────────┐
                     ▼                                             ▼
        ┌─────────────────────────┐                   ┌─────────────────────────┐
        │  Markdown Knowledge     │                   │  Polyglot Codebase      │
        │  (ADRs, wikis, notes)   │                   │  (16 AST Languages)     │
        └────────────┬────────────┘                   └────────────┬────────────┘
                     │                                             │
                     └──────────────────────┬──────────────────────┘
                                            │
                                            ▼
                     ┌──────────────────────────────────────────────┐
                     │         Unified Cross-Modal Engine           │
                     │ • Tantivy BM25 (Exact Identifiers)           │
                     │ • Jina v2 Base Code (768d Vector Space)      │
                     │ • Petgraph (Cross-Modal Knowledge Graph)     │
                     │ • SQLite (Metadata & Symbol Table)           │
                     └──────────────────────────────────────────────┘
```

---

## 🌟 Core System Pillars

### 1. Dual-Layer Cross-Modal Ingestion
Instead of treating code as dumb text or isolating documentation in a silo, ctxvault unifies:
* **Documentation Layer**: Markdown documents parsed with YAML frontmatter, section heading hierarchies, and bidirectional `[[wikilinks]]`.
* **Polyglot Code Layer**: Source code parsed via Tree-sitter AST across 16 modern languages via [[how-cast-chunking-works]].

### 2. Multi-Engine Retrieval
Combines three complementary search paradigms into an optimal 4-way Reciprocal Rank Fusion (RRF) pipeline via [[how-search-pipeline-works]]:
1. **Tantivy BM25**: Instant exact matching of symbol names, variable identifiers, and error strings.
2. **Dense Vector Search**: 768-dimensional ONNX embeddings via `jinaai/jina-embeddings-v2-base-code` with an 8,192 token window.
3. **Petgraph Graph Traversal**: Multi-hop structural link analysis across caller-callee chains, trait implementations, and document links.

### 3. Structural Code Intelligence Tools
Exposes 41 Model Context Protocol (MCP) tools matching and surpassing standalone code tools:
* `get_symbol_definition`: Look up symbols with exact line ranges, docstrings, and signatures.
* `find_callers`: Trace incoming call sites across the knowledge graph.
* `get_architecture`: Compute high-level module clusters and bridge nodes via Louvain community detection (see [[how-architecture-detection-works]]).
* `detect_changes`: Filesystem delta scanner computing impacted symbol blast radii.

---

## 📦 Workspace Crate Topology

* [`crates/ctxvault-core`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-core): Core indexing engine, AST chunking, Tantivy search, HNSW vector index, Petgraph graph, and SQLite persistence.
* [`crates/ctxvault-mcp`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-mcp): MCP protocol implementation, JSON-RPC dispatch, SSE/HTTP streaming server, and tool registries.
* [`crates/ctxvault-cli`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-cli): CLI orchestration, mode switching (`local`, `server`, `client`, `proxy`), and startup synchronization.
* [`crates/ctxvault-common`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-common): Shared configuration schemas, types, and error handling.
