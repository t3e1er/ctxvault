---
title: "Generic Scope Normalization & AST Path Resolution"
category: "code-architecture"
status: "active"
tags: ["ast", "generics", "normalization", "sqlite", "code-intelligence", "ergonomics"]
related:
  - "[[docs/code-architecture/index]]"
  - "[[docs/code-architecture/cast-chunking-engine]]"
  - "[[docs/code-architecture/decisions/adr-016-generic-normalized-scope-resolution]]"
---

# Generic Scope Normalization & AST Path Resolution

When querying Tier-2 `get_snippet` for code symbols, AI agents frequently omit generic type parameters or lifetime annotations (e.g. typing `EarlyBinder > instantiate` instead of the exact compiler signature `EarlyBinder<'tcx, T> > instantiate`).

In naive string lookup systems, this results in an immediate **404 Not Found**.

`ctxvault` implements **Generic-Normalized Scope Resolution**, allowing unspecialized queries to resolve reliably against complex parameterized types.

---

## 1. Normalization Algorithm Mechanics

`normalize_scope_path` is a pure function in `ctxvault-core` that strips balanced angle brackets and lifetime parameters while strictly preserving the scope hierarchy separator (` > `):

```rust
pub fn normalize_scope_path(scope: &str) -> String {
    let mut result = String::with_capacity(scope.len());
    let mut depth = 0;
    for c in scope.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => result.push(c),
            _ => {}
        }
    }
    result
}
```

### Examples:
* `EarlyBinder<'tcx, T> > instantiate` $\to$ `EarlyBinder > instantiate`
* `HashMap<K, Vec<V>> > insert` $\to$ `HashMap > insert`
* `Option<Arc<dyn Trait>> > unwrap` $\to$ `Option > unwrap`

---

## 2. Two-Stage Fallback in SQLite Catalog

When `get_snippet(qualified_name = "...")` is executed:
1. **Stage 1 (Exact Match)**: Executes an indexed query in SQLite for `qualified_name = ?`.
2. **Stage 2 (Normalized Fallback)**: If Stage 1 returns zero hits, the catalog decomposes the query into scope prefix and symbol leaf:
   ```sql
   SELECT * FROM code_symbols 
   WHERE name = ? 
     AND normalized_scope_path(scope_path) = ?;
   ```
3. **Disambiguation**: If multiple parameterized signatures match (e.g. `Type<T>` vs `Type<T, U>`), `ctxvault` returns an informative candidate list with exact signatures rather than failing.

See [[docs/code-architecture/decisions/adr-016-generic-normalized-scope-resolution]] for the formal decision record.
