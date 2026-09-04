---
title: "Knowledge Crystallization: Karpathy LLM Wiki Architecture"
category: "agentic-strategy"
status: "active"
tags: ["crystallization", "karpathy", "wiki", "memory", "lineage", "provenance"]
related:
  - "[[docs/agentic-strategy/index]]"
  - "[[docs/agentic-strategy/progressive-disclosure]]"
  - "[[docs/agentic-strategy/decisions/adr-005-deterministic-vs-llm-graph-extraction]]"
---

# Knowledge Crystallization: Karpathy LLM Wiki Architecture

AI agent development sessions produce massive volumes of **ephemeral exhaust**: compiler error traces, shell outputs, intermediate code attempts, and interactive design deliberations. In traditional workflows, this exhaust evaporates when the session context window resets.

`ctxvault` implements **Continuous Knowledge Crystallization (Principle 3)**, integrating Andrej Karpathy's "LLM Wiki" compilation architecture into Model Context Protocol tooling.

---

## 1. The Compilation vs Retrieval Paradigm

In standard Retrieval-Augmented Generation (RAG), an agent searches raw, uncurated documents on every turn, essentially "rediscovering" context from noisy fragments.

Karpathy's LLM Wiki framework treats the AI agent as a **compiler** that transforms unstructured episodic exhaust into a structured, compounding knowledge base:

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                        The Karpathy Compilation Cycle in ctxvault                      │
├────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                        │
│   1. Ingest Raw Exhaust   ──► 2. Compile into Note    ──► 3. Query Wiki   ──► 4. Audit │
│      (Debug traces, logs,        (`promote_concept`          (`search`        (`validate`│
│       design debates)             with YAML schema)           Tier 1/2/3)      `density`)│
│                                                                                        │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

| Dimension | Standard Stateless RAG | Karpathy LLM Wiki (ctxvault) |
|---|---|---|
| **Knowledge State** | Ephemeral (rediscovered every prompt) | Persistent & Compounding (stored in Markdown) |
| **Data Format** | Fragmented, arbitrary text chunks | Schema-validated, interlinked Markdown notes |
| **Memory Retention** | Lost when context window closes | Permanent across agent lifecycles & team members |
| **Provenance** | Anonymous similarity matches | Explicit `DerivedFrom` lineage edges in SQLite/Petgraph |

---

## 2. The Mechanics of `promote_concept`

When an engineering task resolves a complex bug or establishes an architectural consensus:
1. **Source Isolation**: The agent identifies the source notes or episodic logs (e.g. `incidents/inc-042-directml-tdr.md`).
2. **Atomic Schema Synthesis**: The agent invokes `promote_concept`:
   ```json
   {
     "target_path": "concepts/directml-tdr-budgeting.md",
     "source_notes": ["incidents/inc-042-directml-tdr.md"],
     "template": "system_concept",
     "frontmatter": {
       "title": "DirectML 400ms TDR Watchdog Budgeting",
       "concept_type": "architecture",
       "derived_from": "incidents/inc-042-directml-tdr.md",
       "tags": ["directml", "tdr", "hardware", "vram"]
     },
     "content": "# DirectML 400ms TDR Watchdog Budgeting\n\n## Overview\n..."
   }
   ```
3. **Atomic Transaction & Graph Wiring**: `ctxvault` writes the markdown note, parses AST/frontmatter, extracts `DerivedFrom` lineage edges, and commits them to SQLite `meta.db` and Petgraph `graph.bin` atomically (rolling back on failure).

---

## 3. Bidirectional Lineage Traversal (`traverse_lineage`)

Crystallized knowledge maintains formal provenance. Using `traverse_lineage`, an agent can inspect why a decision was made or trace forward to observe which downstream modules depend on an ADR:

```
[Incident Log / Debug Exhaust]
              │
              │ (DerivedFrom)
              ▼
    [ADR Decision Record]
              │
              │ (Implements)
              ▼
[Concrete Code Symbol: Engine]
```

This guarantees that decisions remain auditable and prevents regressions during long-term maintenance.
