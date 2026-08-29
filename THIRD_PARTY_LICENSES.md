# Third-Party Software & Dependency License Notices

This project (`cxtvault` / `ctxvault`) is licensed under the **MIT License**.

The binary distributions include static linkages of third-party open-source libraries. In compliance with open-source license agreements, this document provides the attributions and license terms for all dependencies in the software supply chain.

All dependencies in this codebase are strictly verified via automated CI policy gates (`cargo-deny`) against an enterprise-approved permissive license allow-list.

---

## 1. Supply Chain License Allow-List & Conformance

Every transitive crate in the dependency graph adheres to one or more of the following standard open-source licenses:

- **MIT License**: (e.g., `serde`, `tokio`, `clap`, `fastembed`, `petgraph`, `tracing`, `blake3`, `hnsw_rs`, `pulldown-cmark`)
- **Apache License 2.0 / Apache-2.0 WITH LLVM-exception**: (e.g., `tantivy`, `axum`, `tower`, `rustls`, `tokio-rustls`, `hyper`)
- **BSD 2-Clause / BSD 3-Clause**: (e.g., `bincode`, `notify`)
- **ISC License**: (e.g., `rustls-webpki`)
- **Unicode-3.0 / Unicode-DFS-2016**: (e.g., `unicode-ident`, `unicode-normalization`)
- **Zlib License**: (e.g., `miniz_oxide`, `flate2`)

**Zero Copyleft / Restrictive Licenses**: No GPL, AGPL, LGPL, SSPL, or non-commercial licenses are permitted or included in any dependency.

---

## 2. Core Upstream Components & Attributions

### 2.1 Tantivy
- **Role**: Full-text inverted index engine (Okapi BM25)
- **License**: MIT / Apache-2.0
- **Copyright**: (c) 2016-2024 Paul Masurel and Tantivy Contributors

### 2.2 FastEmbed-rs & ONNX Runtime (`ort`)
- **Role**: Local transformer embedding inference (`BAAI/bge-small-en-v1.5`)
- **License**: Apache-2.0
- **Copyright**: (c) 2023-2024 Anush Shettigar, FastEmbed Contributors

### 2.3 Petgraph
- **Role**: Graph data structure and BFS graph traversal
- **License**: MIT / Apache-2.0
- **Copyright**: (c) 2014-2024 Petgraph Contributors

### 2.4 HNSW-rs
- **Role**: Hierarchical Navigable Small World vector indexing
- **License**: MIT / Apache-2.0
- **Copyright**: (c) 2018-2024 HNSW-rs Contributors

### 2.5 Rusqlite & SQLite3
- **Role**: Metadata, content hash, and schema persistence (bundled C build)
- **License**: MIT (Rusqlite) / Public Domain (SQLite)
- **Copyright**: (c) 2014-2024 The Rusqlite Developers

### 2.6 Tokio & Axum
- **Role**: Async I/O runtime and HTTP MCP transport
- **License**: MIT
- **Copyright**: (c) 2019-2024 Tokio Contributors

---

## 3. Automated Continuous Verification

This repository automatically audits every commit and pull request using:
1. **`cargo-deny check`**: Prevents unauthorized licenses, wildcards, unmaintained crates, and non-crates.io sources.
2. **`cargo-audit`**: Scans the dependency tree against the official [RustSec Advisory Database](https://rustsec.org/) for CVEs.
3. **`unsafe_code = "forbid"`**: Guaranteed 100% safe Rust across all workspace crates.
