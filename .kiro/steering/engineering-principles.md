# ctxvault — Engineering Principles (Greenfield Discipline)

ctxvault is a greenfield project with no external consumers to protect. Optimize for a
clean, minimal, cohesive codebase over compatibility with any prior shape.

## No backwards compatibility

- Do NOT add compatibility shims, deprecated tool names, aliased handlers, or
  "preserve old behavior when the argument is omitted" fallbacks.
- When you change a type, tool, arg, or on-disk/index layout, REPLACE the old shape
  outright. There is no migration burden — indices are derived and rebuildable, and
  there are no published APIs.
- Defaults exist for ergonomics (a sensible value when an arg is omitted), never to
  emulate a legacy code path.

## No dead code, no tech debt

- Every function, struct, field, enum variant, and branch must be reachable and used.
  If a change makes something unused, delete it in the same change.
- After a change, dead symbols must not remain. Use clippy (`dead_code`,
  `unused`) and grep to confirm removed symbols have no lingering references.
- Do not leave TODO stubs, commented-out code, or "temporary" duplicate paths. If two
  code paths do the same job, collapse them to one.
- Prefer deleting code to guarding it. Fewer, clearer paths beat configurable legacy.

## Consequences for reviewers / agents

- A phase or PR that adds an alias "for now", keeps an unwired old path "just in case",
  or leaves an unused helper is INCOMPLETE. Finish the deletion.
- Clippy runs with `-D warnings`; unused-code warnings are hard failures, not noise to
  silence with `#[allow(dead_code)]`. Remove the code instead.
