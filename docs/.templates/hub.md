---
template:
  name: hub
  description: "Directory Hub and Knowledge Navigation Index"
  target_dir: "docs"

schema:
  fields:
    title:
      type: string
      required: true
    description:
      type: string
      required: false
    category:
      type: enum
      required: true
      values: [root, architecture, concepts, roadmap, adr, building, implementation, trust, progressive-disclosure, search]
    status:
      type: enum
      required: true
      values: [active, draft]
    tags:
      type: list
      required: true

  edges:
    - field: related
      type: NavigatesTo
      class: structural
      direction: outbound
      required: false
      description: "Child subdirectories or companion hub documents"

  sections:
    required: ["Overview"]
  min_words: 30
---
# {Hub Title}

<!--
Guidance:
Provide navigation, pillar hierarchy, and directory index links.
-->

## Overview
{Purpose of this directory pillar and architectural scope}

## Directory Contents
{Structured list and descriptions of submodules, guides, or specifications}
