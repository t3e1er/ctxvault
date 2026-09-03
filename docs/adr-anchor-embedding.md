# ADR: Anchor Embedding + AST Graph Traversal Migration & Automatic GPU Selection

## Status
Accepted / Implemented

## Context
Cold indexing of large polyglot codebases previously suffered from severe latency bottlenecks due to exhaustive chunk-level dense embedding:
1. **Exhaustive Embedding Overhead**: Every chunk produced from every source file underwent a full 12-layer transformer forward pass (`jina-embeddings-v2-base-code`). For the `rust` corpus (5,767 files, ~130k chunks) and `kubernetes` (20,078 files, ~100k chunks), exhaustive forward passes required 10–24+ hours of compute time.
2. **GPU Adapter Contention**: On multi-adapter Windows environments (e.g. integrated Intel HD Graphics alongside a dedicated NVIDIA GeForce GTX GPU), DirectML defaulted to Adapter 0 (the integrated GPU with shared memory), triggering GPU memory exhaustion, sluggish inference, or TDR timeouts.

## Decision

### 1. Anchor Embedding Paradigm (Option 2)
We decoupled dense neural embedding from lexical and structural graph indexing:
- **Lexical Indexing (Tantivy BM25)**: Unconditionally indexes 100% of all chunks, files, and symbols across the entire corpus. Every private helper, variable, test assertion, and error message remains instantly searchable by exact or fuzzy identifier name.
- **Structural Code Graph (Petgraph)**: Unconditionally extracts and resolves AST relationships (`defines`, `imports`, `calls`, `implements_trait`) for all functions, methods, structs, and interfaces.
- **Dense Vector Embedding (HNSW)**: Computed exclusively for high-value semantic anchor nodes:
  - Root markdown architecture and design documentation (`.md` files).
  - Primary type and container definitions (`struct`, `class`, `trait`, `interface`, `enum`).
  - Public module namespaces (`pub mod`).
  - Documented public API entrypoints (`pub fn` with doc comments or top-level visibility).
- **Graph-Only Nodes**:
  - Test files (`tests/`, `tests.rs`, `_test.go`, `*.test.ts`) and test cases (`#[test]`, `Test*`).
  - Implementation blocks (`impl Trait for Type`, `impl Type`) whose methods and structs are already connected via AST edges.
  - Private helper functions, leaf expressions, and undocumented internal methods.

### 2. Automatic Hardware-Aware GPU Selection
In safe Rust (`#![forbid(unsafe_code)]`), ctxvault automatically detects the highest-performing DirectX 12 adapter:
1. Respects user override via `CTX_DEVICE_ID` environment variable if set.
2. Queries Windows system video controllers via CIM/WMI (`Win32_VideoController`), inspecting dedicated `AdapterRAM`.
3. Automatically binds DirectML execution to the adapter with the greatest dedicated VRAM (selecting Device ID 1 NVIDIA GTX 1070 over Device ID 0 integrated Intel HD Graphics 530).
4. Employs dynamic token-budget batching with a 400ms per-dispatch TDR safety ceiling to prevent OS driver hangs (`0x887A0006`).

## Consequences

### Positive
- **Dramatic Speedup**: Reduces neural embedding forward passes from ~80,000+ down to ~5,000–8,000 true semantic anchors, slashing indexing duration by over 80%.
- **Zero Loss of Lexical Discoverability**: Any internal identifier or method remains immediately retrievable via BM25 exact term matching.
- **Multi-Hop Traversal Preservation**: The query planner can land on a high-level architectural anchor via vector similarity and navigate inward to private call sites via AST edges.
- **Zero Configuration GPU Acceleration**: Multi-GPU Windows workstations automatically leverage dedicated graphics hardware without manual environment variable flags.

### Neutral / Trade-offs
- Pure semantic similarity queries on obscure private helper functions without lexical match rely on graph expansion from their caller or container anchor.
