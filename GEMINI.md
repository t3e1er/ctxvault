# ctxvault — Gemini & Antigravity Steering Guide

> **Authoritative Source of Truth**: This steering document is synchronized with `.kiro/steering/` (`product.md`, `engineering-principles.md`, `mcp-tools.md`, `structure.md`, `tech.md`). It governs AI pair programming, architectural discipline, and Model Context Protocol (MCP) interactions for `ctxvault`.

---

## 1. Product Context & Core Invariants

`ctxvault` (`ctxv`) is an enterprise semantic **Model Context Protocol (MCP) server** for markdown knowledge bases and polyglot codebases. It provides AI agents with fast, minimal, high-signal context without file dumping or non-deterministic LLM entity extraction.

Written in 100% pure Rust (`unsafe_code = "forbid"`) for memory safety, zero C-runtime dependencies, and sub-millisecond graph and lexical retrieval.

### Non-Negotiable Invariants
1. **Markdown/source is authoritative ground truth**: Files on disk are king. All indices (Tantivy BM25, HNSW vectors, SQLite catalog, Petgraph) are derived, disposable, and 100% rebuildable. Never treat an index as canonical.
2. **Explicit graph topology, not LLM extraction**: Edges are generated deterministically from typed frontmatter fields, `#tags`, `[[wikilinks]]`, and AST code relations (`calls`, `defines`, `imports`, `implements`) — never from stochastic extraction pipelines.
3. **Continuous knowledge crystallization (Principle 3)**: Ephemeral agent exhaust (debug traces, design consensus, bug resolutions) must be distilled into permanent, schema-validated notes with full lineage/provenance via `promote_concept` and `traverse_lineage`.
4. **Pure Rust sub-millisecond speed**: Multi-hop graph traversal and hybrid ranking operate in real time (lexical p50 ~2.2ms, graph BFS ~1.8ms) with no perceptible agent lag.
5. **Multi-agent memory substrate**: A shared in-memory + on-disk semantic plane for specialized agent swarms (Scouts, Readers, Writers, Crystallizers).

### Retrieval & Multi-Corpus Architecture
- **4-Modality Hybrid Retrieval**: Fused via 3-way Reciprocal Rank Fusion (RRF) across Tantivy Okapi BM25, dense ONNX embeddings (`jina-embeddings-v2-base-code`, 768-dim), and Petgraph typed graph traversal.
- **Cross-Modal Linking**: Unifies documentation and polyglot source code (Rust, TS/JS, Python, Go, Java, C/C++) in a single graph.
- **Multi-Corpus Serving**: A central MCP process serves $N$ index roots via `CorpusManager`. Tools accept optional `corpus` or fan-out `corpora` (`["a", "b"]` or `"all"`).
- **Progressive Disclosure (3 Tiers)**:
  - *Tier 1*: `search` returns lightweight handles (paths, qualified names, line ranges, snippets).
  - *Tier 2*: `get_snippet` fetches exactly one code symbol or doc chunk, bounded, with optional neighbor expansion.
  - *Tier 3*: `read_multiple` / `read_note` / `read_code_file` read full file contents only when exhaustive context is required.

---

## 2. Greenfield Engineering Principles

ctxvault has no legacy external consumers to protect. Optimize for a clean, minimal, cohesive codebase.

### No Backwards Compatibility
- **Never add compatibility shims**, deprecated tool names, aliased handlers, or "fallback to legacy behavior" logic.
- When changing a type, tool signature, argument, or on-disk index layout, **replace the old shape outright**.
- Indices are rebuildable and APIs are unversioned greenfield. Defaults exist for ergonomics, never for legacy emulation.

### No Dead Code, No Tech Debt
- Every function, struct, field, enum variant, and branch must be reachable and used. Delete unused code in the same change.
- Clippy runs with `-D warnings`. Never silence unused code warnings with blanket `#[allow(dead_code)]`.
- Do not leave TODO stubs, commented-out code, or duplicate code paths. Collapse duplicate paths immediately.

