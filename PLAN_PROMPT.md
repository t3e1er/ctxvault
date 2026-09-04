# ctxvault — Multi-Corpus, Cross-Modal, Progressive-Disclosure Upgrade

## How to run this plan

You are the **coordinator** for a large, single-branch feature effort in the ctxvault
Rust workspace. The full work is broken into ordered phases in `todo.txt` (same
directory). Execute the phases **in order**. Do not skip ahead; later phases assume
earlier ones landed.

### Operating loop (per phase)

For each phase in `todo.txt`, top to bottom:

1. **Read** the phase block in `todo.txt` (goal, scope, files, acceptance criteria).
2. **Delegate implementation to a sub-agent** (`general-task-execution`). Give it:
   - the exact phase goal and acceptance criteria copied from `todo.txt`,
   - the specific files/modules it may touch,
   - the constraint that it must NOT start the next phase,
   - the instruction to run `just fmt` then `just clippy` on what it wrote and fix
     warnings before reporting back (clippy is `-D warnings`),
   - the instruction to report: what changed (file list), how it satisfied each
     acceptance criterion, and any deviations.
   Use context-gatherer first only if the sub-agent needs code understanding beyond
   what the phase block already points at.
3. **Check the sub-agent's work as coordinator.** Do NOT trust the report blindly:
   - read the diff (`git diff --stat` then targeted `git diff` / file reads),
   - confirm each acceptance criterion is actually met in code,
   - run `just clippy` yourself; run `just check`,
   - run any per-phase verification command listed in the phase block.
   If checks fail, send it back to a sub-agent with the specific gap. Do not proceed
   until the phase is genuinely complete. If an approach fails twice, diagnose the
   root cause and change approach rather than patching incrementally.
4. **Commit on the current branch** (`feature/codebase-semantic-indexing`) once the
   phase passes checks:
   - stage only the files that belong to this phase (avoid `git add -A`),
   - commit message format: `feat(phaseN): <short phase title>` with a body listing
     the acceptance criteria satisfied,
   - do NOT push, do NOT amend, do NOT bypass hooks. Commit only when the phase is
     verified.
5. **Mark the phase complete in `todo.txt`** by changing its leading `[ ]` to `[x]`.
6. Move to the next phase.

### Global rules

- **One branch:** all work lands on `feature/codebase-semantic-indexing`. Never push
  to `master`. Never force-push.
- **No backwards compatibility. No dead code. No tech debt.** This is a greenfield
  project — there are no external callers to preserve. Do NOT add optional shims,
  compatibility aliases, deprecated tool names, or "keep old behavior when arg omitted"
  fallbacks. When a phase changes a shape, REPLACE the old shape and DELETE the code it
  supersedes (including the currently-unwired single-corpus transport/dispatch paths
  once multi-corpus is the one true path). New args should be required where that yields
  the cleanest design; defaults are for ergonomics, not for preserving legacy behavior.
  Leave no unused functions, structs, fields, or branches behind — if clippy or a grep
  shows something is dead after a phase, remove it in that phase.
- **Model:** run each implementation sub-agent under the user's preferred model
  (configured at the Kiro agent/model level — this prompt cannot pin it). The
  coordinator should confirm the intended model is selected before delegating.
- **Files are ground truth; indices are rebuildable.** Never make an index canonical.
- **Edge types are data, not code** — declared per corpus in `corpus.toml`. Do not
  hardcode edge types.
- **Verification standard (from tech steering):** after each phase `just fmt` →
  `just clippy` (must pass `-D warnings`) → `just check`. The FULL `just test` suite
  runs once at the very end (final phase), plus any targeted tests a phase adds.
- **Keep the tool-surface test in sync.** The registry has an expected-tools test;
  update it whenever tools are added, removed, renamed, or consolidated.
- **MSRV 1.80, edition 2021, `unsafe_code = forbid`.** No new `unsafe`. Add doc
  comments to public items (`missing_docs = warn`).
- **Token-footprint discipline:** the whole point of several phases is to REDUCE the
  `tools/list` payload and per-result token cost. Do not regress it.
- **Do not create tests unless a phase asks for them**, but always run existing tests
  to verify. The final phase runs the whole suite.

### Reporting

After each phase, post a short coordinator summary: phase name, what landed, the
commit hash, checks run and their result, and confirmation the `todo.txt` box is
ticked. Keep it to a few sentences.

## Architecture decisions locked in (do not relitigate)

1. **Hybrid corpus model.** Multi-corpus at the *index-root* level (one central MCP
   serves N roots via `CorpusManager`), AND *modality as a logical filter within each
   corpus* (`modality: docs|code|both`). `corpus` selects which root(s); `modality`
   selects docs vs code vs both. Both are agent-facing args.
2. **Corpus discrimination + cross-corpus edge/symbol linking.** Tools take an optional
   `corpus`; a cross-corpus / `all` mode fans out and RRF-merges, tagging each hit with
   its source corpus. Where a doc `implements`/`documents` a code symbol, preserve the
   cross-modal edge inside a corpus, and support cross-corpus symbol linking where
   resolvable.
3. **Bi-modal search accepted** — `modality` arg on all search tools, threaded into
   BM25 (indexed field), vector (filter / sub-index), and graph (node-kind filter).
4. **Progressive disclosure — adopt AND adapt the codebase-memory-mcp pattern for BOTH
   code and markdown docs.** Three tiers:
   - Tier 1 (handles): search returns IDs/paths/qualified_names + line ranges +
     metadata, no bodies. `detail=ids|default` controls verbosity.
   - Tier 2 (chunk/symbol fetch): a fetch tool returns just the matched doc chunk or
     code symbol source, bounded (line cap), with optional neighbor expansion.
   - Tier 3 (full file): whole-file read, only when needed.
   Tool descriptions must encode the ordering so agents self-enforce it.
5. **Consolidation for parity in a condensed footprint.** Collapse the `search_*`
   family into one `search` tool with a `mode` param; fold status tools into one;
   introduce tool **profiles** (`scout`/`analysis`/`all`) to shrink the always-sent
   schema payload. Pull in parity items still missing vs codebase-memory (Leiden,
   import-resolution confidence, coverage check, batch read) where they fit the
   condensed surface.

## Reference material (read-only repos, do NOT modify)

- `/home/trent/dev/semantic-pages` — TS predecessor (markdown vault MCP). Parity
  reference for doc-side tools and behaviors (move-note wikilink rewrite, batch read).
- `/home/trent/dev/codebase-memory-mcp` — C code-graph MCP. Parity reference for
  progressive disclosure (search→handle→get_code_snippet), tool profiles, Leiden
  clustering, import-resolution confidence, coverage check.

Key ctxvault files (baseline):
- `crates/ctxvault-core/src/corpus_manager.rs` — multi-corpus manager (built, unwired).
- `crates/ctxvault-core/src/engine.rs` — single-corpus Engine; docs/code index split.
- `crates/ctxvault-core/src/search/mod.rs` — BM25/vector/graph/hybrid/RRF.
- `crates/ctxvault-common/src/types.rs` — `SearchResult`, `EntityKind`, `Chunk`.
- `crates/ctxvault-common/src/config.rs` — `CorpusConfig`, edge-type config.
- `crates/ctxvault-mcp/src/tools/mod.rs` — tool registry (source of truth, 33 tools).
- `crates/ctxvault-mcp/src/transport/{dispatch,http,stdio,mod}.rs` — single + multi
  transport paths (multi exists, unwired from CLI).
- `crates/ctxvault-cli/src/main.rs` — CLI; currently wires a single Engine only.
