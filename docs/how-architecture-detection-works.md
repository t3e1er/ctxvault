---
title: How Architecture Detection & Louvain Clustering Works
tags: [architecture, louvain, graph, modularity, impact]
status: active
---

# How Architecture Detection & Louvain Clustering Works

Understanding high-level architecture in a complex repository requires more than viewing a flat file tree.

`ctxvault` implements the **Louvain Community Detection algorithm** on top of the Petgraph cross-modal knowledge graph to automatically discover module clusters, bridge components, and compute change blast radii.

See also: [[docs/index]], [[what-is-ctxvault]], [[how-cast-chunking-works]].

---

## 🔬 The Louvain Modularity Algorithm

The Louvain algorithm groups nodes to maximize the **network modularity score ($Q$)**:

$$Q = \frac{1}{2m} \sum_{i,j} \left[ A_{ij} - \frac{k_i k_j}{2m} \right] \delta(c_i, c_j)$$

Where:
* $A_{ij}$ is the edge weight between nodes $i$ and $j$.
* $k_i, k_j$ are the sum of weights attached to nodes $i$ and $j$.
* $m$ is the total weight of all edges in the graph.
* $\delta(c_i, c_j) = 1$ if nodes $i$ and $j$ belong to the same community, $0$ otherwise.

A high modularity score ($Q > 0.7$) indicates tightly-knit internal cohesive modules with clean, well-defined cross-module interfaces.

---

## 📊 Cross-Modal Clustering in Action

When `get_architecture` is called, the algorithm partitions the knowledge graph:
1. **Hub Communities**: Connects architecture documents (ADRs) to the subsystems they define.
2. **Language/Service Communities**: Groups classes, methods, and functions with their defining files and internal call chains.
3. **Bridge Node Identification**: Flags nodes with high betweenness centrality (e.g. API clients, gateway routers, search engines) that bridge different subsystems.

---

## ⚡ Filesystem Delta Scanning & Blast Radius (`detect_changes`)

When a developer modifies or deletes a file:
1. `detect_changes` performs a fast timestamp and SHA-256 hash comparison across the filesystem.
2. It queries SQLite and Petgraph to find all **impacted symbols** and downstream dependent files.
3. It returns an immediate blast-radius report before tests are run or code is committed.
