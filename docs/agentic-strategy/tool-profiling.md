---
title: "Tool Profiling: Security, Prompt Budgets & Profile Gating"
category: "agentic-strategy"
status: "active"
tags: ["tool-profiling", "mcp", "security", "prompt-budget", "profiles"]
related:
  - "[[docs/agentic-strategy/index]]"
  - "[[docs/agentic-strategy/swarm-topologies]]"
  - "[[docs/mcp-modes/tool-surface-catalog]]"
  - "[[docs/agentic-strategy/decisions/adr-006-role-based-tool-profiling]]"
---

# Tool Profiling: Security, Prompt Budgets & Profile Gating

Exposing a monolithic namespace of 39 tools to an LLM creates two severe issues:
1. **Prompt Token Bloat**: Serializing 39 JSON Schema tool descriptions consumes 4,000–6,000 tokens per prompt turn before conversation content begins.
2. **Safety & Hallucination Risks**: Small reasoning models can accidentally invoke destructive tools (`delete_note`, `reindex_corpus`) during simple read-only queries.

`ctxvault` solves this via **Hierarchical Tool Profiling (`--profile`)**.

---

## 1. Nested Profile Hierarchy

Profiles are organized as strict nested sets:
$$\text{scout} \subset \text{analysis} \subset \text{all}$$

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│ --profile scout (9 Tools)                                                                       │
│ Minimal retrieve & navigate: search, search_related, get_snippet, read_note, read_code_file,     │
│ read_multiple, list_notes, get_frontmatter, status.                                             │
└─────────────────────────────────┬───────────────────────────────────────────────────────────────┘
                                  │ Extends with read-only analysis tools
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│ --profile analysis (30 Tools)                                                                   │
│ scout (9) + backlinks, forwardlinks, graph_path, graph_stats, graph_subgraph, graph_communities,│
│ list_edge_types, traverse_lineage, get_symbol_definition, find_callers, get_architecture,       │
│ validate_note, validate_corpus, list_templates, validate_taxonomy, analyze_density,             │
│ find_semantic_gaps, suggest_splits, coverage_report, check_index_coverage, corpus_list.         │
└─────────────────────────────────┬───────────────────────────────────────────────────────────────┘
                                  │ Extends with mutating & administrative tools
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│ --profile all (39 Tools)                                                                        │
│ analysis (30) + create_note, update_note, delete_note, move_note, promote_concept,               │
│ reembed_corpus, sync_corpus, reindex_corpus, detect_changes.                                    │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Profile Gating Semantics

1. **`tools/list` Filtering**: When an MCP client invokes `tools/list`, `ctxvault-mcp` filters advertised tool definitions according to the active `--profile`. A Scout agent sees only 9 tools, conserving ~4,500 tokens of prompt context on every request.
2. **Non-Blocking Safety Seam**: Profile gating controls what is **advertised** to the LLM. If a specialized agent knows the exact schema of an unadvertised tool, invocation still executes, preventing artificial runtime deadlocks while guiding LLM decision-making.

See [[docs/agentic-strategy/decisions/adr-006-role-based-tool-profiling]] for the formal architectural decision record.
