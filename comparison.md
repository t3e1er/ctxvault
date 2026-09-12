# MCP Code-Intelligence Comparison: ctxvault vs codebase-memory-mcp

**Evaluation date:** 2026-09-11
**Host:** Windows, 32 GB RAM, kiro-cli
**Test corpus:** `C:\dev\aurius\united\aurius` — polyglot banking codebase (~18,463 files / ~2 GB on disk; dominant: 6,861 `.cpp`, 2,212 `.h`, 690 `.cs`, 684 `.sql`, 411 `.ts`, plus XML/HTML)
**Goal:** Determine which MCP server to plug into the Kiro IDE development pipeline for best effect.

> All figures below are from live measurements taken during this evaluation against the same corpus and the same 5 ground-truth queries. Token counts are estimated as `characters / 4` unless noted; treat them as a consistent relative proxy, not tokenizer-exact values.

---

## 1. Executive Summary

| Dimension | Winner | Margin |
|---|---|---|
| Retrieval accuracy | Tie | Both 5/5 correct top result |
| Token efficiency | **codebase-memory-mcp** | ~7× leaner per query set |
| Warm query latency | **codebase-memory-mcp** | ~0 ms marginal vs ~400 ms |
| Cold start | **ctxvault** | ~1.1 s vs ~5 s |
| Index speed (fast mode) | **ctxvault** | 19 min vs 36 min |
| Code-intelligence tools | **codebase-memory-mcp** | architecture / trace / impact |
| Knowledge authoring | **ctxvault** | notes / ADR vault / templates |
| Native Kiro integration | **codebase-memory-mcp** | auto-config + tier profiles |

**Recommendation:** Use **codebase-memory-mcp** as the primary code-intelligence server in the Kiro pipeline. Keep **ctxvault** as a complementary layer for persistent engineering memory (notes, ADRs) and cross-repository search.

---

## 2. Products Under Test

### ctxvault (`ctxv`)
- **Version:** 0.0.50
- **Language/runtime:** 100% Rust, single native binary + bundled ONNX embedding sidecar. Zero C-runtime deps.
- **Engine:** Tantivy BM25 + ONNX 768-d dense vectors (jina-embeddings-v2-base-code) + Petgraph/SQLite-CTE typed graph, fused with 3-way Reciprocal Rank Fusion.
- **Design thesis:** 3-tier progressive disclosure (search → get_snippet/graph_match → read_file) with inline Turn-1 snippets and graph affordances; markdown/source files are authoritative ground truth, indices are disposable.
- **Install path:** `C:\Users\trent.meier\AppData\Local\Programs\ctxvault\bin\ctxvault.exe`

### codebase-memory-mcp (`cbm`)
- **Version:** 0.6.1 (CLI); MCP server surface reports newer build via npm wrapper
- **Language/runtime:** Pure C, single self-contained native binary. 158–162 vendored tree-sitter grammars compiled in. Bundled `nomic-embed-code` embeddings. Optional Hybrid LSP semantic type resolution.
- **Engine:** SQLite knowledge graph (nodes/edges), FTS5 BM25 (`cbm_camel_split`), semantic vector search, Louvain community detection, Cypher-subset query engine. RAM-first indexing pipeline.
- **Design thesis:** Persistent structural knowledge graph of functions/classes/calls/routes/cross-service links; the agent is the intelligence layer, cbm is the structural backend.
- **Install path:** `C:\Users\trent.meier\AppData\Roaming\npm\node_modules\codebase-memory-mcp\bin\codebase-memory-mcp.exe`

---

## 3. Installation & Configuration

Both were installed and registered with kiro-cli's global MCP config at
`C:\Users\trent.meier\.kiro\settings\mcp.json`:

```json
{
  "mcpServers": {
    "codebase-memory": {
      "command": "C:\\Users\\trent.meier\\AppData\\Roaming\\npm\\codebase-memory-mcp.cmd",
      "env": {}
    },
    "ctxvault": {
      "command": "C:\\Users\\trent.meier\\AppData\\Local\\Programs\\ctxvault\\bin\\ctxvault.exe",
      "args": ["--corpus", "C:\\dev\\aurius\\united\\aurius", "--sync"],
      "env": {}
    }
  }
}
```

**Integration verification (live):** `kiro-cli chat` `/tools` shows both servers loaded and all tools `trusted`, grouped as `codebase-memory (MCP)` and `ctxvault (MCP)`.

| Install aspect | ctxvault | codebase-memory-mcp |
|---|---|---|
| Installer | `install.ps1` (downloads prebuilt binary, SHA-256 verified) | Existing npm global; also `install.ps1` / package managers |
| Auto-detects kiro-cli | ❌ (configured Claude Code, VS Code, Copilot Chat only) | ✅ documented native Kiro support (`$KIRO_HOME/settings/mcp.json` + Scout/Analysis subagent profiles + steering) |
| Manual kiro-cli add needed | Yes (done via `kiro-cli mcp add`) | Optional (also done via `kiro-cli mcp add`) |
| MCP stdio handshake | ✅ responds to `initialize` | ✅ responds to `initialize` |

---

## 4. Indexing (fast mode)

