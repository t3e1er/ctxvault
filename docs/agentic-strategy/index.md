---
title: "Agentic Strategy & Swarm Memory Gateway"
category: "agentic-strategy"
status: "active"
tags: ["agentic-strategy", "progressive-disclosure", "swarms", "crystallization", "mcp"]
related:
  - "[[docs/index]]"
  - "[[docs/agentic-strategy/knowledge-crystallization]]"
  - "[[docs/agentic-strategy/progressive-disclosure]]"
  - "[[docs/agentic-strategy/swarm-topologies]]"
  - "[[docs/agentic-strategy/tool-profiling]]"
  - "[[docs/agentic-strategy/decisions/adr-004-progressive-disclosure-token-contract]]"
  - "[[docs/agentic-strategy/decisions/adr-005-deterministic-vs-llm-graph-extraction]]"
  - "[[docs/agentic-strategy/decisions/adr-006-role-based-tool-profiling]]"
---

# Agentic Strategy & Swarm Memory Hub

Welcome to the **Agentic Strategy & Swarm Memory** module of `ctxvault`. This cluster details how AI agent swarms leverage ctxvault as a persistent, high-density, low-latency external memory substrate.

---

## 1. The Multi-Agent Semantic Substrate

```
                            ┌────────────────────────────────────────┐
                            │        Specialized Agent Swarms        │
                            │  [Scout]  [Reader]  [Writer]  [Cryst]  │
                            └───────────────────┬────────────────────┘
                                                │
                                    MCP Protocol (Stdio / HTTP)
                                                │
                                                ▼
                            ┌────────────────────────────────────────┐
                            │    3-Tier Progressive Disclosure       │
                            │  Tier 1: <150 token handles (detail=ids│
                            │  Tier 2: Single AST symbol snippet     │
                            │  Tier 3: Exhaustive whole-file read    │
                            └───────────────────┬────────────────────┘
                                                │
                                                ▼
                            ┌────────────────────────────────────────┐
                            │    Continuous Knowledge Crystallization│
                            │   (Karpathy LLM Wiki Architecture)     │
                            │  Episodic Exhaust ──► Schema Note      │
                            │  Lineage Provenance (DerivedFrom)      │
                            └────────────────────────────────────────┘
```

---

## 2. Core Theoretical Articles

1. **[[docs/agentic-strategy/knowledge-crystallization]]**
   * *Karpathy's LLM Wiki & Principle 3*: Transforming ephemeral, high-entropy conversational traces, terminal transcripts, and debugging resolutions into structured, permanent markdown assets with verified provenance (`promote_concept`, `traverse_lineage`).
2. **[[docs/agentic-strategy/progressive-disclosure]]**
   * *Eliminating Context Window Pollution*: The formal 3-tier retrieval contract. Why dumping raw files causes model confusion and latency bloat, and how lightweight handles (<150 tokens) keep agent reasoning sharp.
3. **[[docs/agentic-strategy/swarm-topologies]]**
   * *Specialized Swarm Roles*: Division of labor between Scout, Reader, Writer, and Crystallizer agents, preventing lock contention and role confusion.
4. **[[docs/agentic-strategy/tool-profiling]]**
   * *Tool Exposure Gating*: The `--profile` flag (`scout` $\subset$ `analysis` $\subset$ `all`) reducing advertised JSON schemas and protecting against accidental mutating operations.

---

## 3. Architectural Decision Records (ADRs)

* **[[docs/agentic-strategy/decisions/adr-004-progressive-disclosure-token-contract]]**: Enforcing the strict Tier-1 contract by stripping lineage graphs and score components from `detail="ids"` to drop token consumption from 5,900 to <150 tokens.
* **[[docs/agentic-strategy/decisions/adr-005-deterministic-vs-llm-graph-extraction]]**: Architectural decision to construct knowledge graphs 100% deterministically from code ASTs and wikilinks rather than using stochastic LLM entity extractors.
* **[[docs/agentic-strategy/decisions/adr-006-role-based-tool-profiling]]**: Gating tool namespace visibility via role-based profiles to prevent LLM prompt dilution.
