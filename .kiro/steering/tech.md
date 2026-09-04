---
inclusion: always
---

# ctxvault — Tech & Conventions

## Language & Toolchain

- 100% pure Rust, edition 2021, **MSRV 1.80** (`rust-toolchain.toml` pins the toolchain).
- Cargo workspace with `resolver = "2"`; four member crates (see structure steering).
- `unsafe_code = "forbid"` workspace-wide. Never introduce `unsafe`.
- `missing_docs = "warn"` — add doc comments to public items.
- Zero C runtime deps: pure Rust TLS (`rustls-tls`), bundled SQLite (`rusqlite` `bundled`).

## Core Dependencies (workspace)

- Async: `tokio` (full), `async-trait`, `futures-util`, `crossbeam-channel`
- Search: `tantivy` (BM25) · `hnsw_rs` (vector ANN) · `ort` + `tokenizers` (ONNX embeddings)
- Graph: `petgraph` (serde-1) serialized via `postcard` (1.1)
- Storage: `rusqlite` (bundled, backup) — metadata catalog, WAL mode
- Markdown: `pulldown-cmark`; frontmatter via `serde_yaml`
- File watching: `notify` v8 + `notify-debouncer-full`; locking via `fs4`
- Hashing: `blake3` (content-hash change detection)
- CLI: `clap` (derive) · HTTP: `axum` + `tower` + `tower-http` + `reqwest`
- Config: `toml` + `serde`; errors: `thiserror` (library) + `anyhow`; logging: `tracing`

Pin exact/compatible versions in `[workspace.dependencies]`. Prefer well-known, maintained crates. Do not add crates with GPL/copyleft licenses — `cargo-deny` enforces MIT/Apache-2.0 compliance.

## Developer Workflow

Use `just` recipes (`cargo install just`); they mirror CI exactly.

| Task | Command |
|---|---|
| Fast type-check | `just check` → `cargo check --workspace --all-features --all-targets` |
| Lint | `just clippy` → `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |
| Format check | `just fmt-check` · Auto-format: `just fmt` |
| Test | `just test` → `cargo test --workspace --all-features --locked` |
| Run one test | `just test-one NAME` |
| Release build | `just build-release` → `target/release/ctxvault` |
| License/vuln scan | `just deny` (`cargo-deny`) · `just audit` (`cargo-audit`) |
| Docs | `just docs` |
| Run CLI | `just run -- <ARGS>` |
| Full CI locally | `just ci` (fmt-check + clippy + test + deny + docs) |

Run these directly (never in watch mode). Use `cargo test ... -- --run`-style single execution; avoid `cargo watch`/`--watch` in agent sessions.

## Verification Standard

After any code change:
1. `just fmt` then `just clippy` — clippy must pass with `-D warnings`.
2. `just test` — the suite (156+ unit/integration/e2e tests) must pass.
3. For dependency changes, run `just deny`.

A command exiting 0 is not proof of correctness — confirm the change satisfies the actual requirement.

## Clippy Policy

Enabled (warn): `correctness`, `suspicious`, `perf`. Allowed: `complexity`, `style`, `pedantic`, `nursery`. Do not silence lints with blanket `#[allow]`; fix the root cause or scope allows narrowly with justification.

## Testing Conventions

- Unit tests live inline in `#[cfg(test)] mod tests`. Integration/e2e tests live in each crate's `tests/`.
- Use `tempfile` / `assert_fs` for filesystem fixtures; `predicates` for assertions.
- Do NOT add tests unless the task requires them, but always run existing tests to verify changes.

## Git

- MIT licensed. Commit only when asked. Push to feature branches, never directly to `master`.
- A `pre-push` git hook exists (`.githooks/pre-push`); enable with `just setup-hooks`. Preserve hooks — do not bypass with `--no-verify` unless explicitly asked.
