## Description
<!-- Provide a brief, high-signal summary of what this pull request accomplishes and the problem it solves. -->

## Greenfield & Architectural Invariants
Please confirm that this pull request complies with `ctxvault` engineering invariants:
- [ ] **Authoritative Ground Truth**: Markdown and source on disk remain authoritative. No index is treated as canonical.
- [ ] **Deterministic Graph Topology**: Graph edges derive strictly from AST relationships, `#tags`, `[[wikilinks]]`, or validated frontmatter — never stochastic LLM extraction.
- [ ] **Progressive Disclosure Contract**: Token contracts (Tier 1 < 150 token handles $\to$ Tier 2 bounded symbol $\to$ Tier 3 line slices) are preserved.
- [ ] **Hexagonal Encapsulation**: Port traits define boundaries. Adapters never leak internal backend types (`rusqlite`, `tantivy`, `hnsw_rs`, `ort`, `petgraph`) across ports.
- [ ] **No Backwards Compatibility / No Dead Code**: No deprecated shims, legacy fallbacks, or unused code branches.
- [ ] **100% Safe Rust**: `#![forbid(unsafe_code)]` holds across all crates. Clippy runs with `-D warnings`.

## Testing & Verification
<!-- Describe the automated tests added or executed to verify correctness. -->
- [ ] `cargo check --workspace --all-features --all-targets`
- [ ] `cargo test --workspace --all-features --locked`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] `cargo fmt --all -- --check`
