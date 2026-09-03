---
title: How the Search Pipeline Works
tags: [search, pipeline, rrf, vector, bm25, graph]
status: active
---

# How the Search Pipeline Works

The `search_hybrid` pipeline executes a synchronized multi-engine retrieval sequence, combining lexical, dense vector, and graph structural signals into a unified result set.

See also: [[docs/index]], [[what-is-ctxvault]], [[why-hybrid-retrieval]], [[how-cast-chunking-works]].

---

## Step-by-Step Retrieval Lifecycle

```
                           Incoming Search Query
                                    │
            ┌───────────────────────┼───────────────────────┐
            ▼                       ▼                       ▼
 ┌─────────────────────┐ ┌─────────────────────┐ ┌─────────────────────┐
 │ 1. Tantivy BM25     │ │ 2. Jina v2 Vector   │ │ 3. Petgraph Graph   │
 │ • Tokenized query   │ │ • 768d fastembed    │ │ • Multi-hop walk    │
 │ • Exact matches     │ │ • Cosine HNSW       │ │ • Seed expansion    │
 └──────────┬──────────┘ └──────────┬──────────┘ └──────────┬──────────┘
            │                       │                       │
            └───────────────────────┼───────────────────────┘
                                    │
                                    ▼
 ┌─────────────────────────────────────────────────────────────────────┐
 │ 4. Reciprocal Rank Fusion (RRF) Execution Engine                    │
 │ • Formula: R(d) = sum(w_i / (60 + r_i(d)))                          │
 │ • Chunk deduplication & max-pooling                                 │
 │ • Lineage graph enrichment (outgoing/incoming edges)                │
 └──────────────────────────────────┬──────────────────────────────────┘
                                    │
                                    ▼
                         Final Ranked Result List
```

---

## Retrieval Phases Explained

### Phase 1: Tantivy Lexical Search
* Performs BM25 scoring over title, body, and code chunk fields.
* Preserves symbol exactness and specific error messages.

### Phase 2: Dense Vector Embedding & HNSW Search
* Embeds the raw query string into a 768-dimensional float vector using `jinaai/jina-embeddings-v2-base-code`.
* Queries the in-memory HNSW vector index (`hnsw_rs`) using Cosine Distance.
* Applies document-level max-pooling if requested.

### Phase 3: Knowledge Graph Structural Expansion
* Identifies entity seeds matching the query.
* Traverses outgoing and incoming graph edges up to $N$ hops (configurable depth).
* Computes proximity scores based on edge weights and path lengths.

### Phase 4: 4-Way Reciprocal Rank Fusion (RRF) & Lineage Enrichment
* Normalizes candidates into rank positions.
* Injects graph lineage metadata (`defines`, `imports`, `calls`, `Wikilink`) directly into each search hit.
* Returns final structured JSON payload to the MCP client in $< 70\text{ ms}$.
