# Architecture: Enterprise Semantic MCP Server

> Codename TBD — A Rust-based MCP server providing hybrid semantic + BM25 + graph RAG retrieval over markdown knowledge bases, with configurable edge types, formal templates, and remote-capable deployment.

---

## 1. Design Principles

1. **Markdown is the source of truth.** Files on disk are authoritative. Indices are derived, rebuildable, and disposable.
2. **Graph structure is explicit, not LLM-extracted.** Wikilinks, frontmatter fields, and tags provide edges. No expensive LLM entity-extraction step.
3. **Edge types are project-configurable.** Each corpus declares its own relationship vocabulary — not hardcoded.
4. **Templates enforce conventions, not assumptions.** A schema-less corpus still works; templates add structural guarantees.
5. **Persistence survives restart.** All indices are durable on disk. Startup is a fast reload + delta check, not a full rebuild.
6. **Incremental by default.** Only changed files are re-processed. File watching keeps indices live.
7. **Local-first, remote-capable.** Same binary serves stdio (local) or Streamable HTTP (remote/enterprise).

---

## 2. Corpus Configuration

Each corpus is an independent, fully-configured unit:

```toml
# corpus.toml — lives at the root of each corpus directory OR in central config
[corpus]
name = "engineering-wiki"
path = "./wiki"
mode = "read-write"           # "read-only" | "read-write"

[chunking]
strategy = "semantic"          # "fixed" | "sentence" | "semantic" | "heading"
target_tokens = 512
max_tokens = 1024
overlap_tokens = 64
respect_headings = true        # never split mid-section
min_chunk_tokens = 50

[embedding]
model = "all-MiniLM-L6-v2"    # or "nomic-embed-text-v1.5", "bge-small-en-v1.5"
dimensions = 384
quantization = "f32"           # "f32" | "f16" | "int8"

[graph]
# Edge types available in THIS corpus (user-configurable)
[[graph.edge_types]]
name = "Wikilink"
source = "wikilink"            # auto-detected from [[links]]
weight = 1.0
bidirectional = false

[[graph.edge_types]]
name = "SharedTag"
source = "tag"                 # auto-detected from shared #tags
weight = 0.5
bidirectional = true
max_tag_frequency = 15         # ignore overly-common tags

[[graph.edge_types]]
name = "ParentChild"
source = "frontmatter"
field = "parent"
weight = 1.0
direction = "inbound"          # parent → child

[[graph.edge_types]]
name = "Supersedes"
source = "frontmatter"
field = "supersedes"
weight = 1.0
direction = "outbound"

[[graph.edge_types]]
name = "Implements"
source = "frontmatter"
field = "implements"
weight = 0.8
direction = "outbound"

[[graph.edge_types]]
name = "Peer"
source = "frontmatter"
field = "peers"
weight = 0.6
bidirectional = true

# Templates available in this corpus
[templates]
dir = ".templates/"            # relative to corpus root
```

Key insight: **edge types are data, not code**. A project about software specs might have `Implements`, `TestedBy`, `BlockedBy`. A research vault might have `ChallengedBy`, `BuildsOn`, `Replicates`. The engine doesn't know or care — it just builds typed, weighted, directed edges and exposes them to the query layer.

---

## 3. Template System

Templates live as TOML files in the corpus's `.templates/` directory:

```toml
# .templates/decision-record.toml
[template]
name = "decision-record"
description = "Architecture Decision Record"

[frontmatter]
# Required fields
[[frontmatter.required]]
name = "status"
type = "enum"
values = ["proposed", "accepted", "deprecated", "superseded"]

[[frontmatter.required]]
name = "date"
type = "date"                  # ISO8601

[[frontmatter.required]]
name = "deciders"
type = "list"                  # list of strings

# Optional fields
[[frontmatter.optional]]
name = "superseded_by"
type = "path"                  # must resolve to existing file

[[frontmatter.optional]]
name = "related_decisions"
type = "list_of_paths"

[[frontmatter.optional]]
name = "implements"
type = "path"

# Edge rules — declarative graph edge generation
[[edge_rules]]
field = "related_decisions"
edge_type = "Peer"             # must match a graph.edge_types entry
direction = "outbound"

[[edge_rules]]
field = "superseded_by"
edge_type = "Supersedes"
direction = "outbound"

[[edge_rules]]
field = "implements"
edge_type = "Implements"
direction = "outbound"

# Content rules
[content]
required_sections = ["Context", "Decision", "Consequences"]
min_word_count = 50
```

