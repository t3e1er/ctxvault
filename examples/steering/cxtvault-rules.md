# Cxtvault Agent Steering Rules (Antigravity / Gemini IDE)

Copy this file into `.agents/rules/cxtvault-rules.md` to automatically guide your AI pair programmer in Antigravity or Gemini IDE.

---

## Directives

1. **Markdown Authority**: Files on disk are the ultimate ground truth. Indices are disposable retrieval caches.
2. **Modal Search Selection**:
   - `search_hybrid`: Default for general research and broad technical queries. Uses 3-way Reciprocal Rank Fusion.
   - `search_bm25`: For exact function names, struct fields, error messages, and verbatim tokens.
   - `search_semantic`: For exploring abstract concepts and semantic analogies.
   - `search_graph`: For relationship and dependency queries across typed edges (`Wikilink`, `ParentChild`, `Supersedes`, `Implements`, `SharedTag`).
3. **Chunk-Level Reading**: Use chunk snippets returned by search tools. Only invoke `read_note` when exhaustive document context is necessary.
4. **Schema Discipline**: Before creating notes, call `list_templates`. Follow required frontmatter fields and section headings. Verify with `validate_note`.
5. **Knowledge Crystallization (Principle 3)**: Transform valuable debugging outcomes and architectural resolutions into durable notes via `promote_concept`.
