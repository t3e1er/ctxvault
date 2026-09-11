---
title: "Turn 1 Affordances & Schema Envelopes"
description: "How Turn 1 search responses ground agents with graph degree counts, node labels, and inline answers."
category: "progressive-disclosure"
status: "active"
tags: ["affordances", "graph-degrees", "schema-envelope", "turn-1", "search"]
related:
  - "[[docs/concepts/progressive-disclosure/index]]"
  - "[[docs/concepts/progressive-disclosure/three-tier-model]]"
  - "[[docs/concepts/search/graph-traversal]]"
---

# Turn 1 Affordances & Schema Envelopes

In single-turn search, standard vector databases return only chunk text and similarity floats. The agent has no idea if a returned function is a leaf utility, an entry point called by 50 modules, or an outdated duplicate.

`ctxvault` enriches every Turn 1 search result with **Graph Affordances** and a dynamic **Schema Envelope**.

---

## 1. Graph Affordance Counters

Every search hit includes deterministic graph degree counts computed from the AST knowledge graph:

```json
{
  "path": "crates/ctxvault-core/src/engine.rs",
  "symbol": "Engine::search",
  "kind": "function",
  "score": 0.88,
  "snippet": "pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> { ... }",
  "graph_affordances": {
    "calls_in": 7,
    "calls_out": 14,
    "implements": 1,
    "imports": 3,
    "wikilinks_in": 4
  }
}
```

### Why Affordances Matter
* **`calls_in: 7`**: Tells the agent that 7 other functions depend on this symbol. Refactoring requires caution.
* **`calls_out: 14`**: Signals that this function orchestrates multiple sub-components.
* **`wikilinks_in: 4`**: Proves that this concept is documented across 4 ADRs or knowledge notes.

The agent gains architectural situational awareness **without making extra tool calls**.

---

## 2. Dynamic Schema Envelope

Every search response also bundles the **Schema Envelope** for the indexed corpus:

```json
{
  "schema_envelope": {
    "node_labels": ["CodeSymbol", "DocNode", "Module", "Tag"],
    "edge_types": ["calls", "defines", "imports", "implements", "wikilink", "derived_from"]
  }
}
```

This prevents hallucinated Cypher-Lite queries in Turn 2. The agent knows exactly which node labels and edge types exist before constructing patterns for `graph_match`.