### Template Lifecycle

1. **On `create_note`**: Engine validates provided fields against template schema, generates frontmatter block, writes file atomically, creates graph edges from edge_rules.
2. **On `update_note`**: If relationship fields changed, old edges are removed and new edges created. Content rules are re-validated.
3. **On `move_note`**: All wikilinks across the corpus pointing to old path are rewritten. Graph edge targets are updated. No re-embedding needed (content unchanged).
4. **On index/re-index**: Template is inferred from `template:` frontmatter field. Edge rules are applied. Validation errors are stored but don't block indexing.

---

## 4. Graph RAG Search Strategy

### 4.1 Graph Construction (No LLM Required)

For markdown KBs, the graph is built deterministically from document structure:

```
Sources of edges:
├── [[Wikilinks]]              → directed edge, type "Wikilink"
├── Frontmatter fields         → directed/bidirectional edges per edge_rules
├── Shared #tags               → bidirectional edges, type "SharedTag" (frequency-capped)
└── Inline [markdown](links)   → directed edge, type "Reference" (optional)
```

This is fundamentally different from LightRAG/GraphRAG which require LLM extraction. Our graph is **exact**, **cheap to build**, and **incrementally updatable**.

### 4.2 Search Strategies

**Strategy A: Seed-then-Traverse (default for `search_hybrid`)**

```
1. SEED:    BM25(query, limit=K*3) ∪ Vector(query, limit=K*3)
2. EXPAND:  For each seed node, BFS/DFS over typed edges to depth D
            - Filter by edge types (configurable per query)
            - Accumulate neighbor nodes
3. RANK:    RRF fusion of:
            - BM25 score (normalized)
            - Vector cosine similarity (normalized)
            - Graph proximity boost (1/hop_distance)
            - Edge weight accumulation along path
4. RETURN:  Top K results with scores + traversal path
```

**Strategy B: Typed Traversal (for `search_graph`)**

```
1. MATCH:   Find nodes where path/title/tags contain query concept
2. TRAVERSE: BFS from matches, filtered by edge_type whitelist
3. RANK:    By hop distance, node degree, edge weight product
4. RETURN:  Connected subgraph as structured results
```

**Strategy C: Personalized PageRank (for `search_related`)**

```
1. SEED:    Set of known-relevant document paths (user-provided)
2. PPR:     Run Personalized PageRank with seeds as teleport set
3. FILTER:  Remove seeds from results, apply edge-type filter
4. RETURN:  Top K by PPR score — "what's most related to these docs?"
```

### 4.3 Dual-Level Retrieval (inspired by LightRAG)

- **Low-level**: Chunk-granularity vector search + exact BM25 matches. Returns specific passages.
- **High-level**: Document-level embeddings + graph traversal. Returns thematically connected documents.

The MCP tool exposes this as a `depth` parameter:
- `"precise"` → low-level only (best for specific factual queries)
- `"broad"` → high-level only (best for "what do we know about X?" sensemaking)
- `"adaptive"` → run both, merge with RRF (default)

---

## 5. Persistence Layer

```
.index/                          # per-corpus index directory (gitignored)
├── meta.db                      # SQLite — metadata catalog
├── tantivy/                     # Tantivy BM25 index segments
│   ├── meta.json
│   └── segments/
├── vectors/                     # HNSW index
│   ├── index.bin                # hnswlib-rs dump (or hannoy LMDB)
│   └── meta.json                # vector ID → (file_path, chunk_index)
└── graph.bin                    # petgraph serialized via bincode
```