| Metric | ctxvault (`index --fast`) | codebase-memory-mcp (`cli index_repository`) |
|---|---|---|
| Wall time | **1,165 s (~19.4 min)** | **2,168 s (~36 min)** |
| Files processed | 11,250 | 5,118 |
| Graph nodes | 59,320 | 42,472 |
| Graph edges | not surfaced in fast status | 90,637 |
| Vectors computed | **0** (true fast: BM25 + graph only) | 15,412 function embeddings (no fast-skip in 0.6.1) |
| On-disk graph | — | ~92 MB |

**Interpretation.** The index times are *not* apples-to-apples. ctxvault's `--fast` genuinely skips all embedding work (`vector_count: 0`, `embedder_active: false`), giving BM25 + graph in ~19 min. cbm 0.6.1 has **no fast-skip flag** — its RAM-first pipeline always computes semantic vectors, so it does more work (adds semantic search capability) and takes ~36 min. cbm also applies stricter language/ignore filtering (5,118 vs 11,250 files), focusing the graph on real code.

**Correct CLI index forms:**
- ctxvault: `ctxvault index "C:\dev\aurius\united\aurius" --fast`
- cbm (0.6.1): `codebase-memory-mcp cli index_repository "{\"repo_path\":\"C:\\dev\\aurius\\united\\aurius\"}"` (inline JSON positional; the `--repo-path` flag documented in the current README belongs to a newer build)

---

## 5. Retrieval Accuracy

Five queries against confirmed ground-truth symbols. Both servers returned the **correct top result on all five**.

| Query | ctxvault top result | cbm top result | Both correct? |
|---|---|---|---|
| `CommandHandler` | `.../RequestHandlers/CommandHandler.cs` (+ snippet + edge counts) | `AddExternalAccountCommandHandler` class/method nodes (262 matches) | ✅ |
| `RegistrationDataDto` | exact class + source snippet | exact class + file + module nodes | ✅ |
| `two factor authentication request handler` (NL) | `two-factor-authentication.component.ts` class | same TS component, method-level with line numbers (auto BM25) | ✅ |
| `ICurrentPrincipal` | exact interface + full snippet | exact interface, `total:1` (precise) | ✅ |
| `ResponseRedirector` | one impl (DepositsPortal) | **both** impls (SmePortal + DepositsPortal) | ✅ (cbm more complete) |

**Qualitative differences:**
- **ctxvault** answers in a **single Turn-1 call**: file path + inline source snippet + `graph_affordances` (e.g. `defines_in`, `imports_in`, `implements`) + BM25/vector/graph score components.
- **cbm** returns **fine-grained graph nodes** (Class / Method / File / Module) with fully qualified names, line ranges, and in/out degree — but **no source in `search_graph`**; source requires a Turn-2 `get_code_snippet`. cbm disambiguates duplicate symbols better (surfaced both `ResponseRedirector` classes).

---

## 6. Speed / Latency

Measured as process wall time (stdio, local mode), isolating fixed startup from per-query cost.

| Measurement | ctxvault | codebase-memory-mcp |
|---|---|---|
| init only (handshake + load) | ~1,286 ms | ~5,255 ms |
| init + 1 query | ~1,105–1,837 ms | ~4,137–5,499 ms |
| init + 10 queries (same session) | ~4,741 ms | ~5,386 ms |
| **Derived marginal cost / query** | **~400 ms** | **< 10 ms** (10 queries added ~0 wall time) |

**Interpretation.**
- **ctxvault** has a low fixed startup (~1.1 s) but a meaningful per-query cost (BM25 + graph fusion + snippet extraction each call).
- **cbm** has a high fixed startup (~5 s to load the graph into memory) but **essentially free** queries thereafter — 10 queries cost the same wall time as 1, confirming its sub-millisecond graph traversal claim.
- In a real IDE session **both run as persistent daemons**, so startup is a one-time cost. Under that model, cbm's near-zero marginal latency dominates for the many-query agent loop. ctxvault is preferable only for rare one-shot cold invocations.

---

## 7. Token Efficiency

Same 5 queries; response payload sizes (est. tokens = chars/4).

| Query | ctxvault tokens | cbm tokens |
|---|---:|---:|
| CommandHandler | 2,642 | 584 |
| RegistrationDataDto | 2,766 | 310 |
| 2FA handler | 3,338 | 584 |
| ICurrentPrincipal | 3,074 | 115 |
| ResponseRedirector | 2,716 | 446 |
| **Total (5 queries)** | **14,535** | **2,038** |

**cbm is ~7× more token-efficient on this set.**

Additional probes:
- **ctxvault `snippets:0` (lean mode):** still ~2,625 tokens for one query — the verbose JSON envelope (`score_components`, `entity_kind` nesting, `graph_affordances`) dominates, so disabling snippets barely helps.
- **Fair per-task cost** ("find symbol *and* see its code"): cbm `search_graph` (~310) + `get_code_snippet` Turn-2 (~200) = **~510 tokens** vs ctxvault's single Turn-1 **~2,766 tokens**. cbm stays far leaner even after paying for the second call.
- **cbm `get_architecture`:** full structural overview in **~271 tokens** (see §8).
- **cbm `trace_path`:** compact call graph in **~146 tokens**; ctxvault `graph_match` equivalent **~797 tokens** (same call graph, more verbose edge-list).

