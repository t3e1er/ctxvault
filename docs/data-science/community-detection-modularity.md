---
title: "Graph Modularity & Community Detection: Louvain vs Leiden Algorithms"
category: "data-science"
status: "active"
tags: ["data-science", "graph", "petgraph", "modularity", "louvain", "leiden", "algorithms"]
related:
  - "[[docs/data-science/index]]"
  - "[[docs/data-science/decisions/adr-003-leiden-louvain-graph-clustering]]"
---

# Graph Modularity & Community Detection: Louvain vs Leiden Algorithms

In complex software repositories, architecture is defined not by directory hierarchies, but by the **topological clustering of dependencies**. 

`ctxvault` constructs a heterogeneous cross-modal graph linking documentation (`.md`) and code AST symbols (`fn`, `struct`, `trait`, `interface`). To extract architectural subsystems, detect bridge components, and analyze change blast radii, `ctxvault` implements **Newman-Girvan Modularity Optimization**.

---

## 1. Newman-Girvan Modularity ($Q$)

The modularity score $Q$ measures the density of edges inside communities compared to the expected density of edges in a randomized null model:

$$Q = \frac{1}{2m} \sum_{i,j} \left[ A_{ij} - \frac{k_i k_j}{2m} \right] \delta(c_i, c_j)$$

Where:
* $A_{ij}$ is the weight of the edge between nodes $i$ and $j$.
* $k_i = \sum_j A_{ij}$ is the degree (sum of edge weights) incident to node $i$.
* $m = \frac{1}{2} \sum_{i,j} A_{ij}$ is the total weight of all edges in the graph.
* $c_i$ is the community assigned to node $i$.
* $\delta(c_i, c_j)$ is the Kronecker delta ($\delta = 1$ if $c_i = c_j$, else $0$).

A high modularity score ($Q > 0.65$) indicates tightly coupled, highly cohesive functional modules with clean, well-defined inter-module interfaces.

---

## 2. The Louvain Pathological Defect: Disconnected Communities

The classical **Louvain algorithm** operates in two greedy phases:
1. **Local Modularity Optimization**: Each node is moved to the neighboring community that yields the largest modularity gain $\Delta Q$.
2. **Community Aggregation**: Communities are contracted into super-nodes, forming a reduced graph, and Phase 1 repeats iteratively.

```
Pathological Louvain Artifact:
Community C1:
   [Node A] ─── (calls) ───► [Node B]  ... [Node C] ─── (calls) ───► [Node D]
         \                     /                 \                     /
          \─── (weak link) ───/                   \─── (weak link) ───/
```

### The Disconnection Problem
Because Louvain only evaluates total modularity gain $\Delta Q$ across the whole graph, it can aggregate two completely disconnected subgraphs into the **same community** if both subgraphs share weak or indirect affinities with common neighbors. This leads to nonsensical architectural clusters where unrelated modules (e.g. `auth` and `logging`) are reported as a single community.

---

## 3. The Leiden Algorithm: Connectivity Refinement

To guarantee that every architectural cluster is structurally cohesive, `ctxvault` implements the **Leiden Community Detection algorithm**:

```
                              ┌──────────────────────────────────────┐
                              │      Heterogeneous Petgraph Graph    │
                              └──────────────────┬───────────────────┘
                                                 │
                                                 ▼
                              ┌──────────────────────────────────────┐
                              │   Step 1: Local Move of Nodes        │
                              │   (Standard Louvain Greedy Pass)     │
                              └──────────────────┬───────────────────┘
                                                 │
                                                 ▼
                              ┌──────────────────────────────────────┐
                              │   Step 2: Connectivity Refinement     │
                              │   Split internally disconnected      │
                              │   components into sub-communities    │
                              └──────────────────┬───────────────────┘
                                                 │
                                                 ▼
                              ┌──────────────────────────────────────┐
                              │   Step 3: Aggregated Partitioning    │
                              │   Re-contract and recompute Q        │
                              └──────────────────────────────────────┘
```

### Key Guarantees of the Leiden Algorithm in `ctxvault`:
1. **Guaranteed Internal Connectivity**: Every detected community is proven to be internally connected. No disconnected components share a community ID.
2. **Sub-millisecond Execution**: Running over in-memory Petgraph instances, Leiden partitions 20,000+ AST nodes in $<45\text{ms}$.
3. **Bridge Node Identification**: Identifies high-betweenness bridge nodes (e.g., facade routers, API gateways, common utility traits) that connect disparate architectural communities.

See [[docs/data-science/decisions/adr-003-leiden-louvain-graph-clustering]] for the decision rationale.