### Hexagonal Architecture (Ports & Adapters)
Every major concern is defined as a trait (**port**) in `ctxvault-common::ports` or `ctxvault-core`; concrete backends (**adapters**) implement them:
- **Major Ports**: `MetadataCatalog` (SQLite), `TextIndex` (Tantivy BM25), `VectorStore` (HNSW), `GraphStore` (Petgraph), `EmbeddingProvider` (ONNX), `SearchService` (multi-modal dispatch + RRF).
- **Encapsulation Barrier**: Adapters never leak backend types (`rusqlite::Connection`, `tantivy::*`, `hnsw_rs::*`, `petgraph::*`, `ort::*`) across ports. Port signatures use domain types from `ctxvault-common` only.
- **Domain Decoupling**: `Engine` holds ports; it does not own concrete backends and does not expose concrete accessors. `ctxvault-mcp` depends on ports, `SearchService`, and domain types, never core internals.
- **Composition Root**: `crates/ctxvault-cli/src/main.rs` is the *only* place adapters are named, constructed, and injected via `CorpusManager` / engine builders.
- **Rust DI Policy**: Prefer generics with trait bounds on hot paths (zero-cost monomorphization). Use `Arc<dyn Trait>` only for runtime pluggable boundaries.

---

## 3. MCP Tool Surface (39 Tools) & Usage Directives

Authoritative tool registry: `crates/ctxvault-mcp/src/tools/mod.rs`. Handlers are `ReadOnly(fn(&Engine, Value))` or `ReadWrite(fn(&mut Engine, Value))`.

### Registered Tool Inventory

| Category | Count | Tools |
|---|---|---|
| **Read** | 6 | `search` handles $\to$ `get_snippet`, `read_note`, `read_code_file`, `read_multiple`, `list_notes`, `get_frontmatter` |
| **Search** | 2 | `search` (single tool; `mode` = `hybrid` \| `bm25` \| `semantic` \| `graph` \| `explain`), `search_related` |
| **Graph** | 8 | `backlinks`, `forwardlinks`, `graph_path`, `graph_stats`, `graph_subgraph`, `graph_communities` (`algorithm` = `leiden` \| `louvain`), `list_edge_types`, `traverse_lineage` |
| **Write** | 5 | `create_note`, `update_note`, `delete_note`, `move_note`, `promote_concept` (all mutating) |
| **Template / Validation** | 4 | `validate_note`, `validate_corpus`, `list_templates`, `validate_taxonomy` |
| **Analysis** | 5 | `analyze_density`, `find_semantic_gaps`, `suggest_splits`, `coverage_report`, `check_index_coverage` |
| **Code Intel** | 4 | `get_symbol_definition`, `find_callers`, `get_architecture`, `detect_changes` (mutating) |
| **System / Corpus** | 5 | `status` (single tool; `scope` = `all` \| `corpus` \| `indexing`), `corpus_list`, `reindex_corpus` (mutating), `sync_corpus` (mutating), `reembed_corpus` (mutating) |

### Tool Profiles (`--profile`)
- **`scout`** (9 tools): Minimal retrieve/navigate set (`search`, `search_related`, `get_snippet`, `read_note`, `read_code_file`, `read_multiple`, `list_notes`, `get_frontmatter`, `status`).
- **`analysis`** (30 tools): `scout` + read-only graph, validation, code intelligence, and corpus analysis.
- **`all`** (39 tools): Full suite including mutating write and administrative tools.

### Agent Directives
1. **Files are ground truth**: Trust file content on disk over cached search snippets.
2. **Select optimal `search` mode**:
   - `mode="hybrid"`: Default for broad exploratory queries (3-way RRF fusion).
   - `mode="bm25"`: Exact symbols, identifiers, struct names, error strings, verbatim tokens.
   - `mode="semantic"`: Conceptual similarity and abstract technical intentions.
   - `mode="graph"`: Typed graph traversal; filter by `edge_types` or `edge_class` (`structural`, `semantic`, `hybrid`).
   - `mode="explain"`: Introspect scoring breakdowns (BM25 vs vector vs graph).