The token gap is the single most decisive differentiator for an agent pipeline, where every tool response consumes context budget.

---

## 8. MCP Tool Surface & Utility

### ctxvault — 18 tools
`search`, `search_related`, `get_snippet`, `read_file`, `graph_match`, `graph_communities`, `trace_cross_corpus`, `list_corpora`, `sync_corpus`, `index_corpus`, `unload_corpus`, `status`, `list_notes`, `write_note`, `delete_note`, `move_note`, `list_templates`, `validate`

- Retrieval + **knowledge-authoring vault** (notes, ADRs with `derived_from` lineage, schema templates, validation).
- **Multi-corpus** management and **cross-repo** graph traversal (`trace_cross_corpus`).
- Role profiles: `scout` (6) / `analysis` (11) / `all` (18).

### codebase-memory-mcp — 14 tools
`index_repository`, `search_graph`, `query_graph`, `trace_path`, `get_code_snippet`, `get_graph_schema`, `get_architecture`, `search_code`, `list_projects`, `delete_project`, `index_status`, `detect_changes`, `manage_adr`, `ingest_traces`

- **Code-intelligence focused**: architecture overview, Cypher-subset `query_graph`, call-graph `trace_path`, git-diff blast-radius `detect_changes`, grep-augmented `search_code`, runtime trace ingestion.
- Native Kiro tier profiles: Scout (`--tool-profile scout`, 7 tools) / Analysis (`--tool-profile analysis`, 11 tools).

**Live `get_architecture` sample (cbm, ~271 tokens):**
```
total_nodes: 42,472  total_edges: 90,637
Function 12,105 | Class 9,392 | Variable 6,872 | File 5,014 | Module 5,003 |
Method 3,307 | Interface 271 | Enum 58 | Route 19 | Type 11 ...
CALLS 21,744 | DEFINES 37,032 | USAGE 14,783 | SIMILAR_TO 6,104 |
INHERITS 659 | IMPORTS 528 | WRITES 488 ...
```
A single call yields a complete, high-signal orientation of the codebase — including 19 detected HTTP routes and inheritance/import topology.

**Graph accuracy cross-check.** For `onAuthenticationStart`, both engines independently resolved the same call graph (calls → `onAuthenticated` and `sendPushNotification`), confirming deterministic AST-based edges in both.

---

## 9. Strengths & Weaknesses

### ctxvault
**Strengths**
- Fastest cold start (~1.1 s) and fastest true fast-mode index (~19 min, no embeddings).
- Single-call answers with inline snippets + graph affordances (progressive disclosure).
- Knowledge-authoring subsystem: crystallize notes/ADRs as git-trackable markdown with lineage.
- Multi-corpus + cross-repo traversal; role-based tool profiles.
- Pure Rust, `forbid(unsafe)`, zero C deps.

**Weaknesses**
- Verbose JSON envelope → high token cost even in lean mode (~7× cbm here).
- Higher per-query latency (~400 ms) in a long session.
- Installer does not auto-detect kiro-cli.
- Fast mode drops semantic vectors (no NL vector search until a full/skeleton reindex).

### codebase-memory-mcp
**Strengths**
- Dramatically token-efficient (~7× leaner); best fit for context-budgeted agent loops.
- Near-zero warm query latency; scales to many queries per session for free.
- Rich code-intelligence tools: `get_architecture`, `trace_path`, `query_graph` (Cypher), `detect_changes`.
- Native Kiro integration with Scout/Analysis tier profiles.
- Precise symbol disambiguation; 158+ languages; optional Hybrid LSP type resolution.
- Team-shared graph artifact (`.codebase-memory/graph.db.zst`).

**Weaknesses**
- Slow cold start (~5 s graph load) — mitigated by persistent daemon.
- 0.6.1 has no fast-skip flag; full index is slower (~36 min) because it always embeds.
- `search_graph` returns handles only; seeing source needs a Turn-2 `get_code_snippet`.
- 0.6.1 CLI arg form differs from current README (inline JSON vs `--repo-path`).

---

## 10. Recommendation for the Kiro IDE Pipeline

**Primary: codebase-memory-mcp.** For an agentic IDE loop the metrics that matter most are token cost per tool call and warm-query latency — cbm wins both by a wide margin (~7× fewer tokens, ~0 ms marginal). Its `get_architecture`, `trace_path`, `detect_changes`, and Cypher `query_graph` map directly onto everyday dev tasks (orientation, impact analysis, refactoring safety), and it ships purpose-built Kiro integration with tiered tool profiles that limit agent distraction.

**Complementary: ctxvault.** Keep it for what it does uniquely well — persistent, git-trackable engineering memory (design notes, ADRs, crystallized decisions with lineage) and multi-corpus / cross-repository search. It is not the better raw code-lookup engine here, but it is a strong knowledge-substrate layer.

**Suggested configuration**
- Run cbm as the always-on code-intelligence backend; enable auto-index/watch so the graph tracks changes.
- Use cbm Scout profile for cheap discovery, Analysis profile for verification.
- Optionally attach ctxvault scoped to `analysis` or `scout` profile for notes/ADR authoring and cross-repo work, so its heavier retrieval tools don't compete with cbm on routine lookups.

