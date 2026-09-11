---
title: "Hexagonal Ports & Adapters Architecture"
description: "How ctxvault encapsulates all storage, indexing, and embedding backends behind clean Rust port traits."
category: "implementation"
status: "active"
tags: ["hexagonal", "ports", "adapters", "domain-driven", "clean-architecture"]
related:
  - "[[docs/architecture/implementation/index]]"
  - "[[docs/architecture/trust/pure-rust-invariants]]"
  - "[[docs/architecture/adr/adr-007-hexagonal-ports-adapters-isolation]]"
---

# Hexagonal Ports & Adapters Architecture

To prevent architectural decay and ensure that storage backends can be swapped or tested in isolation, `ctxvault` follows a strict **Hexagonal (Ports and Adapters)** architecture.

* **Port Trait Definitions**: [`crates/ctxvault-common/src/ports.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-common/src/ports.rs)
* **Composition Root**: [`crates/ctxvault-cli/src/main.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-cli/src/main.rs)
* **Engine Orchestrator**: [`crates/ctxvault-core/src/engine.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-core/src/engine.rs)

---

## The Ports Invariant

All major subsystem interfaces are defined as pure Rust traits (**ports**) in `ctxvault-common::ports` or `ctxvault-core`:
* `MetadataCatalog`: Database abstraction for notes, code symbols, edges, and frontmatter.
* `TextIndex`: Full-text search abstraction (implemented by Tantivy).
* `VectorStore`: Dense vector indexing abstraction (implemented by HNSW).
* `GraphStore`: Graph traversal and reachability abstraction (implemented by Petgraph and SQLite CTEs).
* `EmbeddingProvider`: Vector inference abstraction (implemented by DirectML/ONNX and FastEmbed).
* `SearchService`: Multi-modal dispatch and RRF combination.

---

## Strict Encapsulation Barrier

Concrete backend types **never cross port boundaries**:
* `rusqlite::Connection` is strictly internal to `ctxvault-core::catalog::SqliteCatalog`.
* `tantivy::Index` is strictly internal to `ctxvault-core::index::TantivyTextIndex`.
* `ort::Session` is strictly internal to `ctxvault-core::embedding::DirectMlProvider`.

The MCP layer (`ctxvault-mcp`) and the core `Engine` interact with storage and search **only via domain types and port traits**.

### Composition Root
[`crates/ctxvault-cli/src/main.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-cli/src/main.rs) is the **only composition root** in the entire codebase. It instantiates the concrete adapters, wires them into `EngineBuilder`, and passes the constructed `Engine` or `CorpusManager` to the MCP transport.
