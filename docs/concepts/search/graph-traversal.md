---
title: "Graph Traversal & Cypher-Lite CTEs"
description: "High-speed graph queries, Petgraph topology, recursive SQLite Common Table Expressions, and community detection."
category: "search"
status: "active"
tags: ["graph", "cypher-lite", "petgraph", "sqlite-cte", "leiden", "louvain", "communities"]
related:
  - "[[docs/concepts/search/index]]"
  - "[[docs/architecture/trust/deterministic-graph]]"
  - "[[docs/concepts/progressive-disclosure/turn-1-affordances]]"
  - "[[docs/architecture/adr/adr-003-leiden-louvain-graph-clustering]]"
---

# Graph Traversal & Cypher-Lite CTEs

While BM25 and vector search operate on isolated text chunks, software systems are fundamentally **graphs of dependencies and concepts**.

`ctxvault` integrates two complementary graph representations:
1. **In-Memory Petgraph**: For sub-millisecond Personalized PageRank (`search_related`) and shortest-path reachability.
2. **Recursive SQLite CTE Engine**: Compiling linear **Cypher-Lite** patterns into recursive SQL queries with cycle guards for multi-hop path extraction (`graph_match`).

* **Petgraph KnowledgeGraph**: [`crates/ctxvault-core/src/graph/mod.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-core/src/graph/mod.rs)
* **SQLite Graph Traversal Backend**: [`crates/ctxvault-core/src/catalog/sqlite.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-core/src/catalog/sqlite.rs)
* **GraphStore Port**: [`crates/ctxvault-common/src/ports.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-common/src/ports.rs)

---

## 1. Cypher-Lite Query Language (`graph_match`)

Agents execute multi-hop queries using clean ASCII pattern syntax:

```text
(:CodeSymbol {name: "Engine"})-[:calls*1..2]->(target)
(:DocNode {path: "adrs/001-architecture.md"})-[:derived_from*1..3]->(target)
(source)-[:implements]->(:CodeSymbol {name: "TextIndex"})
```

### The 5 Typed Edge Classes
Traversals can filter across dedicated graph layers:
* `code`: AST relationships (`defines`, `imports`, `calls`, `implements`).
* `semantic`: Markdown links (`wikilink`, `derived_from`, `shared_tag`).
* `structural`: Document hierarchy (`parent_child`, `section`).
* `crossmodal`: Links between code and documentation (`documents`, `implements_spec`).
* `hybrid`: Blended multi-layer traversals.

### Sub-Millisecond SQLite Execution
Cypher-Lite queries compile down to recursive SQL with cycle guards:
```sql
WITH RECURSIVE traversal(src, dst, depth, path) AS (
    SELECT source, target, 1, source || '->' || target
    FROM code_edges WHERE source = ?
    UNION ALL
    SELECT e.source, e.target, t.depth + 1, t.path || '->' || e.target
    FROM code_edges e
    JOIN traversal t ON e.source = t.dst
    WHERE t.depth < 2 AND instr(t.path, e.target) = 0
)
SELECT * FROM traversal;
```
Execution finishes in **under 1.8ms**.

---

## 2. Architectural Community Detection (`graph_communities`)

To understand the macro-architecture of an unfamiliar codebase without reading every file, agents run `graph_communities`:
* **Algorithms**: Leiden (connectivity-refined) and Louvain (modularity maximization).
* **View Modes**:
  * `view="architecture"`: Groups files into functional subsystems (e.g. "Storage Engine", "Transport Layer", "Indexing Pipeline").
  * `view="raw"`: Detailed symbol-level graph clusters.