---

## 11. Why codebase-memory-mcp Wins on Token Efficiency (Deep Analysis)

The ~7× token gap measured in §7 is not incidental — it falls out of four deliberate, architecturally different design choices. This section explains each, grounded in both projects' own documentation and the measured payloads from this evaluation.

### 11.1 AST parsing strategy: cAST chunks vs. a normalized symbol graph

Both tools parse with Tree-sitter, but they persist and *return* the parse differently, and that difference is the root cause of the token gap.

**ctxvault — cAST (chunk-oriented AST).** ctxvault uses "cAST" (chunk-aware AST) chunking. Tree-sitter splits each source file into semantically coherent chunks (a function body, a struct, a module preamble), and each chunk becomes a first-class retrieval unit indexed three ways at once: Tantivy BM25, a 768-dim ONNX dense vector (jina-embeddings-v2-base-code), and a Petgraph node. A search hit is therefore a *chunk* carrying: the chunk text/snippet, an `entity_kind` (language + `scope_path` + `signature` + `symbol_type`), per-modality `score_components` (bm25, vector, graph_boost, graph_hops), and `graph_affordances` (edge counts). That is a rich, self-describing record — and a verbose one. In the measured output a single `RegistrationDataDto` hit carried the class snippet *plus* the scoring envelope *plus* the affordance block, so even `snippets:0` still cost ~2,625 tokens because the envelope, not the snippet, dominates.

**cbm — normalized knowledge graph with thin nodes.** cbm's Tree-sitter pass extracts *definitions, calls, and imports* and normalizes them into typed graph nodes (`Function`, `Class`, `Method`, `Interface`, `Route`, `Module`, `File`, …) and typed edges (`CALLS`, `DEFINES`, `IMPLEMENTS`, `INHERITS`, `IMPORTS`, `USAGE`, …), persisted in SQLite. A `search_graph` hit is a thin handle: `name`, `qualified_name`, `label`, `file_path`, line range, and in/out degree — and *nothing else by default*. Source text is not attached; it is fetched on demand via `get_code_snippet`. In the measured output the `ICurrentPrincipal` result was ~115 tokens because it is literally a 5-field row.

**Consequence.** ctxvault front-loads a heavy, multi-signal record per hit (good for one-shot answers, expensive per token). cbm returns a minimal pointer and lets the agent decide whether to spend tokens on the body. Across the 5-query set that design difference alone accounts for most of the 14,535 vs 2,038 token gap. cbm's Hybrid LSP layer (type-aware call resolution for ~12 languages) further lets it keep nodes thin while still resolving edges accurately — the accuracy lives in the *edges*, not in fat payloads.

### 11.2 Indexing modes: where the work (and the payload weight) is placed

| | ctxvault | codebase-memory-mcp (0.6.1) |
|---|---|---|
| Index modes | `full`, `skeleton`, `docs-embed`, `fast` (explicit flags) | single RAM-first pipeline; "Best/Fast" refers to the *artifact* zstd tier, not a query-payload mode |
| Fast mode meaning | Skip ONNX + vector index entirely (BM25 + graph only) | No query-payload skip; always builds graph (+ semantic vectors in 0.6.1) |
| Effect on retrieval payload | Payload shape is unchanged by mode — the verbose JSON envelope ships regardless | Payload is always the thin graph handle; body is a separate tool call |

ctxvault's mode system optimizes *index build cost and index footprint* (skeleton embeds only signatures; fast drops vectors — confirmed in our run: `vector_count: 0`). It does **not** slim the response envelope: fast-mode search still returns `score_components`, `entity_kind`, and `graph_affordances`. So indexing mode is the wrong lever for token cost in ctxvault — the cost is in the response schema, which is mode-invariant.

cbm has no equivalent "lean/verbose" query mode because it does not need one: the default response *is* lean (handles), and the README documents an additional CLI-side lean/compact-tree renderer plus explicit pagination (`result_limit`/`result_offset`, `max_output_tokens`) and semantic truncation that drops ranked rows before diagnostic rows. Token frugality is the default posture, not an opt-in.

### 11.3 Progressive disclosure: both do it, but at different granularity

Both tools are explicitly built on tiered/progressive disclosure — the philosophies are near-identical on paper, but the *unit of the first turn* differs.

**ctxvault's 3 tiers** (from its own docs):
- Tier 1 `search` — "broad sweep + instant answers", target budget **300–800 tokens**, returns partitioned docs/code hits, **inline snippets for the top K (default 3)**, graph affordance counts, and a schema envelope.
- Tier 2 `get_snippet` / `graph_match` — bounded symbol or multi-hop path, 150–500 tokens.
- Tier 3 `read_file` — bounded line slices, emergency fallback.

**cbm's tiers** (Scout / Verify / Auditor profiles + tool layering):
- Discovery `search_graph` / `get_architecture` — thin handles, structural counts.
- Targeted `get_code_snippet` / `trace_path` — exact body or call chain by qualified name.
- Exhaustive — read flagged ranges / `query_graph` Cypher.

