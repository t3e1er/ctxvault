# Codebase Semantic Indexing & Cross-Modal Retrieval Roadmap (`CODEROADMAP.md`)

This roadmap defines the architectural specification, academic foundations, tooling evaluation, and phased engineering plan for integrating **polyglot codebases** into the **Enterprise Semantic MCP** engine (`ctxvault-core`, `ctxvault-common`, `ctxvault-mcp`).

---

## 1. Executive Summary & Vision

The goal is to expand the engine from indexing Markdown documentation vaults to unifying **semi-structured natural-language documentation** (ADRs, RFCs, design docs, Obsidian notes) and **multi-language source code repositories** (Rust, TypeScript/JavaScript, Python, Go, C/C++, Java, etc.) within a **single, unified hybrid retrieval graph**.

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                   Unified Documentation & Code Knowledge Engine                  │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│   [Document: ADR-004] ──────(implements)─────► [CodeFile: src/search/engine.rs]  │
│           │                                                    │                 │
│      (supersedes)                                          (defines)             │
│           ▼                                                    ▼                 │
│   [Document: ADR-001]                               [CodeSymbol: SearchEngine]   │
│                                                                │                 │
│                                                             (calls)              │
│                                                                ▼                 │
│                                                      [CodeSymbol: rrf_fuse]      │
│                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────┤
│    Modality 1: Tantivy BM25 (Exact Identifiers & Text)                           │
│    Modality 2: fastembed BGE-small / HNSW (Dense Cross-Modal Embeddings)         │
│    Modality 3: Petgraph Directed Typed Graph (Call, Import & Lineage Hops)       │
│    Modality 4: Multi-Way Reciprocal Rank Fusion (RRF) with Modal Scoring         │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Core Invariants Preserved
1. **Source of Truth**: Plain text files on disk (`.md`, `.rs`, `.py`, `.ts`, etc.) remain authoritative. All indices (Tantivy, HNSW, SQLite, Petgraph) are derived, durable, and disposable.
2. **Pure Rust Runtime**: Memory-safe, `#![forbid(unsafe_code)]` at our crate boundary, zero unvetted dependencies, and strict `cargo-deny` compliance (MIT / Apache-2.0).
3. **Continuous Hybrid Retrieval**: Retains the 4-modality retrieval pipeline (BM25 + Vector + Graph Traversal + RRF) while adding deterministic AST/graph lookup tools.
4. **Principle 3 Crystallization**: Extends knowledge crystallization to support bidirectional lineage between design decisions and source code entities.

---

## 2. Comprehensive Literature Review & Academic State-of-the-Art

### 2.1 AST-Aware Structural Chunking (`cAST` Pattern)
Traditional text chunkers (fixed token/character windows) cause severe **context starvation** and **syntax fragmentation** when applied to code, splitting functions mid-statement and discarding vital scope headers.

* **cAST: High-Density Structural Chunking for Code RAG (2025)**:
  Demonstrated that recursive AST-guided decomposition using Tree-sitter achieves an 18–35% relative gain in Pass@1 on code comprehension and SWE-bench benchmarks. cAST ensures that:
  - Every chunk represents a complete, syntactically valid AST node (function, method, class, interface, module block).
  - Docstrings (`///`, `/** */`, `"""`) stay bound to their associated symbol definitions.
  - Sibling statements below token thresholds are merged, and oversized functions are partitioned only at inner logical block boundaries (`match`, `if-else`, loop blocks).
* **RepoCoder (Zhang et al., 2023) & RepoBench (Liu et al., 2023)**:
  Proved that prepending **AST Scope Breadcrumbs** (e.g., `// Scope: crate::search::Engine > search_hybrid`) directly into chunk text bridges lexical and conceptual gaps for both sparse BM25 and dense bi-encoders.
