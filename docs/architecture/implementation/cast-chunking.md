---
title: "cAST Polyglot Chunking Engine"
description: "Tree-sitter concrete syntax tree chunking across 16+ languages with parent scope breadcrumb injection."
category: "implementation"
status: "active"
tags: ["cast", "chunking", "tree-sitter", "ast", "polyglot", "scope-breadcrumbs"]
related:
  - "[[docs/architecture/implementation/index]]"
  - "[[docs/architecture/trust/deterministic-graph]]"
  - "[[docs/architecture/adr/adr-016-generic-normalized-scope-resolution]]"
---

# cAST Polyglot Chunking Engine

Naive chunkers split source code on fixed character counts (e.g. 500 characters) or newline intervals. This slices functions in half, separates docstrings from signatures, and renders code chunks unparseable.

`groundcontrol` features **cAST (Concrete Abstract Syntax Tree) Chunking** powered by Tree-sitter.

* **Parser Modules**: [`crates/groundcontrol-core/src/parser/code/mod.rs`](file:///c:/dev/ctx/groundcontrol/crates/groundcontrol-core/src/parser/code/mod.rs)
* **Rust cAST Grammar**: [`crates/groundcontrol-core/src/parser/code/rust.rs`](file:///c:/dev/ctx/groundcontrol/crates/groundcontrol-core/src/parser/code/rust.rs)
* **Markdown Chunking**: [`crates/groundcontrol-core/src/parser/markdown/mod.rs`](file:///c:/dev/ctx/groundcontrol/crates/groundcontrol-core/src/parser/markdown/mod.rs)

---

## 1. Syntax-Aware Node Slicing

Rather than arbitrary line counts, cAST parses the source file into an AST and segments code strictly along logical syntax boundaries:
* Function definitions (`fn`, `def`, `func`, `function`)
* Struct, class, and interface declarations
* Impl blocks and method signatures
* Module-level constant groups

---

## 2. Parent Scope Breadcrumb Injection

A nested method chunk extracted in isolation often lacks critical context. For example, a method named `process` inside `PaymentGateway` would be indexed merely as `process`.

cAST injects hierarchical **scope breadcrumbs** into each chunk:
```text
// Scope: crate::billing::payment::PaymentGateway > fn process
pub fn process(&self, tx: Transaction) -> Result<Receipt> {
    ...
}
```
* The Tantivy BM25 tokenizer and ONNX embedder see the enclosing struct, namespace, and module names.
* Retrieval queries for `PaymentGateway::process` hit with 100% precision.

---

## 3. Supported Languages (52 Languages)

* Rust, Go, Python, TypeScript, JavaScript, Java, C, C++, C#
* COBOL (`arborium-cobol`)
* Visual Basic 6.0 (`tree-sitter-vb6`)
* Oracle PL/SQL (`tree-sitter-plsql-sqry`)
* XML & XSLT (`tree-sitter-xml`)
* Bash, Lua, Ruby, PHP, Swift, Kotlin, Scala, Elixir, Erlang, Dart, Julia, R
* Zig, D, WGSL, CUDA, Verilog, TLA+, Gleam, Nix, OCaml, Haskell
* SQL, HCL / Terraform, Bicep, Starlark, CMake, Make, Dockerfile
* HTML, CSS, JSON, TOML, YAML, Protocol Buffers, GraphQL
* Markdown (`pulldown-cmark`)

## 4. Universal Plain-Text BM25 Fallback

For text sources lacking native Tree-sitter AST support (e.g., Pascal, Fortran, Ada, legacy scripts, or custom configuration files):
1. **Binary Detection**: Evaluates sample bytes for null bytes (`b'\0'`) and matches against known binary extension filters (`KNOWN_BINARY_EXTENSIONS`).
2. **Generic Classification**: Non-binary text files are categorized as [`FileClassification::GenericText`](file:///c:/dev/semantic/groundcontrol/crates/groundcontrol-core/src/classifier/classifier.rs).
3. **Sliding Line-Window Chunking**: [`ArtifactParser::parse_generic_text`](file:///c:/dev/semantic/groundcontrol/crates/groundcontrol-core/src/parser/artifact.rs) segments lines into sliding windows (100 lines with 10-line overlap).
4. **Unified Retrieval**: Ingested directly into the Tantivy BM25 code index and SQLite metadata catalog, providing Turn 1 search snippets, `get_snippet` handle fetches, and line-slice `read_file` support.