The critical difference is what Turn 1 *contains*. ctxvault deliberately **inlines source snippets in Turn 1** — its thesis is "answer in one round-trip, save follow-up calls." That trades tokens for round-trips: fewer calls, but each call is heavy. Its own target of 300–800 tokens/turn was, in practice on this corpus, exceeded (~2,600–3,300 tokens/query) because the C#/TS chunks plus the scoring/affordance envelope are large.

cbm's Turn 1 is a **handle list with no bodies**. It costs more round-trips when you actually need source (search → snippet), but each turn is tiny, and for the very common "where is X / what calls X / what's the shape of this repo" questions you never fetch a body at all. In an agent loop dominated by navigation and orientation (not verbatim reading), cbm's Turn-1-as-index model wins decisively on cumulative tokens.

**Measured illustration (task = "find RegistrationDataDto and read it"):**
- ctxvault: 1 turn, ~2,766 tokens (snippet inlined).
- cbm: 2 turns, ~510 tokens (310 handle + 200 body).

Even paying for the extra turn, cbm used ~5.4× fewer tokens because its Turn 1 was ~15× lighter.

### 11.4 Graph strategy and what is returned on which turn

Both back their graphs with Tree-sitter-derived, deterministic edges and both run sub-millisecond traversals (ctxvault: recursive SQLite CTEs with cycle guards + in-memory Petgraph; cbm: SQLite graph store + Louvain communities). Accuracy was identical in our call-graph cross-check (§8). The token difference again comes from *turn placement of information*:

| Turn | ctxvault | codebase-memory-mcp |
|---|---|---|
| **Turn 1 (`search` / `search_graph`)** | Snippet **+** `graph_affordances` degree counts (`calls_in/out`, `implements`, `imports`) **+** `score_components` **+** schema envelope, per hit | Thin handle only: name, qualified_name, label, file, line, in/out degree |
| **Turn 2 (graph)** | `graph_match` returns an **edge list** (source/target/direction) — ~797 tokens in our test | `trace_path` returns **directional callees/callers with hop distance** — ~146 tokens in our test |
| **Body** | `get_snippet` (already often unnecessary — snippet was in Turn 1) | `get_code_snippet` by qualified_name (~200 tokens) |
| **Architecture** | `graph_communities` (Leiden/Louvain subsystem grouping) | `get_architecture` — full node/edge histogram in **~271 tokens** |

Two structural wins for cbm's graph strategy:

1. **Affordances are opt-in, not always-on.** ctxvault attaches degree counts to *every* Turn-1 hit whether or not the agent needs them — useful situational awareness, but a fixed per-hit tax. cbm exposes the same information (`in_degree`/`out_degree` on the node, richer via `trace_path`) but only pays for it when asked. For a keyword lookup that doesn't care about topology, cbm ships nothing extra.

2. **`get_architecture` is a single cheap orientation call.** cbm can answer "what is this codebase" — languages, 12,105 Functions / 9,392 Classes / 271 Interfaces / 19 Routes, 21,744 CALLS edges, inheritance/import topology — in one ~271-token call. ctxvault's nearest equivalent, `graph_communities`, returns subsystem clusters (also useful) but there is no single equivalently-cheap "whole-graph census." For the first move of almost any dev task (orient in the repo), cbm gives more structural signal per token.

**`trace_path` vs `graph_match` shape.** cbm's `trace_path` returns a purpose-built answer object (`callees: [...]`, `callers: [...]`, each with `hop`), which is exactly what an impact-analysis question needs and nothing more. ctxvault's `graph_match` returns a general edge list (every `{source, target, direction, edge_type}`), which is more composable but more verbose to express the same "who calls X" answer. Purpose-built beats general-purpose on tokens for the common case.

### 11.5 Summary of the causal chain

1. **Parse/persist model:** cbm normalizes to thin typed graph nodes; ctxvault keeps fat multi-signal cAST chunks → cbm hits are ~5–20× smaller.
2. **Response schema:** cbm defaults to handles + on-demand bodies + pagination; ctxvault always ships snippet + score envelope + affordances → the envelope, not the snippet, dominates ctxvault's cost and is mode-invariant.
3. **Progressive disclosure granularity:** cbm's Turn 1 is an index (no bodies); ctxvault's Turn 1 is an answer (bodies inlined) → cbm trades cheap extra round-trips for a much lighter Turn 1.
4. **Graph info placement:** cbm makes topology/affordances opt-in and offers a single ultra-cheap `get_architecture` census; ctxvault makes affordances an always-on per-hit tax.

Net: for an agent loop dominated by search, navigation, and orientation, cbm's "thin handles, fetch-on-demand, opt-in topology" model is structurally more token-efficient than ctxvault's "rich, self-describing, answer-in-one-turn" model — which is why the ~7× gap appears even though retrieval accuracy and raw graph speed are comparable. The corollary: ctxvault's design is better when you genuinely want the answer *and its evidence* in a single round-trip and are willing to pay tokens for fewer calls (e.g. a human-facing one-shot query, or a knowledge-authoring workflow), which is consistent with its positioning as a memory/vault layer rather than a high-frequency code-navigation backend.

---

## 12. Re-Evaluation: Each Tool at Its Philosophically-Optimal Strategy

