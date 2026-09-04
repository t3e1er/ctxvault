---
inclusion: fileMatch
fileMatchPattern: 'crates/ctxvault-mcp/**'
---

# ctxvault — MCP Tool Surface

Authoritative registry: `crates/ctxvault-mcp/src/tools/mod.rs` (`ToolRegistry`). Handlers are `ReadOnly(fn(&Engine, Value))` or `ReadWrite(fn(&mut Engine, Value))` — read-only tools run concurrently under a reader lock; mutating tools require the exclusive writer lock. When adding a tool, register it with the correct handler kind, a JSON Schema for inputs, and update the expected-tools test.

## Registered Tools (39)

**Read** — `read_note`, `read_code_file`, `read_multiple`, `get_snippet`, `list_notes`, `get_frontmatter`

`read_multiple` is a batch Tier-3 read: one call fetches many files (markdown → parsed note; source → raw content); per-path errors surface as an `error` entry rather than failing the whole call.

**Search** — `search` (one tool; `mode` = `bm25` | `semantic` | `hybrid` (default) | `graph` | `explain`), `search_related`

**Graph** — `backlinks`, `forwardlinks`, `graph_path`, `graph_stats`, `graph_subgraph`, `graph_communities` (`algorithm` = `leiden` (default, connectivity-refined) | `louvain`), `list_edge_types`, `traverse_lineage`

**Write** (mutating) — `create_note`, `update_note`, `delete_note`, `move_note`, `promote_concept`

**Template/Validation** — `validate_note`, `validate_corpus`, `list_templates`, `validate_taxonomy`

**Analysis** — `analyze_density`, `find_semantic_gaps`, `suggest_splits`, `coverage_report`, `check_index_coverage`

`check_index_coverage` reports, for given paths or path prefixes, whether each is indexed, its chunk/symbol counts, and parse gaps (indexed but empty). Distinct from `coverage_report` (query-driven retrieval dead zones).

Community detection defaults to **Leiden**: it runs Louvain, then splits each community into connected components so no community is internally disconnected (Leiden's key fix over Louvain), and recomputes modularity deterministically. `get_architecture` uses the Leiden result; `graph_communities` accepts `algorithm=louvain` for the raw partition. Code call/import edges now carry a `confidence` band (`high`/`medium`/`speculative`) reflecting resolution certainty; `find_callers` surfaces it per caller.

**Code (structural, polyglot)** — `get_symbol_definition`, `find_callers`, `get_architecture`, `detect_changes` (mutating)

**System / Corpus** — `status` (one tool; `scope` = `corpus` | `indexing` | `all` (default); manager-level overview when no `corpus` targeted), `corpus_list`, `reindex_corpus` (mutating), `sync_corpus` (mutating), `reembed_corpus` (mutating)

Keep this list in sync with the registry — the registry is the source of truth if they diverge. The expected-tools test in `tools/mod.rs` asserts the exact count and names; update it whenever tools are added, removed, renamed, or consolidated.

## Tool Profiles (`--profile`)

`tools/list` exposure is gated by a `--profile` flag (default `all`). Nested sets: `scout` ⊂ `analysis` ⊂ `all`. Profiles only gate what is advertised — a hidden tool called directly still executes.

- **scout** (9 tools, minimal retrieve/navigate): `search`, `search_related`, `get_snippet`, `read_note`, `read_code_file`, `read_multiple`, `list_notes`, `get_frontmatter`, `status`.
- **analysis** (scout + read-only graph/validation/analysis/code intel): adds `backlinks`, `forwardlinks`, `graph_path`, `graph_stats`, `graph_subgraph`, `graph_communities`, `list_edge_types`, `traverse_lineage`, `get_symbol_definition`, `find_callers`, `get_architecture`, `validate_note`, `validate_corpus`, `list_templates`, `validate_taxonomy`, `analyze_density`, `find_semantic_gaps`, `suggest_splits`, `coverage_report`, `check_index_coverage`, `corpus_list`.
- **all** (every registered tool): analysis + the mutating/admin tools (`create_note`, `update_note`, `delete_note`, `move_note`, `promote_concept`, `reembed_corpus`, `sync_corpus`, `reindex_corpus`, `detect_changes`).

## Agent Usage Rules (how ctxvault should be used by AI clients)

1. **Files are ground truth.** Search hits are retrieval caches. Trust the note content on disk.
2. **Pick the right `search` mode** (one tool, `mode` param):
   - `mode=hybrid` — default for general/broad research (3-way RRF: BM25 + vector + graph).
   - `mode=bm25` — exact identifiers, function/struct names, error strings, verbatim tokens.
   - `mode=semantic` — abstract concepts, analogies, natural-language intent.
   - `mode=graph` — relationship/dependency queries across typed edges; filter by `edge_types` or `edge_class` (semantic|structural|hybrid).
   - `mode=explain` — when you need the scoring breakdown per result.
   - `search_related` (separate tool) — "more like these" from seed docs (Personalized PageRank).
3. **Read at chunk granularity.** Use snippets from search; call `read_note` only when full document context is required.
4. **Schema discipline on writes.** Call `list_templates` before `create_note`; satisfy required frontmatter fields and sections; confirm with `validate_note` / `validate_corpus`.
5. **Crystallize (Principle 3).** Turn durable outcomes (resolved bugs, accepted decisions) into notes via `promote_concept`; it validates schema and synthesizes lineage edges atomically (rollback on failure). Trace provenance with `traverse_lineage`.
6. **Cross-modal navigation.** From a doc hit, traverse `implements`/`documents` to reach code; from a code hit, traverse back to explaining ADRs/RFCs.
7. **Destructive ops.** `delete_note` removes the file plus all index entries and edges — require explicit confirmation before calling.