### 5.1 SQLite Schema (metadata catalog)

```sql
-- Corpus-level config
CREATE TABLE corpus_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL              -- JSON-encoded
);

-- File tracking for incremental indexing
CREATE TABLE files (
    path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,       -- blake3 of file content
    modified_at INTEGER NOT NULL,     -- unix timestamp
    template TEXT,                    -- declared template name (nullable)
    title TEXT,
    indexed_at INTEGER NOT NULL
);

-- Chunk tracking
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    text TEXT NOT NULL,               -- chunk content for display
    vector_id INTEGER,               -- maps to HNSW internal ID
    tantivy_id TEXT                   -- maps to tantivy doc address
);

-- Edge type registry (from corpus config)
CREATE TABLE edge_types (
    name TEXT PRIMARY KEY,
    source TEXT NOT NULL,             -- "wikilink" | "tag" | "frontmatter" | "reference"
    weight REAL NOT NULL DEFAULT 1.0,
    bidirectional INTEGER NOT NULL DEFAULT 0,
    field TEXT,                       -- frontmatter field name (if source=frontmatter)
    config TEXT                       -- JSON for extra params (max_frequency, etc.)
);

-- Template definitions
CREATE TABLE templates (
    name TEXT PRIMARY KEY,
    definition TEXT NOT NULL          -- full TOML stored as text
);

-- Validation results (non-blocking)
CREATE TABLE validation_issues (
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    severity TEXT NOT NULL,           -- "error" | "warning"
    message TEXT NOT NULL,
    field TEXT,                       -- which field/section failed
    checked_at INTEGER NOT NULL
);
```

### 5.2 Why This Split

| Store | Responsibility | Why This Store |
|-------|---------------|----------------|
| SQLite | Metadata, configs, chunk text, file tracking, templates, validation | SQL queries for admin/analytics, single file, WAL for concurrency, inspectable |
| Tantivy | BM25 full-text search | Self-managing segments, incremental add/delete, persistent by design, proven at scale |
| hnswlib-rs | Vector ANN search | Fast cosine KNN, dump/reload persistence, soft-delete support |
| petgraph + bincode | Knowledge graph | In-memory for fast traversal, serialize on commit, reload on startup |

### 5.3 Startup Sequence

```
1. Open SQLite → load corpus config, edge type registry, templates
2. Open Tantivy index directory → reader ready immediately
3. Load HNSW from disk → vectors ready
4. Deserialize graph.bin → petgraph ready
5. Scan filesystem → compare file hashes against SQLite
6. Queue delta re-index for changed/new/deleted files (background)
7. Start file watcher (notify crate)
8. MCP server ready to accept tool calls
```

Cold start (existing index): < 2 seconds for a 10K-note corpus.
Full rebuild: proportional to corpus size and embedding throughput.

---

## 6. File Watching & Incremental Re-Index

### Watcher Architecture

```
notify v7.0 (cross-platform)
    │
    ▼
notify-debouncer-full (200ms batch window)
    │
    ▼
tokio::mpsc channel
    │
    ▼
Index Coordinator (async task)
    ├── Determines affected corpus from path prefix
    ├── Classifies event: Create | Modify | Delete | Rename
    └── Dispatches to re-index pipeline
```

### Event Handling

| Event | Action |
|-------|--------|
| **Create** | Parse → chunk → embed → add to tantivy + HNSW + graph. Validate against template. |
| **Modify** | Remove old chunks/vectors/edges for file. Re-parse → re-chunk → re-embed → re-add. |
| **Delete** | Remove from all indices. Remove all edges where this file is source or target. |
| **Rename** | Update path key in SQLite, tantivy, HNSW metadata. Update graph node key. Rewrite wikilinks in other files pointing to old path. No re-embedding needed. |

### Consistency Guarantees

- SQLite write + tantivy commit + HNSW dump happen in sequence per batch
- If crash mid-batch: on next startup, hash comparison detects stale entries and re-indexes them
- Graph serialization happens after each batch commit (cheap — typically < 50ms for 10K nodes)

