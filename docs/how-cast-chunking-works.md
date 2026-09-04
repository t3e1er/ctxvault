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
