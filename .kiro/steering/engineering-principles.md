# ctxvault — Engineering Principles (Greenfield Discipline)

ctxvault is a greenfield project with no external consumers to protect. Optimize for a
clean, minimal, cohesive codebase over compatibility with any prior shape.

## No backwards compatibility

- Do NOT add compatibility shims, deprecated tool names, aliased handlers, or
  "preserve old behavior when the argument is omitted" fallbacks.
- When you change a type, tool, arg, or on-disk/index layout, REPLACE the old shape
  outright. There is no migration burden — indices are derived and rebuildable, and
  there are no published APIs.
- Defaults exist for ergonomics (a sensible value when an arg is omitted), never to
  emulate a legacy code path.

## No dead code, no tech debt

- Every function, struct, field, enum variant, and branch must be reachable and used.
  If a change makes something unused, delete it in the same change.
- After a change, dead symbols must not remain. Use clippy (`dead_code`,
  `unused`) and grep to confirm removed symbols have no lingering references.
- Do not leave TODO stubs, commented-out code, or "temporary" duplicate paths. If two
  code paths do the same job, collapse them to one.
- Prefer deleting code to guarding it. Fewer, clearer paths beat configurable legacy.

## Consequences for reviewers / agents

- A phase or PR that adds an alias "for now", keeps an unwired old path "just in case",
  or leaves an unused helper is INCOMPLETE. Finish the deletion.
- Clippy runs with `-D warnings`; unused-code warnings are hard failures, not noise to
  silence with `#[allow(dead_code)]`. Remove the code instead.

## Ports & adapters (hexagonal architecture)

ctxvault is organized as ports-and-adapters. Every major concern is a trait (a
**port**); the concrete backend that satisfies it is an **adapter**. This keeps the
domain decoupled from infrastructure and makes new backends additive rather than
invasive.

- The major ports are: `MetadataCatalog` (SQLite metadata/catalog), `TextIndex`
  (Tantivy BM25), `VectorStore` (HNSW vectors), `GraphStore` (Petgraph graph),
  `EmbeddingProvider` (ONNX embedder), and `SearchService` (search-mode dispatch +
  RRF fusion). Adapters are the concrete implementors (e.g. Tantivy `TextIndex`,
  HNSW `VectorStore`).
- **Adapters never leak their backend types across a port.** Concrete infrastructure
  types (`rusqlite::Connection`, `tantivy::*`, `hnsw_rs::*`, `petgraph::*`, `ort::*`)
  stay encapsulated inside the adapter. Port signatures use domain types from
  `ctxvault-common` only. If a signature would force a heavy infra crate onto a
  consumer, the abstraction is wrong.
- **The domain depends on ports, not adapters.** `Engine` holds ports; it does not
  own concrete backends and does not hand them out via accessors. `ctxvault-mcp`
  depends on ports + `SearchService` + domain types, never on concrete core internals.
- **Composition root is the only place adapters are named.** `ctxvault-cli` (`main.rs`)
  constructs the concrete adapters and injects them via the engine builder /
  `CorpusManager`. Construction is injection, not scattered `new()`/`open()` calls
  reaching down through layers.

### Rust DI policy

- Prefer **generics with trait bounds** for the stable hot path — monomorphized,
  zero-cost. Use **trait objects** (`Arc<dyn _>` / `Box<dyn _>`) only where a runtime
  swap is genuinely the point (plugin-style seams). Do not reach for a DI-container
  crate; Rust's type system is the container.
- Wire dependencies once, at the composition root, via **constructor injection**.
- New backends (SQLite-backed graph, Postgres/S3, managed vector stores, dynamic
  language packs, pipeline-stage plugins) arrive as **new adapters behind existing
  ports** — never by editing the domain or widening a port to leak a backend type.

### Consequences for reviewers / agents

- A change that makes `ctxvault-mcp` (or any consumer) name a concrete backend type,
  or that adds a domain accessor handing out a concrete backend, breaks the
  architecture and is INCOMPLETE — route it through a port instead.
- A change that constructs a concrete adapter anywhere other than the composition
  root has leaked wiring into the domain — move it to the root.