---

## 7. MCP Tool Surface

The registry (`crates/ctxvault-mcp/src/tools/mod.rs`) is the authoritative source; the
expected-tools test asserts the exact set. All read tools accept an optional `corpus`
(single root) or `corpora` (`["a","b"]` or `"all"`, fan-out + RRF-merge, corpus-tagged);
write tools accept a single `corpus`. Search tools accept `modality` (`docs`|`code`|`both`)
and `detail` (`ids`|`default`).

### Progressive disclosure (three tiers)

Tool descriptions encode the ordering so agents self-enforce it:
1. **Tier 1 — handles.** `search` returns paths/qualified names + line ranges, never full
   bodies. `detail=ids` = bare handles for wide sweeps; `default` = a short snippet only.
2. **Tier 2 — fetch.** `get_snippet` returns exactly one code symbol (by `qualified_name`)
   or one doc chunk (by `path`+`chunk_index`), bounded by `max_lines`, with optional
   `include_neighbors` (code callers/callees; adjacent doc chunks).
3. **Tier 3 — full file.** `read_note` (docs), `read_code_file` (source), `read_multiple`
   (token-efficient batch), only when whole-file context is truly needed.

### Read / fetch tools

| Tool | Description |
|------|-------------|
| `read_note` | Tier 3: full markdown note content + parsed frontmatter |
| `read_code_file` | Tier 3: whole source file (or line range), raw |
| `read_multiple` | Tier 3 batch: many files in one call; per-path errors are entries, not failures |
| `get_snippet` | Tier 2: one code symbol OR one doc chunk, bounded, optional neighbors |
| `list_notes` | List indexed notes with metadata |
| `get_frontmatter` | Parsed frontmatter as structured JSON |

### Search tools

| Tool | Description |
|------|-------------|
| `search` | One tool, `mode` = `bm25` \| `semantic` \| `hybrid` (default) \| `graph` \| `explain`; honors `modality` + `detail` |
| `search_related` | PPR from seed documents — "find more like these" |

### Graph tools

| Tool | Description |
|------|-------------|
| `backlinks` / `forwardlinks` | Notes linking to / from a note, grouped by edge type (includes resolved cross-corpus links) |
| `graph_path` | Shortest path between two notes |
| `graph_stats` | Density, orphans, most-connected, edge type distribution |
| `graph_subgraph` | N-hop neighborhood around a node |
| `graph_communities` | `algorithm` = `leiden` (default, connectivity-refined) \| `louvain`; optional per-community density |
| `list_edge_types` | Registered edge types with class/source/weight/direction + live counts |
| `traverse_lineage` | Deterministic traversal along a structural edge type (supersedes/implements/depends_on) |

### Write tools

| Tool | Description |
|------|-------------|
| `create_note` | Template-aware creation: validates schema, generates frontmatter, creates edges |
| `update_note` | Modes: overwrite, append, prepend. Re-validates. Updates edges. |
| `delete_note` | Removes file + all index entries + all edges (requires confirmation) |
| `move_note` | Moves file, rewrites inbound wikilinks across the corpus, updates graph |
| `promote_concept` | Crystallize source notes into a schema-validated concept note (atomic rollback) |

### Template / validation, analytics, code, system

| Tool | Description |
|------|-------------|
| `validate_note` / `validate_corpus` | Template schema conformance |
| `list_templates` | Available templates + field schemas |
| `validate_taxonomy` | Broken links, DAG cycles, orphan ADRs, template constraints |
| `analyze_density` / `find_semantic_gaps` / `suggest_splits` | Graph + retrieval analytics |
| `coverage_report` | Query-driven retrieval dead zones |
| `check_index_coverage` | Index/parse coverage for given paths or prefixes |
| `get_symbol_definition` / `find_callers` | Code symbol lookup + inbound callers (with confidence bands) |
| `get_architecture` | Architectural overview via Leiden community clustering |
| `detect_changes` | Modified files + impact radius (mutating) |
| `status` | `scope` = `corpus` \| `indexing` \| `all` (default); manager-level overview when no corpus targeted |
| `corpus_list` | List configured corpora with modes + stats |
| `reindex_corpus` / `sync_corpus` / `reembed_corpus` | Index maintenance (mutating) |

