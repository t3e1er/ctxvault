# ctxvault (`ctxvault` / `ctxv`)

**Enterprise Semantic Model Context Protocol (MCP) Server** for markdown knowledge bases and polyglot codebases. Features pure Rust hybrid BM25 + ONNX vector + Petgraph typed graph retrieval with 3-Way Reciprocal Rank Fusion (RRF), Cypher-Lite linear pattern queries, formal schema validation, and Principle 3 knowledge crystallization.

Written in 100% pure Rust (`unsafe_code = "forbid"`) for maximum performance, memory safety, zero C-runtime dependencies, and sub-millisecond graph and full-text retrieval.

---

## The `ctxvault` Ethos

`ctxvault` is built around five foundational principles designed for the next generation of AI development and multi-agent orchestration:

1. **Markdown & Source are Authoritative Ground Truth**: Files on disk are king. All indices (Tantivy BM25, HNSW vectors, SQLite metadata, Petgraph) are derived, disposable, and 100% rebuildable. Your knowledge and code remain human-readable, git-trackable, and portable forever.
2. **Explicit Graph Topology over Flaky Extraction**: Knowledge and code structures arise deterministically from typed frontmatter fields, `#tags`, `[[wikilinks]]`, and AST relations (`calls`, `defines`, `imports`, `implements`) — eliminating expensive, non-deterministic LLM entity-extraction pipelines.
3. **Continuous Knowledge Crystallization**: AI agent interactions produce valuable conversational exhaust (debugging traces, design consensus, bug resolutions). `ctxvault` provides first-class primitives (`write_note` with schema templates and `derived_from` frontmatter, plus `graph_match` for ancestry tracing) to distill ephemeral traces into permanent, schema-validated semantic knowledge assets with full provenance.
4. **Pure Rust Sub-Millisecond Speed**: With p50 retrieval latencies under 2.2ms for lexical search and under 1.8ms for graph CTE traversals, AI agents can execute multi-hop graph queries and hybrid ranking in real-time without introducing perceptible reasoning lag.
5. **Multi-Agent Memory Substrate**: Designed to act as a shared in-memory and on-disk semantic plane for swarms of specialized agents (Scouts, Readers, Writers, Crystallizers).

---

## Quickstart & Starter Pack

We provide ready-to-use steering prompts, editor rules, workflow skills, multi-agent blueprints, and a pre-configured starter knowledge base in [`examples/`](examples/):

| Category | Resources | Description |
|---|---|---|
| **AI Steering & Rules** | [`examples/steering/`](examples/steering/) | Drop-in rules for [Cursor (`.cursorrules`)](examples/steering/cursorrules.md), [Antigravity / Gemini](examples/steering/ctxvault-rules.md), [Claude Desktop](examples/steering/claude-system-prompt.md), and [Windsurf](examples/steering/windsurf-rules.md). |
| **Workflow Skills** | [`examples/skills/`](examples/skills/) | Production `SKILL.md` runbooks: [`search`](examples/skills/ctxvault-search/SKILL.md), [`curate`](examples/skills/ctxvault-curate/SKILL.md), [`crystallize`](examples/skills/ctxvault-crystallize/SKILL.md), and [`ops`](examples/skills/ctxvault-ops/SKILL.md). |
| **Multi-Agent Swarms** | [`examples/agents/`](examples/agents/) | Role definitions for [Scout](examples/agents/scout_agent.md), [Reader](examples/agents/reader_agent.md), [Writer](examples/agents/writer_agent.md), and [Crystallizer](examples/agents/crystallizer_agent.md), plus [Swarm Orchestration Blueprints](examples/agents/swarm_orchestration.md). |
| **Starter Knowledge Vault** | [`examples/starter-vault/`](examples/starter-vault/) | Turnkey demo vault with [`corpus.toml`](examples/starter-vault/corpus.toml), 4 formal schema templates, and sample interlinked notes. |

---

## Installation

Install the precompiled native standalone binary for your platform in one command:

### macOS & Linux
```bash
curl -fsSL https://raw.githubusercontent.com/t3e1er/ctxvault/master/install.sh | sh
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/t3e1er/ctxvault/master/install.ps1 | iex
```

### From Source (via Cargo)
```bash
cargo install --locked --path crates/ctxvault-cli
```

