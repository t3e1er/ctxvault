---
inclusion: fileMatch
fileMatchPattern: 'crates/ctxvault-mcp/**'
---

# ctxvault — MCP Tool Surface

Authoritative registry: `crates/ctxvault-mcp/src/tools/mod.rs` (`ToolRegistry`). Handlers are `ReadOnly(fn(&Engine, Value))` or `ReadWrite(fn(&mut Engine, Value))` — read-only tools run concurrently under a reader lock; mutating tools require the exclusive writer lock. When adding a tool, register it with the correct handler kind, a JSON Schema for inputs, and update the expected-tools test.

## Registered Tools (17)

| Domain | Count | Tools |
|---|---|---|
| **Read** | 3 | `read_file` (Tier 3 polymorphic path/paths batch with line slicing), `get_snippet` (Tier 2 symbol/chunk fetch + caller/callee handles + symbol definition lookup), `list_notes` (note catalog & single note frontmatter inspection) |
| **Search** | 2 | `search` (Tier 1 retrieval with Turn 1 hybrid snippets across docs & code via `snippets: usize`; `mode` = `hybrid` \| `bm25` \| `semantic` \| `graph` \| `explain`), `search_related` |
| **Graph** | 2 | `graph_match` (linear Cypher-Lite ASCII path query compiled to recursive SQLite CTEs with cycle guards), `graph_communities` (`algorithm` = `leiden` \| `louvain`, `view` = `architecture` \| `raw`) |
| **Write** (mutating) | 3 | `write_note` (`mode` = `create` \| `overwrite` \| `append` \| `prepend`), `delete_note`, `move_note` (wikilink refactoring) |
| **Template / Validation** | 2 | `validate` (unified single note template check, corpus scan, and taxonomy check via `check_taxonomy=true`), `list_templates` |
| **System / Corpus** | 5 | `status` (unified multi-corpus overview or per-corpus stats, indexing, graph density, coverage via `scope`), `list_corpora`, `sync_corpus` (`mode` = `delta` \| `full` \| `reembed`), `index_corpus`, `unload_corpus` |

Keep this list in sync with the registry — the registry is the source of truth if they diverge. The expected-tools test in `tools/mod.rs` asserts the exact count and names (17 tools).

## Tool Profiles (`--profile`)

`tools/list` exposure is gated by a `--profile` flag (default `all`). Nested sets: `scout` ⊂ `analysis` ⊂ `all`. Profiles only gate what is advertised — a hidden tool called directly still executes.

- **scout** (6 tools, minimal retrieve/navigate): `search`, `search_related`, `get_snippet`, `read_file`, `list_notes`, `status`.
- **analysis** (11 tools, scout + read-only graph and validation): adds `graph_match`, `graph_communities`, `validate`, `list_templates`, `list_corpora`.
- **all** (17 tools, every registered tool): analysis + the mutating/admin tools (`write_note`, `delete_note`, `move_note`, `sync_corpus`, `index_corpus`, `unload_corpus`).

## Cypher-Lite Query Language (`graph_match`)

`graph_match` compiles linear Cypher-Lite ASCII pattern queries into recursive SQLite Common Table Expressions (CTEs) with cycle guards and bounded depths for sub-millisecond graph traversal.

- **Pattern Syntax**:
  - `(source)-[:edge_type]->(target)`
  - `(:CodeSymbol {name: "NewMainKubelet"})-[:calls*1..2]->(target)`
  - `(:DocNode {path: "adrs/001.md"})-[:derived_from*1..3]->(target)`
  - `(source)-[:implements]->(target)`
- **Anchor Resolution**:
  1. `path` property filter
  2. `name` property filter (indexed via `idx_code_symbols_name`)
  3. `title` property filter
  4. `scope` / `scope_path` (indexed via `idx_code_symbols_scope`)
- **Edge Classes (`edge_class`)**:
  - `"code"`: AST code relationships (`defines`, `imports`, `calls`, `implements`)
  - `"structural"`: Document layout relationships (`parent_child`, `section`)
  - `"semantic"`: Markdown graph links (`wikilink`, `derived_from`, `shared_tag`)
  - `"crossmodal"`: Cross-domain links (`documents`, `implements_spec`)
  - `"hybrid"`: Cross-layer blended edges

## Agent Usage Rules (how ctxvault should be used by AI clients)

1. **Files are ground truth.** Search hits are retrieval caches. Trust the note/source content on disk.
2. **Turn 1 Affordance Grounding & Turn 1 Snippets**:
   - `search` automatically inlines source snippets for the top results directly in Turn 1 across docs and code (controlled by `snippets: usize`, default 3).
   - `search` responses return partitioned `docs` and `code` hits enriched with **graph affordances** (degree counts: `calls_in`, `calls_out`, `implements`, `imports`, `wikilinks_in`, `wikilinks_out`) and **schema envelope** (active node labels & edge types).
3. **Pick the right `search` mode** (one tool, `mode` param):
   - `mode=hybrid` — default for general/broad research (3-way RRF: BM25 + vector + graph).
   - `mode=bm25` — exact identifiers, function/struct names, error strings, verbatim tokens.
   - `mode=semantic` — abstract concepts, analogies, natural-language intent.
   - `mode=graph` — relationship/dependency queries across typed edges; filter by `edge_types` or `edge_class` (`code` | `structural` | `semantic` | `crossmodal` | `hybrid`).
   - `mode=explain` — introspect scoring breakdowns (BM25 vs vector vs graph).
   - `search_related` (separate tool) — "more like these" from seed docs (Personalized PageRank).
4. **Follow the 3 Progressive Disclosure Tiers**:
   - **Tier 1**: Query `search` (inspect top hits, snippets, and affordances).
   - **Tier 2**: Fetch specific bounded symbols or doc chunks via `get_snippet`.
   - **Tier 3**: Read full files or line slices (`[start_line, end_line]`) via `read_file` only when exhaustive file context is required.
5. **Turn 2 Path Expansion with Cypher-Lite (`graph_match`)**:
   - Use `graph_match` to trace call graphs, implementation chains, or concept lineage:
     e.g., `(:CodeSymbol {name: "Server"})-[:calls*1..2]->(target)` with `edge_class="code"`.
   - Use `graph_communities(view="architecture")` for high-level architectural subsystem mapping.
6. **Schema discipline on writes**:
   - Call `list_templates` before authoring.
   - Create or update notes via `write_note(path="...", mode="create"|"overwrite"|"append"|"prepend")`.
   - Confirm conformance immediately with `validate(path="...")`.
7. **Principle 3 Knowledge Crystallization**:
   - Distill ephemeral debugging traces, incident resolutions, and design decisions into permanent notes with `write_note`, populating `derived_from` frontmatter.
   - Verify ancestry and provenance via `graph_match(pattern="(:DocNode {path: \"...\"})-[:derived_from*1..]->(source)")`.
8. **Destructive operations**:
   - `delete_note` removes the file plus all index entries and edges — require explicit confirmation before calling.
