---
name: cxtvault-ops
description: >-
  Manage corpus lifecycle, trigger incremental delta syncing, perform full re-indexing,
  and inspect graph topological metrics and community clusters using cxtvault MCP tools.
  Use this skill when synchronizing index state after filesystem changes or analyzing vault connectivity statistics.
---

# Cxtvault Operations, Index Lifecycle & Graph Topology

This skill guides agents through operational maintenance of `cxtvault` knowledge bases, including delta synchronization, index rebuilds, embedding refreshes, and topological community analysis.

---

## 1. Corpus State & Delta Synchronization

When files in the knowledge vault have been edited or added externally:

1. **List Configured Corpora**:
   Call `corpus_list` to inspect active corpora, their base paths, document count, and index status:
   ```json
   {}
   ```
2. **Execute Incremental Delta Scan**:
   Call `sync_corpus` to index newly added or modified markdown files and prune deleted notes:
   ```json
   {}
   ```
3. **Full Re-Index (Cold Rebuild)**:
   When modifying tokenization rules or chunking strategies in `corpus.toml`, call `reindex_corpus`:
   ```json
   {}
   ```
4. **Re-Embed Corpus**:
   When changing embedding models in `corpus.toml` (e.g., from `all-MiniLM-L6-v2` to `bge-small-en-v1.5`), call `reembed_corpus`:
   ```json
   {}
   ```

---

## 2. Graph Analytics & Community Detection

To understand the macro-structure of the knowledge graph:

1. **Inspect Graph Statistics**:
   Call `graph_stats` to review total nodes, edge density, average degree, connected components, and isolated orphan notes:
   ```json
   {}
   ```
2. **Identify Topological Communities**:
   Call `graph_communities` to detect clusters of closely linked notes using modularity/community detection algorithms:
   ```json
   {
     "min_cluster_size": 3
   }
   ```
3. **Audit Structural Health & Taxonomy**:
   Call `validate_taxonomy` to check for broken wikilinks, circular dependencies, and orphan ADRs:
   ```json
   {
     "check_broken": true,
     "check_cycles": true,
     "check_orphans": true
   }
   ```
4. **Generate Coverage Report**:
   Call `coverage_report` to see embedding coverage, template conformance percentage, and link connectivity metrics across the entire corpus.
