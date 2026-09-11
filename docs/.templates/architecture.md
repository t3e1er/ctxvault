---
template:
  name: architecture
  description: "Systems Architecture, Implementation Internals, and Trust Invariants"
  target_dir: "docs/architecture"

schema:
  fields:
    title:
      type: string
      required: true
    description:
      type: string
      required: true
    category:
      type: enum
      required: true
      values: [architecture, implementation, trust]
    status:
      type: enum
      required: true
      values: [active, draft, deprecated]
    tags:
      type: list
      required: true

  edges:
    - field: related
      type: RelatedArchitecture
      class: structural
      direction: outbound
      required: false
      description: "Related architecture, ADR, or concept documents"
    - field: implements
      type: ImplementsSpec
      class: crossmodal
      direction: outbound
      target_kind: code_symbol
      required: false
      description: "Code symbol, port, or adapter implementing this architectural component"

  sections:
    required: ["Overview", "Design & Invariants"]
  min_words: 50
---
# {Architecture Title}

<!--
Guidance:
Document the systems engineering, subsystem design, data flow, memory model, and trust invariants.
-->

## Overview
{High-level architectural purpose, problem domain, and responsibilities}

## Design & Invariants
{Subsystem component interactions, data structures, state machines, and non-negotiable invariants}

## Implementation Details
{Concrete Rust crates, ports, adapters, and performance characteristics}
