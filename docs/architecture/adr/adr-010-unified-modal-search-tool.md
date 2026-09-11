---
title: "ADR 010: Unified Modal Search Tool vs Fragmented Query APIs"
category: "mcp-modes"
status: "accepted"
tags: ["adr", "mcp", "search", "tools", "ergonomics", "decision"]
related:
  - "[[docs/mcp-modes/index]]"
  - "[[docs/mcp-modes/search-modes-modalities]]"
---

# ADR 010: Unified Modal Search Tool vs Fragmented Query APIs

## Status
Accepted / Implemented

## Context
Initial server designs exposed separate MCP tools for each search modality: `search_bm25`, `search_semantic`, `search_hybrid`, `search_graph`, and `search_explain`. This fragmented tool surface caused AI agent confusion (agents struggled to decide which tool to call) and inflated tool schema prompt tokens.

## Decision
We consolidated all search functionality into a single polymorphic **`search`** tool with an explicit `mode` parameter:
* `mode = "hybrid" | "bm25" | "semantic" | "graph" | "explain"` (defaulting to `"hybrid"`).
* Combined with orthogonal filters: `modality = "both" | "docs" | "code"` and `detail = "default" | "ids" | "full"`.

`search_related` remains a separate tool because its input parameters (seed document IDs for Personalized PageRank) differ fundamentally from text queries.

## Consequences

### Positive
- Greatly simplifies tool discovery for LLMs.
- Reduces JSON Schema prompt overhead in `tools/list`.
- Unified return types (`SearchResult`) ensure predictable client-side parsing.

### Trade-offs
- A single tool schema contains multiple optional parameters that only apply to specific modes (e.g. `edge_types` only applies when `mode="graph"`).