The evaluation in §5–§11 mapped both tools onto the *same* query shapes. That is fair for a controlled comparison but under-serves each tool's design intent: ctxvault is built to answer in **one heavy Turn 1** (and I originally used `snippets:0`, which keeps the full envelope, rather than its true lean `detail:"ids"` handle mode); cbm is built for **many cheap turns** with purpose-built tools. This section re-runs search with each tool in its optimal posture and asks whether the outcome changes.

### 12.1 Newly-used optimal knobs

- **ctxvault**: `search` supports `detail:"ids"` ("bare handles: path/qualified_name + line range + metadata, no snippet"), `modality:"code"`, and `limit`. This is ctxvault's real lean posture — distinct from `snippets:0`, which still ships the scoring/affordance envelope. It also has `mode` (`hybrid|bm25|semantic|graph|explain`) and `decompose` for multi-hop.
- **cbm**: `search_graph` with a tight `limit`, plus purpose-built `trace_path` and `get_architecture`.

### 12.2 Task-level results (each tool at optimal)

| Task | ctxvault (optimal) | cbm (optimal) | Winner |
|---|---|---|---|
| Lean symbol lookup | `search detail=ids limit=5 modality=code` → **1,146 tok** (default was 2,766) | `search_graph limit=3` → **310 tok** | cbm 3.7× leaner |
| Find **+ read** a symbol | `search snippets=1 limit=3 modality=code` → **894 tok / 1 turn** | `search_graph limit=1` + `get_code_snippet` → **320 tok / 2 turns** | cbm 2.8× leaner tokens; ctxvault 1 turn vs 2 |
| Impact / call graph | `graph_match` → **693 tok / 1 turn** | `trace_path` → **172 tok / 1 turn** | cbm 4× leaner |
| Repo orientation | `status` → **114 tok** (shallow) *or* `graph_communities(architecture)` → **~1.6M tok (explodes)** | `get_architecture` → **338 tok** (full node/edge histogram + routes) | cbm wins the useful middle |

### 12.3 What changed vs. the original evaluation

1. **ctxvault is materially better than the §7 defaults suggested, once tuned.** `detail:"ids"` + `modality:"code"` + bounded `limit` cuts payloads ~2.4× (2,766 → 1,146), and `snippets:1` answers find-and-read in a **single** round-trip. The headline "~7× / 14,535 vs 2,038" figure reflected ctxvault's *default verbosity*, not its optimal configuration. Tuned per-task, the gap narrows to **~2.8–4×**.

2. **A new ctxvault weakness surfaced at scale.** `graph_communities` — its orientation tool — is deep Leiden/Louvain clustering, not a cheap census. On this 59,320-node graph it returned **~1.6M tokens** (unusable in an agent loop). ctxvault's only cheap orientation is `status` (~114 tok), which is far shallower than cbm's `get_architecture`. So for "orient me in this repo," ctxvault offers either too-shallow or too-heavy, with no cheap middle.

### 12.4 Does it change the outcome?

**No — but it sharpens the reasoning and narrows the margin.**

- Even with each tool playing to its strength, **cbm stays 2.8–4× more token-efficient per task.** The difference is structural: ctxvault's per-hit `entity_kind` + `graph_affordances` + `score` envelope persists *even in `detail:"ids"` mode*, so tuning cannot close the gap — only shrink it.
- The dimension where ctxvault's philosophy genuinely wins is **round-trips**: find-and-read in 1 turn vs cbm's 2. If the cost model weights round-trip count / latency over tokens (slow model, human-in-the-loop, or rate-limited tool calls), ctxvault's one-shot design has real, measurable value.
- cbm's advantage **widens** on graph and orientation tasks (`trace_path`, `get_architecture` are purpose-built and cheap), and ctxvault's orientation tooling is actually a liability at this repo scale.

**Revised nuance to the recommendation:** cbm remains the primary code-intelligence backend for a token-budgeted agent loop. But the honest correction is that **ctxvault, configured with `detail:"ids"` and bounded `limit`/`snippets`, is a far more competitive token citizen than its defaults imply** — and its single-turn answer model is a legitimate advantage wherever round-trip count matters more than raw token volume. A team that adopts ctxvault should set those lean parameters explicitly rather than relying on the verbose defaults measured in §7.

---

## 13. Critique & Improvement Speculation (Tough but Fair)

This section reads each tool's own documentation and observed behavior, then speculates on efficacy and concrete improvements across four axes: **turn systems, graph parsing, search modes, and representation/return values.** Claims about design intent are drawn from each project's docs; claims about behavior are from the measurements in §4–§12. Speculative improvements are labeled as such.

### 13.1 codebase-memory-mcp

**AST parsing modes — what it does.** cbm runs a two-layer pipeline: a Tree-sitter syntactic pass over 158–162 vendored grammars, then a **Hybrid LSP** semantic pass (a C reimplementation of language-server type resolution) for ~12 languages that refines `CALLS`/`CALL_REFERENCE`/`USAGE`. Its own v0.3.0 benchmark is refreshingly honest: 91.8% aggregate over 35 languages, 17 at 100%, but Haskell at 62% and OCaml at 72%, with documented root causes.

