---
title: Hybrid Retrieval and 3-Way Reciprocal Rank Fusion
template: system_concept
concept_type: algorithm
derived_from: decisions/adr-001-graph-engine.md
tags:
  - search
  - rrf
  - architecture
---

# Hybrid Retrieval and 3-Way Reciprocal Rank Fusion

## Overview
Hybrid retrieval combines the lexical precision of BM25 inverted indexes, the conceptual breadth of dense ONNX vector embeddings, and the topological connectivity of typed graph traversals. By merging these distinct signals, agents achieve high recall without suffering from vector hallucinations or lexical vocabulary mismatch.

See also [[decisions/adr-001-graph-engine.md]] for the underlying storage decisions.

## Mechanisms
The ranking fusion uses **Reciprocal Rank Fusion (RRF)**:
$$RRF(d) = \sum_{m \in M} \frac{w_m}{k + r_m(d)}$$
where $k=60$ dampens top-rank outliers, $w_m$ weights each modality, and $r_m(d)$ represents the 1-based ordinal rank of document $d$ within modality $m$.

## Trade-Offs
- **Pros**: Calibrated rank scores without brittle min-max normalization heuristics; robust against vector density drift.
- **Cons**: Requires executing parallel search passes across Tantivy, HNSW, and Petgraph graph structures before merging.
