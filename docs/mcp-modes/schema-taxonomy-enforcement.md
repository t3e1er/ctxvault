---
title: "Schema & Taxonomy Enforcement: Templates, Validation & Auditing"
category: "mcp-modes"
status: "active"
tags: ["templates", "validation", "taxonomy", "frontmatter", "auditing"]
related:
  - "[[docs/mcp-modes/index]]"
  - "[[docs/agentic-strategy/knowledge-crystallization]]"
---

# Schema & Taxonomy Enforcement: Templates, Validation & Auditing

Knowledge vaults deteriorate over time without strict structural discipline. Broken links multiply, tag vocabularies fragment with inconsistent casing, and required metadata fields are omitted.

`ctxvault` provides built-in **Template Enforcement, Frontmatter Validation, and Taxonomy Auditing**.

---

## 1. Declarative Corpus Templates (`.templates/`)

Each corpus defines declarative templates in TOML format within its `.templates/` directory:

```toml
# .templates/decision_record.toml
name = "decision_record"
description = "Architecture Decision Record (ADR)"
required_frontmatter = ["title", "status", "date", "tags"]
required_sections = ["Context", "Decision", "Consequences"]

[frontmatter_enums]
status = ["proposed", "accepted", "superseded", "rejected"]
```

When an agent invokes `create_note(template="decision_record", ...)`:
* Missing required frontmatter fields or sections reject the mutation immediately.
* Invalid enum values (e.g. `status = "maybe"`) trigger validation errors before disk writes.

---

## 2. Vault-Wide Structural Auditing Tools

`ctxvault` exposes 4 dedicated validation tools:

```
┌──────────────────────────────┬────────────────────────────────────────────────────────────────────────┐
│ Validation Tool              │ Verification Scope                                                     │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ `validate_note`              │ Verifies a single note against its declared template and checks wikilinks.│
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ `validate_corpus`            │ Audits the entire corpus for broken wikilinks, missing required        │
│                              │ frontmatter, empty sections, and orphan notes.                         │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ `list_templates`             │ Lists available template schemas, required fields, and section headers. │
├──────────────────────────────┼────────────────────────────────────────────────────────────────────────┤
│ `validate_taxonomy`          │ Checks for orphan tags, low-frequency typos, inconsistent casing,     │
│                              │ and category hierarchies.                                              │
└──────────────────────────────┴────────────────────────────────────────────────────────────────────────┘
```

By scheduling `validate_corpus` and `validate_taxonomy` during continuous integration, teams maintain high-integrity, machine-queryable knowledge bases.
