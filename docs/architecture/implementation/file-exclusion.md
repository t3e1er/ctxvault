---
title: "File Exclusion & Gitignore Pattern Engine"
description: "Centralized, multi-layered file discovery filtering for indexing mode using gitignore-compatible pattern matching."
category: "implementation"
status: "active"
tags: ["indexing", "discovery", "exclude", "gitignore", "cbmignore", "ctxvaultignore"]
related:
  - "[[docs/architecture/implementation/index]]"
  - "[[docs/architecture/implementation/hexagonal-architecture]]"
---

# File Exclusion & Gitignore Pattern Engine

During repository and knowledge base indexing, ingesting test suites, build outputs, mock data, or vendor packages leads to index bloat, wasted embedding compute, and polluted search results.

`ctxvault` incorporates a centralized, multi-layered file exclusion engine that filters files and directories at **discovery time** before any files are read, parsed, or embedded.

* **Configuration**: [`crates/ctxvault-common/src/config.rs`](file:///c:/dev/semantic/ctxvault/crates/ctxvault-common/src/config.rs)
* **Matcher Engine**: [`crates/ctxvault-core/src/index/exclude.rs`](file:///c:/dev/semantic/ctxvault/crates/ctxvault-core/src/index/exclude.rs)
* **Discovery Walker**: [`crates/ctxvault-core/src/engine.rs`](file:///c:/dev/semantic/ctxvault/crates/ctxvault-core/src/engine.rs)
* **Continuous File Watcher**: [`crates/ctxvault-core/src/watcher/mod.rs`](file:///c:/dev/semantic/ctxvault/crates/ctxvault-core/src/watcher/mod.rs)

---

## 1. Filter Hierarchy & Precedence

Discovery evaluates paths through a fixed multi-layered priority hierarchy:

1. **Non-Negatable Safety Core**:
   Directories that can never be crawled under any circumstances to prevent infinite recursion, catalog corruption, or thousands of external packages:
   - `.git`
   - `.index`
   - `node_modules`
2. **Base Indexing Patterns**:
   Configured in `CorpusConfig.exclude.patterns`. By default, this excludes:
   - **VCS & Tool metadata**: `.git`, `.svn`, `.hg`, `.index/`, `.fastembed_cache/`
   - **Dependencies**: `node_modules/`, `vendor/`, `Pods/`
   - **Build artifacts**: `target/`, `dist/`, `build/`, `out/`, `bin/`, `obj/`
   - **Virtualenvs & caches**: `.venv/`, `venv/`, `env/`, `__pycache__/`, `.cache/`, `.next/`, `.nuxt/`, `.turbo/`
   - **Test suites & fixtures**: `tests/`, `test/`, `__tests__/`, `fixtures/`, `testdata/`, `spec/`, `specs/`, `*.test.*`, `*.spec.*`, `*_test.go`, `*_test.py`
   - **Binaries & compiled objects**: `*.exe`, `*.dll`, `*.so`, `*.dylib`, `*.bin`, `*.wasm`, `*.pyc`, `*.o`, `*.a`, `*.db`, `*.sqlite`
   - **Archives**: `*.zip`, `*.tar`, `*.gz`, `*.bz2`, `*.xz`, `*.7z`
3. **Repository `.gitignore`**:
   Automatically read from `<corpus_root>/.gitignore` and `<corpus_root>/.git/info/exclude` when `use_gitignore = true` (default).
4. **Dedicated Project Ignore File (`.ctxvaultignore`)**:
   Project-specific ignore file at `<corpus_root>/.ctxvaultignore` (or `.cbmignore`) when `use_ctxvaultignore = true` (default).
5. **Additional Patterns**:
   Project-specific rules defined in `corpus.toml` via `additional_patterns`.

---

## 2. Configuration Schema (`corpus.toml`)

The exclude policy lives in the central `CorpusConfig`:

```toml
name = "my-project"
path = "."
index_mode = "full"

[exclude]
use_gitignore = true
use_ctxvaultignore = true
patterns = [
    # Override default pattern list if needed
]
additional_patterns = [
    # Append custom rules or un-ignore specific paths
    "!tests/e2e/",
    "generated_docs/"
]
```

### Negation (`!`) Support

Standard gitignore negation semantics are supported. For example, to index a specific test folder while keeping the rest of the test suite excluded:

```gitignore
# in .ctxvaultignore or additional_patterns
!tests/e2e/
```

---

## 3. Subtree Pruning & Uniform Enforcement

- **Discovery Traversal**: `walk_dir_recursive` tests directory paths against `ExcludeMatcher::is_excluded(&path, true)` and prunes whole subtrees immediately, avoiding traversal into `node_modules`, `target`, or `tests`.
- **Live Watcher**: `CorpusWatcher` and `classify_event_with_matcher` apply the same matcher to incoming notify events so changes inside excluded directories or test files never trigger unnecessary delta re-indexing.
- **Wikilink Rewriting**: `walk_markdown_files_for_rewrite` honors the same matcher during note moves and renames.
