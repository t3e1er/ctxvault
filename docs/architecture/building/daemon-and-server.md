---
title: "Auto-Daemon & Shared Server Deployment"
description: "Deploying ctxvault as a background daemon, hosting multi-corpus servers, and script automation."
category: "building"
status: "active"
tags: ["daemon", "server", "http", "sse", "multi-corpus", "concurrency"]
related:
  - "[[docs/architecture/building/index]]"
  - "[[docs/architecture/implementation/cross-corpus-federation]]"
  - "[[docs/architecture/implementation/mcp-transport]]"
---

# Auto-Daemon & Shared Server Deployment

`ctxvault` supports multiple operational modes: standard per-agent stdio processes, transparent background daemons, and dedicated multi-corpus HTTP servers.

---

## 1. The Auto-Daemon Pattern

When multiple subagents or editor windows run concurrently on the same machine, spawning multiple independent index engines wastes memory and locks SQLite databases.

The **Auto-Daemon** solves this transparently:
1. When launched with `--daemon`, the process checks if a local daemon is listening on port `9090`.
2. If absent, it detaches a shared background process serving the indexed corpora.
3. The foreground CLI bridges standard stdio JSON-RPC to the daemon over HTTP SSE with zero subagent configuration changes.

```bash
ctxvault --corpus /path/to/project --daemon
```

---

## 2. Dedicated Multi-Corpus Server Mode

For team environments, sandboxed CI agents, or multi-agent swarms, run `ctxvault` as a persistent standalone service hosting $N$ distinct index roots:

```bash
ctxvault --mode server --bind 0.0.0.0:9090 \
  --corpus docs=/opt/knowledge/docs \
  --corpus backend=/opt/services/backend \
  --corpus frontend=/opt/services/frontend \
  --default-corpus backend \
  --profile all \
  --sync
```

### Endpoints
* `POST /v1/mcp`: Standard MCP JSON-RPC 2.0 request/response handling.
* `GET /v1/sse`: Server-Sent Events stream for asynchronous agent notifications and progress reporting.
* `GET /health`: Health probe returning corpus status, VRAM usage, and active connections.

---

## 3. Scripted CLI Client Mode

Interact with a running daemon or local engine directly from shell scripts or CI pipelines without an MCP editor:

```bash
# Search using hybrid mode
ctxvault --mode client --server http://127.0.0.1:9090 \
  --call search \
  --args '{"query": "authentication token", "mode": "hybrid", "snippets": 3}'

# Inspect multi-hop graph lineage
ctxvault --mode client --server http://127.0.0.1:9090 \
  --call graph_match \
  --args '{"pattern": "(:CodeSymbol {name: \"verify_jwt\"})-[:calls*1..2]->(target)"}'
```