* **CoIR: Code Information Retrieval Benchmark (Li et al., 2024)**:
  Evaluated code retrieval across 10 distinct datasets and 8 tasks. Core empirical finding: Hybrid sparse-dense retrieval with RRF significantly outperforms either pure dense embeddings or lexical BM25 alone. Sparse BM25 excels at exact symbol/identifier queries, while dense bi-encoders capture natural-language semantic intent.

### 2.2 Graph-Based Code Indexing & Semantic Navigation
* **Scope Graphs & Stack Graphs (Creager et al., GitHub / OOPSLA 2021)**:
  Formalized language-agnostic name resolution by mapping source code to graph-based scope structures using Tree-sitter AST queries without requiring full compilation or type checking. Enabled zero-build jump-to-definition and reference resolution across files.
* **RepoGraph (2025) & RANGER (2025/2026)**:
  Constructed multi-relational code knowledge graphs (nodes: files, classes, methods, variables; edges: `calls`, `defines`, `imports`, `contains`). Used $k$-hop ego-network retrieval seeded by hybrid search to give LLMs structural context that prevents reasoning errors on cross-file dependencies.
* **Aider’s Repo Map (Gauthier, 2023–2025)**:
  Industrial state-of-the-art for lightweight repository mapping. Uses Tree-sitter to extract definitions and identifier references into a bipartite graph, applies **PageRank** to rank the most architecturally central symbols, and packs high-ranking signatures into a compact structural map.
* **Source Code Intelligence Protocol (SCIP / LSIF - Sourcegraph)**:
  Defines an open, Protobuf-based index format for compiler-exact symbol definitions, occurrences, relationships, and docstrings.

---

## 3. Rust Tooling Ecosystem Evaluation

| Crate / Tool | License | Multi-Language | Role & Capability | Assessment & Decision for `ctxvault` |
| :--- | :--- | :--- | :--- | :--- |
| **`tree-sitter`** (v0.22+) | MIT | Yes (100+ langs) | Fast incremental C-CST parser with safe Rust bindings | **Core Substrate**: Universal parser for syntax tree generation. |
| **`tree-sitter-language-pack`** | MIT / Apache | Yes (370+ langs) | Bundled pre-compiled grammars for instant polyglot support | **Recommended**: Eliminates managing individual grammar crates in `Cargo.toml`. |
| **`tree-sitter-tags`** | MIT | Yes (Polyglot) | Query-based extraction of symbol definitions, refs, and docstrings | **Recommended**: High-performance extraction of symbol tables and doc comments without compiler overhead. |
| **`ast-grep-core`** | MIT | Yes (Polyglot) | Structural AST pattern search and rewrite engine | **Alternative**: Useful for custom AST pattern extraction rules. |
| **`stack-graphs`** | MIT / Apache | Yes (Polyglot) | Incremental scope-graph name resolution | **Reference Only**: Upstream archived late 2025; adopt lightweight scope resolution directly in Petgraph. |
| **`scip`** | Apache-2.0 | Yes (Polyglot via CLI) | Protobuf parser for compiler-generated code indexes | **Optional Phase 4**: Ingests compiler-precise `.scip` files if pre-generated in CI. |
| **`petgraph`** | MIT / Apache | N/A (Graph Engine) | In-memory directed typed graph store and algorithms | **Core Substrate**: Already integrated in `ctxvault-core`; houses code and doc edges. |

---

## 4. Benchmark & Comparative Analysis: `codebase-memory-mcp`

The open-source **`DeusData/codebase-memory-mcp`** represents an industry baseline for code-focused MCP servers. Below is a head-to-head architectural comparison:

