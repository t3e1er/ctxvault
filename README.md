# cxtvault (`cxtvault` / `cxtv`)

**Enterprise Semantic Model Context Protocol (MCP) Server** for markdown knowledge bases and polyglot codebases. Features pure Rust hybrid BM25 + ONNX vector + Petgraph typed graph retrieval with 3-Way Reciprocal Rank Fusion (RRF), formal schema validation, and Principle 3 knowledge crystallization.

Written in 100% pure Rust (`unsafe = forbid`) for maximum performance, safety, zero-dependency deployment, and sub-millisecond graph and full-text retrieval.

---

## ⚡ Installation

Install the precompiled native standalone binary for your platform in one command:

### macOS & Linux
```bash
curl -fsSL https://raw.githubusercontent.com/YOUR_ORG/cxtvault/main/install.sh | sh
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/YOUR_ORG/cxtvault/main/install.ps1 | iex
```

### From Source (via Cargo)
```bash
cargo install --locked --path crates/cxtvault-cli
```

---

## 🔌 MCP Client Configuration

### Claude Desktop (`claude_desktop_config.json`)
```json
{
  "mcpServers": {
    "cxtvault": {
      "command": "cxtvault",
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
    "cxtvault": {
      "command": "cxtvault",
      "args": [
        "--corpus", "${workspaceFolder}",
        "--sync"
      ]
    }
  }
}
```

### Shared Multi-Agent HTTP Server Mode
You can also run `cxtvault` as a shared local daemon so multiple IDEs/agents share a single in-memory index:
```bash
cxtvault --mode server --bind 127.0.0.1:9090 --corpus /path/to/vault
```
```json
{
  "mcpServers": {
    "cxtvault": {
      "url": "http://127.0.0.1:9090/sse"
    }
  }
}
```

---

## 🚀 Key Architectural Features

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

## 🛠️ Developer Workflow

```bash
cargo check                     # Fast type-checking
cargo test                      # Run all 156+ unit, integration & e2e tests
cargo clippy --all-targets -- -D warnings
cargo build --release           # Build release binary (target/release/cxtvault)
```

---

## 📦 Workspace Layout

| Crate | Role |
|---|---|
| [`cxtvault-common`](crates/cxtvault-common) | Shared domain types, TOML configurations, error definitions |
| [`cxtvault-core`](crates/cxtvault-core) | Retrieval engine: Tantivy, FastEmbed, Petgraph, SQLite, chunking, file watcher |
| [`cxtvault-mcp`](crates/cxtvault-mcp) | Model Context Protocol JSON-RPC transport and 31+ MCP tools |
| [`cxtvault-cli`](crates/cxtvault-cli) | Native CLI binary: argument parsing, mode selection, orchestration |

---

## 🔒 Security & Quality Gates

- `unsafe` is forbidden workspace-wide (`unsafe_code = "forbid"`).
- Zero C runtime dependencies (pure Rust TLS via `rustls-tls`, bundled SQLite via `rusqlite`).
- `cargo-deny` enforces strict license compliance and dependency security.

---

## 📄 License

MIT