### Auto-Configuration & Agent Steering Setup
Run `ctxvault install` to automatically detect installed coding agents (Antigravity IDE, Gemini CLI, Cursor, Claude Desktop, Claude Code, Windsurf, VS Code, Zed) and configure their MCP launchers:
```bash
ctxvault install -y
```

### Embedding Model (Sidecar)

Semantic and vector search uses a local ONNX embedding model ([`jinaai/jina-embeddings-v2-base-code`](https://huggingface.co/jinaai/jina-embeddings-v2-base-code), 768 dimensions, Apache-2.0). The release archives **bundle it as a sidecar** next to the binary (`<binary-dir>/models/jina-embeddings-v2-base-code/`), and `install.sh` / `install.ps1` place it automatically — no separate download required.

For source builds or development:
```bash
just fetch-model                      # downloads into ./models
export CTX_MODELS_DIR="$(pwd)/models" # point ctxvault (and cargo test) at it
```
The embedder resolves the model from `CTX_MODELS_DIR`, then a `models/` sidecar next to the binary, then `../models/` (for `cargo test`). Fast mode (`--fast`) skips embeddings entirely for instant Tantivy BM25 + graph indexing.

---

## MCP Tool Surface (17 Authoritative Tools)

The authoritative tool registry lives in `crates/ctxvault-mcp/src/tools/mod.rs` (17 tools across 5 domains):

| Domain | Count | Tools | Description |
|---|---|---|---|
| **Read** | 3 | `read_file`, `get_snippet`, `list_notes` | Tier 3 batch polymorphic reader (`read_file` with `[start_line, end_line]`), Tier 2 bounded symbol/chunk fetcher (`get_snippet`), and catalog inspector (`list_notes`). |
| **Search** | 2 | `search`, `search_related` | Tier 1 retrieval with Turn 1 hybrid snippets (`snippets: usize`, default 3) across docs & code (`mode` = `hybrid` \| `bm25` \| `semantic` \| `graph` \| `explain`), and Personalized PageRank (`search_related`). |
| **Graph** | 2 | `graph_match`, `graph_communities` | Linear Cypher-Lite ASCII path query compiled to recursive SQLite CTEs (`graph_match`), and Leiden/Louvain community detection (`graph_communities`). |
| **Write** | 3 | `write_note`, `delete_note`, `move_note` | Schema-driven authoring (`write_note` with `mode="create"|"overwrite"|"append"|"prepend"`), note removal (`delete_note`), and wikilink refactoring (`move_note`). |
| **Validation** | 2 | `validate`, `list_templates` | Unified template and taxonomy validator (`validate` with `check_taxonomy=true`), and template discovery (`list_templates`). |
| **System** | 5 | `status`, `list_corpora`, `sync_corpus`, `index_corpus`, `unload_corpus` | Multi-corpus overview (`status` with `scope="corpus"|"indexing"|"graph"|"coverage"|"all"`), corpus listing, delta/full reindexing, and dynamic runtime management. |

### Tool Exposure Profiles (`--profile`)
Gate advertised tools to fit specific agent roles:
- **`scout`** (6 tools): `search`, `search_related`, `get_snippet`, `read_file`, `list_notes`, `status`.
- **`analysis`** (11 tools): `scout` + `graph_match`, `graph_communities`, `validate`, `list_templates`, `list_corpora`.
- **`all`** (17 tools, default): full suite including mutating tools (`write_note`, `delete_note`, `move_note`, `sync_corpus`, `index_corpus`, `unload_corpus`).

---

## Progressive Disclosure & Turn 1 Affordances

`ctxvault` eliminates context rot and multi-turn reasoning lag through a strict 3-tier progressive disclosure model:

```
Turn 1: search(query, snippets=3)
  ├── Partitioned docs and code hits
  ├── Turn 1 inline text / symbol snippets (zero round-trip answers)
  ├── Graph affordances (calls_in, calls_out, implements, imports, wikilinks)
  └── Schema envelope (available node labels & edge types)
          │
          ▼ (if deeper symbol inspection or traversal is needed)
Turn 2: get_snippet(symbol="...") OR graph_match(pattern="...")
          │
          ▼ (only as an exhaustive last resort)
Turn 3: read_file(path="...", start_line=1, end_line=120)
```

---

## Cypher-Lite Query Language (`graph_match`)

`ctxvault` features **Cypher-Lite**, a linear ASCII graph query language compiled directly into recursive SQLite Common Table Expressions (CTEs) with cycle guards and bounded depths for sub-millisecond execution.

### Pattern Syntax
```text
(source)-[:edge_type]->(target)
(:CodeSymbol {name: "NewMainKubelet"})-[:calls*1..2]->(target)
(:DocNode {path: "adrs/001-architecture.md"})-[:derived_from*1..3]->(target)
(source)-[:implements]->(target)
```

### Anchor Resolution Order
1. `path` property filter
2. `name` property filter (indexed via `idx_code_symbols_name`)
3. `title` property filter
4. `scope` / `scope_path` (indexed via `idx_code_symbols_scope`)

### 5 Typed Edge Classes (`edge_class`)
Filter traversals across dedicated graph layers:
- `"code"`: AST code relationships (`defines`, `imports`, `calls`, `implements`).
- `"structural"`: Document layout relationships (`parent_child`, `section`).
- `"semantic"`: Markdown graph links (`wikilink`, `derived_from`, `shared_tag`).
- `"crossmodal"`: Cross-domain links (`documents`, `implements_spec`).
- `"hybrid"`: Cross-layer blended edges.

---

## Bi-Modal Retrieval Architecture

Documentation and Polyglot Source Code are treated as distinct first-class modalities:
- **`modality="docs"`**: Searches documentation notes, ADRs, RFCs, and markdown chunks using heading-aware chunking.
- **`modality="code"`**: Searches polyglot source code (Rust, Go, TypeScript/JavaScript, Python, Java, C/C++) chunked via Tree-sitter cAST parsing.
- **`modality="both"` (default)**: Independent 3-way RRF rank fusion across both modalities, returning partitioned `docs` and `code` result sets.

---

## Deployment Modes

### 1. Local / Stdio Mode (Default)
Single process communicating over standard input/output. Used directly by Cursor, Claude Desktop, Antigravity, and VS Code:
```json
{
  "mcpServers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": ["--corpus", "${workspaceFolder}", "--sync"]
    }
  }
}
```

### 2. Auto-Daemon Mode
Probes port 9090; if not running, detaches a shared background daemon and bridges stdio JSON-RPC transparently.

### 3. Shared Multi-Agent / Server Mode
Host a central daemon serving multiple corpora to team members or sandboxed swarms over HTTP SSE:
```bash
ctxvault --mode server --bind 0.0.0.0:9090 \
  --corpus wiki=/path/to/notes --corpus repo=/path/to/code \
  --default-corpus wiki --profile analysis --sync
```

### 4. CLI / Scripted Client Mode
```bash
ctxvault --mode client --server http://127.0.0.1:9090 --call search --query "authentication" --args '{"mode":"hybrid","snippets":3}'
```

---

## Workspace Layout

| Crate | Role |
|---|---|
| [`ctxvault-common`](crates/ctxvault-common) | Shared domain types, TOML configurations, error definitions, ports traits |
| [`ctxvault-core`](crates/ctxvault-core) | Engine: Tantivy BM25, ONNX embedder (`ort`), Petgraph, SQLite catalog, cAST parser |
| [`ctxvault-mcp`](crates/ctxvault-mcp) | Model Context Protocol JSON-RPC transport and authoritative 17 tools |
| [`ctxvault-cli`](crates/ctxvault-cli) | Native CLI binary: composition root, multi-corpus manager, agent installer |
| [`examples`](examples) | Steering snippets, workflow skills, multi-agent swarms, and starter vault |

---

## Developer Workflow

```bash
cargo check                     # Fast type-checking
cargo test                      # Run all 156+ unit, integration & e2e tests
cargo clippy --all-targets -- -D warnings
cargo build --release           # Build release binary (target/release/ctxvault)
```

---

## Security & Quality Gates

- `unsafe` is forbidden workspace-wide (`unsafe_code = "forbid"`).
- Zero C runtime dependencies (pure Rust TLS via `rustls-tls`, bundled SQLite via `rusqlite`).
- `cargo-deny` enforces strict license compliance and dependency security.
- Automated CI pipeline executes format verification, Clippy lints, MSRV checks, and unit tests across Ubuntu, Windows, and macOS.

---

## License

MIT