| Dimension | `codebase-memory-mcp` | Our Unified `ctxvault-core` Architecture |
| :--- | :--- | :--- |
| **Core Philosophy** | **Code-only structural property graph** | **Unified Cross-Modal Doc + Code Knowledge Engine** |
| **Implementation Language** | Static C binary | **Pure Rust** (`ctxvault-core`, `ctxvault-mcp`, `#![forbid(unsafe_code)]`) |
| **Retrieval Mechanism** | **Discrete graph queries** (Cypher-like queries, caller/callee traces) | **4-Modality Continuous Hybrid Retrieval** (BM25 + Dense Vector + Graph Proximity + RRF) |
| **Documentation Handling** | Basic ADR records in SQLite | **Full Markdown Vault Indexing** (ADRs, RFCs, wikilinks `[[...]]`, tags, templates, Principle 3 crystallization) |
| **Cross-Modal Lineage** | Explicit parameter links | **Native Graph Lineage** (`implements`, `documents`, `supersedes`) bridging docs and code |
| **Vector Search** | Secondary / None | **First-Class HNSW Vectors** (fastembed BGE-small with AST breadcrumbs and max-pooling) |
| **Full-Text Lexical Search** | SQLite `LIKE` / basic FTS | **Tantivy Okapi BM25** with custom code/doc tokenizers |
| **Graph Storage** | Relational tables in SQLite | **In-memory Petgraph** (`graph.bin` via bincode) + SQLite metadata catalog |
| **Agent Experience** | Multi-hop tool exploration | **Single-shot hybrid retrieval** + deterministic navigation tools |

### Key Ideas Adopted from `codebase-memory-mcp`:
1. **Lightweight "Hybrid LSP" Import Resolution**: Resolving `import`/`use` statements across the SQLite symbol table to connect cross-file `calls` and `implements` edges without compiler passes.
2. **Community Detection (Louvain / Infomap)**: Clustering symbols in Petgraph to generate architectural module summaries automatically.
3. **Targeted Structural MCP Tools**: Exposing `get_symbol_definition`, `find_callers`, and `get_module_graph` alongside continuous hybrid search.

---

## 5. Architectural Specification & Data Model

### 5.1 Unified Entity Discrimination (`ctxvault-common`)
Every indexed item is tagged with an `EntityKind` to prevent index pollution and enable precise filtering:

```rust
/// Discriminates between documentation notes and polyglot source code entities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// Markdown documentation note, RFC, or ADR.
    Documentation,
    /// Whole source code file (e.g. `src/engine.rs`).
    CodeFile { 
        language: String 
    },
    /// Distinct code symbol (function, struct, class, trait, interface).
    CodeSymbol {
        language: String,
        symbol_type: CodeSymbolType, // Struct, Function, Trait, Enum, Method, Class
        scope_path: String,          // e.g. "kb_core::search::SearchEngine"
        signature: String,           // e.g. "pub fn search_hybrid(&self, query: &str)"
    },
    /// Syntactically coherent AST chunk for vector/BM25 indexing.
    CodeChunk {
        language: String,
        scope_path: String,
        start_line: usize,
        end_line: usize,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeSymbolType {
    Function,
    Method,
    Struct,
    Class,
    Trait,
    Interface,
    Enum,
    Module,
    Constant,
    TypeAlias,
}
```

### 5.2 Extended Graph Edge Schema (`corpus.toml` & `ctxvault-core`)
The graph engine is extended with code-specific and cross-modal relationship types:

| Edge Type | Source Node | Target Node | Default Weight | Description |
| :--- | :--- | :--- | :--- | :--- |
| `defines` | `CodeFile` | `CodeSymbol` | 1.0 | File declares the symbol. |
| `imports` | `CodeFile` | `CodeFile` / `Module` | 0.6 | File imports another module/file. |
| `calls` | `CodeSymbol` | `CodeSymbol` | 0.8 | Function/method invokes another symbol. |
| `implements_trait` | `CodeSymbol` (Struct/Class) | `CodeSymbol` (Trait/Interface) | 0.9 | Type implements an interface/trait. |
| `documents` | `Document` (Doc/ADR) | `CodeFile` / `CodeSymbol` | 1.0 | Markdown document specifies or documents code. |
| `implements_adr` | `CodeFile` / `CodeSymbol` | `Document` (ADR) | 1.0 | Code entity implements an architecture decision. |

---