**Total: 39 tools.** Consolidated from the older per-mode `search_*` / per-status
`get_*` families to shrink the `tools/list` footprint (scout profile advertises 9).

### Tool profiles (`--profile`)

`tools/list` exposure is gated by `--profile` (default `all`), nested `scout` ⊂ `analysis`
⊂ `all`. Profiles gate only what is advertised — a hidden tool called directly still
executes. `scout` = minimal retrieve/navigate (`search`, `search_related`, `get_snippet`,
`read_note`, `read_code_file`, `read_multiple`, `list_notes`, `get_frontmatter`, `status`);
`analysis` adds the read-only graph/validation/analysis/code-intel tools; `all` adds the
mutating/admin tools.

---

## 8. Transport & Deployment

One central process serves N corpus roots via `CorpusManager`. `--corpus` is repeatable and
accepts `name=path` or a bare `path`; `--default-corpus <name>` and `--profile
<scout|analysis|all>` are optional.

### Local Mode (stdio)

```bash
ctxvault --corpus wiki=./wiki --corpus ./docs --mode local --profile scout
```

Standard MCP stdio transport. Agent talks JSON-RPC over stdin/stdout. Single process,
serving one or many corpora.

### Server Mode (Streamable HTTP)

```bash
ctxvault --corpus ./wiki --corpus ./code --mode server --bind 0.0.0.0:9090
```

Axum-based HTTP server. Streamable HTTP transport. Multiple agents can connect
simultaneously. Supports CORS for browser-based MCP clients and a health endpoint at
`/health`.

### Proxy Mode (stdio → remote)

```bash
ctxvault --mode proxy --server https://kb.internal:9090
```

Local process speaks stdio to the agent, forwards JSON-RPC to remote server over HTTP. The agent doesn't know the difference. Use case: team KB hosted centrally, each developer's agent connects through a local proxy.

---

## 9. Crate Dependencies

| Component | Crate | Role |
|-----------|-------|------|
| MCP protocol | `rmcp` | Official Rust MCP SDK (stdio + Streamable HTTP) |
| HTTP server | `axum` + `tower` | Transport layer for server mode |
| BM25 / FTS | `tantivy` | Full-text search, segment-based persistence |
| Vector index | `hnswlib-rs` | HNSW ANN with dump/reload |
| Embeddings | `fastembed-rs` | ONNX-based local inference (MiniLM, nomic, BGE) |
| Knowledge graph | `petgraph` | Directed graph with typed edges |
| Graph serialization | `bincode` + `serde` | Fast binary graph persistence |
| Markdown parsing | `pulldown-cmark` | CommonMark parser |
| Frontmatter | `serde_yaml` | YAML deserialization |
| Wikilink extraction | Custom parser | `[[link]]` and `[[link|alias]]` patterns |
| Config | `toml` + `serde` | Corpus/template configuration |
| Metadata DB | `rusqlite` | SQLite with WAL mode |
| File watching | `notify` v7 + debouncer | Cross-platform filesystem events |
| Async runtime | `tokio` | Async I/O, channels, task spawning |
| Hashing | `blake3` | Fast content hashing for change detection |
| CLI | `clap` | Argument parsing |
| Tracing | `tracing` + `opentelemetry` | Observability |

