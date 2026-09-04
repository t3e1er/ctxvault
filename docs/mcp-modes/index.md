---
title: "Model Context Protocol Surface & Transports Gateway"
category: "mcp-modes"
status: "active"
tags: ["mcp-modes", "mcp", "transports", "tools", "multi-corpus", "json-rpc"]
related:
  - "[[docs/index]]"
  - "[[docs/mcp-modes/transport-architecture]]"
  - "[[docs/mcp-modes/tool-surface-catalog]]"
  - "[[docs/mcp-modes/search-modes-modalities]]"
  - "[[docs/mcp-modes/multi-corpus-serving]]"
  - "[[docs/mcp-modes/schema-taxonomy-enforcement]]"
  - "[[docs/mcp-modes/decisions/adr-010-unified-modal-search-tool]]"
  - "[[docs/mcp-modes/decisions/adr-011-readonly-readwrite-handler-model]]"
  - "[[docs/mcp-modes/decisions/adr-012-in-process-multi-corpus-manager]]"
---

# Model Context Protocol Surface & Transports Hub

Welcome to the **Model Context Protocol (MCP) Surface & Transports** module of `ctxvault`. This cluster provides the authoritative specification for ctxvault's MCP server implementation, transport layers, multi-corpus routing, and 39-tool registry.

---

## 1. Protocol Architecture & Transports

```
                               ┌──────────────────────────────────────────────┐
                               │           AI Agent Client (JSON-RPC)         │
                               └──────────────────────┬───────────────────────┘
                                                      │
                       ┌──────────────────────────────┴──────────────────────────────┐
                       ▼                                                             ▼
         ┌───────────────────────────┐                                 ┌───────────────────────────┐
         │     Stdio Transport       │                                 │   Streamable HTTP / SSE   │
         │   Local Single-Process    │                                 │   Remote Enterprise Svc   │
         └─────────────┬─────────────┘                                 └─────────────┬─────────────┘
                       │                                                             │
                       └──────────────────────────────┬──────────────────────────────┘
                                                      │
                                                      ▼
                                       ┌──────────────────────────────┐
                                       │        CorpusManager         │
                                       │   Serves Roots C_1 ... C_N   │
                                       └──────────────┬───────────────┘
                                                      │
                                                      ▼
                                       ┌──────────────────────────────┐
                                       │    ToolRegistry (39 Tools)   │
                                       │  ReadOnly vs ReadWrite Locks │
                                       └──────────────────────────────┘
```

---

## 2. Core Architectural Articles

1. **[[docs/mcp-modes/transport-architecture]]**
   * *Dual Transports*: Stdio IPC with content-length framing for local IDE subagents vs Axum-based Streamable HTTP with Server-Sent Events (SSE) and bearer token authentication for remote multi-tenant deployments.
2. **[[docs/mcp-modes/tool-surface-catalog]]**
   * *The 39 Authoritative Tools*: Comprehensive inventory across 8 functional categories: Read (6), Search (2), Graph (8), Write (5), Template/Validation (4), Analysis (5), Code Intel (4), and System/Corpus (5).
3. **[[docs/mcp-modes/search-modes-modalities]]**
   * *Unified Search Parameterization*: Modal execution across `mode="hybrid"`, `mode="bm25"`, `mode="semantic"`, `mode="graph"`, and `mode="explain"`, combined with bi-modal filtering (`modality="code"|"docs"|"both"`).
4. **[[docs/mcp-modes/multi-corpus-serving]]**
   * *Multi-Root Orchestration*: Serving $N$ independent index roots from a single process via `CorpusManager`. Query fan-out across corpora and unambiguous cross-corpus symbol linking (`link_cross_corpus_symbols`).
5. **[[docs/mcp-modes/schema-taxonomy-enforcement]]**
   * *Taxonomy Integrity & Templates*: Formal note validation against `.templates/` schemas, frontmatter linting, tag taxonomy checking (`validate_taxonomy`), and whole-vault health audits (`validate_corpus`).

---

## 3. Architectural Decision Records (ADRs)

* **[[docs/mcp-modes/decisions/adr-010-unified-modal-search-tool]]**: Consolidating multiple disparate search endpoints into a single polymorphic `search` tool with a `mode` parameter.
* **[[docs/mcp-modes/decisions/adr-011-readonly-readwrite-handler-model]]**: Segregating tool handlers into `ReadOnly` and `ReadWrite` execution paths for lock-free reader concurrency under `RwLock`.
* **[[docs/mcp-modes/decisions/adr-012-in-process-multi-corpus-manager]]**: Managing multiple independent index roots within a single process to amortize ONNX embedding model memory.
