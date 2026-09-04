---
title: "Code Architecture & Systems Engineering Gateway"
category: "code-architecture"
status: "active"
tags: ["code-architecture", "rust", "hexagonal", "cast", "ast", "invariants"]
related:
  - "[[docs/index]]"
  - "[[docs/code-architecture/pure-rust-invariants]]"
  - "[[docs/code-architecture/hexagonal-ports-adapters]]"
  - "[[docs/code-architecture/cast-chunking-engine]]"
  - "[[docs/code-architecture/generic-scope-normalization]]"
  - "[[docs/code-architecture/decisions/adr-007-hexagonal-ports-adapters-isolation]]"
  - "[[docs/code-architecture/decisions/adr-008-anchor-embedding-paradigm]]"
  - "[[docs/code-architecture/decisions/adr-009-greenfield-no-backwards-compat]]"
  - "[[docs/code-architecture/decisions/adr-016-generic-normalized-scope-resolution]]"
---

# Code Architecture & Systems Engineering Hub

Welcome to the **Code Architecture & Systems Engineering** module of `ctxvault`. This cluster documents the structural invariants, pure Rust engineering discipline, hexagonal ports-and-adapters architecture, and AST code intelligence engines powering ctxvault.

---

## 1. System Architecture Layout

```
                        ┌──────────────────────────────────────────┐
                        │          ctxvault-cli (Binary)           │
                        │             Composition Root             │
                        └────────────────────┬─────────────────────┘
                                             │ Constructs & Injects
                                             ▼
                        ┌──────────────────────────────────────────┐
                        │          ctxvault-mcp (Transport)        │
                        │       Stdio JSON-RPC  •  Axum HTTP       │
                        └────────────────────┬─────────────────────┘
                                             │ Calls Ports & Domain
                                             ▼
                        ┌──────────────────────────────────────────┐
                        │          ctxvault-core (Engine)          │
                        │      Hexagonal Ports & Concrete Adapters │
                        │  Tantivy  •  HNSW  •  SQLite  • Petgraph │
                        └────────────────────┬─────────────────────┘
                                             │ Implements Traits
                                             ▼
                        ┌──────────────────────────────────────────┐
                        │        ctxvault-common (Domain)          │
                        │   Ports Traits  •  Config  •  Errors     │
                        └──────────────────────────────────────────┘
```

---

## 2. Core Architectural Articles

1. **[[docs/code-architecture/pure-rust-invariants]]**
   * *100% Pure Safe Rust*: `#![forbid(unsafe_code)]`, pinned MSRV 1.80, zero C-runtime dependencies, and sub-millisecond retrieval latency guarantees (p50 lexical 2.2ms, BFS graph 1.8ms).
2. **[[docs/code-architecture/hexagonal-ports-adapters]]**
   * *Hexagonal Architecture (Ports & Adapters)*: Complete decoupling of domain models from storage infrastructure. Trait ports (`MetadataCatalog`, `TextIndex`, `VectorStore`, `GraphStore`, `EmbeddingProvider`) and zero adapter type leakage.
3. **[[docs/code-architecture/cast-chunking-engine]]**
   * *Context-Aware AST Chunking (`cAST`)*: Polyglot concrete syntax tree extraction across 16 modern languages using Tree-sitter. Preserving syntactically valid atomic units and injecting language-specific scope breadcrumbs (`// Scope: Class > method`).
4. **[[docs/code-architecture/generic-scope-normalization]]**
   * *Generic-Normalized Scope Resolution*: Stripping balanced angle brackets `<...>` and lifetime parameters while preserving ` > ` hierarchy delimiters, resolving unspecialized generic queries without 404s.

---

## 3. Architectural Decision Records (ADRs)

* **[[docs/code-architecture/decisions/adr-007-hexagonal-ports-adapters-isolation]]**: Prohibiting the leakage of concrete backend types (`rusqlite::Connection`, `tantivy::*`, `ort::*`) across port trait boundaries.
* **[[docs/code-architecture/decisions/adr-008-anchor-embedding-paradigm]]**: Decoupling dense embeddings from 100% lexical BM25 and structural AST indexing, slashing cold-index runtime by over 80%.
* **[[docs/code-architecture/decisions/adr-009-greenfield-no-backwards-compat]]**: Strict greenfield engineering discipline—zero compatibility shims, no legacy aliased handlers, and instant removal of dead code.
* **[[docs/code-architecture/decisions/adr-016-generic-normalized-scope-resolution]]**: AST generic-normalization fallback pattern in SQLite catalog to resolve unspecialized symbol paths.
