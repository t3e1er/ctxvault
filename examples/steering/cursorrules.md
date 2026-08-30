# Cursor AI Steering Rules (.cursorrules)

Place the following content into `.cursorrules` in your project root to instruct Cursor AI when using `cxtvault` MCP server tools.

```markdown
# Cxtvault Knowledge Base Protocol

You have access to the `cxtvault` MCP server. Follow these rules when querying or modifying knowledge in this repository:

1. Retrieval Strategy:
   - For codebase and documentation queries, prioritize `search_hybrid` (combines BM25 lexical + ONNX embeddings via 3-way RRF).
   - Use `search_bm25` when searching for exact identifier names, error strings, or struct symbols.
   - Use `search_graph`, `forwardlinks`, and `backlinks` to navigate relationships, dependencies, and wikilinks.
   - Use `graph_path` to find the shortest connection between two architecture entities.

2. Reading Efficiency:
   - Prefer working from the snippets returned by search tools.
   - Do NOT call `read_note` on entire documents unless synthesizing a complete document summary.

3. Note Creation & Schemas:
   - Before creating a new documentation note or ADR, call `list_templates` to discover corpus schemas.
   - Adhere to the required frontmatter properties and headings.
   - Always run `validate_note` immediately after creating or modifying a note to ensure zero schema errors.

4. Principle 3 Crystallization:
   - When resolving complex architectural or implementation questions, offer to crystallize the findings into a permanent note using `promote_concept`.
```
