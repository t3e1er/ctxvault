---
name: cxtvault-crystallize
description: >-
  Distill raw episodic traces, discussion logs, and debugging sessions into permanent semantic knowledge notes.
  Use this skill to promote concepts, track knowledge lineage, analyze concept density, and identify semantic coverage gaps.
---

# Cxtvault Knowledge Crystallization

This skill teaches agents how to implement **Continuous Knowledge Crystallization**: transforming ephemeral, high-entropy conversational and debugging traces into structured, durable semantic assets with formal provenance.

---

## 1. The Crystallization Workflow

When an engineering task, debugging session, or design consensus produces non-obvious knowledge:

1. **Identify Volatile Episodic Context**:
   Review the conversation trajectory or scratch log to isolate:
   - Root causes of subtle bugs
   - Architecture decisions and trade-offs
   - Recurring implementation patterns or invariants
2. **Promote to Semantic Concept**:
   Call `promote_concept` to generate a structured note with formal lineage:
   ```json
   {
     "target_path": "concepts/sqlite-concurrency-patterns.md",
     "source_notes": [
       "incidents/inc-001-index-lock.md"
     ],
     "template": "system_concept",
     "frontmatter": {
       "title": "SQLite WAL Concurrency and Shared Cache",
       "concept_type": "architecture",
       "derived_from": "incidents/inc-001-index-lock.md",
       "tags": ["sqlite", "concurrency", "storage"]
     },
     "content": "# SQLite WAL Concurrency\n\n## Overview\nMechanisms for multi-reader single-writer SQLite WAL concurrency.\n\n## Mechanisms\n...\n\n## Trade-Offs\n...",
     "archive_sources": false
   }
   ```
3. **Trace Knowledge Lineage**:
   To inspect the origin of a concept and view upstream decisions or source notes, call `traverse_lineage`:
   ```json
   {
     "start_path": "concepts/sqlite-concurrency-patterns.md",
     "edge_type": "DerivedFrom",
     "direction": "outgoing",
     "max_depth": 3
   }
   ```

---

## 2. Density & Semantic Gap Auditing

To maintain high knowledge quality and prevent vault bloat:

1. **Calculate Knowledge Density & Graph Hubs**:
   Call `analyze_density` to inspect top hub nodes, isolated orphans, and graph density metrics:
   ```json
   {
     "top_hubs": 10
   }
   ```
2. **Detect Retrieval Blind Spots & Semantic Gaps**:
   Call `find_semantic_gaps` with sample test queries to evaluate where BM25 and dense vector search diverge:
   ```json
   {
     "queries": [
       "how to configure fastembed vectors",
       "sqlite lock contention"
     ],
     "top_k": 5
   }
   ```
3. **Refactor Oversized Notes**:
   If a document covers too many distinct topics, call `suggest_splits` to receive automated modularization recommendations.
