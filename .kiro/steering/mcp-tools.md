---
inclusion: fileMatch
fileMatchPattern: 'crates/ctxvault-mcp/**'
---

# ctxvault — MCP Tool Surface

Authoritative registry: `crates/ctxvault-mcp/src/tools/mod.rs` (`ToolRegistry`). Handlers are `ReadOnly(fn(&Engine, Value))` or `ReadWrite(fn(&mut Engine, Value))` — read-only tools run concurrently under a reader lock; mutating tools require the exclusive writer lock. When adding a tool, register it with the correct handler kind, a JSON Schema for inputs, and update the expected-tools test.

## Registered Tools (41)

**Read** — `read_note`, `list_notes`, `get_frontmatter`

**Search** — `search_bm25`, `search_semantic`, `search_hybrid`, `search_graph`, `search_related`, `search_explain`

**Graph** — `backlinks`, `forwardlinks`, `graph_path`, `graph_stats`, `graph_subgraph`, `graph_communities` (Louvain), `list_edge_types`, `traverse_lineage`

**Write** (mutating) — `create_note`, `update_note`, `delete_note`, `move_note`, `promote_concept`

**Template/Validation** — `validate_note`, `validate_corpus`, `list_templates`, `validate_taxonomy`

**Analysis** — `analyze_density`, `find_semantic_gaps`, `suggest_splits`, `coverage_report`

**Code (structural, polyglot)** — `get_symbol_definition`, `find_callers`, `get_architecture`, `detect_changes` (mutating)

**System / Corpus** — `get_status`, `get_corpus_stats`, `get_indexing_status`, `corpus_list`, `reindex_corpus` (mutating), `sync_corpus` (mutating), `reembed_corpus` (mutating)

Keep this list in sync with the registry — the registry is the source of truth if they diverge. The expected-tools test in `tools/mod.rs` asserts the exact count and names; update it whenever tools are added, removed, renamed, or consolidated.

## Agent Usage Rules (how ctxvault should be used by AI clients)

1. **Files are ground truth.** Search hits are retrieval caches. Trust the note content on disk.
2. **Pick the right modality:**
   - `search_hybrid` — default for general/broad research (3-way RRF: BM25 + vector + graph).
   - `search_bm25` — exact identifiers, function/struct names, error strings, verbatim tokens.
   - `search_semantic` — abstract concepts, analogies, natural-language intent.
   - `search_graph` — relationship/dependency queries across typed edges; filter by `edge_types` or `edge_class` (semantic|structural|hybrid).
   - `search_related` — "more like these" from seed docs (Personalized PageRank).
   - `search_explain` — when you need the scoring breakdown per result.
3. **Read at chunk granularity.** Use snippets from search; call `read_note` only when full document context is required.
4. **Schema discipline on writes.** Call `list_templates` before `create_note`; satisfy required frontmatter fields and sections; confirm with `validate_note` / `validate_corpus`.
5. **Crystallize (Principle 3).** Turn durable outcomes (resolved bugs, accepted decisions) into notes via `promote_concept`; it validates schema and synthesizes lineage edges atomically (rollback on failure). Trace provenance with `traverse_lineage`.
6. **Cross-modal navigation.** From a doc hit, traverse `implements`/`documents` to reach code; from a code hit, traverse back to explaining ADRs/RFCs.
7. **Destructive ops.** `delete_note` removes the file plus all index entries and edges — require explicit confirmation before calling.
