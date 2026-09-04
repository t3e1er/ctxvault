---
title: "Data Science & Retrieval Mathematics Gateway"
category: "data-science"
status: "active"
tags: ["data-science", "mathematics", "retrieval", "rrf", "vectors", "modularity", "vram"]
related:
  - "[[docs/index]]"
  - "[[docs/data-science/hybrid-retrieval-theory]]"
  - "[[docs/data-science/rrf-mathematics]]"
  - "[[docs/data-science/code-embeddings-landscape]]"
  - "[[docs/data-science/community-detection-modularity]]"
  - "[[docs/data-science/transformer-memory-physics]]"
  - "[[docs/data-science/decisions/adr-001-rrf-vs-learned-fusion]]"
  - "[[docs/data-science/decisions/adr-002-jina-code-768d-selection]]"
  - "[[docs/data-science/decisions/adr-003-leiden-louvain-graph-clustering]]"
---

# Data Science & Retrieval Mathematics Hub

Welcome to the **Data Science & Retrieval Mathematics** module of `ctxvault`. This cluster establishes the theoretical, statistical, and empirical foundations of cross-modal codebase intelligence, high-dimensional vector spaces, and graph modularity.

---

## 1. Mathematical Architecture Overview

```
                      ┌────────────────────────────────────────────────────────┐
                      │              Incoming Query Formulation                │
                      │               Q_text  |  Q_ident  |  Q_graph           │
                      └───────────────────────────┬────────────────────────────┘
                                                  │
                ┌─────────────────────────────────┼─────────────────────────────────┐
                ▼                                 ▼                                 ▼
    ┌───────────────────────┐         ┌───────────────────────┐         ┌───────────────────────┐
    │   Lexical Space       │         │  Dense Vector Space   │         │  Graph Topology       │
    │   Tantivy BM25        │         │  Jina Code v2 (768d)  │         │  Petgraph BFS / PPR   │
    │   (Exact Token Hits)  │         │  (Semantic Intent)    │         │  (Structural Edges)   │
    └───────────┬───────────┘         └───────────┬───────────┘         └───────────┬───────────┘
                │                                 │                                 │
                │ Rank: r_bm25(d)                 │ Rank: r_vec(d)                  │ Rank: r_graph(d)
                └─────────────────────────────────┼─────────────────────────────────┘
                                                  ▼
                      ┌────────────────────────────────────────────────────────┐
                      │         Reciprocal Rank Fusion (RRF, k=60)             │
                      │          R(d) = Σ [ w_i / (k + r_i(d)) ]               │
                      └───────────────────────────┬────────────────────────────┘
                                                  ▼
                                      Optimal Unified Ranking
```

---

## 2. Core Theoretical Articles

1. **[[docs/data-science/hybrid-retrieval-theory]]**
   * *Why Multi-Modal Retrieval?* An analysis of single-modality failure modes: BM25 term vocabulary mismatch, dense bi-encoder identifier hallucination, and topological graph isolation.
2. **[[docs/data-science/rrf-mathematics]]**
   * *Mathematical Mechanics of Reciprocal Rank Fusion*: Derivation of the $k=60$ smoothing constant, monotonic rank combination, and why ordinal ranks eliminate the need for cross-distribution score normalization.
3. **[[docs/data-science/code-embeddings-landscape]]**
   * *Vector Representation of Polyglot Code*: Comparative evaluation of 768-dimensional `jina-embeddings-v2-base-code` vs 384-dimensional sentence transformers. Asymmetric bi-encoder contrastive pre-training across code-docstring pairs.
4. **[[docs/data-science/community-detection-modularity]]**
   * *Network Modularity ($Q$) Optimization*: Graph partitioning using the Louvain and Leiden algorithms across heterogeneous software graphs (code AST nodes + markdown documentation). Hub node centrality and architectural boundary identification.
5. **[[docs/data-science/transformer-memory-physics]]**
   * *Attention Activation Scaling*: Mathematical proof of quadratic memory consumption $\mathcal{O}(S^2)$ during transformer forward passes. Sequence length sorting, bucketing heuristics, and dynamic activation budgeting.

---

## 3. Architectural Decision Records (ADRs)

* **[[docs/data-science/decisions/adr-001-rrf-vs-learned-fusion]]**: Selection of rank-based Reciprocal Rank Fusion over learned linear regression weights to guarantee zero hyperparameter calibration drift across polyglot corpora.
* **[[docs/data-science/decisions/adr-002-jina-code-768d-selection]]**: Architectural justification for selecting the 768-dimensional Jina Code v2 model over lightweight 384-dimensional text embeddings.
* **[[docs/data-science/decisions/adr-003-leiden-louvain-graph-clustering]]**: Choosing the Leiden connectivity-refined community detection algorithm over standard Louvain to prevent disconnected module clusters.
