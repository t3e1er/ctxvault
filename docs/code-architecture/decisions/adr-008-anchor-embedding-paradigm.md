---
title: "ADR 008: Anchor Embedding Paradigm for Cold Indexing Acceleration"
category: "code-architecture"
status: "accepted"
tags: ["adr", "anchor-embedding", "indexing", "performance", "decision"]
related:
  - "[[docs/code-architecture/index]]"
  - "[[docs/code-architecture/cast-chunking-engine]]"
  - "[[docs/gpu-optimization/decisions/adr-015-dynamic-token-budgeting-tdr-safety]]"
---

# ADR 008: Anchor Embedding Paradigm for Cold Indexing Acceleration

## Status
Accepted / Implemented

## Context
Cold indexing large codebases (e.g. `rust` with 5,767 files / ~130,000 chunks; `kubernetes` with 20,078 files / ~100,000 chunks) previously suffered severe performance bottlenecks when every single chunk underwent a 12-layer transformer neural forward pass (`jina-embeddings-v2-base-code`). Initial cold indexes required 10–24+ hours of continuous compute.

## Decision
We adopted the **Anchor Embedding Paradigm**:
1. **100% Lexical Indexing (Tantivy BM25)**: Unconditionally indexes 100% of all chunks, files, variables, and error strings across the entire repository.
2. **100% Structural Code Graph (Petgraph)**: Unconditionally extracts and resolves AST relations (`defines`, `imports`, `calls`, `implements_trait`) for all functions and types.
3. **Dense Vector Embedding (HNSW)**: Computed exclusively for high-value **semantic anchor nodes**:
   - Root markdown architecture and design documentation (`.md` files).
   - Primary type and container definitions (`struct`, `class`, `trait`, `interface`, `enum`).
   - Public module namespaces (`pub mod`).
   - Documented public API entrypoints (`pub fn` with attached doc comments).
4. **Graph-Only Nodes**: Tests (`tests/`, `#[test]`), internal implementation blocks, and private helper functions bypass neural forward passes.

## Consequences

### Positive
- Forward passes are slashed from ~80,000+ down to ~6,000 semantic anchors, cutting cold indexing duration by **over 80%**.
- Zero loss of lexical discoverability: every private variable and error string remains instantly searchable via BM25.
- Multi-hop traversal allows an agent to land on a high-level architectural anchor via vector search and navigate inward to private call sites via AST edges.

### Trade-offs
- Obscure private helper functions with no docstring and no lexical match rely on graph expansion from their caller or container anchor.
