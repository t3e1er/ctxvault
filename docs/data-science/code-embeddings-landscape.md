---
title: "Polyglot Code Embeddings Landscape & Representation Theory"
category: "data-science"
status: "active"
tags: ["embeddings", "onnx", "jina", "vector-space", "code-search", "cross-modal"]
related:
  - "[[docs/data-science/index]]"
  - "[[docs/data-science/transformer-memory-physics]]"
  - "[[docs/data-science/decisions/adr-002-jina-code-768d-selection]]"
---

# Polyglot Code Embeddings Landscape & Representation Theory

Embedding code differs fundamentally from embedding natural language prose. Code contains rigid syntax, deep scoping hierarchies, identifiers with mixed naming conventions (`kebab-case`, `camelCase`, `snake_case`), and cross-file type dependencies.

This document evaluates the embedding landscape for polyglot code retrieval and justifies `ctxvault`'s model selection.

---

## 1. Architectural Comparison of Embedding Models

```
┌─────────────────────────────────────┬──────────────┬────────────┬─────────────┬──────────────────────────────────────────┐
│ Model Name                          │ Dimensions   │ Context    │ Params      │ Primary Training Objective               │
├─────────────────────────────────────┼──────────────┼────────────┼─────────────┼──────────────────────────────────────────┤
│ all-MiniLM-L6-v2                    │ 384          │ 256 tokens │ 22.7M       │ Natural language sentence pairs (MSMARCO)│
│ bge-small-en-v1.5                   │ 384          │ 512 tokens │ 33.4M       │ General web documents & Wikipedia        │
│ nomic-embed-text-v1.5               │ 768          │ 8,192 tok  │ 137M        │ Long-form natural language text          │
│ jina-embeddings-v2-base-code (ctxv) │ 768          │ 8,192 tok  │ 137M        │ Polyglot CodeSearchNet + GitHub commits  │
└─────────────────────────────────────┴──────────────┴────────────┴─────────────┴──────────────────────────────────────────┘
```

---

## 2. Why General Text Models Fail on Code

When lightweight text models like `all-MiniLM-L6-v2` or `bge-small-en` parse source code, three failure modes manifest:

### 2.1 Context Window Truncation (256–512 Tokens)
An average enterprise class, struct declaration, or trait implementation exceeds 800 tokens. General text models truncate sequences at 256 or 512 tokens, discarding method bodies, inner return types, and error handlers. `jina-embeddings-v2-base-code` features an **8,192 token window**, allowing full AST blocks to be embedded without artificial severance.

### 2.2 Tokenizer Vocabulary Fragmentation
Standard text BPE tokenizers lack subword representations for programming language idioms. A syntax token like `pub(crate) unsafe fn` or `EarlyBinder<'tcx, T>` is fragmented into 8–12 meaningless character-level pieces, inflating sequence lengths and destroying semantic cohesion.

### 2.3 Absence of Asymmetric Cross-Modal Pre-training
Code retrieval is inherently **asymmetric**:
* **Input Query**: Natural language question (*"how do we handle concurrent writes to SQLite?"*).
* **Target Document**: Concrete programming language implementation (`fn execute_write_batch(&mut self, ...)`).

General sentence transformers are trained on symmetric sentence similarity ($A \approx B$). `jina-embeddings-v2-base-code` was pre-trained on millions of **code-docstring pairs** and GitHub pull requests, aligning natural language intent with AST code structures in a unified 768-dimensional manifold.

---

## 3. High-Dimensional Vector Quantization

While 768-dimensional float32 representations provide high semantic fidelity, they require:
$$\text{Storage} = 768 \times 4 \text{ bytes} = 3,072 \text{ bytes per vector}$$

For large enterprise vaults with 100,000+ chunks, this demands ~307 MB of RAM just for raw vectors, plus HNSW graph index overhead.

To optimize memory efficiency on developer workstations, `ctxvault` supports **dynamic INT8 weight quantization** via ONNX Runtime:
* Model weight footprint drops from **548 MB (FP32)** to **137 MB (INT8)**.
* Forward pass latency improves by 1.8x–2.4x on CPU SIMD backends (AVX2/AVX-512).
* Cosine retrieval accuracy degradation is bounded to **<0.7% relative drop in Mean Reciprocal Rank (MRR@10)**.

See [[docs/data-science/decisions/adr-002-jina-code-768d-selection]] for the formal architectural decision record.
