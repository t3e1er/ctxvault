---
template:
  name: roadmap
  description: "Long-Term Technical Roadmap and Phased Milestone Tracker"
  target_dir: "docs/roadmap"

schema:
  fields:
    title:
      type: string
      required: true
    description:
      type: string
      required: false
    category:
      type: string
      required: true
    status:
      type: enum
      required: true
      values: [active, proposed, completed, superseded]
    tags:
      type: list
      required: true

  edges:
    - field: related
      type: RelatedRoadmap
      class: structural
      direction: outbound
      required: false
      description: "Related RFCs, ADRs, or milestone documents"

  sections:
    required: ["Executive Summary & Vision"]
  min_words: 60
---
# {Roadmap Title}

<!--
Guidance:
Outline long-term strategic vision, technical roadmap milestones, research foundations, and delivery phases.
-->

## Executive Summary & Vision
{Strategic goal, scope, and technical vision}

## Architectural Pillars
{Key technical disciplines and structural paradigms}

## Phased Implementation Plan
{Milestones, deliverables, risk mitigation, and verification criteria}
