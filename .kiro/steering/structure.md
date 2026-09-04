---
inclusion: always
---

# ctxvault — Structure & Workspace

## Cargo Workspace Crates

| Crate | Role |
|---|---|
| `crates/ctxvault-common` | Shared domain types, TOML configs (`config.rs`), error definitions (`error.rs`), `EntityKind` / `CodeSymbolType` discriminators |
| `crates/ctxvault-core` | Retrieval engine: Tantivy, embeddings (`ort`), Petgraph, SQLite, chunking, file watcher, search strategies, crystallization |
| `crates/ctxvault-mcp` | MCP JSON-RPC transport (stdio + Streamable HTTP), client/proxy, and the tool registry (`src/tools/mod.rs`) |
| `crates/ctxvault-cli` | Native binary `ctxvault`: arg parsing, mode selection, orchestration |

Keep layering clean: `common` has no deps on the others; `core` depends on `common`; `mcp` depends on `core` + `common`; `cli` wires everything. Do not create upward or circular dependencies.

## Key Modules in `ctxvault-core/src`

- `engine.rs` — top-level `Engine` orchestrating index + search + write.
- `search/` — hybrid/graph/related/explain search strategies + RRF fusion.
- `graph/` — Petgraph construction; `graph/code.rs` extracts code edges (`defines`, `imports`, `calls`).
- `index/` — indexing pipeline (`pipeline.rs`).
- `parser/` — `markdown.rs`, `frontmatter.rs`, `wikilink.rs`, `chunker.rs`; `parser/code/` holds AST-aware polyglot code chunking + `languages.rs`.
- `persistence/`, `vector_index.rs`, `embedding.rs`, `corpus_manager.rs`, `template.rs`, `analytics.rs`, `watcher/`.

## Corpus & Index Layout

- `corpus.toml` — per-corpus config: `[chunking]`, `[embedding]`, `[[graph.edge_types]]` (name, source, weight, direction/bidirectional, `class` = structural|semantic|hybrid), `[templates]`.
- Edge types are **data, not code** — declared per corpus, never hardcoded.
- Templates: TOML files in the corpus's `.templates/` dir; enforce required frontmatter/sections and declarative `edge_rules`.
- `.index/` (gitignored, per corpus): `meta.db` (SQLite), `tantivy/`, `vectors/` (HNSW), `graph.bin` (bincode). All derived and rebuildable.

## Docs

Authoritative design docs live in `docs/`:
- `ARCHITECTURE.md` — full system design, persistence, MCP tool surface, transport.
- `CODEROADMAP.md` — polyglot code indexing (Tree-sitter, cAST chunking, code graph) roadmap.
- `how-*.md` — architecture detection, cAST chunking, search pipeline explainers.
- `adr-anchor-embedding.md`, `why-hybrid-retrieval.md`, `optimisation.md`, `what-is-ctxvault.md`.

Consult these before changing retrieval, chunking, or graph behavior. Docs may describe aspirational tool names — the authoritative registered tool set lives in `crates/ctxvault-mcp/src/tools/mod.rs`.

## examples/

Drop-in starter pack (not part of the build): `examples/steering/` (Cursor/Gemini/Claude/Windsurf rules), `examples/skills/` (SKILL.md runbooks), `examples/agents/` (Scout/Reader/Writer/Crystallizer + swarm blueprints), `examples/starter-vault/`.

## Multi-Root Workspace

`ctxvault.code-workspace` opens ctxvault alongside sibling reference/test repos (peers, NOT subdirectories):

| Folder | Purpose |
|---|---|
| `../codebase-memory-mcp` | **Reference repo.** `DeusData/codebase-memory-mcp` — code-focused MCP server (C binary, structural property graph). Benchmark/comparison baseline for polyglot code indexing ideas (import resolution, community detection, structural tools). Do NOT modify. |
| `../semantic-pages` | **Reference repo.** Node.js/TS predecessor MCP over markdown vaults. Prior-art baseline that ctxvault supersedes (adds BM25, configurable edges, templates, multi-corpus, crash-safe persistence). Do NOT modify. |
| `../kubernetes` | **Test corpus.** Real open-source codebase for validating retrieval against a large polyglot repo. `code/` = source, `docs/` = documentation. Use for end-to-end / benchmark testing; do NOT treat as ctxvault source. |

When a task targets a specific repo, use its exact folder name and never nest one repo under another. Only `ctxvault` itself is the product source; the other three are read-only references/fixtures.
