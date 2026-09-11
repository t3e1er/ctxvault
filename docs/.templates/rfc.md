---
template:
  name: rfc
  description: "Request for Comments Technical Specification"
  target_dir: "docs/roadmap"

schema:
  fields:
    title:
      type: string
      required: true
    category:
      type: string
      required: true
    status:
      type: enum
      required: true
      values: [draft, proposed, accepted, implemented, rejected, superseded]
    tags:
      type: list
      required: true
    author:
      type: string
      required: false
    date:
      type: date
      required: false
    scope:
      type: string
      required: false
    target_version:
      type: string
      required: false

  edges:
    - field: implements
      type: ImplementsSpec
      class: crossmodal
      direction: outbound
      target_kind: code_symbol
      required: false
      description: "Code symbol or port implementing this RFC"
    - field: related
      type: RelatedRfc
      class: structural
      direction: outbound
      target_template: rfc
      required: false
      description: "Related or prerequisite RFC"

  sections:
    required: ["Executive Summary", "Architectural Design", "Verification Plan"]
  min_words: 60
---
# RFC: {Title}

<!--
Guidance:
Provide a comprehensive architectural and technical specification for major system features.
-->

## Executive Summary
{Brief problem statement, motivation, and solution overview}

## Architectural Design
{Technical design, component interactions, and data structures}

## Verification Plan
{Automated testing and manual validation strategy}
