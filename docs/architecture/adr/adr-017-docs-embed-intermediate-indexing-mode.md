---
title: "ADR 017: Intermediate Docs-Only Embedding Mode (DocsEmbed)"
category: "code-architecture"
status: "accepted"
tags: ["adr", "docs-embed", "indexing", "performance", "decision"]
related:
  - "[[docs/code-architecture/index]]"
  - "[[docs/code-architecture/decisions/adr-008-anchor-embedding-paradigm]]"
  - "[[docs/roadmap/RFC-docs-embed-intermediate-indexing-mode]]"
---

# ADR 017: Intermediate Docs-Only Embedding Mode (`DocsEmbed`)

## Status
Accepted / Implemented

## Context
In large polyglot software repositories, source code constitutes 95%+ of the file and chunk volume, while documentation (.md files) constitutes less than 5%. 
- `IndexMode::Full` computes dense ONNX embeddings for both documentation and code anchor nodes (classes, structs, public APIs). In massive codebases, cold-start embedding of thousands of code anchors consumes substantial GPU/CPU time.
- `IndexMode::Fast` skips dense embedding entirely, which eliminates cold-start time but deprives AI agents of semantic vector search over conceptual documentation, architecture decision records, and guides.

Because source code has high lexical token density and explicit AST relationships (`calls`, `defines`, `imports`, `implements_trait`), it is retrieved with high fidelity by Tantivy BM25 and Petgraph graph walks. In contrast, documentation relies on natural language prose, synonyms, and conceptual descriptions where dense vector search provides the greatest marginal utility.

## Decision
We introduce an intermediate indexing mode: **`IndexMode::DocsEmbed`** (serialized as `"docs-embed"`):
1. **Markdown Documentation**: Anchor chunks (H1 headers, H2 section overviews, ADR decisions) are vectorized via ONNX runtime (`jina-embeddings-v2-base-code`, 768-dim) into the HNSW vector store (`vectors.json`).
2. **Polyglot Source Code**: Chunk embedding policies are coerced to `ChunkEmbedPolicy::GraphOnly`. Zero code chunks undergo neural tensor forward passes or vector storage.
3. **100% Lexical & Graph Parity**: 100% of all code and documentation files, chunks, symbols, and AST relationships are indexed unconditionally into Tantivy BM25, SQLite (`meta.db`), and Petgraph (`graph.bin`).

## Consequences

### Positive
- **Dramatic Acceleration**: Slashes cold indexing tensor forward passes by 90%–99% on large codebases, reducing embedding overhead from minutes/hours to seconds.
- **High-Signal Doc Retrieval**: AI agents retain full semantic vector search over architectural documentation and ADRs.
- **Zero Loss of Code Discoverability**: All code identifiers, private helpers, and structural AST relationships remain fully retrievable via BM25 exact matching and Petgraph graph expansion.
- **Minimal VRAM Footprint**: Reduces activation memory and batch staging pressure on GPU hardware.

### Trade-offs
- Code cannot be retrieved via pure semantic similarity if the agent does not know any matching identifiers or AST call paths. Agents rely on finding architectural documentation anchors and traversing cross-modal wikilinks into code.