> These backend crates are consumed **only** through the ports described in the
> next section. See [Ports & Adapters Architecture](#ports--adapters-architecture)
> for how they are encapsulated behind traits.

---

## Ports & Adapters Architecture

ctxvault is organized as **ports and adapters** (hexagonal architecture). A
*port* is a trait expressing a capability the domain needs; an *adapter* is a
concrete backend that satisfies a port. The domain depends on the ports, never
on a concrete backend — so a backend type never crosses a port boundary, and the
upper layers name no concrete backend.

### The six ports (`ctxvault-common::ports`)

All six port traits live **low**, in `ctxvault-common` (`crates/ctxvault-common/src/ports.rs`),
so consumers depend on the contract rather than on a backend. Every signature
speaks only `ctxvault-common` domain types (`ctxvault-common::types`) plus the
standard library — no `rusqlite`, `tantivy`, `hnsw_rs`, `petgraph`, `ort`, or
`tokenizers` type appears in a port signature.

| Port | Contract |
|------|----------|
| `MetadataCatalog` | Durable catalog: files, text chunks, code symbols, edge-type config, key/value corpus config, and resumable indexing state. |
| `TextIndex` | Full-text BM25 lexical retrieval: add/remove documents, commit/writer lifecycle, and ranked `search` / `search_with_modality`. |
| `VectorStore` | Dense ANN vector store: single/batch `add`, per-document `remove`, modality-filtered `search`, `save`, plus dimension / model-version / stale / dirty bookkeeping. |
| `GraphStore` | Typed knowledge graph: node/edge mutation, edge construction from parsed documents, traversal (BFS / shortest path / lineage), backlinks/forwardlinks, taxonomy validation, community detection, stats, and `save`. |
| `EmbeddingProvider` | Dense embeddings: `embed_query`, `embed_batch`, and `dimensions`. |
| `SearchService` | Search-mode dispatch (`bm25` \| `semantic` \| `hybrid` \| `graph` \| `explain`) plus `search_related`, fusing signals via RRF over the other ports; inputs arrive as a `SearchQuery`. |

The domain records these ports exchange (`FileRecord`, `ChunkRecord`,
`EdgeTypeRecord`, `IndexingState`/`IndexingStatus`, `VectorSearchResult`,
`VectorMeta`, `GraphStats`, `LineageNode`, `BrokenLink`, `CircularDependency`,
`OrphanAdr`, `Community`, `CommunityDetectionResult`, `CommunityDensity`, …) all
live in `ctxvault-common::types`, so no adapter type leaks through a return value.

### The adapters (`ctxvault-core`)

Each adapter is a concrete backend implementing one port and keeping its backend
crate encapsulated inside `ctxvault-core`:

| Adapter | Backend | Implements | Location |
|---------|---------|------------|----------|
| `Store` | SQLite (`rusqlite`, bundled) | `MetadataCatalog` | `persistence/mod.rs` |
| `BM25Index` | Tantivy | `TextIndex` | `index/mod.rs` |
| `VectorIndex` | HNSW (`hnsw_rs`) | `VectorStore` | `vector_index.rs` |
| `KnowledgeGraph` | Petgraph | `GraphStore` | `graph/mod.rs` |
| `Embedder` | ONNX (`ort` + `tokenizers`) | `EmbeddingProvider` | `embedding.rs` |
| `CoreSearchService` | port-generic `search::` free functions | `SearchService` | `search_service.rs` |

That is five infrastructure adapters plus `CoreSearchService`, which owns no
backend of its own — it forwards to the port-generic search free functions that
operate over the other adapters.

### Domain orchestrator: `Engine` (concrete)

`Engine` (`ctxvault-core/src/engine.rs`) is a single **concrete** type — it is
**not** generic over the ports (this is "Approach B"). It owns the five adapters
by value (the `Embedder` lazily, behind an `RwLock<Option<Arc<Embedder>>>`), and
exposes consumers **port-typed** access rather than the concrete backends:

- `graph() -> &impl GraphStore`, `graph_mut() -> &mut impl GraphStore`
- `store() -> &impl MetadataCatalog`
- `search_service() -> CoreSearchService` (a `SearchService`)
- plus narrow domain methods (analytics wrappers such as `analyze_density` /
  `find_semantic_gaps` / `suggest_splits` / `coverage_report`; and
  `has_vector_index`, `embedder_active`, `vector_count`, indexing / commit /
  delta-scan / reembed operations).

There are no concrete-returning accessors — consumers cannot reach a `BM25Index`,
`VectorIndex`, or `Embedder` directly.

### Construction seam: `EngineBuilder`

`EngineBuilder::open(config, index_dir) -> Result<Engine>`
(`ctxvault-core/src/engine_builder.rs`) is the single place the concrete adapters
are constructed and injected. It derives the `.index/` paths
(`meta.db`, `tantivy/`, `vectors.json`, `graph.bin`), reconciles vector staleness,
persists edge types, and hands the assembled adapters to `Engine::from_parts`.
`Engine::open` is a thin delegate to it.

### Multi-corpus router: `CorpusManager`

`CorpusManager` (`ctxvault-core`) holds N `Engine`s keyed by corpus name, building
each via `EngineBuilder::open`. It routes read/write tool calls to the right
engine and performs cross-corpus symbol linking through the `MetadataCatalog` and
`GraphStore` ports.

### Composition root: `ctxvault-cli`

`ctxvault-cli` (`main.rs`) is the entry composition root. It parses flags/config,
builds the `CorpusManager` (which drives `EngineBuilder` to construct and inject
the concrete adapters), and selects the transport mode (Local stdio / Server HTTP
/ Proxy / Client). The CLI names **no** concrete backend type — construction is
injection that flows through the builder.

### Layering (acyclic, ports low)

```
ctxvault-common   (ports + domain types; dependency-light)
      ▲
ctxvault-core     (adapters + Engine + EngineBuilder + CorpusManager; owns the heavy backend crates)
      ▲
ctxvault-mcp      (tool registry + transport; depends only on ports + SearchService + domain types + the Engine/CorpusManager orchestrators)
      ▲
ctxvault-cli      (composition root: constructs and injects the concrete adapters)
```

The invariant that keeps this clean: **a backend type never crosses a port
boundary, and `ctxvault-mcp` and `ctxvault-cli` name no concrete backend type.**
`ctxvault-common` stays dependency-light precisely so the ports remain
backend-free.

### Wiring policy and design intent

Hot-path ports are consumed as generics / `&impl Trait` — monomorphized and
zero-cost. There is **no** `Arc<dyn>` runtime-swap seam today. `Engine` is
deliberately concrete (Approach B): the ports exist to decouple consumers from
backends and to keep the layering acyclic, not to make `Engine` runtime-pluggable.

The design intent the ports **unblock** — *not yet built* — is that an alternative
backend (for example a different persistence or vector store) would arrive as a
**new crate with its own adapters implementing these same ports**, plus its own
composition root, without editing the domain or widening a port. No such
alternative backend exists today; the current adapters (SQLite, Tantivy, HNSW,
Petgraph, ONNX) are the only ones.

---

## 10. Roadmap (Revisited Post-Research)

### Phase 1: Core Engine (Weeks 1–4)
- Project scaffold with rmcp, tantivy, fastembed-rs, petgraph, hnswlib-rs
- Single-corpus indexer: parse markdown, extract frontmatter/wikilinks/tags, chunk, embed
- Configurable chunking strategies
- Hybrid search (BM25 + vector + RRF)
- Basic graph from wikilinks + tags (hardcoded edge types initially)
- SQLite metadata catalog with incremental re-index
- stdio MCP transport with core search + read tools
- Persistence: all indices survive restart

### Phase 2: Configurable Graph & Templates (Weeks 5–8)
- Corpus config TOML with user-defined edge types
- Edge type registry in SQLite
- Template system: schema validation, edge rules, content rules
- Write tools: create_note, update_note, move_note (with wikilink rewriting)
- Validation tools
- Multi-corpus support (independent indices, shared MCP surface)
- Graph tools: typed traversal, PPR, subgraph extraction
- File watcher with debounced incremental re-index

### Phase 3: Analytics & Intelligence (Weeks 9–12)
- Dual-level retrieval (chunk-level + document-level embeddings)
- search_explain with full scoring breakdown
- Analytics tools: density analysis, semantic gap detection, coverage reports
- Graph community detection (Louvain/Leiden) for topic clustering
- Configurable embedding models (hot-swap without full re-index via model versioning)

### Phase 4: Enterprise Transport (Weeks 13–16)
- Streamable HTTP transport (axum)
- Proxy mode (stdio → remote)
- Bearer token auth
- Multi-tenant corpus isolation
- OpenTelemetry instrumentation
- Health/readiness probes
- Binary releases for Linux/macOS/Windows

### Phase 5: Multi-Corpus, Cross-Modal & Progressive Disclosure (shipped)
- Multi-corpus / multi-root from one central MCP via `CorpusManager` (`--corpus name=path`, `--default-corpus`).
- Corpus discrimination (`corpus`/`corpora`) with cross-corpus fan-out + RRF merge, per-hit corpus tagging.
- Cross-modal + cross-corpus symbol/edge linking (unique-match only, with confidence bands).
- Bi-modal search (`modality` = docs|code|both) across BM25, vector, graph, and the fused path.
- Three-tier progressive disclosure (search handles → `get_snippet` fetch → whole-file read), encoded in tool descriptions.
- Tool consolidation (`search`/`status`) + profiles (`scout`|`analysis`|`all`) to shrink the `tools/list` footprint.
- Parity fill-ins: Leiden community refinement, import-resolution confidence bands, `check_index_coverage`, `read_multiple`.

### Future
- Hierarchical embeddings (doc-level summaries → chunk details) and adaptive token-budget retrieval depth.
- Concept drift velocity tracking and context density scoring.
- Per-modality vector sub-indices (currently a post-filter) if retrieval perf demands it.

---

## 11. Key Differentiators vs. Semantic-Pages

| Dimension | semantic-pages | This project |
|-----------|---------------|--------------|
| Language | Node.js (ONNX/WASM) | Rust (native, no GC) |
| BM25 | None | Tantivy (full Lucene-class) |
| Chunk config | Hardcoded 2000 chars | Per-corpus strategy + size config |
| Edge types | Hardcoded (wikilink=1.0, tag=0.5) | User-configurable per corpus |
| Templates | None | Formal schemas with validation + edge generation |
| Multi-corpus | Single directory | Multiple isolated corpora, single MCP surface |
| Graph queries | BFS only | BFS, DFS, typed traversal, PPR, subgraph extraction |
| Persistence | File-based (fragile) | SQLite + Tantivy + HNSW dump (crash-safe) |
| Incremental | Full rebuild on change | Hash-based delta, only changed files re-processed |
| Transport | stdio only | stdio + Streamable HTTP + proxy mode |
| Analytics | None | Gap analysis, density, coverage, scoring explanation |
| Write-back | Basic CRUD | Template-enforced, edge-generating, link-rewriting |
| File watching | chokidar (JS) | notify v7 (native, used by rust-analyzer) |

---

## References

- [LightRAG](https://arxiv.org/abs/2410.05779) — Dual-level retrieval, incremental graph updates
- [SPRIG](https://arxiv.org/abs/2602.23372) — CPU-only PPR-based graph retrieval
- [LazyGraphRAG](https://www.microsoft.com/en-us/research/blog/lazygraphrag-setting-a-new-standard-for-quality-and-cost/) — Query-time graph building, no upfront LLM cost
- [OKF Spec](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) — Open Knowledge Format for typed markdown KBs
- [MarkScribe](https://github.com/Erodenn/markscribe) — Convention-enforced markdown MCP with schemas
- [mdaifs](https://mdaifs.org/) — Deterministic graph from frontmatter fields
- [Tantivy Architecture](https://github.com/quickwit-oss/tantivy/blob/main/ARCHITECTURE.md) — Segment-based BM25 index
- [hannoy/Meilisearch](https://www.meilisearch.com/blog/3xfaster-vector-store) — LMDB-backed HNSW
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) — Official Rust MCP SDK
