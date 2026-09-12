---
title: "Project Roadmap & RFC Archive"
description: "High-level technical roadmap, feature RFCs, and evolutionary specifications for ctxvault."
category: "roadmap"
status: "active"
tags: ["roadmap", "rfc", "planning", "evolution", "architecture"]
related:
  - "[[docs/index]]"
  - "[[docs/architecture/adr/index]]"
  - "[[docs/architecture/implementation/index]]"
---

# Project Roadmap & RFC Archive

This section tracks the technical roadmap and Request for Comments (RFC) engineering specifications for `ctxvault`.

---

## Technical Roadmap

* **[[docs/roadmap/coderoadmap]]**: The comprehensive engineering roadmap, covering completed phases and upcoming milestones for graph scaling, distributed swarms, and multi-modal expansion.

---

## Architectural RFCs

| RFC | Title | Status | Primary Focus |
|---|---|---|---|
| **[[docs/roadmap/RFC-cross-corpus-graph-federation]]** | Cross-Corpus Graph Federation | Implemented | Multi-repo routing, external symbol resolution, federated BFS traversal. |
| **[[docs/roadmap/RFC-adaptive-graph-expansion]]** | Adaptive Graph Expansion & SQL Backend | Implemented | Recursive SQLite CTEs, bounded depths, and cycle protection for sub-2ms queries. |
| **[[docs/roadmap/RFC-zero-copy-file-offsets-and-binary-vectors]]** | Zero-Copy File Offsets & Packed Vectors | Implemented | Eliminating memory duplication with aligned binary vectors and disk byte offsets. |
| **[[docs/roadmap/RFC-treesitter-expansion-and-lsp-analysis]]** | Tree-sitter Polyglot AST & Language Expansion | Implemented | cAST chunking across 16+ languages with parent scope breadcrumb injection. |
| **[[docs/roadmap/RFC-docs-embed-intermediate-indexing-mode]]** | Intermediate Docs-Embed Indexing Mode | Implemented | Fast indexing mode prioritizing markdown doc embeddings over raw code vectors. |
| **[[docs/roadmap/RFC-markdown-templates-and-frontmatter-edge-schema]]** | Markdown Templates & Frontmatter Edge Schema | Proposed | Native .templates/*.md standard with frontmatter schema, edge synthesis, and scaffolding. |
| **[[docs/roadmap/RFC-lean-multiline-text-emission]]** | Lean Multiline Text Emission Protocol | Proposed | Eliminating JSON context overhead via indented Cypher ASCII trees and markdown blocks (~69% token savings). |
