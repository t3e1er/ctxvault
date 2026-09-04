---
title: "Hexagonal Ports and Adapters Architecture & Dependency Injection"
category: "code-architecture"
status: "active"
tags: ["hexagonal", "ports-and-adapters", "dependency-injection", "architecture", "rust"]
related:
  - "[[docs/code-architecture/index]]"
  - "[[docs/code-architecture/pure-rust-invariants]]"
  - "[[docs/code-architecture/decisions/adr-007-hexagonal-ports-adapters-isolation]]"
---

# Hexagonal Ports and Adapters Architecture & Dependency Injection

To ensure that the retrieval domain remains completely decoupled from underlying storage engines, `ctxvault` is strictly organized under a **Hexagonal Ports-and-Adapters Architecture**.

---

## 1. The Port Trait Abstraction Barrier

Every major infrastructure capability is defined as a pure Rust trait (**Port**) inside `ctxvault-common::ports`. Concrete infrastructure implementations (**Adapters**) live inside `ctxvault-core`:

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                               ctxvault-common (Domain Core)                           │
│                                                                                        │
│   trait MetadataCatalog  •  trait TextIndex  •  trait VectorStore                      │
│   trait GraphStore       •  trait EmbeddingProvider  •  trait SearchService            │
└───────────────────────────────────────────▲────────────────────────────────────────────┘
                                            │ Implements Traits (Zero Type Leakage)
┌───────────────────────────────────────────┴────────────────────────────────────────────┐
│                               ctxvault-core (Adapters)                                 │
│                                                                                        │
│   [SqliteCatalog]  ──► implements MetadataCatalog (rusqlite hidden)                    │
│   [TantivyIndex]   ──► implements TextIndex (tantivy::* hidden)                        │
│   [HnswStore]      ──► implements VectorStore (hnsw_rs::* hidden)                      │
│   [PetgraphStore]  ──► implements GraphStore (petgraph::* hidden)                      │
│   [OrtEmbedder]    ──► implements EmbeddingProvider (ort::* hidden)                    │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### The Strict Non-Leakage Rule
**Adapters never leak their internal backend types across a port trait.**
* `rusqlite::Connection` stays strictly inside `SqliteCatalog`.
* `tantivy::IndexReader` stays strictly inside `TantivyIndex`.
* `ort::Session` stays strictly inside `OrtEmbedder`.

Port method signatures accept and return only standard library types or domain types defined in `ctxvault-common`.

---

## 2. Dependency Injection & The Composition Root

`ctxvault` avoids runtime DI container frameworks. Instead, dependency injection is performed via **monomorphized compile-time generics** on hot paths, and constructor injection at a single **Composition Root**:

```
Composition Root: `crates/ctxvault-cli/src/main.rs`
Constructs concrete adapters:
   let catalog = Arc::new(SqliteCatalog::open(&db_path)?);
   let text_index = Arc::new(TantivyIndex::open(&tantivy_path)?);
   let vector_store = Arc::new(HnswStore::open(&vector_path)?);
   let graph_store = Arc::new(PetgraphStore::open(&graph_path)?);
   let embedder = Arc::new(OrtEmbedder::new(&model_path)?);

Injects into EngineBuilder:
   let engine = EngineBuilder::new()
       .with_catalog(catalog)
       .with_text_index(text_index)
       .with_vector_store(vector_store)
       .with_graph_store(graph_store)
       .with_embedder(embedder)
       .build()?;
```

No concrete adapter is ever constructed inside `ctxvault-mcp` or `ctxvault-core::engine`.

See [[docs/code-architecture/decisions/adr-007-hexagonal-ports-adapters-isolation]] for the decision rationale.
