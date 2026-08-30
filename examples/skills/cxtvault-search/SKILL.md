---
name: cxtvault-search
description: >-
  Retrieve knowledge, execute multi-hop graph queries, search semantic chunks,
  and trace entity paths using cxtvault MCP tools. Use this skill when the user
  asks questions about codebase architecture, documentation, past decisions, or multi-hop entity connections.
---

# Cxtvault Hybrid Search & Graph Navigation

This skill teaches agents how to effectively query knowledge bases using `cxtvault`'s hybrid 3-Way Reciprocal Rank Fusion (RRF), BM25 lexical search, dense ONNX vector search, and Petgraph typed graph traversal.

---

## 1. Fast Hybrid Retrieval Flow

When answering questions or researching topics:

1. **Perform Hybrid RRF Search**:
   Call `search_hybrid` with a descriptive query string:
   ```json
   {
     "query": "How does vector index compaction work?",
     "limit": 5
   }
   ```
2. **Review Snippets & Reciprocal Rank Scores**:
   Examine returned chunk snippets. The RRF algorithm balances lexical precision and dense semantic similarity.
3. **Targeted Reading**:
   If a snippet is sufficient to answer the prompt, cite the note path and proceed.
   If the snippet needs broader context, call `read_note` with the specific note path.

---

## 2. Multi-Hop Graph Exploration

When the query requires understanding connections between distinct components, decisions, or concepts:

1. **Locate Seed Nodes**:
   Search for the primary entity using `search_hybrid` or `search_bm25`.
2. **Expand Traversal via Links**:
   - Call `forwardlinks` on the seed note to see outbound connections (`Wikilink`, `ParentChild`, `Implements`, etc.).
   - Call `backlinks` to see inbound references.
3. **Trace Connection Paths**:
   To discover the shortest path connecting Note A to Note B, call `graph_path`:
   ```json
   {
     "from": "concepts/hybrid-retrieval.md",
     "to": "decisions/adr-001-graph-engine.md"
   }
   ```
4. **Extract Local Neighborhood**:
   To inspect all neighbors within $N$ hops around a note, call `graph_subgraph`:
   ```json
   {
     "path": "concepts/hybrid-retrieval.md",
     "depth": 2
   }
   ```

---

## 3. Lexical Keyword & Symbol Precision

When looking for exact error codes, struct names, CLI flags, or function signatures:

1. Call `search_bm25`:
   ```json
   {
     "query": "ReciprocalRankFusion min_chunk_tokens",
     "limit": 10
   }
   ```
2. To diagnose how BM25, vector, and graph components contributed to scores, call `search_explain`:
   ```json
   {
     "query": "ReciprocalRankFusion",
     "limit": 5
   }
   ```

---

## 4. Verification & Best Practices

- **Never guess note paths**: Always retrieve candidates via search tools first.
- **Avoid Context Flooding**: Retrieve top 5-10 chunks rather than reading entire vaults into LLM context.
- **Always Cite Sources**: State note paths and section headers in answers (e.g., `[concepts/hybrid-retrieval.md#3-way-rrf]`).
