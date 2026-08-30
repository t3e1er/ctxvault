---
title: "ADR 001: In-Memory Petgraph with Typed Edges for Knowledge Navigation"
template: decision_record
status: accepted
date: 2026-08-30
tags:
  - architecture
  - graph
  - petgraph
---

# ADR 001: In-Memory Petgraph with Typed Edges for Knowledge Navigation

## Context
AI agents querying knowledge bases require multi-hop entity exploration and link navigation (e.g. parent/child relationships, superseding decisions, and direct wikilinks). Querying relational databases recursively in SQL introduces high latency and query complexity, while graph databases introduce bulky external service dependencies.

## Decision
We implement the knowledge graph in pure Rust using `petgraph::graphmap::DiGraphMap` backed by SQLite persistence.
1. Edges are typed (`Wikilink`, `SharedTag`, `ParentChild`, `Supersedes`, `Implements`, `DerivedFrom`).
2. Graph traversals (BFS, shortest path, subgraphs) execute in-memory with sub-2ms latency.
3. Disk synchronization occurs incrementally via SQLite transactions on startup and file updates.

Related concept: [[concepts/hybrid-retrieval.md]].
Resolved issue: [[incidents/inc-001-index-lock.md]].

## Consequences
- **Positive**: Sub-millisecond graph hops and BFS traversals; zero external database daemon required.
- **Negative**: Graph size is bounded by available RAM (sufficient for 100,000+ notes within <50MB).
