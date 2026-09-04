---
title: "ADR 012: In-Process Multi-Corpus Serving via CorpusManager"
category: "mcp-modes"
status: "accepted"
tags: ["adr", "multi-corpus", "corpus-manager", "architecture", "decision"]
related:
  - "[[docs/mcp-modes/index]]"
  - "[[docs/mcp-modes/multi-corpus-serving]]"
---

# ADR 012: In-Process Multi-Corpus Serving via CorpusManager

## Status
Accepted / Implemented

## Context
Developers working on complex systems often require access to multiple corpora simultaneously (e.g. documentation vault, microservice repo, shared library). Spawning a separate OS process for each corpus multiplies VRAM consumption (each process loading its own ONNX embedding model, ~550 MB each) and prevents cross-corpus symbol resolution.

## Decision
We implemented **`CorpusManager`** inside `crates/ctxvault-core/src/corpus_manager.rs`:
1. A single persistent MCP process manages $N$ independent index roots.
2. The ONNX embedding model instance and thread pool are shared across corpora, avoiding VRAM duplication.
3. Every engine manages its own isolated `.index/` directory (`meta.db`, `tantivy/`, `vectors.json`, `graph.bin`).
4. Cross-corpus fan-out queries and unambiguous symbol linking are performed in-memory.

## Consequences

### Positive
- Memory footprint is amortized: serving 5 corpora consumes roughly the same VRAM as serving 1 corpus.
- Cross-corpus symbol linking enables seamless cross-modal navigation between architecture notes and code repositories.
- CLI ergonomics: `--corpus name=path` can be specified multiple times on a single invocation.

### Trade-offs
- A crash or panic in one corpus engine could terminate the shared process (mitigated by `#![forbid(unsafe_code)]` and pure Rust error handling).
