---
title: "Agent Swarm Topologies: Roles, Substrates & Concurrency"
category: "agentic-strategy"
status: "active"
tags: ["swarms", "agentic-strategy", "personas", "concurrency", "topologies"]
related:
  - "[[docs/agentic-strategy/index]]"
  - "[[docs/agentic-strategy/tool-profiling]]"
  - "[[docs/agentic-strategy/decisions/adr-006-role-based-tool-profiling]]"
---

# Agent Swarm Topologies: Roles, Substrates & Concurrency

As AI development scales, monolithic single-agent loops become brittle. Complex tasks—such as codebase migration or security audits—are best executed by **specialized multi-agent swarms**.

`ctxvault` serves as the shared, thread-safe, cross-modal memory substrate across four specialized agent personas.

---

## 1. The Four Specialized Agent Personas

```
                     ┌────────────────────────────────────────────────────────┐
                     │                  Shared Memory Substrate               │
                     │          SQLite Catalog  •  Petgraph  •  HNSW          │
                     └───────────────────────────┬────────────────────────────┘
                                                 │
            ┌──────────────────┬─────────────────┴────────────────┬──────────────────┐
            ▼                  ▼                                  ▼                  ▼
      ┌───────────┐      ┌───────────┐                      ┌───────────┐      ┌───────────┐
      │   Scout   │      │  Reader   │                      │  Writer   │      │Crystallizr│
      │  (Search) │      │ (Analyze) │                      │ (Mutate)  │      │ (Distill) │
      └───────────┘      └───────────┘                      └───────────┘      └───────────┘
       Read-Only          Read-Only                          Mutating           Mutating
       Fast Nav           Deep AST                           Schema Safe        Provenance
```

### 1.1 Scout Agent (Navigator)
* **Objective**: Rapid exploration of codebases and documentation vaults.
* **Tool Profile**: `--profile scout` (9 tools).
* **Behavior**: Uses Tier-1 `search(detail="ids")`, `search_related`, and `graph_path`. Returns lightweight handle sets to coordinator agents without inflating its own context window.

### 1.2 Reader Agent (Comprehension Specialist)
* **Objective**: Deep semantic analysis and cross-modal verification.
* **Tool Profile**: `--profile analysis` (30 tools).
* **Behavior**: Focuses on Tier-2 `get_snippet`, `get_symbol_definition`, `find_callers`, and `get_architecture`. Never mutates the corpus.

### 1.3 Writer Agent (Documentation & Code Author)
* **Objective**: Creating and updating notes while maintaining corpus invariants.
* **Tool Profile**: `--profile all` (39 tools).
* **Behavior**: Executes `create_note`, `update_note`, `move_note`. Strictly enforces YAML frontmatter and template compliance via `validate_note`.

### 1.4 Crystallizer Agent (Memory Compiler)
* **Objective**: Distilling unstructured session traces into durable, permanent knowledge.
* **Tool Profile**: `--profile all` (39 tools).
* **Behavior**: Calls `promote_concept` to synthesize permanent markdown notes with `DerivedFrom` lineage edges, keeping knowledge persistent across sessions.

---

## 2. Multi-Agent Concurrency Model

In `ctxvault-mcp`, tool handlers are segregated into two execution categories:
* `ReadOnly(fn(&Engine, Value))`: Concurrent reads execute under a shared reader lock (`std::sync::RwLockReadGuard`), allowing dozens of Scout and Reader agents to query the engine simultaneously without latency penalties.
* `ReadWrite(fn(&mut Engine, Value))`: Mutating operations (`create_note`, `update_note`, `reindex_corpus`) acquire an exclusive writer lock (`std::sync::RwLockWriteGuard`), ensuring atomic index updates and zero torn reads.

See [[docs/agentic-strategy/decisions/adr-006-role-based-tool-profiling]] for tool profile design details.
