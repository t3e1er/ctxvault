---
title: "Progressive Disclosure: The 3-Tier Token Retrieval Contract"
category: "agentic-strategy"
status: "active"
tags: ["progressive-disclosure", "tokens", "ergonomics", "handles", "snippets", "retrieval"]
related:
  - "[[docs/agentic-strategy/index]]"
  - "[[docs/agentic-strategy/swarm-topologies]]"
  - "[[docs/agentic-strategy/decisions/adr-004-progressive-disclosure-token-contract]]"
---

# Progressive Disclosure: The 3-Tier Token Retrieval Contract

AI agents suffer from **context window pollution** and **attention dilution** when presented with massive text dumps. If an agent receives 6,000 tokens of file contents to answer a simple query, its inference latency increases, hallucination rates spike, and instruction-following fidelity degrades.

`ctxvault` strictly enforces **3-Tier Progressive Disclosure**, delivering the minimal relevant context slice required for each cognitive task.

---

## 1. The Three Retrieval Tiers

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Tier 1: Lightweight Handles (<150 tokens)                                                       │
│ Tool: `search(mode="hybrid"|"bm25"|"semantic", detail="ids")`                                   │
│ Returns: File paths, symbol qualified names, 1-indexed line ranges, score handles.              │
│ Strips: Snippet bodies, full lineage graphs, and heavy score component maps.                    │
└─────────────────────────────────┬───────────────────────────────────────────────────────────────┘
                                  │ Agent selects target handle
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Tier 2: Bounded Snippet / AST Symbol (150–500 tokens)                                           │
│ Tool: `get_snippet(path="...", qualified_name="...")`                                           │
│ Returns: Exactly ONE atomic AST unit (function, struct, doc section) with scope breadcrumbs.    │
│ Optional: `include_neighbors=true` surfaces immediate caller/callee signatures.                 │
└─────────────────────────────────┬───────────────────────────────────────────────────────────────┘
                                  │ Only if exhaustive context is strictly required
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Tier 3: Exhaustive File Access (1,000–8,000+ tokens)                                            │
│ Tools: `read_note`, `read_code_file`, `read_multiple`                                           │
│ Returns: Complete raw file content from disk.                                                   │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Empirical Token Consumption Comparison

In empirical audits across the `rust` (5,767 files) and `kubernetes` (20,078 files) corpora, progressive disclosure produces dramatic token savings:

```
┌──────────────────────────────┬────────────────────────┬────────────────────────┬──────────────────────┐
│ Retrieval Mode               │ Legacy Flat Search     │ ctxvault Tier-1 Audit  │ Net Token Reduction  │
├──────────────────────────────┼────────────────────────┼────────────────────────┼──────────────────────┤
│ 5-Hit Search Query           │ ~5,931 tokens          │ 128 tokens             │ 💥 97.8% Reduction   │
│ Code Symbol Lookup           │ ~3,200 tokens (file)   │ 210 tokens (snippet)   │ 💥 93.4% Reduction   │
│ 10-Step Investigation Flow   │ ~48,000 tokens         │ ~2,100 tokens          │ 💥 95.6% Reduction   │
└──────────────────────────────┴────────────────────────┴────────────────────────┴──────────────────────┘
```

---

## 3. Tier-2 Ergonomics: Generic Normalization & Near-Miss Suggestions

When querying Tier-2 `get_snippet` for complex language symbols (e.g. Rust, C++, TypeScript):
1. **Generic-Normalized Scope Resolution**: An agent querying `EarlyBinder > instantiate` successfully matches `EarlyBinder<'tcx, T> > instantiate`.
2. **Context Enrichment**: Symbol responses include fully qualified scope breadcrumbs, parsed method signatures, and attached docstrings.
3. **Candidate Suggestions**: If an exact match fails, `get_snippet` performs a leaf-name fallback query in SQLite and returns candidate scope paths to assist disambiguation, rather than failing with an opaque 404.

See [[docs/agentic-strategy/decisions/adr-004-progressive-disclosure-token-contract]] for the engineering details.