**Tough critique (graph parsing):**
- **`properties=null` is the most common deduction in its own benchmark.** Parameter types and return types are not extracted for most languages. For an IDE pipeline this is a real gap: "what's the signature of X" — one of the most frequent dev questions — is exactly where a graph should shine, and cbm punts to a snippet fetch.
- **Paradigm-specific call blindness.** Haskell function composition (`f . g . h`) forms no `CALLS` edges; OCaml functor indirection defeats call resolution; Scala/Ruby block-based and abstract-method calls trace to zero. These aren't bugs so much as the limit of AST-plus-lightweight-LSP without a real type system — but they mean `trace_path` silently under-reports on functional and highly-dynamic code, and a zero-result trace is indistinguishable from "genuinely no callers."
- **The precision/recall trade is made globally, not surfaced per query.** The PHP-LSP notes show cbm suppresses name-fallback edges when the receiver is an un-indexed vendor type (−43% CALLS edges, higher precision, lower recall). That is the right call for correctness, but the agent is never told "this edge was suppressed for safety" — so recall loss is invisible at query time.
- **`query_graph` Cypher subset has sharp edges:** no `IS NOT NULL`, a silent 200-row aggregation cap that undercounts on large graphs. A silent cap is the dangerous kind — an agent doing dead-code analysis on Django could conclude "42 uncalled functions" when the real number is truncated.

**Turn system critique.** cbm's thin-handle-then-fetch model is token-optimal (§11) but pays in round-trips: find-and-read is always 2 turns, and `search_graph` returning Class+Method+File+Module nodes for one symbol (the `CommandHandler` result had 262 rows with type-duplication) can force the agent to disambiguate across near-identical rows. The node-type fan-out is noise for "where is X."

**Search modes critique.** cbm's semantic search exists but is gated behind full/moderate index mode; in the common `fast`/default posture the agent leans on `name_pattern` regex and BM25. That is powerful for known identifiers but weak for the "where do we handle DB failover?" intent query — the exact synonym-blindspot ctxvault's theory names. cbm has the machinery (nomic embeddings, `SEMANTICALLY_RELATED`) but its default doesn't always light it up.

**Representation/return critique.** Returning fully-qualified names as the join key is correct and stable, but the qualified names are extremely long (`C-dev-aurius-united-aurius.United.CoreInternetWebAPI.InternetBankingAPI.Services.Services.Signup.RegistrationDataDto.RegistrationDataDto`) and repeat the class segment twice. In a 5-result response that repetition is a measurable, avoidable token tax — ironic for the token-efficiency winner.

**Speculative improvements (cbm):**
1. **Populate `properties` at least for the ~12 Hybrid-LSP languages.** The type resolver already computes parameter/return types to resolve calls; persisting them on the node would kill the single most common PARTIAL and remove a whole class of Turn-2 snippet fetches.
2. **Emit suppressed-edge counts as metadata.** When name-fallback edges are blocked, return `suppressed_low_confidence: N` on the trace so the agent knows recall was traded, rather than seeing a clean-looking zero.
3. **Make the aggregation cap explicit.** Replace the silent 200-row `query_graph` cap with `has_more`/`total` (the tool already does this for `search_graph` — extend it to Cypher aggregates).
4. **Collapse node-type fan-out in `search_graph` by default.** Return one ranked symbol row with a `kinds:[Class,Method,File]` field instead of 3–4 near-duplicate rows; expose the fan-out only behind a flag.
5. **Prefix-compress qualified names in multi-row responses** (the README already hints at a `<section>_refs` prefix directory for CLI — apply it to MCP responses too).

### 13.2 ctxvault

**AST parsing modes — what it does.** ctxvault uses cAST (chunk-aware AST) Tree-sitter chunking across ~16 languages, producing tri-indexed chunks (BM25 + 768-dim ONNX vector + Petgraph node). Its index modes (`full`/`skeleton`/`docs-embed`/`fast`) are a genuine strength the benchmark-style tools lack: `skeleton` (embed signatures only) is a clever middle ground, and `fast` cleanly drops the vector tier.

**Tough critique (graph parsing):**
- **Far narrower language coverage.** ~16 languages vs cbm's 158+. For the aurius corpus (C++, C#, SQL, TS) it was fine, but a polyglot shop with Kotlin/Swift/Rust/Go microservices will hit chunks that index lexically but carry weaker graph edges. ctxvault publishes no per-language accuracy tiering, so a user cannot know *which* languages have trustworthy `calls`/`implements` edges — cbm's brutal honesty here (Haskell 62%) is something ctxvault should emulate.
- **No type-aware call resolution layer.** ctxvault's edges come from Tree-sitter AST relationships directly; there is no equivalent to cbm's Hybrid LSP. For overloaded methods, interface dispatch, and generic instantiation, AST-only resolution will over- or under-connect. The docs assert deterministic edges (true) but determinism is not accuracy — a deterministically wrong edge is still wrong.
- **`graph_communities` does not scale gracefully.** On the 59,320-node graph it returned ~1.6M tokens (§12) — a Leiden/Louvain partition dumped wholesale. This is a real defect: the one tool meant for macro-orientation is unusable on exactly the large repos where orientation matters most.

**Turn system critique.** The 3-tier model is elegant and the Turn-1-answers-most-questions thesis is sound for human-facing use. But the stated **300–800 token Turn-1 budget was exceeded 3–4× on real C#/TS chunks** (§7: ~2,600–3,300 tokens). The budget is aspirational, not enforced. An agent that trusts the "one round-trip" promise will burn context faster than the docs imply.

