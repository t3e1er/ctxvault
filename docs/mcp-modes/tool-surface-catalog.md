---
title: "The 39 MCP Tools: Complete Functional Catalog"
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

# The 39 MCP Tools: Complete Functional Catalog

The authoritative tool registry lives in `crates/ctxvault-mcp/src/tools/mod.rs` (`ToolRegistry`). All 39 tools are partitioned across 8 functional domains.

---

## 1. Registered Tool Inventory

```
┌──────────────────────────────┬────────┬────────────────────────────────────────────────────────────────────────┐
│ Domain                       │ Count  │ Registered Tools                                                       │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 1. Read                      │ 6      │ `read_note`, `read_code_file`, `read_multiple`, `get_snippet`,         │
│                              │        │ `list_notes`, `get_frontmatter`                                        │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 2. Search                    │ 2      │ `search` (unified tool), `search_related`                              │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 3. Graph                     │ 8      │ `backlinks`, `forwardlinks`, `graph_path`, `graph_stats`,              │
│                              │        │ `graph_subgraph`, `graph_communities`, `list_edge_types`,              │
│                              │        │ `traverse_lineage`                                                     │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 4. Write (Mutating)          │ 5      │ `create_note`, `update_note`, `delete_note`, `move_note`,               │
│                              │        │ `promote_concept`                                                      │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 5. Template & Validation     │ 4      │ `validate_note`, `validate_corpus`, `list_templates`,                  │
│                              │        │ `validate_taxonomy`                                                    │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 6. Corpus Analysis           │ 5      │ `analyze_density`, `find_semantic_gaps`, `suggest_splits`,             │
│                              │        │ `coverage_report`, `check_index_coverage`                              │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 7. Code Intelligence         │ 4      │ `get_symbol_definition`, `find_callers`, `get_architecture`,           │
│                              │        │ `detect_changes` (mutating)                                            │
├──────────────────────────────┼────────┼────────────────────────────────────────────────────────────────────────┤
│ 8. System & Corpus Admin     │ 5      │ `status` (unified tool), `corpus_list`, `reindex_corpus` (mutating),   │
│                              │        │ `sync_corpus` (mutating), `reembed_corpus` (mutating)                  │
└──────────────────────────────┴────────┴────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Handler Execution Categories

Every tool is bound to one of two execution paths:
1. **`ReadOnly(fn(&Engine, Value))`**: Executes under `RwLockReadGuard`. Multiple read tools run concurrently across worker threads with zero contention.
2. **`ReadWrite(fn(&mut Engine, Value))`**: Executes under exclusive `RwLockWriteGuard`. Serializes mutations, ensures atomic index commits, and triggers filesystem cache synchronization.

---

## 3. High-Value Specialist Tools

* **`read_multiple`**: Tier-3 batch reader. Fetches $N$ file paths in a single round-trip. Per-file missing paths return error entries within the JSON array rather than failing the entire invocation.
* **`check_index_coverage`**: Audits whether specified directories or file paths are indexed, reporting symbol counts and flag parse gaps (empty files).
* **`detect_changes`**: Fast SHA-256 hash comparison across the filesystem computing the downstream symbol blast radius before code is committed.

See [[docs/mcp-modes/decisions/adr-011-readonly-readwrite-handler-model]] for concurrency details.
