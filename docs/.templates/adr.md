---
template:
  name: adr
  description: "Architecture Decision Record"
  target_dir: "docs/architecture/adr"

schema:
  fields:
    title:
      type: string
      required: true
    category:
      type: enum
      required: true
      values: [adr, architecture, data-science, agentic-strategy, code-architecture, mcp-modes, gpu-optimization]
    status:
      type: enum
      required: true
      values: [proposed, accepted, rejected, deprecated, superseded]
    tags:
      type: list
      required: true
    date:
      type: date
      required: false
    deciders:
      type: list
      required: false

  edges:
    - field: supersedes
      type: Supersedes
      class: structural
      direction: outbound
      bidirectional: false
      target_template: adr
      required: false
      description: "Previous decision superseded by this one"
    - field: implements
      type: ImplementsSpec
      class: crossmodal
      direction: outbound
      target_kind: code_symbol
      required: false
      description: "Code symbol or spec implemented by this ADR"
    - field: related
      type: RelatedNote
      class: structural
      direction: outbound
      required: false
      description: "Related documentation or ADRs"

  sections:
    required: ["Context", "Decision", "Consequences"]
  min_words: 50
---
# ADR-{id}: {Title}

<!--
Guidance:
Explain the architectural context, forces at play, options evaluated, and decision rationale.
-->

## Context
{Describe the architectural context, problem statement, forces, and constraints}

## Decision
{State the decision clearly and outline the architectural mechanism and technical design}

## Consequences
{Document positive, negative, and neutral trade-offs, along with any technical debt incurred}
