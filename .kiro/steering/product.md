---
inclusion: always
---

# ctxvault — Product Context

## What It Is

ctxvault (`ctxvault` / `ctxv`) is an enterprise semantic **Model Context Protocol (MCP) server** for markdown knowledge bases and polyglot codebases. It gives AI agents fast, minimal, high-signal context instead of dumping whole files or relying on flaky LLM entity extraction.

Written in 100% pure Rust (`unsafe_code = "forbid"`) for performance, safety, zero-dependency deployment, and sub-millisecond graph + full-text retrieval.

## The Problem

AI development tools suffer from **context rot** (too much irrelevant context slows and confuses agents) and **flat knowledge** (decisions, specs, code live in unstructured, disconnected files). ctxvault organizes knowledge into a typed, queryable graph and retrieves the minimal relevant slice per query.

## Core Principles (non-negotiable invariants)

1. **Markdown/source is the authoritative ground truth.** Files on disk are king. All indices (Tantivy BM25, HNSW vectors, SQLite catalog, Petgraph) are derived, disposable, and 100% rebuildable. Never treat an index as canonical.
2. **Explicit graph topology, not LLM extraction.** Edges come deterministically from typed frontmatter fields, `#tags`, and `[[wikilinks]]` — never from expensive, non-deterministic extraction pipelines.
3. **Continuous knowledge crystallization (Principle 3).** Distill ephemeral agent exhaust (debug traces, design consensus, bug resolutions) into permanent, schema-validated notes with full lineage/provenance via `promote_concept` / `traverse_lineage`.
4. **Pure Rust sub-millisecond speed.** Multi-hop graph traversal and hybrid ranking run in real time (p50 lexical ~2.2ms, graph BFS ~1.8ms) with no perceptible agent lag.
5. **Multi-agent memory substrate.** A shared in-memory + on-disk semantic plane for swarms of specialized agents (Scouts, Readers, Writers, Crystallizers).

## Retrieval Model

4-modality hybrid retrieval fused with 3-way Reciprocal Rank Fusion (RRF):
- **Tantivy Okapi BM25** — full-text inverted index (exact identifiers, verbatim tokens).
- **Dense vectors** — ONNX `jina-embeddings-v2-base-code` (768-dim) embeddings, document-level chunk max-pooling.
- **Petgraph typed graph traversal** — frontmatter relations, `#tags`, `[[wikilinks]]`, and code edges (`calls`, `defines`, `imports`, `implements`).
- **RRF fusion** — calibrated rank combination, no brittle score-scaling heuristics.

ctxvault unifies **documentation** (ADRs, RFCs, design docs, Obsidian vaults) and **polyglot source code** (Rust, TS/JS, Python, Go, Java, C/C++) in a single cross-modal graph, so a doc can link `implements` → a code symbol, and code can link back to explaining ADRs.

### Multi-corpus, cross-modal, progressive disclosure

- **Multi-corpus.** One central MCP process serves N index roots via a `CorpusManager`. Read tools take an optional `corpus` (single root) or `corpora` (`["a","b"]` or `"all"`); cross-corpus queries fan out and RRF-merge, tagging each hit with its source corpus. A single root is just N=1.
- **Cross-corpus symbol linking.** A doc that `implements`/`documents` a code symbol resolves to that symbol even when it lives in a different corpus, but only when the qualified name resolves uniquely (ambiguous/unresolved ⇒ no false edge). Resolved cross-corpus edges carry a confidence band.
- **Bi-modal filtering.** Every search accepts `modality` = `docs` | `code` | `both` (default), applied consistently across BM25, vector, graph, and the fused hybrid path.
- **Progressive disclosure (three tiers).** Tier 1 — `search` returns handles (paths/qualified names + line ranges), never full bodies; `detail=ids` gives bare handles, `default` a short snippet. Tier 2 — `get_snippet` fetches exactly one code symbol or one doc chunk, bounded, with optional neighbor expansion. Tier 3 — `read_note` / `read_code_file` / `read_multiple` read whole files as a last resort.
- **Condensed tool surface + profiles.** The `search_*` family is one `search` tool (`mode` param); status tools are one `status` tool (`scope` param). A `--profile` flag (`scout` ⊂ `analysis` ⊂ `all`) gates the advertised `tools/list` to keep the payload small for narrow agent roles.

## Deployment Modes

- **Local (stdio)** — standard MCP over stdin/stdout, single process, serving one or many corpora.
- **Server (Streamable HTTP)** — Axum HTTP server, multiple concurrent agents, bearer-token auth, CORS, health endpoint.
- **Proxy (stdio → remote)** — local stdio process forwarding JSON-RPC to a central shared server.
- **Client (scripted CLI)** — one-shot tool calls against a remote server.
