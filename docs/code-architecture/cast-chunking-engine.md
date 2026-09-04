---
title: "Context-Aware AST (cAST) Chunking: Polyglot Grammar Traversal"
category: "code-architecture"
status: "active"
tags: ["cast", "ast", "treesitter", "chunking", "polyglot", "code-intelligence"]
related:
  - "[[docs/code-architecture/index]]"
  - "[[docs/code-architecture/generic-scope-normalization]]"
  - "[[docs/code-architecture/decisions/adr-008-anchor-embedding-paradigm]]"
---

# Context-Aware AST (cAST) Chunking: Polyglot Grammar Traversal

Standard text splitters split code naively at fixed character or line counts, severing functions in mid-statement and detaching docstrings from their declarations.

`ctxvault` implements **cAST (Context-Aware AST Chunking)** using native Tree-sitter parsers to extract structurally atomic, semantically enriched code units across 16 modern languages.

---

## 1. The 16 Supported Modern Languages

```
┌──────────────────────────────┬──────────────────────────────────────────┬──────────────────────────────────────────────┐
│ Support Tier                 │ Languages                                │ Extracted AST Structural Node Types          │
├──────────────────────────────┼──────────────────────────────────────────┼──────────────────────────────────────────────┤
│ Tier 1 (Core)                │ Rust, TypeScript, TSX, JavaScript, Python│ Functions, Methods, Classes, Traits, Structs │
│ Tier 2 (Major Compiled)      │ Go, C, C++, Java, C#                     │ Methods, Namespaces, Interfaces, Structs     │
│ Tier 3 (Extended/Scripting)  │ Ruby, PHP, Swift, Elixir, Lua, Bash      │ Defmodules, Classes, Functions, Protocols    │
└──────────────────────────────┴──────────────────────────────────────────┴──────────────────────────────────────────────┘
```

---

## 2. Structural Scope Breadcrumb Injection

When an isolated function is embedded into a vector space, it loses its enclosing module and class context. `cAST` injects language-appropriate comment breadcrumbs into chunk headers before indexing:

```rust
// Scope: crate::engine::EngineBuilder > with_catalog
// Language: rust
// File: crates/ctxvault-core/src/engine_builder.rs
pub fn with_catalog(mut self, catalog: Arc<dyn MetadataCatalog>) -> Self {
    self.catalog = Some(catalog);
    self
}
```

### Comment Syntax Conventions:
* Standard C-Style (`//`): Rust, TypeScript, JavaScript, Go, C, C++, Java, C#, Swift, PHP.
* Hash-Style (`#`): Python, Ruby, Elixir, Bash.
* Double-Hyphen (`--`): Lua.

---

## 3. Atomic Docstring Binding

In standard line-based chunkers, leading documentation comments (`///`, `/** */`, `"""`) are frequently split into a separate chunk from the function signature they describe.

`cAST` recognizes docstrings as attached trivia of their associated syntax nodes:
* Docstrings are permanently bound to their declaration.
* When vectorizing, docstrings provide rich natural-language semantic terms that align directly with human queries.
* Raw signatures and docstrings are indexed in SQLite `code_symbols` for instantaneous sub-millisecond retrieval via `get_symbol_definition`.

See [[docs/code-architecture/decisions/adr-008-anchor-embedding-paradigm]] for how cAST integrates with anchor embedding.
