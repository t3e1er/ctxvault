# Ctxvault — Gemini Steering Rules

This file can be placed at the root of any repository as `GEMINI.md` to guide Gemini Code Assist and Google Antigravity agents when working with `ctxvault`.

---

## Directives

1. **Markdown Authority**: Files on disk are the ultimate ground truth. Indices are disposable retrieval caches.
2. **Modal Search Selection** (`search` tool):
   - `mode="hybrid"`: Default for general research and broad technical queries (3-way RRF).
   - `mode="bm25"`: For exact function names, struct fields, error messages, and verbatim tokens.
   - `mode="semantic"`: For exploring abstract concepts and semantic analogies.
   - `mode="graph"`: For relationship and dependency queries across typed edges.
   - `mode="explain"`: Introspect scoring breakdowns.
3. **Progressive Disclosure**:
   - Tier 1: Query `search` (returns handles and snippets).
   - Tier 2: Fetch targeted symbols or chunks with `get_snippet`.
   - Tier 3: Read full files (`read_note`, `read_code_file`, `read_multiple`) only when exhaustive context is required.
4. **Schema Discipline**: Query `list_templates` before authoring; follow schema; verify with `validate_note`.
5. **Knowledge Crystallization (Principle 3)**: Distill insights into permanent vault notes using `promote_concept`; trace with `traverse_lineage`.
6. **Greenfield Discipline**: No backwards compatibility shims, no dead code, clippy `-D warnings`.
7. **Hexagonal Architecture**: Infrastructure never leaks across ports; composition root in CLI `main.rs`.
