---
title: "ADR 005: Deterministic Syntax & Wikilink Graph Construction over LLM Extraction"
category: "agentic-strategy"
status: "accepted"
tags: ["adr", "graph", "deterministic", "treesitter", "decision"]
related:
  - "[[docs/agentic-strategy/index]]"
  - "[[docs/agentic-strategy/knowledge-crystallization]]"
  - "[[docs/code-architecture/cast-chunking-engine]]"
---

# ADR 005: Deterministic Syntax & Wikilink Graph Construction over LLM Extraction

## Status
Accepted / Implemented

## Context
Many knowledge management and RAG systems use large language models (LLMs) to extract entity-relationship graphs from text (e.g. running an extraction prompt on every paragraph to extract triplets `(subject, predicate, object)`).

When applied to codebases and engineering vaults, LLM-based graph extraction suffers from:
1. **Extreme Latency & Cost**: Cold indexing a 10,000-file repository requires millions of LLM prompt calls, costing hundreds of dollars and taking hours.
2. **Stochastic Inconsistency**: The same file indexed twice produces non-identical entity sets and hallucinated relationships.
3. **Imprecise Syntax Parsing**: LLMs frequently misidentify variable assignments as function definitions or invent relationships absent from code.

## Decision
We established **Invariable Principle 2**: Graph topology must be derived **100% deterministically** from typed frontmatter fields, `#tags`, `[[wikilinks]]`, and Tree-sitter AST relationships (`defines`, `imports`, `calls`, `implements_trait`). Stochastic LLM entity extraction is strictly forbidden in the core indexing pipeline.

## Consequences

### Positive
- Sub-millisecond graph construction speed: 10,000 files indexed in $<15$ seconds on pure Rust.
- 100% reproducible and verifiable graph topologies.
- Zero API token costs during cold indexing.

### Trade-offs
- Relationships in unstructured prose that lack explicit wikilinks or tags are not captured in the graph (they are captured instead by the dense vector semantic modality).
