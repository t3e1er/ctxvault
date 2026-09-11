---
title: "MCP Transport & Authoritative Tool Registry"
description: "Stdio framing, Axum HTTP SSE transport, and the authoritative 17-tool handler registration in ctxvault."
category: "implementation"
status: "active"
tags: ["mcp", "transport", "stdio", "http", "sse", "registry", "17-tools"]
related:
  - "[[docs/architecture/implementation/index]]"
  - "[[docs/concepts/progressive-disclosure/tool-profiles]]"
  - "[[docs/architecture/adr/adr-011-readonly-readwrite-handler-model]]"
---

# MCP Transport & Authoritative Tool Registry

`ctxvault-mcp` implements the **Model Context Protocol (MCP)** specification with dual transports: standard input/output (stdio JSON-RPC) for local subagents, and Axum HTTP with Server-Sent Events (SSE) for distributed swarms.

---

## 1. Concurrency Model: ReadOnly vs ReadWrite Handlers

To maximize agent throughput, tool handlers are registered as either:
* **`ReadOnly(fn(&Engine, Value))`**: Can execute concurrently across multiple threads without locking the engine. (e.g. `search`, `get_snippet`, `read_file`, `graph_match`).
* **`ReadWrite(fn(&mut Engine, Value))`**: Acquires an exclusive write lock to perform atomic updates. (e.g. `write_note`, `delete_note`, `sync_corpus`).

This prevents read requests from stalling behind background indexing jobs.

---

## 2. Authoritative 17-Tool Registry

The authoritative registry in `crates/ctxvault-mcp/src/tools/mod.rs` defines the complete protocol surface:

```rust
// Authoritative 17 Tools across 5 Functional Domains:
// Read:       read_file, get_snippet, list_notes
// Search:     search, search_related
// Graph:      graph_match, graph_communities
// Write:      write_note, delete_note, move_note
// Validation: validate, list_templates
// System:     status, list_corpora, sync_corpus, index_corpus, unload_corpus
```

Each tool handler performs strict schema validation on incoming JSON-RPC payloads, returning actionable error diagnostics (such as available taxonomy values or valid line slices) if arguments are invalid.
