---
name: cxtvault-curate
description: >-
  Create, update, move, and formally validate markdown notes against corpus schemas and taxonomies.
  Use this skill when authoring Architecture Decision Records (ADRs), creating technical documentation,
  updating frontmatter metadata, or verifying corpus structural integrity.
---

# Cxtvault Knowledge Curation & Schema Validation

This skill guides agents through drafting, updating, and formally validating notes and taxonomy hierarchies in `cxtvault` knowledge bases.

---

## 1. Creating Notes from Templates

When authoring a new note (e.g. ADR, Incident Report, Architecture Concept):

1. **List Available Templates**:
   Call `list_templates` to discover the schema definitions configured in the corpus `.templates/` directory:
   ```json
   {}
   ```
2. **Inspect Template Requirements**:
   Note the required frontmatter fields (e.g. `title`, `status`, `date`), optional fields, and required section headers.
3. **Create the Note**:
   Call `create_note` with the target path, frontmatter attributes, and markdown body:
   ```json
   {
     "path": "decisions/adr-002-tantivy-bm25.md",
     "template": "decision_record",
     "frontmatter": {
       "title": "ADR 002: Tantivy Inverted Index for BM25 Retrieval",
       "status": "accepted",
       "date": "2026-08-30",
       "tags": ["architecture", "search", "bm25"]
     },
     "content": "# ADR 002: Tantivy Inverted Index\n\n## Context\n...\n\n## Decision\n...\n\n## Consequences\n..."
   }
   ```
4. **Validate Immediate Conformance**:
   Call `validate_note` on the newly created path to ensure zero schema errors:
   ```json
   {
     "path": "decisions/adr-002-tantivy-bm25.md"
   }
   ```

---

## 2. Updating and Moving Notes

When modifying existing documentation:

1. **Read Current Content or Frontmatter**:
   Call `read_note` or `get_frontmatter` to inspect current document contents.
2. **Apply Content Updates**:
   Call `update_note` with the updated content string and mode (`overwrite`, `append`, or `prepend`):
   ```json
   {
     "path": "decisions/adr-002-tantivy-bm25.md",
     "mode": "append",
     "content": "\n\n## Implementation Notes\nValidated with sub-2ms query latency."
   }
   ```
3. **Move / Rename Notes**:
   Call `move_note` to rename notes while automatically rewriting inbound wikilinks across other notes:
   ```json
   {
     "from": "decisions/adr-002-tantivy-bm25.md",
     "to": "decisions/adr-002-tantivy-index.md"
   }
   ```
4. **Re-Validate**:
   Call `validate_note` to confirm that changes satisfy template constraints.

---

## 3. Vault-Wide Structural Audits

To audit the health and consistency of the entire knowledge base:

1. **Validate All Notes**:
   Call `validate_corpus` to check for missing required frontmatter, invalid enum values, broken links, or empty sections across all notes.
2. **Validate Tag & Category Taxonomy**:
   Call `validate_taxonomy` to identify orphan tags, inconsistent casing, or misspelled categories.