## 6. AST-Aware Code Chunking Engine (`CodeChunker`)

```
Source Code (.rs, .py, .ts, .go, .java)
   │
   ▼
[Tree-sitter Parser] ──► Concrete Syntax Tree (CST)
   │
   ▼
[AST Traversal & Tag Extraction]
   ├─ Module Headers & Import Blocks
   ├─ Types, Structs, Classes & Trait Declarations
   └─ Functions & Methods (with bound docstrings `///` or `"""`)
   │
   ▼
[Scope Breadcrumb Enrichment]
   Prepends: "// Scope: crate::search::SearchEngine > search_hybrid\n// Language: rust\n"
   │
   ▼
[AST-Aligned Chunks] (Syntactically complete, byte-offset tracked, ready for Tantivy & HNSW)
```

---

## 7. Multi-Modal Hybrid Search & Query Routing

```
                           User Query
                                │
                ┌───────────────┴───────────────┐
                ▼                               ▼
      [Natural Language Query]         [Symbol / Code Query]
    "how is RRF score calculated?"    "pub fn rrf_fuse doc_rank"
                │                               │
                ▼                               ▼
       [Boost Documentation]           [Boost Code Chunks]
                │                               │
                └───────────────┬───────────────┘
                                ▼
          [4-Modality Hybrid Retrieval (BM25 + Vector + Graph)]
                                │
                                ▼
           [Cross-Modal Lineage & Graph Link Expansion]
         (Doc -> Implemented Code / Code -> Explaining Docs)
                                │
                                ▼
                    [Reciprocal Rank Fusion]