**Search modes critique — this is ctxvault's strongest area.** The 5-mode design (`hybrid`/`bm25`/`semantic`/`graph`/`explain`) plus `modality` and `decompose` is more expressive than cbm's, and the 4-modality RRF theory correctly names the three single-modality failure modes. `explain` mode (per-modality rank breakdown) is a genuinely great affordance cbm has no answer to. The gap is not capability but **default discipline**: `hybrid` + `snippets:3` is the token-heavy default, and the lean `detail:"ids"` mode is undocumented enough that my own first evaluation missed it.

**Representation/return critique — this is ctxvault's weakest area and the root of its token disadvantage.** Every hit ships `entity_kind` (nested), `score_components` (bm25/vector/graph_boost/graph_hops — four floats even when vector=0 in fast mode), `graph_affordances`, and a `score`. The verbosity is *mode-invariant*: `snippets:0` and even `detail:"ids"` still carry most of the envelope. Returning `vector: 0.0` and `graph_boost: 0.0` on every hit in fast mode is pure waste — those fields are structurally zero and known to be zero.

**Speculative improvements (ctxvault):**
1. **Make the return envelope mode-aware.** In `fast` mode, drop `vector`/`graph_boost`/`graph_hops` (structurally zero) and collapse `score_components` to the one live signal. This alone would likely halve fast-mode payloads and is the single highest-leverage token fix.
2. **Add a truly lean handle mode** that returns only `{path, symbol, line_range, score}` — cbm-parity handles — for wide sweeps, distinct from today's `detail:"ids"` which still carries `entity_kind`/`affordances`.
3. **Paginate/summarize `graph_communities`.** Return top-N communities with sizes + representative nodes and a continuation cursor, never the full partition. Fixes the 1.6M-token blow-up.
4. **Publish per-language edge-accuracy tiers** (à la cbm's benchmark) so users know where `calls`/`implements` can be trusted.
5. **Enforce (or report) the Turn-1 token budget.** If a Turn-1 response exceeds 800 tokens, either auto-degrade `snippets` or return a `budget_exceeded` hint so the agent can re-query lean — turning the aspirational budget into a real contract.
6. **Consider a lightweight type-resolution pass** for its top languages to close the AST-only accuracy gap, mirroring cbm's Hybrid LSP bet.

### 13.3 Cross-cutting: what each should learn from the other

| Axis | cbm should adopt from ctxvault | ctxvault should adopt from cbm |
|---|---|---|
| Turn system | An optional "inline snippet on Turn 1" flag for one-shot find-and-read (save the 2nd round-trip) | Honest enforcement of the Turn-1 token budget; a genuinely lean handle mode |
| Graph parsing | Index modes (`skeleton`/`fast`) to control build cost and payload weight | A type-resolution layer (Hybrid-LSP-style) and per-language accuracy tiers |
| Search modes | An `explain`-style per-signal rank breakdown for debugging retrieval | Default-lean discipline + documented `detail` mode; always-on cheap `get_architecture`-style census |
| Representation | Populate `properties` (signatures) so the handle is self-sufficient | Mode-aware envelopes; drop structurally-zero fields; prefix-compress |

**Fair verdict.** Neither tool is close to its own ceiling. cbm's efficiency win is real but rests on an AST/LSP graph that under-reports on functional/dynamic code and hides its own recall trades; its return values are lean but its qualified-name representation is wasteful. ctxvault's retrieval *theory* and *search-mode surface* are the more sophisticated of the two, and its index-mode system is genuinely ahead — but its return-value engineering is undisciplined, its orientation tool doesn't scale, and its narrow language set is unquantified. The token gap measured in §7 is a representation-engineering gap, not a retrieval-quality gap — which means it is the *most fixable* difference between them, and the tool that ships mode-aware lean envelopes first will erase most of the margin this evaluation found.

---

## Appendix A — Reproduction

- Eval harnesses (kept): `C:\Users\trent.meier\mcp_eval\eval_ctxvault.ps1`, `C:\Users\trent.meier\mcp_eval\eval_cbm.ps1`
- Ground-truth query set: `CommandHandler`, `RegistrationDataDto`, `two factor authentication request handler`, `ICurrentPrincipal`, `ResponseRedirector`
- ctxvault query: `search { query, snippets:3 }` via stdio `tools/call`
- cbm query: `search_graph { project, name_pattern|query, limit:5 }` via stdio `tools/call`
- Latency method: batch N JSON-RPC messages over one stdio session, measure wall time; derive marginal cost from (10-query − 1-query) / 9.
- Token proxy: response `characters / 4`.

## Appendix B — Caveats

- Token counts are a chars/4 proxy, not tokenizer-exact.
- Index-time comparison is not apples-to-apples: ctxvault fast mode skips embeddings; cbm 0.6.1 does not.
- Latencies measured with fresh process spawns per batch; persistent-daemon operation (the real IDE case) amortizes the fixed startup for both.
- Results are specific to this corpus (large C++/C# polyglot) on this host; a smaller or single-language repo may shift the balance.
