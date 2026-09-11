---
template:
  name: guide
  description: "Technical Guide and How-To Documentation"
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
      type: string
      required: true
    status:
      type: enum
      required: true
      values: [active, draft, deprecated]
    tags:
      type: list
      required: true
    difficulty:
      type: enum
      required: false
      values: [beginner, intermediate, advanced]

  edges:
    - field: related
      type: RelatedGuide
      class: structural
      direction: outbound
      required: false
      description: "Related documentation guides"

  sections:
    required: ["Overview", "Prerequisites", "Step-by-Step Instructions"]
  min_words: 40
---
# {Guide Title}

<!--
Guidance:
Explain the procedure clearly with actionable steps, code snippets, and verification commands.
-->

## Overview
{What this guide covers and the end goal}

## Prerequisites
{Required tools, dependencies, or environment configurations}

## Step-by-Step Instructions
{Sequential instructions with code snippets}