```

### Search Modes Supported:
1. **Faceted / Filtered Search**:
   - `search(query: "...", filter: { entity_kind: ["documentation"] })` $\rightarrow$ Docs only.
   - `search(query: "...", filter: { entity_kind: ["code_symbol", "code_chunk"], language: ["rust"] })` $\rightarrow$ Rust code only.
2. **Unified Cross-Modal Retrieval (Default)**:
   - Evaluates BM25 and Dense Vector against all chunks (docs + code).
   - Seeds graph traversal from top hits:
     - Top Doc hit $\rightarrow$ traverses `implements` $\rightarrow$ returns implementing code chunks.
     - Top Code hit $\rightarrow$ traverses `documented_by` $\rightarrow$ returns explaining ADRs/RFCs.
   - RRF fuses scores across text, vector, and graph proximity with lineage annotations.

---

## 8. Phased Implementation Roadmap

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Engineering Roadmap                              │
├─────────────────────────────────────────────────────────────────────────────┤
│  Phase 1: Polyglot Parsing & AST-Aware Semantic Chunker                     │
│  Phase 2: Code Graph Extractor & Lightweight Import Resolver                │
│  Phase 3: Multi-Modal Search & Query Discrimination Engine                  │
│  Phase 4: Specialized Structural MCP Tools & Architecture Overview         │
│  Phase 5: Principle 3 Cross-Modal Knowledge Crystallization                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Polyglot Parsing & AST-Aware Semantic Chunker
* **Crates Impacted**: `ctxvault-common`, `ctxvault-core`
* **Deliverables**:
  - Add `tree-sitter` and `tree-sitter-language-pack` to `Cargo.toml`.
  - Implement `CodeChunker` in `ctxvault-core/src/parser/code/chunker.rs`.
  - Support top 6 languages: Rust, TypeScript/JavaScript, Python, Go, Java, C/C++.
  - Inject AST scope breadcrumbs into chunk text for Tantivy and fastembed embedding passes.
* **Verification**: Unit tests validating that AST chunks never split functions mid-expression and docstrings remain bound to signatures.

### Phase 2: Code Graph Extractor & Lightweight Import Resolver
* **Crates Impacted**: `ctxvault-core`
* **Deliverables**:
  - Implement `CodeGraphExtractor` in `ctxvault-core/src/graph/code.rs` using `tree-sitter-tags`.
  - Extract `defines`, `imports`, and `calls` relationships.
  - Implement a lightweight import resolver pass across SQLite symbol tables to connect cross-file call sites.
  - Ingest code nodes and edges into `petgraph` (`graph.bin`).
* **Verification**: Integration tests confirming cross-file graph traversal from caller function to callee function in a multi-file project.

### Phase 3: Multi-Modal Search & Query Discrimination Engine
* **Crates Impacted**: `ctxvault-core`, `ctxvault-mcp`
* **Deliverables**:
  - Add `EntityKind` filtering to Tantivy index schema and HNSW metadata.
  - Update `SearchEngine` to perform cross-modal seed-then-traverse graph expansion.
  - Update `ctxvault-mcp` search tool parameters: `query`, `entity_types`, `languages`, `depth`.
* **Verification**: Benchmark evaluation verifying that natural language queries retrieve documentation while surfacing relevant code via graph hops.

### Phase 4: Structural MCP Tools & Architecture Overview
* **Crates Impacted**: `ctxvault-mcp`, `ctxvault-core`
* **Deliverables**:
  - Add deterministic structural tools to MCP server:
    - `get_symbol_definition(symbol_path)`
    - `find_callers(symbol_name, max_depth)`
    - `get_module_graph(module_path)`
  - Implement Louvain community detection in `ctxvault-core` to generate automated architectural module summaries.
* **Verification**: End-to-end MCP JSON-RPC test suite for all new structural tools.

### Phase 5: Principle 3 Cross-Modal Knowledge Crystallization
* **Crates Impacted**: `ctxvault-core`, `ctxvault-mcp`
* **Deliverables**:
  - Extend `promote_concept` tool to accept code symbol links and synthesize `implements`/`documents` lineage edges.
  - Add automated **Code Drift Detection**: scan indexed code to alert when an ADR references deprecated or renamed symbols/functions.
* **Verification**: Crystallization benchmark testing lineage integrity between ADR notes and source code.

---

## 9. Delivered: Multi-Corpus, Cross-Modal & Progressive-Disclosure Enhancements

The multi-corpus upgrade (branch `feature/codebase-semantic-indexing`) extended the code
roadmap above with cross-cutting capabilities that apply to both code and docs:

* **Multi-corpus from one MCP.** `CorpusManager` serves N roots; read tools take
  `corpus`/`corpora` and cross-corpus queries RRF-merge with per-hit corpus tagging.
* **Cross-corpus symbol/edge linking.** A doc's `implements`/`documents` target (or a code
  import) resolves to a code symbol in another corpus by qualified name — only on a unique
  match, never producing a false edge — carrying a `ResolutionConfidence` band.
* **Import-resolution confidence bands.** `calls` edges are tagged `High` (unique in-file /
  unique in-workspace), `Medium` (same-directory disambiguation), or `Speculative`
  (first-of-many); `imports` are `Speculative`; `defines`/`implements_trait` are `High`.
  `find_callers` surfaces the band per caller.
* **Bi-modal search.** `modality` = `docs`|`code`|`both` threads through BM25 (indexed
  field), vector (post-filter), and graph (code-path classifier), consistently in the fused
  hybrid path.
* **Progressive disclosure.** `search` (handles) → `get_snippet` (one symbol/chunk, bounded,
  neighbors) → `read_note`/`read_code_file`/`read_multiple` (whole file), encoded in tool
  descriptions.
* **Consolidated surface + profiles.** `search` (`mode`) and `status` (`scope`) replace the
  former per-mode/per-status families; `--profile scout|analysis|all` gates `tools/list`.
* **Leiden community refinement.** `detect_communities_leiden` refines the Louvain partition
  so every community is internally connected (deterministic); `get_architecture` uses it,
  `graph_communities` accepts `algorithm=louvain` for the raw partition.
* **`check_index_coverage`.** Reports index/parse coverage for given paths or prefixes
  (distinct from the query-driven `coverage_report`).
