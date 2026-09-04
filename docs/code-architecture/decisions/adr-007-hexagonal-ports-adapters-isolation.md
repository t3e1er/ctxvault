---
title: "ADR 007: Hexagonal Ports & Adapters Encapsulation Barrier"
category: "code-architecture"
status: "accepted"
tags: ["adr", "hexagonal", "ports", "adapters", "architecture", "decision"]
related:
  - "[[docs/code-architecture/index]]"
  - "[[docs/code-architecture/hexagonal-ports-adapters]]"
---

# ADR 007: Hexagonal Ports & Adapters Encapsulation Barrier

## Status
Accepted / Implemented

## Context
Early prototypes allowed internal storage types (such as `rusqlite::Connection`, `tantivy::IndexReader`, and `ort::Session`) to leak through helper functions into `ctxvault-core::engine` and `ctxvault-mcp`. This tightly coupled the domain to specific third-party storage crates, making testing difficult and preventing modular backend replacement.

## Decision
We implemented a strict **Hexagonal Architecture encapsulation barrier**:
1. All core infrastructure capabilities are defined as pure Rust traits (**ports**) in `ctxvault-common::ports`.
2. Adapters in `ctxvault-core` implement these ports and completely encapsulate their underlying infrastructure crates.
3. No concrete backend type is ever exposed in port method signatures.
4. Construction of concrete adapters is restricted exclusively to the **Composition Root** (`ctxvault-cli/src/main.rs`).

## Consequences

### Positive
- The domain core and MCP server are completely decoupled from database and search engine internals.
- New backends (e.g. Postgres-backed catalog or cloud vector stores) can be introduced purely as new adapters without modifying domain logic.
- Unit testing with mock ports is trivial and fast.

### Trade-offs
- Requires boilerplate trait definitions and mapping from storage-specific row types to `ctxvault-common` domain structs.
