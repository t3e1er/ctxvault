---
title: "The 17 MCP Tools: Complete Functional Catalog"
category: "mcp-modes"
status: "active"
tags: ["tools", "catalog", "mcp", "api-reference", "tools-registry"]
related:
  - "[[docs/mcp-modes/index]]"
  - "[[docs/mcp-modes/search-modes-modalities]]"
  - "[[docs/agentic-strategy/tool-profiling]]"
  - "[[docs/mcp-modes/decisions/adr-010-unified-modal-search-tool]]"
  - "[[docs/mcp-modes/decisions/adr-011-readonly-readwrite-handler-model]]"
---

# The 17 MCP Tools: Complete Functional Catalog

The authoritative tool registry lives in `crates/ctxvault-mcp/src/tools/mod.rs` (`ToolRegistry`). All 17 tools are partitioned across 5 functional domains.

---

## 1. Registered Tool Inventory (17 Tools)

```
┌──────────────────────────────┬────────┬────────────────────────────────────────────────────────────────────────┐
│ Domain                       │ Count  │ Registered Tools                                                       │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 1. Read                      │ 3      │ `read_file`, `get_snippet`, `list_notes`                               │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 2. Search                    │ 2      │ `search` (unified tool with Turn 1 hybrid snippets), `search_related`  │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 3. Graph                     │ 2      │ `graph_match`, `graph_communities` (view="architecture"|"raw")         │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 4. Write (Mutating)          │ 3      │ `write_note` (mode="create"|"overwrite"|"append"|"prepend"),           │
│                              │        │ `delete_note`, `move_note`                                             │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 5. Template & Validation     │ 2      │ `validate` (note template & corpus taxonomy check), `list_templates`   │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 6. System & Corpus Admin     │ 5      │ `status` (unified scope tool: corpus, indexing, graph, coverage, all), │
│                              │        │ `list_corpora`, `sync_corpus` (mode="delta"|"full"|"reembed"),         │
│                              │        │ `index_corpus` (mutating), `unload_corpus` (mutating)                  │
└──────────────────────────────┴────────┴────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Handler Execution Categories

Every tool is bound to one of two execution paths:
1. **`ReadOnly(fn(&Engine, Value))`**: Executes under `RwLockReadGuard`. Multiple read tools run concurrently across worker threads with zero contention.
2. **`ReadWrite(fn(&mut Engine, Value))`**: Executes under exclusive `RwLockWriteGuard`. Serializes mutations, ensures atomic index commits, and triggers filesystem cache synchronization.

---

## 3. High-Value Specialist Tools

* **`read_file`**: Tier-3 reader. Supports single path string or array of paths (`paths`), with line slicing (`start_line`, `end_line`) and `max_lines` bounds.
* **`get_snippet`**: Tier-2 fetch. Retrieves exact code symbol sources (by `name` or `qualified_name`) or bounded doc chunks (by `path` + `chunk_index`), with optional neighbor expansion.
* **`status(scope="coverage")`**: Audits whether specified directories or file paths are indexed and parsed.
* **`graph_communities(view="architecture")`**: Generates a high-level subsystem component map with top key nodes and cluster density.

See [[docs/mcp-modes/decisions/adr-011-readonly-readwrite-handler-model]] for concurrency details.
