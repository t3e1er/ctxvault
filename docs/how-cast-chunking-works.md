---
title: How Context-Aware AST (cAST) Chunking Works
tags: [cast, chunking, polyglot, ast, treesitter]
status: active
---

# How Context-Aware AST (cAST) Chunking Works

Standard text splitters split code at arbitrary line counts (e.g. 100 lines), severing function signatures from their bodies, or dropping critical class enclosing scopes.

**cAST (Context-Aware AST Chunking)** uses native **Tree-sitter concrete syntax trees** across 16 modern languages to extract structurally atomic code units enriched with scope breadcrumbs.

See also: [[docs/index]], [[what-is-ctxvault]], [[how-search-pipeline-works]].

---

## 16 Supported Modern Languages

| Tier | Language Grammars | AST Structural Node Types Extracted |
| :--- | :--- | :--- |
| **Tier 1 (Core)** | Rust, TypeScript, TSX, JavaScript, Python | Functions, Methods, Classes, Interfaces, Enums, Structs, Impls |
| **Tier 2 (Major Compiled)** | Go, C, C++, Java, C# | Methods, Functions, Classes, Structs, Namespaces, Interfaces |
| **Tier 3 (Extended & Scripting)** | Ruby, PHP, Swift, Elixir, Lua, Bash | Modules, Classes, Functions, Protocols, Methods, Defmodules |

---

## Scope Breadcrumb Injection

When an embedding model vectorizes a code snippet, an isolated function `def authenticate(...)` loses its enclosing context (`AuthManager`).

`cAST` injects language-appropriate comment breadcrumbs into the text before embedding:

```ruby
# Scope: AuthManager > authenticate
# Language: ruby
# File: src/auth.rb
def authenticate(user, password)
  puts "Authenticating #{user}..."
  true
end
```

### Supported Comment Syntax:
* Standard C-style (`//`): Rust, TS, JS, Go, C, C++, Java, C#, Swift, PHP.
* Hash-style (`#`): Python, Ruby, Elixir, Bash.
* Double-hyphen (`--`): Lua.

---

## SQLite Symbol Extraction

As `cAST` traverses the syntax tree, every symbol is recorded in SQLite:
* `name`: Symbol identifier (`"authenticate"`).
* `scope_path`: Fully qualified scope path (`"AuthManager > authenticate"`).
* `symbol_type`: `"function"`, `"method"`, `"class"`, `"struct"`, `"trait"`, `"interface"`, `"enum"`.
* `start_line` / `end_line`: Exact 1-indexed source code boundaries.
* `signature`: Complete method signature.
* `docstring`: Attached leading documentation comments.

This enables instantaneous sub-millisecond lookups via `get_symbol_definition` and `find_callers`.

---

## Future cAST Optimization Variations & High-Throughput Anchoring

To eliminate dense embedding bottlenecks on large monorepos (e.g. 50k+ files) and legacy/APU hardware without sacrificing semantic retrieval, three cAST optimization variants are planned:

### 1. Pre-parsing Graph & Centrality-Guided Anchoring (PageRank / In-Degree)
- **Concept**: Decouple AST parsing from neural embedding into a two-pass architecture.
  - *Pass 1 (Pure Rust, Sub-Minute)*: Parse the entire repository syntax trees, populate SQLite symbol tables, and build the in-memory Petgraph relation graph (`calls`, `implements`, `imports`).
  - *Pass 2 (Centrality Filter)*: Compute symbol in-degree or PageRank on the AST graph. Only promote nodes to `ChunkEmbedPolicy::Anchor` if they are high-centrality architectural hubs ($\text{in\_degree} \ge K$ callers) or exported package interfaces.
- **Benefit**: Reduces code vector volume from ~5.6 anchors/file down to ~0.8 anchors/file (**5x–10x reduction in GPU forward passes**). Semantic queries land on the architectural entrypoint, and Turn 2 graph traversal (`graph_match`, `get_snippet`) walks the AST down to leaf functions.

### 2. cAST Skeleton & Signature-Only Embedding
- **Concept**: Instead of embedding the full multi-line implementation body of a function/struct, strip the body before neural tokenization and embed only `signature + docstring + scope breadcrumbs`.
- **Benefit**: Reduces sequence length from ~350 tokens to ~45 tokens. Because transformer attention scales quadratically ($\mathcal{O}(L^2)$), GPU GEMM execution speeds up **4x–5x**, raising embedding throughput from ~12 chunks/s to ~60 chunks/s on legacy GPUs. The full implementation body remains 100% searchable in Tantivy BM25.

### 3. File-Level & Module Outline Anchors
- **Concept**: Generate a single synthetic file/module summary anchor chunk containing the file's primary abstraction, docstring, and top-level declaration outline, rather than generating individual vectors for every declared symbol.
- **Benefit**: Bounds vector cardinality to exactly 1 vector per file ($N$ files = $N$ vectors), reducing neural embedding load by over 80% on symbol-dense repos.

