# ctxvault (`ctxvault` / `ctxv`)

**Enterprise Semantic Model Context Protocol (MCP) Server** for markdown knowledge bases and polyglot codebases. Features pure Rust hybrid BM25 + ONNX vector + Petgraph typed graph retrieval with 3-Way Reciprocal Rank Fusion (RRF), formal schema validation, and Principle 3 knowledge crystallization.

Written in 100% pure Rust (`unsafe = forbid`) for maximum performance, safety, zero-dependency deployment, and sub-millisecond graph and full-text retrieval.

---

## The `ctxvault` Ethos

`ctxvault` is built around five foundational principles designed for the next generation of AI development and multi-agent orchestration:

1. **Markdown is the Authoritative Ground Truth**: Files on disk are king. Indices (BM25, HNSW vectors, SQLite relation caches) are derived, disposable, and 100% rebuildable. Your knowledge remains human-readable, git-trackable, and portable forever.
2. **Explicit Graph Topology over Flaky Extraction**: Knowledge structure arises deterministically from typed frontmatter fields, `#tags`, and `[[wikilinks]]` — eliminating expensive, non-deterministic LLM entity-extraction pipelines.
3. **Continuous Knowledge Crystallization**: AI agent interactions produce valuable conversational exhaust (debugging traces, design consensus, bug resolutions). `ctxvault` provides first-class primitives (`promote_concept`, `traverse_lineage`) to distill ephemeral traces into permanent, schema-validated semantic knowledge assets with full provenance.
4. **Pure Rust Sub-Millisecond Speed**: With p50 retrieval latencies under 2.2ms, AI agents can execute multi-hop graph traversals and hybrid ranking in real-time without introducing perceptible reasoning lag.
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

---

## MCP Client Configuration

### Claude Desktop (`claude_desktop_config.json`)
```json
{
  "mcpServers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": [
        "--corpus", "/path/to/your/markdown/vault",
        "--sync"
      ]
    }
  }
}
```

### Cursor / VS Code / Antigravity (`.mcp.json` or Settings)
```json
{
  "mcpServers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": [
        "--corpus", "${workspaceFolder}",
        "--sync"
      ]
    }
  }
}
```

### Shared Multi-Agent / Local Network Server Mode
Host `ctxvault` as a shared daemon so multiple IDEs, team members on LAN, or sandboxed agent environments share a single in-memory index:
```bash
# Bind to localhost (local multi-agent) or 0.0.0.0 (LAN hackathon / team sharing)
ctxvault --mode server --bind 0.0.0.0:9090 --corpus /path/to/vault --sync

# Serve multiple corpora from one process; name them and pick a tool profile.
# --corpus accepts `name=path` or a bare `path`; --profile is scout|analysis|all (default all).
ctxvault --mode server --bind 0.0.0.0:9090 \
  --corpus vault=/path/to/wiki --corpus code=/path/to/repo \
  --default-corpus vault --profile analysis --sync
```

Read tools then take an optional `corpus` (single root) or `corpora` (`["vault","code"]`
or `"all"`, fan-out + RRF-merge with per-hit corpus tagging). Search tools also take
`modality` (`docs`|`code`|`both`) and `detail` (`ids`|`default`).

#### Direct Remote Client (Antigravity / Remote SSE-capable IDEs)
```json
{
  "mcpServers": {
    "ctxvault": {
      "serverUrl": "http://<HOST_IP>:9090/sse"
    }
  }
}
```

#### Stdio Proxy Mode (Claude Desktop, Cursor, Sandboxed Containers)
For IDEs and containerized agents that only support local stdio processes, run `ctxvault` in proxy mode:
```json
{
  "mcpServers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": [
        "--mode", "proxy",
        "--server", "http://<HOST_IP>:9090"
      ]
    }
  }
}
```

#### CLI / Scripted Client Mode
```bash
ctxvault --mode client --server http://<HOST_IP>:9090 --call search --query "architecture" --args '{"mode":"hybrid"}'
```

---

## Key Architectural Features

- **4-Modality Hybrid Retrieval**:
  - **Tantivy Okapi BM25**: Full-text inverted index with field norms, term positions, and tokenization.
  - **Dense Vector Search**: ONNX `BGE-small-en-v1.5` embeddings with document-level chunk max-pooling.
  - **Petgraph Typed Graph Traversal**: Direct frontmatter relations, `#tags`, and `[[wikilinks]]`.
  - **3-Way Reciprocal Rank Fusion (RRF)**: Calibrated multi-modal rank combination without brittle score-scaling heuristics.
- **Principle 3 Knowledge Crystallization**:
  - `promote_concept` tool synthesizes structured architecture decisions (ADRs) and incident post-mortems from raw episodic logs with 100% schema validation and lineage graph edge synthesis.
- **Sub-Millisecond Engine Latency**:
  - Lexical BM25 search p50: **2.2 ms** (>400 QPS)
  - Graph BFS proximity hops p50: **1.8 ms** (>500 QPS)
- **Multi-Corpus Isolation**: Multiple knowledge bases isolated in a single server process with atomic synchronization.

---

## Developer Workflow

```bash
cargo check                     # Fast type-checking
cargo test                      # Run all 156+ unit, integration & e2e tests
cargo clippy --all-targets -- -D warnings
cargo build --release           # Build release binary (target/release/ctxvault)
```

---

## Workspace Layout

| Crate | Role |
|---|---|
| [`ctxvault-common`](crates/ctxvault-common) | Shared domain types, TOML configurations, error definitions |
| [`ctxvault-core`](crates/ctxvault-core) | Retrieval engine: Tantivy, FastEmbed, Petgraph, SQLite, chunking, file watcher |
| [`ctxvault-mcp`](crates/ctxvault-mcp) | Model Context Protocol JSON-RPC transport and 31+ MCP tools |
| [`ctxvault-cli`](crates/ctxvault-cli) | Native CLI binary: argument parsing, mode selection, orchestration |
| [`examples`](examples) | Steering snippets, workflow skills, multi-agent swarms, and starter vault |

---

## Security & Quality Gates

- `unsafe` is forbidden workspace-wide (`unsafe_code = "forbid"`).
- Zero C runtime dependencies (pure Rust TLS via `rustls-tls`, bundled SQLite via `rusqlite`).
- `cargo-deny` enforces strict license compliance and dependency security.
- Automated CI pipeline executes format verification, Clippy lints, MSRV checks, and unit tests across Ubuntu, Windows, and macOS.

---

## License

MIT
