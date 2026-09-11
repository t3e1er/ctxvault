---
title: "The 3-Tier Progressive Disclosure Model"
description: "The strict contractual tiers governing agent retrieval from broad handle sweep to precise symbol extraction."
category: "progressive-disclosure"
status: "active"
tags: ["tiers", "tokens", "search", "get-snippet", "read-file", "contract"]
related:
  - "[[docs/concepts/progressive-disclosure/index]]"
  - "[[docs/concepts/progressive-disclosure/turn-1-affordances]]"
  - "[[docs/architecture/adr/adr-004-progressive-disclosure-token-contract]]"
---

# The 3-Tier Progressive Disclosure Model

`ctxvault` enforces a strict 3-tier retrieval progression to protect the agent's context window while guaranteeing complete answers.

```mermaid
flowchart TD
    subgraph Tier1["Tier 1: search (Broad Sweep)"]
        S1["Query & Modality Filter"]
        S2["Partitioned Docs & Code Hits"]
        S3["Top K Inline Snippets (snippets=3)"]
        S4["Graph Affordance Counts (calls, wikilinks)"]
    end

    subgraph Tier2["Tier 2: get_snippet / graph_match (Deep Focus)"]
        G1["Exact AST Symbol Extraction"]
        G2["Parent Scope Breadcrumbs"]
        G3["Cypher-Lite CTE Path Expansion"]
    end

    subgraph Tier3["Tier 3: read_file (Exhaustive Last Resort)"]
        R1["Bounded Line Slices [start_line, end_line]"]
        R2["Batch Multi-Path Reading"]
    end

    Tier1 -->|Needs deeper symbol?| Tier2
    Tier2 -->|Needs exhaustive file context?| Tier3
    
    style Tier1 fill:#1e293b,stroke:#3b82f6,stroke-width:2px,color:#fff
    style Tier2 fill:#1e293b,stroke:#10b981,stroke-width:2px,color:#fff
    style Tier3 fill:#1e293b,stroke:#f59e0b,stroke-width:2px,color:#fff
```

---

## Tier 1: `search` (Broad Sweep & Instant Answers)

* **Tool**: `search(query, mode="hybrid", snippets=3)`
* **Code Implementation**: [`crates/ctxvault-mcp/src/tools/mod.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-mcp/src/tools/mod.rs)
* **Token Budget**: 300 – 800 tokens total.
* **Payload**:
  * Partitioned `docs` and `code` hits.
  * Direct inline source snippets for the top $K$ hits (default 3), answering most questions in Turn 1 with zero follow-up calls.
  * Graph affordance counters (`calls_in`, `calls_out`, `implements`, `imports`, `wikilinks_in`).
  * Active schema envelope (valid node labels and edge types).

---

## Tier 2: `get_snippet` & `graph_match` (Targeted Inspection)

* **Tool**: `get_snippet(symbol="...")` or `graph_match(pattern="...")`
* **Code Implementation**: [`crates/ctxvault-mcp/src/tools/mod.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-mcp/src/tools/mod.rs), backed by [`crates/ctxvault-core/src/catalog/sqlite.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-core/src/catalog/sqlite.rs)
* **Token Budget**: 150 – 500 tokens per call.
* **Payload**:
  * Exact bounded AST function or struct definition with scope breadcrumbs (e.g. `impl Engine > fn search`).
  * Multi-hop relationship paths connecting symbols or documents.
* **Rule**: Agents only call Tier 2 when Turn 1 snippets signal that deeper structural understanding is needed.

---

## Tier 3: `read_file` (Exhaustive Reading)

* **Tool**: `read_file(path="...", start_line=1, end_line=100)`
* **Code Implementation**: [`crates/ctxvault-mcp/src/tools/mod.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-mcp/src/tools/mod.rs)
* **Token Budget**: Bounded strictly by `start_line` and `end_line`.
* **Rule**: Full file reads without line boundaries are treated as an emergency fallback. Agents are instructed to read targeted slices.