3. **Progressive disclosure**: Always query `search` $\to$ fetch targeted code/doc sections via `get_snippet` $\to$ read whole files (`read_note`, `read_code_file`) only when necessary.
4. **Schema discipline on writes**: Query `list_templates` before authoring, adhere to required frontmatter, and confirm validity with `validate_note`.
5. **Crystallize lasting knowledge (Principle 3)**: Turn non-obvious solutions, debugging insights, and architectural decisions into durable vault notes using `promote_concept`. Trace provenance with `traverse_lineage`.
6. **Destructive operations**: `delete_note` permanently removes files and index entries; confirm with user before executing.

---

## 4. Workspace Structure & Module Layout

```
ctxvault/
├── crates/
│   ├── ctxvault-common/  # Domain types, ports traits, config, errors
│   ├── ctxvault-core/    # Engine, Tantivy, embeddings (DirectML/ort), Petgraph, AST chunkers
│   ├── ctxvault-mcp/     # Stdio & HTTP transport, MCP protocol, tool registry (39 tools)
│   └── ctxvault-cli/     # Composition root binary, multi-corpus CLI
├── docs/                 # Authoritative architecture, cAST chunking, and search docs
└── .index/               # Derived indices: meta.db, tantivy/, vectors.json, graph.bin
```

### Key Modules in `ctxvault-core`
- `engine.rs` / `engine_builder.rs`: Core engine orchestration and port assembly.
- `corpus_manager.rs`: Multi-corpus routing and cross-corpus symbol resolution (`link_cross_corpus_symbols`).
- `search/`: Modal search strategies (`bm25`, `semantic`, `hybrid`, `graph`, `related`, `explain`) and RRF fusion.
- `graph/code.rs`: AST-derived code edges (`defines`, `imports`, `calls`, `implements`) with confidence bands.
- `index/pipeline.rs`: Hardware-accelerated indexing pipeline with batched ONNX tensor staging.
- `parser/code/`: Tree-sitter polyglot AST chunker across 12+ languages.

---

## 5. Technology Stack, Toolchain & Quality Standards

- **Language**: 100% pure Rust, Edition 2021, pinned **MSRV 1.80** (`rust-toolchain.toml`).
- **Safety**: `unsafe_code = "forbid"` workspace-wide.
- **Linting**: `missing_docs = "warn"`, Clippy enabled for `correctness`, `suspicious`, `perf`. Runs with `-D warnings`.
- **Hardware Acceleration**: Windows DirectML, macOS CoreML, Linux CUDA, with automatic CPU SIMD fallback (`ort` 2.0.0-rc.13).

### Developer Workflow (`just`)
| Recipe | Action |
|---|---|
| `just check` | `cargo check --workspace --all-features --all-targets` |
| `just clippy` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |
| `just fmt-check` | `cargo fmt --all -- --check` (`just fmt` to format) |
| `just test` | `cargo test --workspace --all-features --locked` |
| `just build-release` | `cargo build --workspace --all-features --release --locked` |
| `just ci` | Full local CI check (fmt-check + clippy + test + deny + docs) |

---

## 6. SCM, Branching & Versioning Protocols

- **Branch Naming**: `feature/<name>`, `fix/<name>`, `refactor/<name>`, `chore/<name>`, `release/vX.Y.Z`. Never commit directly to `master`.
- **Commit Messages**: Imperative Conventional Commits: `<type>(<scope>): <summary>` (`feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `chore`).
- **Hooks**: Git hooks enabled via `just setup-hooks` (`.githooks/pre-push` enforces formatting, clippy, tests, and tag alignment with `Cargo.toml`).
