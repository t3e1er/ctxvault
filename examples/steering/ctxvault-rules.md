# Ctxvault Agent Steering Rules (Antigravity & Gemini IDE)

Copy this file into `.agents/rules/ctxvault-rules.md` or as `GEMINI.md` in your repository root to guide your AI pair programmer. Synchronized with `.kiro/steering/`.

---

## Directives

1. **Markdown Authority**: Files on disk are the ultimate ground truth. Indices are disposable retrieval caches.
2. **Modal Search Selection** (`search` tool):
   - `mode="hybrid"`: Default for general research and broad technical queries (3-way RRF).
   - `mode="bm25"`: For exact function names, struct fields, error messages, and verbatim tokens.
   - `mode="semantic"`: For exploring abstract concepts and semantic analogies.
   - `mode="graph"`: For relationship and dependency queries across typed edges (`Wikilink`, `ParentChild`, `Supersedes`, `Implements`, `SharedTag`).
   - `mode="explain"`: Introspect scoring breakdowns.
3. **Chunk-Level Reading**: Use chunk snippets returned by `search`. Use `get_snippet` for bounded single symbols/chunks. Only invoke `read_note` / `read_code_file` when exhaustive document context is necessary.
4. **Schema Discipline**: Before creating notes, call `list_templates`. Follow required frontmatter fields and section headings. Verify with `validate_note`.
5. **Knowledge Crystallization (Principle 3)**: Transform valuable debugging outcomes and architectural resolutions into durable notes via `promote_concept` and trace lineage with `traverse_lineage`.
