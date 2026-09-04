---
title: "Multi-Corpus Serving: Routing, Fan-Out & Cross-Corpus Symbol Linking"
category: "mcp-modes"
status: "active"
tags: ["multi-corpus", "corpus-manager", "routing", "fan-out", "symbol-linking"]
related:
  - "[[docs/mcp-modes/index]]"
  - "[[docs/mcp-modes/decisions/adr-012-in-process-multi-corpus-manager]]"
---

# Multi-Corpus Serving: Routing, Fan-Out & Cross-Corpus Symbol Linking

In large software enterprises, developers do not work within a single isolated folder. Architecture decision records live in a company-wide documentation vault, core microservices reside in sibling repositories, and common utilities live in shared libraries.

`ctxvault` serves $N$ independent index roots concurrently from a single persistent process via **`CorpusManager`**.

---

## 1. Multi-Corpus Routing Architecture

```
                          ┌──────────────────────────────────────────────┐
                          │             AI Agent Query Request           │
                          │   corpus: "engine" | corpora: ["docs", "rs"] │
                          └──────────────────────┬───────────────────────┘
                                                 │
                                                 ▼
                          ┌──────────────────────────────────────────────┐
                          │                CorpusManager                 │
                          └──────────────┬────────────────┬──────────────┘
                                         │                │
                    ┌────────────────────┴───┐        ┌───┴────────────────────┐
                    ▼                        ▼        ▼                        ▼
             [Engine: docs]           [Engine: rust]  [Engine: kube]           [Engine: ui]
               Index Root 1             Index Root 2   Index Root 3             Index Root 4
                    │                        │        │                        │
                    └────────────────────┬───┘        └───┬────────────────────┘
                                         ▼                ▼
                          ┌──────────────────────────────────────────────┐
                          │    Cross-Corpus RRF Fusion & Hit Tagging     │
                          │        Hit 1: [corpus="rust"] EarlyBinder    │
                          │        Hit 2: [corpus="docs"] ADR-016        │
                          └──────────────────────────────────────────────┘
```

---

## 2. Query Fan-Out Semantics

When invoking `search`:
* **Targeting a Single Corpus**: `corpus = "rust"` directs the query strictly to that index root.
* **Targeting Multiple Corpora**: `corpora = ["rust", "docs"]` fans out queries in parallel across worker threads.
* **Targeting All Corpora**: `corpora = "all"` queries all $N$ managed corpora.

Results are merged using `rrf_fuse_cross_corpus`, tagging each result with its source corpus name so callers know exactly where the hit originated.

---

## 3. Unambiguous Cross-Corpus Symbol Linking

When an architecture document in `docs` declares:
```markdown
implements: "crate::search::Engine"
```
`CorpusManager` resolves this symbol across sibling code corpora via `link_cross_corpus_symbols`:
* **Unambiguous Resolution**: If `crate::search::Engine` exists in exactly one registered code corpus, a virtual cross-corpus edge is synthesized with a high confidence band.
* **Ambiguity Safety**: If multiple corpora declare identical symbols, no speculative false edge is generated, preventing hallucinated dependencies.

See [[docs/mcp-modes/decisions/adr-012-in-process-multi-corpus-manager]] for architectural details.
