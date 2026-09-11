---
template:
  name: concept
  description: "Theoretical Paradigms, Retrieval Algorithms, and Agent Interaction Models"
  target_dir: "docs/concepts"

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
      values: [progressive-disclosure, search, concepts, agentic-strategy, data-science]
    status:
      type: enum
      required: true
      values: [active, draft, deprecated]
    tags:
      type: list
      required: true

  edges:
    - field: related
      type: RelatedConcept
      class: structural
      direction: outbound
      required: false
      description: "Related concepts, architecture specs, or ADRs"
    - field: applies_to
      type: AppliesTo
      class: crossmodal
      direction: outbound
      target_kind: code_symbol
      required: false
      description: "Code symbol or engine pipeline component embodying this concept"

  sections:
    required: ["Overview", "Theoretical Foundation"]
  min_words: 50
---
# {Concept Title}

<!--
Guidance:
Explain theoretical principles, mathematical models, agent behaviors, or algorithmic trade-offs.
-->

## Overview
{Conceptual definition, core thesis, and problem statement}

## Theoretical Foundation
{Mathematical foundations, formulas, algorithmic mechanics, and formal reasoning}

## Practical Agent Implications
{How this concept guides agent context consumption, token budgeting, or retrieval workflows}
