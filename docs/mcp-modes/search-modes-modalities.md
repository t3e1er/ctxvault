---
title: "Search Modes & Modality Filtering Parameterization"
category: "mcp-modes"
status: "active"
tags: ["search", "modes", "modalities", "bm25", "hybrid", "explain", "filtering"]
related:
  - "[[docs/mcp-modes/index]]"
  - "[[docs/data-science/hybrid-retrieval-theory]]"
  - "[[docs/mcp-modes/decisions/adr-010-unified-modal-search-tool]]"
---

# Search Modes & Modality Filtering Parameterization

Rather than fragmenting search into multiple competing tools (`search_bm25`, `search_hybrid`, `search_graph`), `ctxvault` unifies all search functionality under a single, ergonomic `search` tool.

---

## 1. The Five Search Modes (`mode`)

```
┌─────────────────┬─────────────────────────────────────────────────────────────────────────────┐
│ Mode Value      │ Search Strategy & Execution Pipeline                                        │
├─────────────────┼─────────────────────────────────────────────────────────────────────────────┤
│ `hybrid`        │ Default mode. Fuses Tantivy BM25, Jina Code v2 dense vectors, and Petgraph  │
│                 │ graph hops via 3-way Reciprocal Rank Fusion (RRF).                          │
├─────────────────┼─────────────────────────────────────────────────────────────────────────────┤
│ `bm25`          │ Pure lexical retrieval. Ideal for exact compiler error codes, identifiers,  │
│                 │ struct/class names, and verbatim string literals.                           │
├─────────────────┼─────────────────────────────────────────────────────────────────────────────┤
│ `semantic`      │ Dense vector similarity search using 768-dimensional ONNX embeddings.      │
│                 │ Best for abstract architectural intent and natural language questions.      │
├─────────────────┼─────────────────────────────────────────────────────────────────────────────┤
│ `graph`         │ Typed property graph traversal. Walks structural and semantic edges;        │
│                 │ filterable by `edge_types` or `edge_class` (`structural`, `semantic`).      │
├─────────────────┼─────────────────────────────────────────────────────────────────────────────┤
│ `explain`       │ Introspection mode. Preserves raw score breakdowns (`bm25_score`,            │
│                 │ `vector_cosine`, `graph_distance`) alongside final RRF scores.             │
└─────────────────┴─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Bi-Modal Filtering (`modality`)

Real-world repositories contain both prose documentation (`.md`) and source code (`.rs`, `.ts`, `.py`, `.go`). `search` supports bi-modal filtering:
* `modality="both"` (default): Searches documentation vaults and AST code chunks simultaneously.
* `modality="docs"`: Restricts candidates strictly to markdown files, ADRs, and notes.
* `modality="code"`: Restricts candidates strictly to source code files and AST symbols.

Filtering is applied **deep within the retrieval kernels** (in Tantivy indexed fields, SQLite queries, and Petgraph node bitsets), preventing candidate set pollution before RRF ranking occurs.

---

## 3. Related Search (`search_related`)

For "find more like these" exploration, `search_related` uses **Personalized PageRank (PPR)** over Petgraph, seeding random walks from an initial set of target notes or symbols to surface structurally and semantically related items.

See [[docs/mcp-modes/decisions/adr-010-unified-modal-search-tool]] for the design rationale.
