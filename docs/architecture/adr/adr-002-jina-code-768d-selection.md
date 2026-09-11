---
title: "ADR 002: Selection of Jina Code v2 768d vs Lightweight 384d Embeddings"
category: "data-science"
status: "accepted"
tags: ["adr", "embeddings", "jina", "vector-space", "decision"]
related:
  - "[[docs/data-science/index]]"
  - "[[docs/data-science/code-embeddings-landscape]]"
  - "[[docs/data-science/transformer-memory-physics]]"
---

# ADR 002: Selection of Jina Code v2 768d vs Lightweight 384d Embeddings

## Status
Accepted / Implemented

## Context
Early prototypes used lightweight sentence transformers (`all-MiniLM-L6-v2` or `bge-small-en-v1.5`) producing 384-dimensional vectors. While these models have minimal RAM footprints (~80 MB) and fast CPU inference, they suffer critical performance collapses when indexing polyglot codebases:
1. **Severe Truncation**: Truncates code chunks at 256–512 tokens, cutting off method bodies, struct fields, and error logic.
2. **Natural Language Asymmetry**: Inability to map natural language architectural queries to concrete programming language constructs across 16+ languages.

## Decision
We selected **`jinaai/jina-embeddings-v2-base-code`** (137M parameters, 768 dimensions, 8,192 token window) via ONNX Runtime (`ort` v2).

### Rationale:
1. **8,192 Token Window**: Enables embedding complete classes, interfaces, and complex AST nodes without artificial slicing.
2. **CodeSearchNet Asymmetric Contrastive Pre-training**: Trained specifically on multi-language code-docstring pairs and commit diffs.
3. **Cross-Modal Code Understanding**: Successfully bridges natural-language conceptual questions to syntax declarations.
4. **Quantization Path**: Supports dynamic INT8 quantization, reducing weight footprint to 137 MB while retaining 99.3% of retrieval MRR.

## Consequences

### Positive
- Substantially higher retrieval accuracy on cross-modal code searches.
- No truncation of structural AST units.
- Universal support across Windows DirectML, Apple CoreML, and Linux CPU SIMD.

### Trade-offs
- Forward pass computation is ~3.5x heavier than 384d MiniLM.
- Requires dynamic sequence-length bucketing to prevent GPU VRAM exhaustion (addressed in ADR 015).
