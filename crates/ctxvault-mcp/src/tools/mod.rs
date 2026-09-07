//! MCP tool definitions: maps tool names → core engine calls.
//!
//! Each tool is a named handler function that takes `(&mut Engine, Value)` and returns
//! `Result<Value>`. The [`ToolRegistry`] manages registration and dispatch.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use ctxvault_common::config::CorpusMode;
use ctxvault_common::ports::{GraphStore, MetadataCatalog, SearchQuery, SearchService};
use ctxvault_common::{Error, Result};
use ctxvault_core::engine::Engine;
use ctxvault_core::search;
use ctxvault_core::template::Template;

// ---------------------------------------------------------------------------
// Registry types
// ---------------------------------------------------------------------------

/// MCP tool handler function signature for read-only vs mutating tools.
#[derive(Clone)]
pub enum ToolHandler {
    /// Read-only handler (can execute concurrently under reader lock).
    ReadOnly(fn(&Engine, Value) -> Result<Value>),
    /// Mutating handler (requires exclusive writer lock).
    ReadWrite(fn(&mut Engine, Value) -> Result<Value>),
}

/// Metadata and handler for a single MCP tool.
#[derive(Clone)]
pub struct ToolInfo {
    /// Tool name (used in MCP `tools/call` requests).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema describing the expected input parameters.
    pub input_schema: Value,
    /// The handler function to execute.
    pub handler: ToolHandler,
}

impl ToolInfo {
    /// Check whether the tool is read-only.
    pub fn is_read_only(&self) -> bool {
        matches!(self.handler, ToolHandler::ReadOnly(_))
    }
}

/// Tool exposure profile: gates which tools `tools/list` advertises to keep the
/// listing footprint small for narrow agent roles.
///
/// The sets are nested: `Scout` ⊂ `Analysis` ⊂ `All`. Profiles only gate what the
/// listing advertises — a tool called directly still executes regardless of profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolProfile {
    /// Minimal retrieve/navigate set for lightweight scout agents.
    Scout,
    /// Scout plus read-only graph/validation/analysis/code-intel tools.
    Analysis,
    /// Every registered tool, including mutating/admin tools.
    All,
}

/// Tools exposed under the `scout` profile (minimal retrieve/navigate set).
const SCOUT_TOOLS: [&str; 6] =
    ["search", "search_related", "get_snippet", "read_file", "list_notes", "status"];

/// Read-only tools added by the `analysis` profile on top of `scout`.
const ANALYSIS_ONLY_TOOLS: [&str; 5] =
    ["graph_match", "graph_communities", "validate", "list_templates", "list_corpora"];

impl ToolProfile {
    /// Parse a profile from its lowercase name, defaulting to [`ToolProfile::All`]
    /// for unknown values.
    pub fn from_str_name(name: &str) -> Self {
        match name {
            "scout" => ToolProfile::Scout,
            "analysis" => ToolProfile::Analysis,
            _ => ToolProfile::All,
        }
    }

    /// Whether `tools/list` under this profile should advertise `tool_name`.
    ///
    /// `All` admits every registered tool (so newly added tools appear without a
    /// list edit). `Analysis` admits the scout set plus the read-only analysis
    /// additions. `Scout` admits only the scout set.
    pub fn includes(&self, tool_name: &str) -> bool {
        match self {
            ToolProfile::All => true,
            ToolProfile::Analysis => {
                SCOUT_TOOLS.contains(&tool_name) || ANALYSIS_ONLY_TOOLS.contains(&tool_name)
            }
            ToolProfile::Scout => SCOUT_TOOLS.contains(&tool_name),
        }
    }
}

/// Registry of all available MCP tools.
pub struct ToolRegistry {
    tools: HashMap<String, ToolInfo>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    /// Register a single tool.
    pub fn register(&mut self, info: ToolInfo) {
        let _ = self.tools.insert(info.name.clone(), info);
    }

    /// Register a read-only tool handler.
    pub fn register_read(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: fn(&Engine, Value) -> Result<Value>,
    ) {
        self.register(ToolInfo {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            handler: ToolHandler::ReadOnly(handler),
        });
    }

    /// Register a mutating tool handler.
    pub fn register_write(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: fn(&mut Engine, Value) -> Result<Value>,
    ) {
        self.register(ToolInfo {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            handler: ToolHandler::ReadWrite(handler),
        });
    }

    /// Read tools that are corpus-scoped or manager-level and therefore must NOT
    /// accept the fan-out `corpus`/`corpora` discrimination args.
    const NON_DISCRIMINATED_READ_TOOLS: [&'static str; 2] = ["status", "list_corpora"];

    /// Write tools that operate at the manager level and don't accept corpus arg.
    const MANAGER_WRITE_TOOLS: [&'static str; 2] = ["index_corpus", "unload_corpus"];

    /// Inject the optional `corpus` and `corpora` discrimination properties into
    /// the JSON input schema of every read tool that supports fan-out.
    ///
    /// `corpus` targets a single corpus; `corpora` fans out across several corpora
    /// (an array of names, or the string `"all"`) with RRF-merged, corpus-tagged
    /// results. Manager-level / corpus-scoped read tools are skipped.
    fn inject_corpus_args(&mut self) {
        let corpus_prop = serde_json::json!({
            "type": "string",
            "description": "Target a single corpus by name. Omit to use the default corpus."
        });
        let corpora_prop = serde_json::json!({
            "description": "Search across multiple corpora: an array of corpus names, or the string \"all\". Results are RRF-merged and each hit is tagged with its source corpus.",
            "oneOf": [
                { "type": "array", "items": { "type": "string" } },
                { "type": "string", "enum": ["all"] }
            ]
        });

        for tool in self.tools.values_mut() {
            let manager_level = Self::NON_DISCRIMINATED_READ_TOOLS.contains(&tool.name.as_str());
            let manager_write = Self::MANAGER_WRITE_TOOLS.contains(&tool.name.as_str());
            let Some(props) =
                tool.input_schema.get_mut("properties").and_then(Value::as_object_mut)
            else {
                continue;
            };

            match tool.handler {
                // Read tools (except manager-level ones) get single `corpus` + fan-out `corpora`.
                ToolHandler::ReadOnly(_) if !manager_level => {
                    let _ = props.insert("corpus".to_string(), corpus_prop.clone());
                    let _ = props.insert("corpora".to_string(), corpora_prop.clone());
                }
                // Write tools get only single `corpus` — they never fan out.
                ToolHandler::ReadWrite(_) if !manager_write => {
                    let _ = props.insert("corpus".to_string(), corpus_prop.clone());
                }
                _ => {}
            }
        }
    }

    /// Register all available tools.
    pub fn register_all(&mut self) {
        // Read tools
        self.register_read(
            "read_file",
            "Tier 3 (last resort): read one or more markdown or source code files. Accepts a single path string or an array of path strings ('paths' or 'path'). Supports start_line, end_line, max_lines. Prefer search → get_snippet first.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "description": "Relative path to a file, OR an array of relative paths to read in batch.",
                        "oneOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" } }
                        ]
                    },
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Alternative batch paths argument."
                    },
                    "start_line": { "type": "integer", "description": "Optional 1-based start line (applies when reading a single file)" },
                    "end_line": { "type": "integer", "description": "Optional 1-based end line (inclusive, applies when reading a single file)" },
                    "max_lines": { "type": "integer", "description": "Hard cap on returned lines (default 1000 for single file, 500 per file in batch)" }
                },
                "required": []
            }),
            handle_read_file,
        );

        self.register_read(
            "get_snippet",
            "Tier 2 fetch: retrieve exactly one code symbol's source (by qualified_name or name) or one doc chunk (by path+chunk_index), bounded by max_lines. Call this for the specific handles a search returned — do NOT read whole files unless necessary.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Symbol name to look up definition for" },
                    "qualified_name": { "type": "string", "description": "Code symbol scope_path (exact) or name (fuzzy) to fetch one symbol's source" },
                    "path": { "type": "string", "description": "Relative path — for a DOC chunk fetch (with chunk_index) or a code FILE hint" },
                    "chunk_index": { "type": "integer", "description": "With path, fetch that specific doc chunk (zero-based)" },
                    "max_lines": { "type": "integer", "description": "Hard cap on returned lines (default 500)" },
                    "include_neighbors": { "type": "boolean", "description": "Include neighbor context: code callers/callees as handles, or adjacent doc chunks (default false)" }
                },
                "required": []
            }),
            handle_get_snippet,
        );

        self.register_read(
            "list_notes",
            "List indexed notes with metadata (path, title, template, content_hash), or inspect a single note's frontmatter and metadata by passing 'path'.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Optional relative path to inspect a specific note's parsed YAML frontmatter and metadata" },
                    "limit": { "type": "number", "description": "Maximum number of notes to return (default 100)" },
                    "offset": { "type": "number", "description": "Offset for pagination (default 0)" }
                },
                "required": []
            }),
            handle_list_notes,
        );

        // Search tools
        self.register_read(
            "search",
            "Tier 1 retrieval with Turn 1 hybrid snippets: returns handles across docs and code, with source snippets inlined for the top K results (configured via `snippets`, default 3). Modes: bm25, semantic, hybrid (default), graph, explain.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "mode": { "type": "string", "enum": ["bm25", "semantic", "hybrid", "graph", "explain"], "description": "Retrieval mode (default: hybrid)." },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" },
                    "snippets": { "type": "number", "description": "Number of top results across docs and code to inline source snippets for in Turn 1 (default: 3). Set to 0 for pure handles." },
                    "depth": { "type": "string", "enum": ["precise", "broad", "adaptive"], "description": "Semantic mode only: retrieval depth — precise (chunk-level, default), broad (doc-level), adaptive (both + RRF)" },
                    "graph_depth": { "type": "number", "description": "hybrid/graph/explain modes: max graph traversal depth (default 2 for hybrid/explain, 3 for graph)" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "hybrid/graph/explain modes: filter graph traversal by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "hybrid/graph/explain modes: filter graph traversal by edge class (default: semantic for hybrid/explain, structural for graph)" },
                    "decompose": { "type": "boolean", "description": "hybrid mode only: enable query decomposition for multi-hop queries (default: false)" },
                    "modality": { "type": "string", "enum": ["docs", "code", "both"], "description": "Restrict results to documentation, code, or both (default)." },
                    "detail": { "type": "string", "enum": ["ids", "default"], "description": "ids = bare handles (path/qualified_name + line range + metadata, no snippet) for wide sweeps; default = handle plus top-K snippets." }
                },
                "required": ["query"]
            }),
            handle_search,
        );

        self.register_read(
            "search_related",
            "Tier 1: returns handles (paths/qualified names + line ranges), not bodies; fetch source with get_snippet, read whole files only as a last resort. Find related documents via graph-based Personalized PageRank approximation.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "seeds": { "type": "array", "items": { "type": "string" }, "description": "Seed document paths to find related notes for" },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" },
                    "modality": { "type": "string", "enum": ["docs", "code", "both"], "description": "Restrict results to documentation, code, or both (default)." },
                    "detail": { "type": "string", "enum": ["ids", "default"], "description": "ids = bare handles (path/qualified_name + line range + metadata, no snippet) for wide sweeps; default = handle plus a short snippet." }
                },
                "required": ["seeds"]
            }),
            handle_search_related,
        );

        // Graph tools
        self.register_read(
            "graph_match",
            "Linear Cypher-Lite graph path query compiled to recursive SQLite CTEs. Traverses heterogeneous relations across code symbols and documentation notes. Cycle-safe with depth bounding.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Linear Cypher-Lite ASCII path pattern, e.g. '(:CodeSymbol {name: \"SelectVictimsOnNode\"})-[:implements]->(:Interface)<-[:calls*1..2]-(c:CodeSymbol)'"
                    },
                    "edge_class": {
                        "type": "string",
                        "enum": ["structural", "semantic", "hybrid"],
                        "description": "Optional edge class filter: structural (AST/wikilinks), semantic (tags/similarity), hybrid (both)"
                    },
                    "where": {
                        "type": "string",
                        "description": "Optional filter predicate on candidate paths"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of paths to return (default 20, max 100)"
                    },
                    "max_depth": {
                        "type": "number",
                        "description": "Hard cap on recursive traversal depth (default 3, max 5)"
                    }
                },
                "required": ["pattern"]
            }),
            handle_graph_match,
        );

        self.register_read(
            "graph_communities",
            "Detect communities in the knowledge graph. Defaults to Leiden partition. Pass view='architecture' for a high-level subsystem component overview with top key nodes, or view='raw' for raw community assignments.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "algorithm": { "type": "string", "enum": ["leiden", "louvain"], "description": "Community detection algorithm (default: leiden)" },
                    "view": { "type": "string", "enum": ["architecture", "raw"], "description": "View mode: 'architecture' for high-level components with top key nodes, 'raw' for raw community clusters (default: 'raw')" },
                    "include_density": { "type": "boolean", "description": "Include per-community density statistics (default false)" }
                },
                "required": []
            }),
            handle_graph_communities,
        );

        // Write tools
        self.register_write(
            "write_note",
            "Create or update a markdown note. Supports modes: 'create' (fails if note already exists), 'overwrite', 'append', or 'prepend' (default: 'create'). Automatically keeps all indices in sync.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path for the note (e.g. 'projects/my-note.md')" },
                    "content": { "type": "string", "description": "Body content of the note (markdown)" },
                    "mode": { "type": "string", "enum": ["create", "overwrite", "append", "prepend"], "description": "Write mode: 'create' (default, fails if file exists), 'overwrite', 'append', or 'prepend'" },
                    "frontmatter": { "type": "object", "description": "YAML frontmatter fields as key-value pairs" },
                    "template": { "type": "string", "description": "Template schema to associate in frontmatter" }
                },
                "required": ["path", "content"]
            }),
            handle_write_note,
        );

        self.register_write(
            "delete_note",
            "Delete a note from disk and remove it from all indices.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note to delete" }
                },
                "required": ["path"]
            }),
            handle_delete_note,
        );

        self.register_write(
            "move_note",
            "Move/rename a note, updating wikilinks in other notes that reference it.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Current relative path of the note" },
                    "to": { "type": "string", "description": "New relative path for the note" }
                },
                "required": ["from", "to"]
            }),
            handle_move_note,
        );

        // Validation tools
        self.register_read(
            "validate",
            "Validate a single note schema (if 'path' provided) or the entire corpus against declared templates and structural graph integrity (broken wikilinks, DAG cycles, orphan ADRs).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Optional relative path to validate a single note. If omitted, validates the entire corpus." },
                    "check_taxonomy": { "type": "boolean", "description": "When validating corpus, also check structural graph taxonomy: broken links, cycles, orphan ADRs (default true)" },
                    "limit": { "type": "number", "description": "Maximum issues to return when validating corpus" }
                },
                "required": []
            }),
            handle_validate,
        );

        self.register_read(
            "list_templates",
            "List all available templates with their field schemas and content rules.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handle_list_templates,
        );

        // System tools
        self.register_read(
            "status",
            "Corpus, indexing, graph topology, and coverage status in one tool via `scope`: corpus = per-corpus statistics and configuration; indexing = progress/throughput; graph = topology stats, density, and orphans; coverage = path-level index and parse status (requires 'paths'); all (default) = combined. When no specific corpus is targeted the multi-corpus overview is returned.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["corpus", "indexing", "graph", "coverage", "all"], "description": "corpus = per-corpus stats/config; indexing = indexing progress; graph = topology stats & density; coverage = path-level index & parse status; all (default) = combined." },
                    "corpus": { "type": "string", "description": "Target a single corpus by name for per-corpus stats/indexing. Omit for the multi-corpus overview across all configured corpora." },
                    "paths": { "type": "array", "items": { "type": "string" }, "description": "Scope 'coverage' only: paths or path prefixes to check for index coverage and parse status." }
                },
                "required": []
            }),
            handle_status,
        );

        self.register_read(
            "list_corpora",
            "List all loaded and discovered corpora in central cache with node/edge/vector statistics.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "include_cached": { "type": "boolean", "description": "Include dormant cached corpora (default true)" }
                },
                "required": []
            }),
            handle_list_corpora_dummy,
        );

        self.register_write(
            "sync_corpus",
            "Corpus index maintenance. Mode 'delta' (default) syncs filesystem changes incrementally; mode 'full' forces a full reindex with checkpoint resumption; mode 'reembed' recomputes dense embeddings.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["delta", "full", "reembed"], "description": "Sync mode: 'delta' (default, incremental sync), 'full' (full reindex), 'reembed' (recompute embeddings)" },
                    "fast": { "type": "boolean", "description": "Enable Fast Mode: skip dense embedding and vector indexing for instant indexing" },
                    "docs_embed": { "type": "boolean", "description": "Enable DocsEmbed Mode: compute embeddings for markdown doc anchors only, skipping code" },
                    "index_mode": { "type": "string", "enum": ["full", "docs-embed", "fast"], "description": "Indexing mode override ('full', 'docs-embed', 'fast')" },
                    "batch_size": { "type": "number", "description": "Batch size for commits / intermediate checkpoints (default 50)" },
                    "resume": { "type": "boolean", "description": "For mode 'full': resume from last indexing checkpoint if available (default true)" }
                },
                "required": []
            }),
            handle_sync_corpus,
        );

        self.register_write(
            "index_corpus",
            "Dynamically index and mount a new repository by path without restarting the server.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Filesystem path to the repository/corpus directory" },
                    "sync": { "type": "boolean", "description": "Run delta sync after mounting (default true)" },
                    "reindex": { "type": "boolean", "description": "Force full reindex from scratch (default false)" },
                    "fast": { "type": "boolean", "description": "Skip dense vector embedding for instant indexing (default false)" },
                    "docs_embed": { "type": "boolean", "description": "Compute embeddings for markdown docs anchors only (default false)" },
                    "batch_size": { "type": "integer", "description": "Batch size for indexing (default 50)" }
                },
                "required": ["path"]
            }),
            handle_index_corpus_dummy,
        );

        self.register_write(
            "unload_corpus",
            "Free memory by unloading an inactive corpus from the central daemon.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Name of the corpus to unload" }
                },
                "required": ["name"]
            }),
            handle_unload_corpus_dummy,
        );

        // Inject corpus/corpora discrimination args into tool schemas.
        self.inject_corpus_args();
    }

    /// Check if a tool is read-only.
    pub fn is_read_only(&self, name: &str) -> bool {
        self.tools.get(name).map(|t| t.is_read_only()).unwrap_or(false)
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolInfo> {
        self.tools.get(name)
    }

    /// List all registered tools (for MCP `tools/list` response).
    pub fn list(&self) -> Vec<&ToolInfo> {
        let mut tools: Vec<&ToolInfo> = self.tools.values().collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }

    /// Execute a read-only tool with shared immutable access to the Engine.
    pub fn execute_read(&self, name: &str, engine: &Engine, args: Value) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| Error::NotFound(format!("tool not found: {}", name)))?;
        match &tool.handler {
            ToolHandler::ReadOnly(h) => h(engine, args),
            ToolHandler::ReadWrite(_) => {
                Err(Error::Config(format!("tool '{}' is mutating and requires write lock", name)))
            }
        }
    }

    /// Execute a tool with exclusive mutable access to the Engine.
    pub fn execute_write(&self, name: &str, engine: &mut Engine, args: Value) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| Error::NotFound(format!("tool not found: {}", name)))?;
        match &tool.handler {
            ToolHandler::ReadOnly(h) => h(engine, args),
            ToolHandler::ReadWrite(h) => h(engine, args),
        }
    }

    /// Execute a tool by name with given arguments (convenience wrapper around `execute_write`).
    pub fn execute(&self, name: &str, engine: &mut Engine, args: Value) -> Result<Value> {
        self.execute_write(name, engine, args)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Multi-Corpus Routing
// ---------------------------------------------------------------------------

use ctxvault_core::corpus_manager::CorpusManager;

/// Multi-corpus tool registry: wraps a `CorpusManager` and routes tool calls
/// to the correct engine(s) based on the `corpus` / `corpora` arguments.
///
/// - `corpus = "name"` targets a single corpus engine.
/// - `corpora = ["a", "b"]` or `corpora = "all"` fans out across several corpora;
///   search-style results are RRF-merged and each hit is tagged with its source
///   corpus.
/// - Omitting both resolves to the default corpus. This is an ergonomic default,
///   not a legacy code path.
pub struct MultiCorpusToolRegistry {
    registry: ToolRegistry,
    profile: ToolProfile,
}

/// The resolved fan-out target for a read tool call.
enum CorpusTarget {
    /// Exactly one corpus (explicit `corpus` or the default).
    Single(String),
    /// Two or more corpora to fan out across (deduplicated, order preserved).
    Multi(Vec<String>),
}

impl MultiCorpusToolRegistry {
    /// Create a new multi-corpus registry with all tools registered and the
    /// [`ToolProfile::All`] exposure profile.
    pub fn new() -> Self {
        Self::with_profile(ToolProfile::All)
    }

    /// Create a new multi-corpus registry exposing tools under `profile`.
    ///
    /// The profile only gates what [`Self::list`] advertises; every registered
    /// tool remains executable regardless of profile.
    pub fn with_profile(profile: ToolProfile) -> Self {
        let mut registry = ToolRegistry::new();
        registry.register_all();

        Self { registry, profile }
    }

    /// The active tool exposure profile.
    pub fn profile(&self) -> ToolProfile {
        self.profile
    }

    /// Check if a tool is read-only.
    pub fn is_read_only(&self, name: &str) -> bool {
        self.registry.is_read_only(name)
    }

    /// List the tools advertised under the active profile (for MCP `tools/list`).
    pub fn list(&self) -> Vec<&ToolInfo> {
        self.registry.list().into_iter().filter(|t| self.profile.includes(&t.name)).collect()
    }

    /// List every registered tool regardless of profile (for internal use).
    pub fn list_all(&self) -> Vec<&ToolInfo> {
        self.registry.list()
    }

    /// Execute a read-only tool call, routing to one corpus or fanning out across
    /// several with RRF-merged, corpus-tagged results.
    pub fn execute_read(&self, name: &str, manager: &CorpusManager, args: Value) -> Result<Value> {
        // `status` without an explicit `corpus` returns the manager-level overview
        // (all corpora). With a `corpus` it routes to that engine's status below.
        if name == "status" && !has_corpus_arg(&args) {
            return handle_get_status(manager);
        }

        if name == "list_corpora" {
            return handle_list_corpora_manager(manager, args);
        }

        // Parse both discrimination args out of the call, resolving the target set.
        let (target, clean_args) = resolve_corpus_target(args, manager)?;

        match target {
            CorpusTarget::Single(corpus_name) => {
                let engine = manager.get_engine(&corpus_name)?;
                let output = self.registry.execute_read(name, engine, clean_args)?;
                Ok(tag_search_output(output, &corpus_name))
            }
            CorpusTarget::Multi(names) => self.fan_out_read(name, manager, &names, clean_args),
        }
    }

    /// Fan out a read tool across multiple corpora and merge the results.
    ///
    /// Search-style outputs (JSON arrays of `SearchResult`) are RRF-merged via
    /// [`search::rrf_fuse_cross_corpus`] and returned as one tagged array. Other
    /// (non-array) outputs are returned as a JSON object keyed by corpus name.
    fn fan_out_read(
        &self,
        name: &str,
        manager: &CorpusManager,
        names: &[String],
        clean_args: Value,
    ) -> Result<Value> {
        let limit =
            clean_args.get("limit").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(10);

        let mut per_corpus: Vec<(String, Value)> = Vec::new();
        let mut last_err: Option<Error> = None;

        for corpus_name in names {
            let engine = match manager.get_engine(corpus_name) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(corpus = %corpus_name, error = %e, "fan-out: engine resolve failed");
                    last_err = Some(e);
                    continue;
                }
            };
            match self.registry.execute_read(name, engine, clean_args.clone()) {
                Ok(v) => per_corpus.push((corpus_name.clone(), v)),
                Err(e) => {
                    tracing::warn!(corpus = %corpus_name, error = %e, "fan-out: tool call failed");
                    last_err = Some(e);
                }
            }
        }

        if per_corpus.is_empty() {
            return Err(last_err.unwrap_or_else(|| {
                Error::NotFound("no corpora available for fan-out".to_string())
            }));
        }

        let all_search_responses = per_corpus
            .iter()
            .all(|(_, v)| v.is_object() && (v.get("docs").is_some() || v.get("code").is_some()));
        if all_search_responses {
            let mut docs_tagged: Vec<(String, Vec<ctxvault_common::types::SearchResult>)> =
                Vec::new();
            let mut code_tagged: Vec<(String, Vec<ctxvault_common::types::SearchResult>)> =
                Vec::new();
            for (corpus_name, value) in per_corpus {
                let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(value)
                    .map_err(|e| Error::Config(format!("invalid search response: {}", e)))?;
                if let Some(d) = resp.docs {
                    docs_tagged.push((corpus_name.clone(), d.results));
                }
                if let Some(c) = resp.code {
                    code_tagged.push((corpus_name, c.results));
                }
            }
            let merged_docs = search::rrf_fuse_cross_corpus(&docs_tagged, limit);
            let merged_code = search::rrf_fuse_cross_corpus(&code_tagged, limit);
            let docs_partition = if !merged_docs.is_empty() {
                Some(ctxvault_common::types::SearchPartition {
                    total_matches: merged_docs.len(),
                    top_k_returned: merged_docs.len(),
                    schema_envelope: ctxvault_common::types::SchemaEnvelope {
                        node_labels: vec![
                            "DocNode".to_string(),
                            "ADR".to_string(),
                            "Concept".to_string(),
                        ],
                        active_edges: vec![
                            "wikilink".to_string(),
                            "supersedes".to_string(),
                            "documents".to_string(),
                            "tag".to_string(),
                        ],
                    },
                    results: merged_docs,
                })
            } else {
                None
            };
            let code_partition = if !merged_code.is_empty() {
                Some(ctxvault_common::types::SearchPartition {
                    total_matches: merged_code.len(),
                    top_k_returned: merged_code.len(),
                    schema_envelope: ctxvault_common::types::SchemaEnvelope {
                        node_labels: vec![
                            "CodeSymbol".to_string(),
                            "CodeChunk".to_string(),
                            "Function".to_string(),
                            "Method".to_string(),
                            "Struct".to_string(),
                        ],
                        active_edges: vec![
                            "calls".to_string(),
                            "implements".to_string(),
                            "imports".to_string(),
                            "defines".to_string(),
                        ],
                    },
                    results: merged_code,
                })
            } else {
                None
            };
            let resp = ctxvault_common::types::SearchResponse {
                docs: docs_partition,
                code: code_partition,
            };
            return serde_json::to_value(resp)
                .map_err(|e| Error::Config(format!("serialize merged search response: {}", e)));
        }

        // If every successful output is a JSON array, treat as search-style and RRF-merge.
        let all_arrays = per_corpus.iter().all(|(_, v)| v.is_array());
        if all_arrays {
            let mut tagged_lists: Vec<(String, Vec<ctxvault_common::types::SearchResult>)> =
                Vec::with_capacity(per_corpus.len());
            for (corpus_name, value) in per_corpus {
                let results: Vec<ctxvault_common::types::SearchResult> =
                    serde_json::from_value(value).map_err(|e| {
                        Error::Config(format!("invalid search result array: {}", e))
                    })?;
                tagged_lists.push((corpus_name, results));
            }
            let merged = search::rrf_fuse_cross_corpus(&tagged_lists, limit);
            return serde_json::to_value(merged)
                .map_err(|e| Error::Config(format!("serialize merged results: {}", e)));
        }

        // Otherwise: return an object keyed by corpus name → raw output.
        let obj: serde_json::Map<String, Value> = per_corpus.into_iter().collect();
        Ok(Value::Object(obj))
    }

    /// Execute a tool call with exclusive access to the CorpusManager.
    ///
    /// Write tools always resolve a SINGLE corpus (explicit `corpus` or the default)
    /// and never fan out. Omitting `corpus` selects the default corpus as an
    /// ergonomic default.
    pub fn execute_write(
        &self,
        name: &str,
        manager: &mut CorpusManager,
        args: Value,
    ) -> Result<Value> {
        if name == "index_corpus" {
            return handle_index_corpus_manager(manager, args);
        }
        if name == "unload_corpus" {
            return handle_unload_corpus_manager(manager, args);
        }

        // `status` without an explicit `corpus` returns the manager-level overview.
        if name == "status" && !has_corpus_arg(&args) {
            return handle_get_status(manager);
        }

        // Extract and remove the `corpus` param from arguments (writes never fan out).
        let (corpus_name, clean_args) = extract_corpus_param(args);

        // Resolve the engine mutably.
        let engine = manager.resolve_engine_mut(corpus_name.as_deref())?;

        // Execute the tool.
        self.registry.execute_write(name, engine, clean_args)
    }

    /// Execute a tool call, routing to the correct corpus engine.
    pub fn execute(&self, name: &str, manager: &mut CorpusManager, args: Value) -> Result<Value> {
        self.execute_write(name, manager, args)
    }

    /// Get underlying registry reference (for listing tools etc).
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }
}

impl Default for MultiCorpusToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether the tool arguments explicitly target a single `corpus` by name.
fn has_corpus_arg(args: &Value) -> bool {
    args.get("corpus").and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}

/// Extract the optional single `corpus` field from tool arguments, returning
/// the corpus name and the arguments with `corpus` removed. Used by write tools,
/// which never fan out.
fn extract_corpus_param(args: Value) -> (Option<String>, Value) {
    match args {
        Value::Object(mut map) => {
            let corpus = map.remove("corpus").and_then(|v| v.as_str().map(|s| s.to_string()));
            (corpus, Value::Object(map))
        }
        other => (None, other),
    }
}

/// Parse both `corpus` and `corpora` out of read-tool arguments and resolve the
/// target corpus set, returning it alongside the arguments with BOTH keys removed.
///
/// Resolution precedence:
/// - `corpora == "all"` → every corpus (sorted for determinism);
/// - `corpora` as a non-empty array → those names (each validated to exist);
/// - `corpus` set → that single corpus;
/// - neither → the default corpus (single).
fn resolve_corpus_target(args: Value, manager: &CorpusManager) -> Result<(CorpusTarget, Value)> {
    let Value::Object(mut map) = args else {
        // Non-object args cannot carry discrimination — fall back to default corpus.
        let default = manager
            .default_corpus_name()
            .ok_or_else(|| Error::NotFound("no default corpus configured".to_string()))?
            .to_string();
        return Ok((CorpusTarget::Single(default), args));
    };

    let corpus = map.remove("corpus").and_then(|v| v.as_str().map(|s| s.to_string()));
    let corpora = map.remove("corpora");
    let clean_args = Value::Object(map);

    let target = match corpora {
        Some(Value::String(s)) if s == "all" => {
            let mut names: Vec<String> =
                manager.corpus_names().into_iter().map(|s| s.to_string()).collect();
            names.sort();
            multi_or_single(names)?
        }
        Some(Value::Array(items)) => {
            let mut names: Vec<String> = Vec::with_capacity(items.len());
            for item in items {
                let n = item
                    .as_str()
                    .ok_or_else(|| Error::Config("corpora array must contain strings".to_string()))?
                    .to_string();
                if !manager.has_corpus(&n) {
                    return Err(Error::NotFound(format!("corpus not found: {}", n)));
                }
                names.push(n);
            }
            if names.is_empty() {
                // Empty array behaves like "omitted": resolve default.
                single_default(corpus, manager)?
            } else {
                multi_or_single(names)?
            }
        }
        Some(Value::String(s)) => {
            return Err(Error::Config(format!(
                "invalid corpora value '{}': expected an array of names or \"all\"",
                s
            )));
        }
        Some(_) => {
            return Err(Error::Config(
                "invalid corpora value: expected an array of names or \"all\"".to_string(),
            ));
        }
        None => single_default(corpus, manager)?,
    };

    Ok((target, clean_args))
}

/// Resolve the single-corpus target from an explicit `corpus` or the default.
fn single_default(corpus: Option<String>, manager: &CorpusManager) -> Result<CorpusTarget> {
    match corpus {
        Some(name) => {
            if !manager.has_corpus(&name) {
                return Err(Error::NotFound(format!("corpus not found: {}", name)));
            }
            Ok(CorpusTarget::Single(name))
        }
        None => {
            let default = manager
                .default_corpus_name()
                .ok_or_else(|| Error::NotFound("no default corpus configured".to_string()))?
                .to_string();
            Ok(CorpusTarget::Single(default))
        }
    }
}

/// Collapse a resolved name list into `Single` (one, deduped) or `Multi` (many).
fn multi_or_single(mut names: Vec<String>) -> Result<CorpusTarget> {
    names.dedup();
    match names.len() {
        0 => Err(Error::NotFound("no corpora resolved for fan-out".to_string())),
        1 => Ok(CorpusTarget::Single(names.into_iter().next().unwrap())),
        _ => Ok(CorpusTarget::Multi(names)),
    }
}

/// Tag a single-corpus read output: if it is a JSON array of `SearchResult`,
/// stamp each hit with the source corpus; otherwise return it unchanged.
fn tag_search_output(output: Value, corpus_name: &str) -> Value {
    if output.is_array() {
        if let Ok(results) =
            serde_json::from_value::<Vec<ctxvault_common::types::SearchResult>>(output.clone())
        {
            let tagged: Vec<ctxvault_common::types::SearchResult> =
                results.into_iter().map(|r| r.with_corpus(Some(corpus_name.to_string()))).collect();
            return serde_json::to_value(tagged).unwrap_or(output);
        }
    } else if let Ok(mut resp) =
        serde_json::from_value::<ctxvault_common::types::SearchResponse>(output.clone())
    {
        if let Some(ref mut d) = resp.docs {
            for r in &mut d.results {
                r.corpus = Some(corpus_name.to_string());
            }
        }
        if let Some(ref mut c) = resp.code {
            for r in &mut c.results {
                r.corpus = Some(corpus_name.to_string());
            }
        }
        return serde_json::to_value(resp).unwrap_or(output);
    }
    output
}

/// Get overall system status from the CorpusManager.
fn handle_get_status(manager: &CorpusManager) -> Result<Value> {
    let corpora = manager.list_corpora();
    let default_name = manager.default_corpus_name().unwrap_or("none");

    let corpora_info: Vec<Value> = corpora
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "path": c.path,
                "mode": c.mode,
                "index_mode": c.index_mode,
                "file_count": c.file_count,
                "embedder_active": c.embedder_active,
                "vector_count": c.vector_count,
                "graph_node_count": c.graph_node_count,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "corpus_count": manager.corpus_count(),
        "default_corpus": default_name,
        "corpora": corpora_info,
    }))
}

/// Handle `list_corpora` across active and cached corpora.
fn handle_list_corpora_manager(manager: &CorpusManager, args: Value) -> Result<Value> {
    let include_cached = args.get("include_cached").and_then(Value::as_bool).unwrap_or(true);
    let loaded = manager.list_corpora();
    let loaded_names: HashSet<String> = loaded.iter().map(|c| c.name.clone()).collect();

    let mut corpora_info: Vec<Value> = loaded
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "path": c.path,
                "status": "active",
                "mode": c.mode,
                "index_mode": c.index_mode,
                "file_count": c.file_count,
                "embedder_active": c.embedder_active,
                "vector_count": c.vector_count,
                "graph_node_count": c.graph_node_count,
            })
        })
        .collect();

    if include_cached {
        for cached in manager.discover_cached_corpora() {
            if !loaded_names.contains(&cached) {
                corpora_info.push(serde_json::json!({
                    "name": cached,
                    "status": "cached",
                    "path": ctxvault_common::config::get_corpora_cache_dir().join(&cached).to_string_lossy().replace('\\', "/"),
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "corpora": corpora_info,
        "default_corpus": manager.default_corpus_name(),
        "total_active": manager.corpus_count(),
    }))
}

/// Handle `index_corpus` dynamically mounting and indexing a new repository.
fn handle_index_corpus_manager(manager: &mut CorpusManager, args: Value) -> Result<Value> {
    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Config("missing required argument 'path'".to_string()))?;

    let corpus_path = PathBuf::from(path_str);
    let name = manager.ensure_corpus(&corpus_path)?;

    let do_reindex = args.get("reindex").and_then(Value::as_bool).unwrap_or(false);
    let do_sync = args.get("sync").and_then(Value::as_bool).unwrap_or(true);
    let fast = args.get("fast").and_then(Value::as_bool).unwrap_or(false);
    let docs_embed = args.get("docs_embed").and_then(Value::as_bool).unwrap_or(false);
    let batch_size =
        args.get("batch_size").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(50);

    let engine = manager.get_engine_mut(&name)?;

    if fast {
        engine.config_mut().index_mode = ctxvault_common::config::IndexMode::Fast;
    } else if docs_embed {
        engine.config_mut().index_mode = ctxvault_common::config::IndexMode::DocsEmbed;
    }

    let index_stats = if do_reindex {
        let count = engine.full_reindex_paginated(batch_size, false)?;
        serde_json::json!({ "reindexed_files": count })
    } else if do_sync {
        let delta = engine.delta_scan_paginated(batch_size)?;
        serde_json::json!({
            "new_files": delta.new_files.len(),
            "modified_files": delta.modified_files.len(),
            "deleted_files": delta.deleted_files.len()
        })
    } else {
        serde_json::json!({ "status": "mounted_without_indexing" })
    };

    let file_count = engine.store().list_files().map(|f| f.len()).unwrap_or(0);

    Ok(serde_json::json!({
        "status": "success",
        "corpus": name,
        "path": path_str,
        "file_count": file_count,
        "indexing": index_stats,
    }))
}

/// Handle `unload_corpus` freeing memory from an open engine.
fn handle_unload_corpus_manager(manager: &mut CorpusManager, args: Value) -> Result<Value> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Config("missing required argument 'name'".to_string()))?;

    let unloaded = manager.unload_corpus(name)?;
    Ok(serde_json::json!({
        "status": if unloaded { "unloaded" } else { "not_found" },
        "corpus": name
    }))
}

fn handle_list_corpora_dummy(_engine: &Engine, _args: Value) -> Result<Value> {
    Err(Error::Config("list_corpora is a manager-level tool".to_string()))
}

fn handle_index_corpus_dummy(_engine: &mut Engine, _args: Value) -> Result<Value> {
    Err(Error::Config("index_corpus is a manager-level tool".to_string()))
}

fn handle_unload_corpus_dummy(_engine: &mut Engine, _args: Value) -> Result<Value> {
    Err(Error::Config("unload_corpus is a manager-level tool".to_string()))
}

// ---------------------------------------------------------------------------
// Parameter structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum PathOrPaths {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReadFileParams {
    pub path: Option<PathOrPaths>,
    pub paths: Option<Vec<String>>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub max_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListNotesParams {
    pub path: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetSnippetParams {
    pub name: Option<String>,
    pub path: Option<String>,
    pub chunk_index: Option<usize>,
    pub qualified_name: Option<String>,
    pub max_lines: Option<usize>,
    #[serde(default)]
    pub include_neighbors: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchParams {
    pub query: String,
    #[serde(default)]
    pub mode: Option<String>,
    pub limit: Option<usize>,
    pub depth: Option<String>,
    pub graph_depth: Option<usize>,
    pub edge_types: Option<Vec<String>>,
    pub edge_class: Option<String>,
    pub decompose: Option<bool>,
    pub modality: Option<String>,
    pub detail: Option<String>,
    pub snippets: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchRelatedParams {
    pub seeds: Vec<String>,
    pub limit: Option<usize>,
    pub modality: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StatusParams {
    pub scope: Option<String>,
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphMatchParams {
    pub pattern: String,
    pub edge_class: Option<String>,
    #[serde(rename = "where")]
    pub where_clause: Option<String>,
    pub limit: Option<usize>,
    pub max_depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphCommunitiesParams {
    pub algorithm: Option<String>,
    pub view: Option<String>,
    pub include_density: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WriteNoteParams {
    pub path: String,
    pub content: String,
    pub mode: Option<String>,
    pub frontmatter: Option<Value>,
    pub template: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteNoteParams {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MoveNoteParams {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ValidateParams {
    pub path: Option<String>,
    pub check_taxonomy: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SyncCorpusParams {
    pub mode: Option<String>,
    pub batch_size: Option<usize>,
    pub resume: Option<bool>,
    pub fast: Option<bool>,
    pub docs_embed: Option<bool>,
    pub index_mode: Option<String>,
}

// ---------------------------------------------------------------------------
// Response structs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct NoteListItem {
    path: String,
    title: Option<String>,
    template: Option<String>,
    content_hash: String,
}

/// Detect a source language from a file extension. Returns `"text"` when unknown.
fn language_from_path(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("ts") | Some("tsx") => "typescript",
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => "javascript",
        Some("py") => "python",
        Some("go") => "go",
        Some("java") => "java",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") | Some("hh") => "cpp",
        Some("md") | Some("markdown") => "markdown",
        _ => "text",
    }
}

/// Bound a body of source lines to `max_lines`, joining with newlines and
/// reporting whether truncation occurred.
fn cap_lines(lines: &[&str], max_lines: usize) -> (String, bool) {
    if lines.len() > max_lines {
        (lines[..max_lines].join("\n"), true)
    } else {
        (lines.join("\n"), false)
    }
}

/// Build a bare handle (no body) for a code symbol: scope_path + file + line range + signature/docstring.
fn code_symbol_handle(sym: &ctxvault_common::types::CodeSymbol) -> Value {
    serde_json::json!({
        "scope_path": sym.scope_path,
        "name": sym.name,
        "file_path": sym.file_path,
        "start_line": sym.start_line,
        "end_line": sym.end_line,
        "language": sym.language,
        "symbol_type": sym.symbol_type,
        "signature": sym.signature,
        "docstring": sym.docstring,
    })
}

/// Read a single file for [`handle_read_file`].
fn read_single_file(
    corpus_root: &Path,
    path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    max_lines: usize,
) -> Result<Value> {
    let full_path = corpus_root.join(path);
    let raw = std::fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", path, e)))?;

    let is_markdown = matches!(language_from_path(path), "markdown");
    if is_markdown && start_line.is_none() && end_line.is_none() {
        let doc = ctxvault_core::parser::parse_document(Path::new(path), &raw)?;
        let lines: Vec<&str> = doc.content.lines().collect();
        let (content, truncated) = cap_lines(&lines, max_lines);
        return Ok(serde_json::json!({
            "kind": "markdown_note",
            "path": path,
            "title": doc.title,
            "frontmatter": doc.frontmatter,
            "content": content,
            "truncated": truncated,
            "content_hash": doc.content_hash,
        }));
    }

    let file_lines: Vec<&str> = raw.lines().collect();
    let total_lines = file_lines.len();
    let start = start_line.unwrap_or(1).max(1);
    let end = end_line.unwrap_or(total_lines).min(total_lines);

    if start > total_lines {
        return Ok(serde_json::json!({
            "kind": if is_markdown { "markdown_note" } else { "code_file" },
            "path": path,
            "start_line": start,
            "end_line": end,
            "total_lines": total_lines,
            "content": "",
            "truncated": false,
        }));
    }

    let slice_start = start - 1;
    let slice_end = end.max(slice_start);
    let slice = &file_lines[slice_start..slice_end];
    let (content, truncated) = cap_lines(slice, max_lines);

    Ok(serde_json::json!({
        "kind": if is_markdown { "markdown_note" } else { "code_file" },
        "path": path,
        "start_line": start,
        "end_line": end,
        "total_lines": total_lines,
        "language": language_from_path(path),
        "content": content,
        "truncated": truncated,
    }))
}

/// Tier 3 read of one or more files (markdown or source code).
fn handle_read_file(engine: &Engine, args: Value) -> Result<Value> {
    let params: ReadFileParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_root = PathBuf::from(&engine.config().path);

    let target_paths = if let Some(paths) = params.paths {
        PathOrPaths::Multiple(paths)
    } else if let Some(p) = params.path {
        p
    } else {
        return Err(Error::Config("read_file requires 'path' or 'paths'".to_string()));
    };

    match target_paths {
        PathOrPaths::Single(p) => {
            let max_lines = params.max_lines.unwrap_or(1000).max(1);
            read_single_file(&corpus_root, &p, params.start_line, params.end_line, max_lines)
        }
        PathOrPaths::Multiple(paths) => {
            let max_lines = params.max_lines.unwrap_or(500).max(1);
            let results: Vec<Value> = paths
                .iter()
                .map(|p| match read_single_file(&corpus_root, p, None, None, max_lines) {
                    Ok(val) => val,
                    Err(e) => serde_json::json!({ "path": p, "error": e.to_string() }),
                })
                .collect();
            Ok(serde_json::json!({
                "count": results.len(),
                "results": results,
            }))
        }
    }
}

/// Tier 2 fetch: return exactly one code symbol's source or one doc chunk,
/// bounded by `max_lines`, with optional neighbor expansion.
fn handle_get_snippet(engine: &Engine, args: Value) -> Result<Value> {
    let params: GetSnippetParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let max_lines = params.max_lines.unwrap_or(500).max(1);
    let corpus_root = Path::new(&engine.config().path);

    let target_name = params.qualified_name.or(params.name);
    if let Some(ref qualified_name) = target_name {
        return fetch_code_symbol(
            engine,
            corpus_root,
            qualified_name,
            max_lines,
            params.include_neighbors,
        );
    }

    if let Some(path) = params.path.as_deref() {
        if let Some(chunk_index) = params.chunk_index {
            return fetch_doc_chunk(engine, path, chunk_index, max_lines, params.include_neighbors);
        }
        return Err(Error::Config(format!(
            "get_snippet needs a chunk_index for a doc fetch on '{path}'. \
             For a whole file use Tier 3: read_file.",
        )));
    }

    Err(Error::Config(
        "get_snippet requires either `name`/`qualified_name` (code) or `path`+`chunk_index` (doc)."
            .to_string(),
    ))
}

/// Fetch a single code symbol's bounded source by qualified name (or fuzzy name),
/// optionally attaching caller/callee handles.
fn fetch_code_symbol(
    engine: &Engine,
    corpus_root: &Path,
    qualified_name: &str,
    max_lines: usize,
    include_neighbors: bool,
) -> Result<Value> {
    let mut matches = engine.store().find_symbols_by_qualified_name(qualified_name)?;
    if matches.is_empty() {
        matches = engine.store().find_symbols_by_normalized_scope(qualified_name)?;
    }
    if matches.is_empty() {
        matches = engine.store().find_symbols_by_name(qualified_name)?;
    }

    match matches.len() {
        0 => {
            let leaf = qualified_name.split(" > ").last().unwrap_or(qualified_name).trim();
            let leaf_candidates = engine.store().find_symbols_by_name(leaf).unwrap_or_default();
            let leaf_matches: Vec<_> =
                leaf_candidates.into_iter().filter(|s| s.name.eq_ignore_ascii_case(leaf)).collect();

            if !leaf_matches.is_empty() {
                let candidates: Vec<Value> = leaf_matches.iter().map(code_symbol_handle).collect();
                Ok(serde_json::json!({
                    "kind": "candidate_suggestions",
                    "note": format!(
                        "No code symbol matches '{qualified_name}', but found {} candidate(s) with leaf name '{leaf}'. Disambiguate with an exact scope_path.",
                        candidates.len()
                    ),
                    "candidates": candidates,
                }))
            } else {
                Err(Error::NotFound(format!("no code symbol matches '{qualified_name}'")))
            }
        }
        1 => {
            let sym = &matches[0];
            let full_path = corpus_root.join(&sym.file_path);
            let content = fs::read_to_string(&full_path)
                .map_err(|e| Error::NotFound(format!("cannot read {}: {}", sym.file_path, e)))?;
            let file_lines: Vec<&str> = content.lines().collect();

            let (source, truncated) = if sym.start_line > 0 && sym.start_line <= file_lines.len() {
                let start_idx = sym.start_line - 1;
                let end_idx = sym.end_line.min(file_lines.len());
                cap_lines(&file_lines[start_idx..end_idx], max_lines)
            } else {
                (String::new(), false)
            };

            let mut out = serde_json::json!({
                "kind": "code_symbol",
                "path": sym.file_path,
                "scope_path": sym.scope_path,
                "name": sym.name,
                "language": sym.language,
                "symbol_type": sym.symbol_type,
                "start_line": sym.start_line,
                "end_line": sym.end_line,
                "signature": sym.signature,
                "docstring": sym.docstring,
                "source": source,
                "truncated": truncated,
            });

            if include_neighbors {
                let all_symbols = engine.store().get_all_code_symbols().unwrap_or_default();
                let edges = engine.graph().get_all_edges();
                let matches_sym =
                    |candidate: &str| candidate == sym.scope_path || candidate == sym.name;

                // Callers: "calls" edges whose TARGET is this symbol → source is a caller.
                let callers: Vec<Value> = edges
                    .iter()
                    .filter(|e| e.edge_type == "calls" && matches_sym(&e.target))
                    .filter_map(|e| {
                        all_symbols
                            .iter()
                            .find(|s| s.scope_path == e.source || s.name == e.source)
                            .map(code_symbol_handle)
                    })
                    .collect();

                // Callees: "calls" edges whose SOURCE is this symbol → target is a callee.
                let callees: Vec<Value> = edges
                    .iter()
                    .filter(|e| e.edge_type == "calls" && matches_sym(&e.source))
                    .filter_map(|e| {
                        all_symbols
                            .iter()
                            .find(|s| s.scope_path == e.target || s.name == e.target)
                            .map(code_symbol_handle)
                    })
                    .collect();

                out["callers"] = Value::Array(callers);
                out["callees"] = Value::Array(callees);
            }

            Ok(out)
        }
        _ => {
            let candidates: Vec<Value> = matches.iter().map(code_symbol_handle).collect();
            Ok(serde_json::json!({
                "kind": "ambiguous",
                "note": format!(
                    "'{qualified_name}' is ambiguous ({} matches); disambiguate with an exact scope_path.",
                    candidates.len()
                ),
                "candidates": candidates,
            }))
        }
    }
}

/// Fetch a single doc chunk's bounded text, optionally with adjacent chunks.
fn fetch_doc_chunk(
    engine: &Engine,
    path: &str,
    chunk_index: usize,
    max_lines: usize,
    include_neighbors: bool,
) -> Result<Value> {
    let chunks = engine.store().get_chunks_for_file(path)?;
    if chunks.is_empty() {
        return Err(Error::NotFound(format!("no indexed chunks for '{path}'")));
    }

    let chunk = chunks
        .iter()
        .find(|c| c.chunk_index == chunk_index)
        .ok_or_else(|| Error::NotFound(format!("chunk {chunk_index} not found for '{path}'")))?;

    let chunk_text = engine.fetch_chunk_text(path, chunk.start_byte, chunk.end_byte)?;
    let text_lines: Vec<&str> = chunk_text.lines().collect();
    let (text, truncated) = cap_lines(&text_lines, max_lines);

    let mut out = serde_json::json!({
        "kind": "doc_chunk",
        "path": path,
        "chunk_index": chunk.chunk_index,
        "start_byte": chunk.start_byte,
        "end_byte": chunk.end_byte,
        "text": text,
        "truncated": truncated,
    });

    if include_neighbors {
        let neighbor_cap = (max_lines / 2).max(1);
        let neighbor = |target: usize| -> Option<Value> {
            chunks.iter().find(|c| c.chunk_index == target).and_then(|c| {
                let n_text = engine.fetch_chunk_text(path, c.start_byte, c.end_byte).ok()?;
                let nlines: Vec<&str> = n_text.lines().collect();
                let (ntext, ntrunc) = cap_lines(&nlines, neighbor_cap);
                Some(serde_json::json!({
                    "chunk_index": c.chunk_index,
                    "start_byte": c.start_byte,
                    "end_byte": c.end_byte,
                    "text": ntext,
                    "truncated": ntrunc,
                }))
            })
        };

        out["previous"] = chunk_index.checked_sub(1).and_then(neighbor).unwrap_or(Value::Null);
        out["next"] = neighbor(chunk_index + 1).unwrap_or(Value::Null);
    }

    Ok(out)
}

/// Report index coverage + parse status for the given paths or path prefixes.
fn check_index_coverage_inner(engine: &Engine, paths: &[String]) -> Result<Value> {
    let all_files = engine.store().list_files()?;

    let mut reports = Vec::with_capacity(paths.len());
    let mut covered = 0usize;

    for scope in paths {
        let matched: Vec<&str> = all_files
            .iter()
            .map(|f| f.path.as_str())
            .filter(|p| *p == scope || p.starts_with(scope.as_str()))
            .collect();

        let indexed = !matched.is_empty();
        let mut chunk_count = 0usize;
        let mut symbol_count = 0usize;
        for file_path in &matched {
            chunk_count += engine.store().get_chunks_for_file(file_path).map(|c| c.len())?;
            symbol_count += engine.store().get_code_symbols_for_file(file_path).map(|s| s.len())?;
        }

        let parsed = indexed && (chunk_count > 0 || symbol_count > 0);
        if indexed {
            covered += 1;
        }

        let mut matched_files: Vec<String> = matched.iter().map(|p| p.to_string()).collect();
        matched_files.sort();

        reports.push(serde_json::json!({
            "path": scope,
            "indexed": indexed,
            "parsed": parsed,
            "chunk_count": chunk_count,
            "symbol_count": symbol_count,
            "matched_files": matched_files,
        }));
    }

    let total = paths.len();
    Ok(serde_json::json!({
        "reports": reports,
        "summary": {
            "total": total,
            "covered": covered,
            "uncovered": total - covered,
        },
    }))
}

/// List all indexed notes with metadata, or inspect single note's frontmatter and metadata if `path` is provided.
fn handle_list_notes(engine: &Engine, args: Value) -> Result<Value> {
    let params: ListNotesParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    if let Some(path) = params.path {
        let corpus_path = PathBuf::from(&engine.config().path);
        let full_path = corpus_path.join(&path);
        let content = std::fs::read_to_string(&full_path)
            .map_err(|e| Error::NotFound(format!("cannot read {}: {}", path, e)))?;
        let doc = ctxvault_core::parser::parse_document(Path::new(&path), &content)?;
        return Ok(serde_json::json!({
            "path": path,
            "title": doc.title,
            "frontmatter": doc.frontmatter,
            "content_hash": doc.content_hash,
        }));
    }

    let limit = params.limit.unwrap_or(100);
    let offset = params.offset.unwrap_or(0);

    let files = engine.store().list_files()?;

    let items: Vec<NoteListItem> = files
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|f| NoteListItem {
            path: f.path,
            title: f.title,
            template: f.template,
            content_hash: f.content_hash,
        })
        .collect();

    serde_json::to_value(items).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Full-text BM25 keyword search.
/// Apply Tier-1 progressive-disclosure verbosity to a set of search results.
/// Apply detail level shaping to search results.
///
/// `detail == "ids"` strips the `snippet`, `lineage`, and `score_components`
/// from every result, leaving bare handles (path/qualified-name + line range + metadata carried by
/// `entity_kind`/`language`/`chunk_index`). Any other value (including the
/// omitted default) keeps the existing short snippet and metadata. Full bodies are never
/// emitted here — callers fetch source via `get_snippet`.
fn apply_detail(
    mut results: Vec<ctxvault_common::types::SearchResult>,
    detail: Option<&str>,
) -> Vec<ctxvault_common::types::SearchResult> {
    if detail == Some("ids") {
        for r in &mut results {
            r.snippet = None;
            r.lineage = None;
            r.score_components = None;
        }
    }
    results
}

/// Inlines bounded source text for the top K results across a partition.
fn populate_top_snippets(
    engine: &Engine,
    results: &mut [ctxvault_common::types::SearchResult],
    k: usize,
    max_lines: usize,
) {
    let corpus_root = Path::new(&engine.config().path);
    for (i, item) in results.iter_mut().enumerate() {
        if i >= k {
            item.snippet = None;
            continue;
        }

        if let Some(ref s) = item.snippet {
            if s.len() > 120 {
                let lines: Vec<&str> = s.lines().collect();
                if lines.len() > max_lines {
                    let (capped, _) = cap_lines(&lines, max_lines);
                    item.snippet = Some(capped);
                }
                continue;
            }
        }

        if let Some(chunk_index) = item.chunk_index {
            if let Ok(chunks) = engine.store().get_chunks_for_file(&item.path) {
                if let Some(chunk) = chunks.iter().find(|c| c.chunk_index == chunk_index) {
                    if let Ok(chunk_text) =
                        engine.fetch_chunk_text(&item.path, chunk.start_byte, chunk.end_byte)
                    {
                        let lines: Vec<&str> = chunk_text.lines().collect();
                        let (capped, _) = cap_lines(&lines, max_lines);
                        item.snippet = Some(capped);
                        continue;
                    }
                }
            }
        }

        if let Ok(symbols) = engine.store().find_symbols_by_name(&item.path) {
            if let Some(sym) = symbols.first() {
                let full_path = corpus_root.join(&sym.file_path);
                if let Ok(content) = fs::read_to_string(&full_path) {
                    let file_lines: Vec<&str> = content.lines().collect();
                    if sym.start_line > 0 && sym.start_line <= file_lines.len() {
                        let start_idx = sym.start_line - 1;
                        let end_idx = sym.end_line.min(file_lines.len());
                        let (capped, _) = cap_lines(&file_lines[start_idx..end_idx], max_lines);
                        item.snippet = Some(capped);
                        continue;
                    }
                }
            }
        }

        let full_path = corpus_root.join(&item.path);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let lines: Vec<&str> = content.lines().collect();
            let (capped, _) = cap_lines(&lines, max_lines);
            item.snippet = Some(capped);
        }
    }
}

/// Consolidated search tool: dispatches to a retrieval mode selected by `mode`
/// (default `hybrid`). Modes: `bm25`, `semantic`, `hybrid`, `graph`, `explain`.
///
/// This is a thin adapter: it obtains the engine's search service (which
/// resolves the retrieval backends internally) and delegates the mode dispatch
/// to it via the [`SearchService`] port, then applies detail/verbosity shaping
/// and JSON serialization. Every mode honors `modality` (docs|code|both) and
/// `detail` (ids|default) via [`apply_detail`]. `explain` returns the
/// score-breakdown shape ([`SearchService::explain`]) rather than a plain
/// result array.
fn handle_search(engine: &Engine, args: Value) -> Result<Value> {
    let params: SearchParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let mode = params.mode.as_deref().unwrap_or("hybrid");
    let is_semantic = mode == "semantic";
    let is_explain = mode == "explain";
    let modality = params
        .modality
        .as_deref()
        .and_then(ctxvault_common::types::Modality::from_str_name)
        .unwrap_or_default();
    let depth = params
        .depth
        .as_deref()
        .and_then(ctxvault_common::types::SearchDepth::from_str_name)
        .unwrap_or_default();

    // Semantic mode lazily initializes the embedder, but only once the fast-mode
    // guard (no vector index) has passed — mirroring the original ordering.
    if is_semantic && engine.has_vector_index() {
        let _ = engine.ensure_embedder()?;
    }

    // Build the search service from the engine (it resolves its own backends
    // internally) and dispatch through the port. Detail/verbosity shaping and
    // serialization stay here.
    let service = engine.search_service();

    let query = SearchQuery {
        query: params.query,
        mode: params.mode,
        limit: params.limit,
        modality,
        depth,
        graph_depth: params.graph_depth,
        edge_types: params.edge_types,
        edge_class: params.edge_class,
        decompose: params.decompose,
        snippets: params.snippets,
    };

    if is_explain {
        let mut explanations = service.explain(&query)?;

        // Tier-1: `detail=ids` strips snippets, leaving bare handles + score breakdown.
        if params.detail.as_deref() == Some("ids") {
            for e in &mut explanations {
                e.snippet = None;
            }
        }

        serde_json::to_value(explanations)
            .map_err(|e| Error::Config(format!("serialize error: {}", e)))
    } else {
        let results = service.search(&query)?;
        let results = apply_detail(results, params.detail.as_deref());

        let mut docs_items = Vec::new();
        let mut code_items = Vec::new();

        for r in results {
            let is_code = r
                .entity_kind
                .as_ref()
                .map(|k| k.is_code())
                .unwrap_or_else(|| !r.path.ends_with(".md"));
            if is_code {
                code_items.push(r);
            } else {
                docs_items.push(r);
            }
        }

        let k =
            if params.detail.as_deref() == Some("ids") { 0 } else { params.snippets.unwrap_or(3) };

        populate_top_snippets(engine, &mut docs_items, k, 40);
        populate_top_snippets(engine, &mut code_items, k, 40);

        let docs_partition =
            if !docs_items.is_empty() || modality != ctxvault_common::types::Modality::Code {
                Some(ctxvault_common::types::SearchPartition {
                    total_matches: docs_items.len(),
                    top_k_returned: docs_items.len(),
                    schema_envelope: ctxvault_common::types::SchemaEnvelope {
                        node_labels: vec![
                            "DocNode".to_string(),
                            "ADR".to_string(),
                            "Concept".to_string(),
                        ],
                        active_edges: vec![
                            "wikilink".to_string(),
                            "supersedes".to_string(),
                            "documents".to_string(),
                            "tag".to_string(),
                        ],
                    },
                    results: docs_items,
                })
            } else {
                None
            };

        let code_partition =
            if !code_items.is_empty() || modality != ctxvault_common::types::Modality::Docs {
                Some(ctxvault_common::types::SearchPartition {
                    total_matches: code_items.len(),
                    top_k_returned: code_items.len(),
                    schema_envelope: ctxvault_common::types::SchemaEnvelope {
                        node_labels: vec![
                            "CodeSymbol".to_string(),
                            "CodeChunk".to_string(),
                            "Function".to_string(),
                            "Method".to_string(),
                            "Struct".to_string(),
                        ],
                        active_edges: vec![
                            "calls".to_string(),
                            "implements".to_string(),
                            "imports".to_string(),
                            "defines".to_string(),
                        ],
                    },
                    results: code_items,
                })
            } else {
                None
            };

        let response =
            ctxvault_common::types::SearchResponse { docs: docs_partition, code: code_partition };

        serde_json::to_value(response).map_err(|e| Error::Config(format!("serialize error: {}", e)))
    }
}

/// Find related documents via PPR approximation.
fn handle_search_related(engine: &Engine, args: Value) -> Result<Value> {
    let params: SearchRelatedParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(10);
    let modality = params
        .modality
        .as_deref()
        .and_then(ctxvault_common::types::Modality::from_str_name)
        .unwrap_or_default();

    // Related search only traverses the graph, but the service is built the same
    // way `handle_search` builds it; the embedder is left as-is (never lazily
    // initialized here, matching prior behaviour) since related does not touch
    // it. Detail/verbosity shaping stays here.
    let service = engine.search_service();

    let results = service.search_related(&params.seeds, limit, modality)?;
    let results = apply_detail(results, params.detail.as_deref());

    serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Execute a Cypher-Lite graph path query compiled to SQLite recursive CTE.
fn handle_graph_match(engine: &Engine, args: Value) -> Result<Value> {
    let params: GraphMatchParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(20);
    let max_depth = params.max_depth.unwrap_or(3);

    let match_result = engine.graph_match(
        &params.pattern,
        params.edge_class.as_deref(),
        params.where_clause.as_deref(),
        limit,
        max_depth,
    )?;

    serde_json::to_value(match_result).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Detect communities via Leiden or Louvain, or architectural components overview.
fn handle_graph_communities(engine: &Engine, args: Value) -> Result<Value> {
    let params: GraphCommunitiesParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let view = params.view.as_deref().unwrap_or("raw");
    if view == "architecture" {
        let result = engine.graph().detect_communities_leiden();
        let densities = engine.graph().community_densities();
        let density_map: HashMap<usize, f64> =
            densities.into_iter().map(|d| (d.community_id, d.density)).collect();

        let mut clusters = Vec::new();
        let edges = engine.graph().get_all_edges();

        for comm in &result.communities {
            let comm_id = comm.id;
            let mut nodes = comm.members.clone();
            nodes.sort();
            let density = density_map.get(&comm_id).copied().unwrap_or(0.0);

            let mut node_degree: HashMap<String, usize> = HashMap::new();
            for edge in &edges {
                if nodes.contains(&edge.source) || nodes.contains(&edge.target) {
                    *node_degree.entry(edge.source.clone()).or_insert(0) += 1;
                    *node_degree.entry(edge.target.clone()).or_insert(0) += 1;
                }
            }
            let mut key_nodes: Vec<_> = nodes
                .iter()
                .filter_map(|n| node_degree.get(n).map(|deg| (n.clone(), *deg)))
                .collect();
            key_nodes.sort_by(|a, b| b.1.cmp(&a.1));
            let top_key_nodes: Vec<String> =
                key_nodes.into_iter().take(5).map(|(n, _)| n).collect();

            clusters.push(serde_json::json!({
                "component_id": comm_id,
                "node_count": nodes.len(),
                "internal_density": density,
                "top_nodes": top_key_nodes,
                "members": nodes,
            }));
        }

        return Ok(serde_json::json!({
            "algorithm": "leiden",
            "component_count": clusters.len(),
            "modularity": result.modularity,
            "components": clusters,
        }));
    }

    let algo = params.algorithm.as_deref().unwrap_or("leiden");
    let result = match algo {
        "louvain" => engine.graph().detect_communities(),
        _ => engine.graph().detect_communities_leiden(),
    };

    if params.include_density.unwrap_or(false) {
        let densities = engine.graph().community_densities();
        let response = serde_json::json!({
            "communities": result.communities,
            "modularity": result.modularity,
            "iterations": result.iterations,
            "community_densities": densities,
        });
        Ok(response)
    } else {
        serde_json::to_value(result).map_err(|e| Error::Config(format!("serialize error: {}", e)))
    }
}

/// Helper to apply index_mode overrides dynamically on an engine.
fn apply_index_mode_override(
    engine: &mut Engine,
    index_mode: Option<&str>,
    docs_embed: Option<bool>,
    fast: Option<bool>,
) -> Result<()> {
    if let Some(mode_str) = index_mode {
        match mode_str.to_lowercase().as_str() {
            "fast" => engine.set_index_mode(ctxvault_common::config::IndexMode::Fast),
            "docs-embed" | "docsembed" | "docs_embed" => {
                engine.set_index_mode(ctxvault_common::config::IndexMode::DocsEmbed);
            }
            "full" => engine.set_index_mode(ctxvault_common::config::IndexMode::Full),
            other => return Err(Error::Config(format!("invalid index_mode '{}'", other))),
        }
    } else if let Some(true) = docs_embed {
        engine.set_index_mode(ctxvault_common::config::IndexMode::DocsEmbed);
    } else if let Some(fast) = fast {
        engine.set_index_mode(if fast {
            ctxvault_common::config::IndexMode::Fast
        } else {
            ctxvault_common::config::IndexMode::Full
        });
    }
    Ok(())
}

/// Sync or reindex corpus in configurable batches. Supports mode: "delta" | "full" | "reembed".
fn handle_sync_corpus(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SyncCorpusParams = serde_json::from_value(args).unwrap_or(SyncCorpusParams {
        mode: None,
        batch_size: None,
        resume: None,
        fast: None,
        docs_embed: None,
        index_mode: None,
    });
    apply_index_mode_override(
        engine,
        params.index_mode.as_deref(),
        params.docs_embed,
        params.fast,
    )?;

    match params.mode.as_deref().unwrap_or("delta") {
        "reembed" => {
            let was_stale = engine.vectors_stale();
            let old_version = engine.stored_model_version().map(|s| s.to_string());
            let chunks_reembedded = engine.reembed()?;
            let new_version = engine.stored_model_version().unwrap_or("unknown").to_string();
            Ok(serde_json::json!({
                "status": "complete",
                "mode": "reembed",
                "chunks_reembedded": chunks_reembedded,
                "was_stale": was_stale,
                "previous_model_version": old_version,
                "current_model_version": new_version,
            }))
        }
        "full" => {
            let batch_size = params.batch_size.unwrap_or(50);
            let resume = params.resume.unwrap_or(true);
            let count = engine.full_reindex_paginated(batch_size, resume)?;
            Ok(serde_json::json!({
                "status": "complete",
                "mode": "full",
                "files_indexed": count,
                "batch_size": batch_size,
                "resumed": resume,
            }))
        }
        _ => {
            let batch_size = params.batch_size.unwrap_or(50);
            let result = engine.delta_scan_paginated(batch_size)?;
            Ok(serde_json::json!({
                "status": "complete",
                "mode": "delta",
                "new_files": result.new_files.len(),
                "modified_files": result.modified_files.len(),
                "deleted_files": result.deleted_files.len(),
                "new": result.new_files,
                "modified": result.modified_files,
                "deleted": result.deleted_files,
            }))
        }
    }
}

/// Per-corpus statistics (document counts, mode, chunking, embedding model).
fn corpus_stats(engine: &Engine) -> Result<Value> {
    let files = engine.store().list_files()?;
    let is_indexed = engine.is_indexed();
    Ok(serde_json::json!({
        "status": "healthy",
        "corpus_name": engine.config().name,
        "corpus_path": engine.config().path,
        "document_count": files.len(),
        "indexed": is_indexed,
        "mode": format!("{:?}", engine.config().mode),
        "index_mode": format!("{:?}", engine.config().index_mode),
        "chunking": format!("{:?}", engine.config().chunking.strategy),
        "embedding_model": engine.config().embedding.model,
    }))
}

/// Consolidated status tool (engine-level): combines per-corpus statistics,
/// indexing progress, graph topology/density, and coverage inspection.
fn handle_status(engine: &Engine, args: Value) -> Result<Value> {
    let params: StatusParams =
        serde_json::from_value(args).unwrap_or(StatusParams { scope: None, paths: None });
    let scope = params.scope.as_deref().unwrap_or("all");

    match scope {
        "corpus" => corpus_stats(engine),
        "indexing" => {
            let status = engine.get_indexing_status()?;
            serde_json::to_value(status)
                .map_err(|e| Error::Config(format!("serialize error: {}", e)))
        }
        "graph" => {
            let stats = engine.graph().stats();
            let density = engine.analyze_density(10);
            Ok(serde_json::json!({
                "stats": stats,
                "density": density,
            }))
        }
        "coverage" => {
            let paths = params.paths.unwrap_or_default();
            check_index_coverage_inner(engine, &paths)
        }
        _ => {
            let corpus = corpus_stats(engine)?;
            let indexing = serde_json::to_value(engine.get_indexing_status()?)
                .map_err(|e| Error::Config(format!("serialize error: {}", e)))?;
            let stats = engine.graph().stats();
            let density = engine.analyze_density(10);
            Ok(serde_json::json!({
                "corpus": corpus,
                "indexing": indexing,
                "graph": {
                    "stats": stats,
                    "density": density,
                },
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Write tool handlers
// ---------------------------------------------------------------------------

/// Build note content from optional frontmatter and body text.
fn build_note_content(frontmatter: Option<&Value>, template: Option<&str>, body: &str) -> String {
    let mut content = String::new();

    // Merge template into frontmatter if provided.
    let has_fm = frontmatter.is_some() || template.is_some();
    if has_fm {
        content.push_str("---\n");
        let mut fm_map = match frontmatter {
            Some(Value::Object(map)) => map.clone(),
            _ => serde_json::Map::new(),
        };
        if let Some(tmpl) = template {
            let _ = fm_map.insert("template".to_string(), Value::String(tmpl.to_string()));
        }
        if let Ok(yaml) = serde_yaml::to_string(&Value::Object(fm_map)) {
            content.push_str(&yaml);
        }
        content.push_str("---\n\n");
    }

    content.push_str(body);
    if !body.ends_with('\n') {
        content.push('\n');
    }
    content
}

/// Write a note to disk (create, overwrite, append, or prepend) and index it.
fn handle_write_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: WriteNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    if engine.config().mode == CorpusMode::ReadOnly {
        return Err(Error::Config(format!("corpus '{}' is read-only", engine.config().name)));
    }

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    let mode = params.mode.as_deref().unwrap_or("create");

    let new_content = match mode {
        "create" => {
            if full_path.exists() {
                return Err(Error::Config(format!("file already exists: {}", params.path)));
            }
            build_note_content(
                params.frontmatter.as_ref(),
                params.template.as_deref(),
                &params.content,
            )
        }
        "overwrite" => {
            if params.frontmatter.is_some() || params.template.is_some() {
                build_note_content(
                    params.frontmatter.as_ref(),
                    params.template.as_deref(),
                    &params.content,
                )
            } else {
                let mut s = params.content.clone();
                if !s.ends_with('\n') {
                    s.push('\n');
                }
                s
            }
        }
        "append" => {
            if !full_path.exists() {
                return Err(Error::NotFound(format!("file not found: {}", params.path)));
            }
            let mut existing = fs::read_to_string(&full_path).map_err(|e| {
                Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("cannot read {}: {}", params.path, e),
                ))
            })?;
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(&params.content);
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing
        }
        "prepend" => {
            if !full_path.exists() {
                return Err(Error::NotFound(format!("file not found: {}", params.path)));
            }
            let existing = fs::read_to_string(&full_path).map_err(|e| {
                Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("cannot read {}: {}", params.path, e),
                ))
            })?;
            let mut s = params.content.clone();
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&existing);
            s
        }
        other => {
            return Err(Error::Config(format!("unrecognized write mode: '{}'", other)));
        }
    };

    // Ensure parent directory exists.
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("cannot create directory for {}: {}", params.path, e),
            ))
        })?;
    }

    // Write file atomically-ish.
    fs::write(&full_path, &new_content).map_err(|e| {
        Error::Io(std::io::Error::new(e.kind(), format!("cannot write {}: {}", params.path, e)))
    })?;

    // Re-index.
    engine.index_file(&params.path, &new_content)?;
    engine.commit()?;

    debug!("Written note: {} (mode={})", params.path, mode);

    Ok(serde_json::json!({
        "path": params.path,
        "mode": mode,
        "written": true
    }))
}

/// Delete a note from disk and all indices.
fn handle_delete_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: DeleteNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    if engine.config().mode == CorpusMode::ReadOnly {
        return Err(Error::Config(format!("corpus '{}' is read-only", engine.config().name)));
    }

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    if !full_path.exists() {
        return Err(Error::NotFound(format!("file not found: {}", params.path)));
    }

    // Remove file from disk.
    fs::remove_file(&full_path).map_err(|e| {
        Error::Io(std::io::Error::new(e.kind(), format!("cannot delete {}: {}", params.path, e)))
    })?;

    // Remove from indices.
    engine.remove_file(&params.path)?;
    engine.commit()?;

    debug!("Deleted note: {}", params.path);

    Ok(serde_json::json!({
        "path": params.path,
        "deleted": true
    }))
}

/// Move/rename a note, updating wikilinks in other files.
fn handle_move_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: MoveNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    if engine.config().mode == CorpusMode::ReadOnly {
        return Err(Error::Config(format!("corpus '{}' is read-only", engine.config().name)));
    }

    let corpus_path = PathBuf::from(&engine.config().path);
    let from_full = corpus_path.join(&params.from);
    let to_full = corpus_path.join(&params.to);

    if !from_full.exists() {
        return Err(Error::NotFound(format!("source file not found: {}", params.from)));
    }

    if to_full.exists() {
        return Err(Error::Config(format!("destination already exists: {}", params.to)));
    }

    // Ensure destination parent directory exists.
    if let Some(parent) = to_full.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("cannot create directory for {}: {}", params.to, e),
            ))
        })?;
    }

    // Move the file.
    fs::rename(&from_full, &to_full).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("cannot move {} to {}: {}", params.from, params.to, e),
        ))
    })?;

    // Compute old and new note names (filename without extension) for wikilink rewriting.
    let old_name =
        Path::new(&params.from).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let new_name =
        Path::new(&params.to).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();

    // Rewrite wikilinks in other .md files if the note name changed.
    let mut links_rewritten: usize = 0;
    if old_name != new_name && !old_name.is_empty() {
        let old_link = format!("[[{}]]", old_name);
        let new_link = format!("[[{}]]", new_name);

        // Walk all .md files in corpus.
        let files = walk_markdown_files_for_rewrite(&corpus_path)?;
        for (rel_path, file_path) in &files {
            // Skip the moved file itself.
            if *rel_path == params.to {
                continue;
            }

            let content = match fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            if content.contains(&old_link) {
                let updated = content.replace(&old_link, &new_link);
                if let Err(e) = fs::write(file_path, &updated) {
                    debug!("Failed to rewrite links in {}: {}", rel_path, e);
                    continue;
                }
                // Re-index the modified file.
                engine.index_file(rel_path, &updated)?;
                links_rewritten += 1;
            }
        }
    }

    // Remove old path from engine.
    engine.remove_file(&params.from)?;

    // Index the file at the new path.
    let new_content = fs::read_to_string(&to_full).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("cannot read moved file {}: {}", params.to, e),
        ))
    })?;
    engine.index_file(&params.to, &new_content)?;
    engine.commit()?;

    debug!("Moved note: {} -> {} ({} links rewritten)", params.from, params.to, links_rewritten);

    Ok(serde_json::json!({
        "from": params.from,
        "to": params.to,
        "moved": true,
        "links_rewritten": links_rewritten
    }))
}

/// Walk .md files for wikilink rewriting (same as engine's internal walk but accessible here).
fn walk_markdown_files_for_rewrite(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut results = Vec::new();
    if !root.exists() {
        return Ok(results);
    }
    walk_dir_for_rewrite(root, root, &mut results)?;
    Ok(results)
}

fn walk_dir_for_rewrite(
    root: &Path,
    current: &Path,
    results: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let entries = fs::read_dir(current)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }
            walk_dir_for_rewrite(root, &path, results)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = path.strip_prefix(root).map_err(|e| {
                Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            })?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            results.push((rel_str, path.clone()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Validation tool handlers
// ---------------------------------------------------------------------------

/// Load templates for the corpus.
fn load_corpus_templates(engine: &Engine) -> Result<HashMap<String, Template>> {
    let corpus_path = PathBuf::from(&engine.config().path);
    let templates_dir = corpus_path.join(&engine.config().templates_dir);
    Template::load_from_dir(&templates_dir)
}

/// Validate a single note against its declared template.
fn validate_single_note(
    engine: &Engine,
    path: &str,
) -> Result<ctxvault_core::template::ValidationResult> {
    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(path);

    let content = fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", path, e)))?;

    let doc = ctxvault_core::parser::parse_document(Path::new(path), &content)?;

    let template_name = doc.template.clone();

    let (valid, issues, tmpl_name) = if let Some(ref name) = template_name {
        let templates = load_corpus_templates(engine)?;
        if let Some(tmpl) = templates.get(name) {
            let issues = tmpl.validate(&doc.frontmatter, &doc.content);
            let valid =
                !issues.iter().any(|i| i.severity == ctxvault_core::template::Severity::Error);
            (valid, issues, Some(name.clone()))
        } else {
            let issues = vec![ctxvault_core::template::ValidationIssue {
                severity: ctxvault_core::template::Severity::Warning,
                message: format!("template '{}' not found in templates directory", name),
                field: Some("template".to_string()),
            }];
            (true, issues, Some(name.clone()))
        }
    } else {
        (true, Vec::new(), None)
    };

    Ok(ctxvault_core::template::ValidationResult {
        path: path.to_string(),
        template: tmpl_name,
        valid,
        issues,
    })
}

/// Validate all templated notes in the corpus.
fn validate_corpus_notes(
    engine: &Engine,
    limit: Option<usize>,
) -> Result<Vec<ctxvault_core::template::ValidationResult>> {
    let templates = load_corpus_templates(engine)?;
    let files = engine.store().list_files()?;
    let corpus_path = PathBuf::from(&engine.config().path);

    let mut results: Vec<ctxvault_core::template::ValidationResult> = Vec::new();

    for file in &files {
        let tmpl_name = match &file.template {
            Some(name) => name.clone(),
            None => continue,
        };

        let full_path = corpus_path.join(&file.path);
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let doc = match ctxvault_core::parser::parse_document(Path::new(&file.path), &content) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let issues = if let Some(tmpl) = templates.get(&tmpl_name) {
            tmpl.validate(&doc.frontmatter, &doc.content)
        } else {
            vec![ctxvault_core::template::ValidationIssue {
                severity: ctxvault_core::template::Severity::Warning,
                message: format!("template '{}' not found in templates directory", tmpl_name),
                field: Some("template".to_string()),
            }]
        };

        if !issues.is_empty() {
            let valid =
                !issues.iter().any(|i| i.severity == ctxvault_core::template::Severity::Error);
            results.push(ctxvault_core::template::ValidationResult {
                path: file.path.clone(),
                template: Some(tmpl_name),
                valid,
                issues,
            });
        }

        if let Some(limit_val) = limit {
            if results.len() >= limit_val {
                break;
            }
        }
    }

    Ok(results)
}

/// Validate structural ontology and graph integrity (broken links, cycle detection, orphan ADRs).
fn run_taxonomy_validation(engine: &Engine) -> Result<Value> {
    let files = engine.store().list_files()?;
    let existing_paths: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();

    let broken_links = engine.graph().detect_broken_links(&existing_paths);
    let circular_dependencies = engine.graph().detect_circular_dependencies(&[
        "supersedes",
        "depends_on",
        "implements",
        "parent_of",
    ]);

    let adr_paths: Vec<String> = files
        .iter()
        .filter(|f| {
            f.template.as_deref() == Some("adr")
                || f.template.as_deref() == Some("decision-record")
                || f.path.starts_with("docs/adrs/")
                || f.path.starts_with("adrs/")
        })
        .map(|f| f.path.clone())
        .collect();
    let orphan_adrs = engine.graph().detect_orphan_adrs(&adr_paths);

    let valid =
        broken_links.is_empty() && circular_dependencies.is_empty() && orphan_adrs.is_empty();

    Ok(serde_json::json!({
        "valid": valid,
        "broken_links_count": broken_links.len(),
        "broken_links": broken_links,
        "circular_dependencies_count": circular_dependencies.len(),
        "circular_dependencies": circular_dependencies,
        "orphan_adrs_count": orphan_adrs.len(),
        "orphan_adrs": orphan_adrs,
    }))
}

/// Unified validation tool: validates a single note, entire corpus notes against templates, and/or graph taxonomy.
fn handle_validate(engine: &Engine, args: Value) -> Result<Value> {
    let params: ValidateParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    if let Some(path) = &params.path {
        let note_res = validate_single_note(engine, path)?;
        if params.check_taxonomy == Some(true) {
            let tax_res = run_taxonomy_validation(engine)?;
            let valid = note_res.valid && tax_res["valid"].as_bool().unwrap_or(true);
            Ok(serde_json::json!({
                "valid": valid,
                "note": note_res,
                "taxonomy": tax_res,
            }))
        } else {
            serde_json::to_value(note_res)
                .map_err(|e| Error::Config(format!("serialize error: {}", e)))
        }
    } else {
        let check_taxonomy = params.check_taxonomy.unwrap_or(true);
        let note_issues = validate_corpus_notes(engine, params.limit)?;
        let notes_valid = note_issues.is_empty() || note_issues.iter().all(|r| r.valid);

        if check_taxonomy {
            let tax_res = run_taxonomy_validation(engine)?;
            let tax_valid = tax_res["valid"].as_bool().unwrap_or(true);
            Ok(serde_json::json!({
                "valid": notes_valid && tax_valid,
                "notes_with_issues": note_issues,
                "taxonomy": tax_res,
            }))
        } else {
            Ok(serde_json::json!({
                "valid": notes_valid,
                "notes_with_issues": note_issues,
            }))
        }
    }
}

/// List all available templates.
fn handle_list_templates(engine: &Engine, _args: Value) -> Result<Value> {
    let templates = load_corpus_templates(engine)?;

    let mut list: Vec<&Template> = templates.values().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));

    serde_json::to_value(list).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ctxvault_common::config::{
        ChunkingConfig, CorpusConfig, CorpusMode, EdgeClass, EdgeSource, EdgeTypeConfig,
        EmbeddingConfig, GraphConfig, IndexMode,
    };
    use ctxvault_common::types::EdgeProvenance;
    use std::fs;
    use tempfile::TempDir;

    /// Create a minimal corpus config pointing at the given path.
    fn test_config(corpus_path: &std::path::Path) -> CorpusConfig {
        CorpusConfig {
            name: "test".to_string(),
            path: corpus_path.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Full,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig {
                edge_types: vec![EdgeTypeConfig {
                    name: "Wikilink".to_string(),
                    source: EdgeSource::Wikilink,
                    weight: 1.0,
                    bidirectional: false,
                    field: None,
                    direction: None,
                    max_frequency: None,
                    class: None,
                    description: None,
                    allowed_source_templates: None,
                    allowed_target_templates: None,
                }],
            },
            templates_dir: ".templates".to_string(),
        }
    }

    /// Create a test engine with an empty corpus.
    fn create_test_engine(tmp: &TempDir) -> Engine {
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");
        let config = test_config(&corpus_dir);
        Engine::open(config, &index_dir).unwrap()
    }

    #[test]
    fn test_registry_has_all_tools() {
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let tools = registry.list();
        assert_eq!(tools.len(), 17, "Expected 17 tools registered");

        // Verify each expected tool exists.
        let expected = [
            "read_file",
            "get_snippet",
            "list_notes",
            "search",
            "search_related",
            "graph_match",
            "graph_communities",
            "write_note",
            "delete_note",
            "move_note",
            "validate",
            "list_templates",
            "status",
            "list_corpora",
            "sync_corpus",
            "index_corpus",
            "unload_corpus",
        ];

        assert_eq!(expected.len(), 17, "expected-name list must match the 17-tool count");

        for name in expected {
            assert!(registry.get(name).is_some(), "Tool '{}' should be registered", name);
        }

        // The consolidated / deleted legacy tools must not be registered.
        for gone in [
            "read_note",
            "read_code_file",
            "read_multiple",
            "get_frontmatter",
            "create_note",
            "update_note",
            "promote_concept",
            "validate_note",
            "validate_corpus",
            "validate_taxonomy",
            "analyze_density",
            "find_semantic_gaps",
            "suggest_splits",
            "coverage_report",
            "check_index_coverage",
            "corpus_list",
            "reembed_corpus",
            "reindex_corpus",
            "get_symbol_definition",
            "get_architecture",
            "search_bm25",
            "search_semantic",
            "search_hybrid",
            "search_graph",
            "search_explain",
            "get_status",
            "get_corpus_stats",
            "get_indexing_status",
            "backlinks",
            "forwardlinks",
            "graph_path",
            "graph_stats",
            "graph_subgraph",
            "list_edge_types",
            "traverse_lineage",
            "find_callers",
            "detect_changes",
        ] {
            assert!(registry.get(gone).is_none(), "Tool '{}' must no longer be registered", gone);
        }

        // Verify read-only classification
        assert!(registry.is_read_only("read_file"));
        assert!(registry.is_read_only("get_snippet"));
        assert!(registry.is_read_only("list_notes"));
        assert!(registry.is_read_only("search"));
        assert!(registry.is_read_only("search_related"));
        assert!(registry.is_read_only("graph_match"));
        assert!(registry.is_read_only("graph_communities"));
        assert!(registry.is_read_only("validate"));
        assert!(registry.is_read_only("list_templates"));
        assert!(registry.is_read_only("status"));
        assert!(registry.is_read_only("list_corpora"));
        assert!(!registry.is_read_only("write_note"));
        assert!(!registry.is_read_only("delete_note"));
        assert!(!registry.is_read_only("move_note"));
        assert!(!registry.is_read_only("sync_corpus"));
        assert!(!registry.is_read_only("index_corpus"));
        assert!(!registry.is_read_only("unload_corpus"));
    }

    #[test]
    fn test_tool_profiles_gate_listing() {
        let all = MultiCorpusToolRegistry::with_profile(ToolProfile::All);
        let analysis = MultiCorpusToolRegistry::with_profile(ToolProfile::Analysis);
        let scout = MultiCorpusToolRegistry::with_profile(ToolProfile::Scout);

        let all_count = all.list().len();
        let analysis_count = analysis.list().len();
        let scout_count = scout.list().len();

        // scout ⊂ analysis ⊂ all.
        assert!(scout_count < analysis_count, "scout must expose fewer tools than analysis");
        assert!(analysis_count < all_count, "analysis must expose fewer tools than all");
        assert_eq!(all_count, 17, "all profile advertises every registered tool");
        assert_eq!(analysis_count, 11, "analysis profile advertises scout + analysis tools");
        assert_eq!(scout_count, 6, "scout profile advertises the minimal set");

        // scout includes core retrieval/fetch but not writes or analysis-only tools.
        let scout_names: HashSet<&str> = scout.list().iter().map(|t| t.name.as_str()).collect();
        assert!(scout_names.contains("search"));
        assert!(scout_names.contains("get_snippet"));
        assert!(scout_names.contains("read_file"));
        assert!(scout_names.contains("status"));
        assert!(!scout_names.contains("write_note"));
        assert!(!scout_names.contains("graph_match"));

        // Hidden tools still execute (advertise-only filtering): write_note is
        // registered even though scout does not advertise it.
        assert!(scout.registry().get("write_note").is_some());

        // analysis adds read-only tools but still hides writes.
        let analysis_names: HashSet<&str> =
            analysis.list().iter().map(|t| t.name.as_str()).collect();
        assert!(analysis_names.contains("graph_match"));
        assert!(analysis_names.contains("graph_communities"));
        assert!(analysis_names.contains("validate"));
        assert!(analysis_names.contains("list_corpora"));
        assert!(!analysis_names.contains("write_note"));
        assert!(!analysis_names.contains("sync_corpus"));
    }

    #[test]
    fn test_read_only_tool_execution() {
        let tmp = TempDir::new().unwrap();
        let engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        // Read tool with immutable &engine should succeed
        let result = registry.execute_read("list_notes", &engine, serde_json::json!({})).unwrap();
        let notes: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(notes.is_empty());

        // Calling mutating tool with execute_read should return error
        let err = registry.execute_read(
            "write_note",
            &engine,
            serde_json::json!({ "path": "fail.md", "content": "hello" }),
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_list_notes_empty() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry.execute("list_notes", &mut engine, serde_json::json!({})).unwrap();

        let notes: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(notes.is_empty(), "Empty corpus should return empty list");
    }

    #[test]
    fn test_search_bm25_tool() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");
        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        // Write and index a test file.
        let content =
            "# Rust Programming\n\nRust is a systems programming language focused on safety.\n";
        fs::write(corpus_dir.join("rust.md"), content).unwrap();
        engine.index_file("rust.md", content).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "search",
                &mut engine,
                serde_json::json!({ "query": "systems programming", "mode": "bm25" }),
            )
            .unwrap();

        let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(result).unwrap();
        let docs = resp.docs.unwrap();
        assert!(!docs.results.is_empty(), "Should find indexed file via search");
        assert_eq!(docs.results[0].path, "rust.md");
        assert!(docs.results[0].graph_affordances.is_some());
    }

    #[test]
    fn test_write_note_create() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "write_note",
                &mut engine,
                serde_json::json!({
                    "path": "new-note.md",
                    "content": "# Hello\n\nThis is a new note.",
                    "frontmatter": { "tags": ["test", "demo"] }
                }),
            )
            .unwrap();

        assert_eq!(result["path"], "new-note.md");
        assert_eq!(result["written"], true);
        assert_eq!(result["mode"], "create");

        // Verify file exists on disk.
        let corpus_dir = tmp.path().join("corpus");
        let file_content = fs::read_to_string(corpus_dir.join("new-note.md")).unwrap();
        assert!(file_content.contains("# Hello"));
        assert!(file_content.contains("---"));

        // Verify indexed (searchable).
        let search_result = registry
            .execute(
                "search",
                &mut engine,
                serde_json::json!({ "query": "new note", "mode": "bm25" }),
            )
            .unwrap();
        let resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(search_result).unwrap();
        assert!(!resp.docs.unwrap().results.is_empty());
    }

    #[test]
    fn test_write_note_with_template() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "write_note",
                &mut engine,
                serde_json::json!({
                    "path": "templated.md",
                    "content": "Body text here.",
                    "template": "meeting"
                }),
            )
            .unwrap();

        assert_eq!(result["written"], true);

        let corpus_dir = tmp.path().join("corpus");
        let file_content = fs::read_to_string(corpus_dir.join("templated.md")).unwrap();
        assert!(file_content.contains("template: meeting"));
    }

    #[test]
    fn test_write_note_already_exists() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");
        fs::write(corpus_dir.join("existing.md"), "# Existing").unwrap();

        let result = registry.execute(
            "write_note",
            &mut engine,
            serde_json::json!({ "path": "existing.md", "content": "overwrite?", "mode": "create" }),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_write_note_overwrite() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");
        let original = "# Original\n\nOld content.\n";
        fs::write(corpus_dir.join("update-me.md"), original).unwrap();
        engine.index_file("update-me.md", original).unwrap();
        engine.commit().unwrap();

        let result = registry
            .execute(
                "write_note",
                &mut engine,
                serde_json::json!({
                    "path": "update-me.md",
                    "content": "# Replaced\n\nNew content.",
                    "mode": "overwrite"
                }),
            )
            .unwrap();

        assert_eq!(result["written"], true);
        assert_eq!(result["mode"], "overwrite");

        let file_content = fs::read_to_string(corpus_dir.join("update-me.md")).unwrap();
        assert!(file_content.contains("New content"));
        assert!(!file_content.contains("Old content"));
    }

    #[test]
    fn test_write_note_append() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");
        let original = "# Append Test\n\nFirst line.\n";
        fs::write(corpus_dir.join("append.md"), original).unwrap();
        engine.index_file("append.md", original).unwrap();
        engine.commit().unwrap();

        let result = registry
            .execute(
                "write_note",
                &mut engine,
                serde_json::json!({
                    "path": "append.md",
                    "content": "Second line.",
                    "mode": "append"
                }),
            )
            .unwrap();

        assert_eq!(result["written"], true);
        assert_eq!(result["mode"], "append");

        let file_content = fs::read_to_string(corpus_dir.join("append.md")).unwrap();
        assert!(file_content.contains("First line."));
        assert!(file_content.contains("Second line."));
    }

    #[test]
    fn test_write_note_prepend() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");
        let original = "# Prepend Test\n\nOriginal.\n";
        fs::write(corpus_dir.join("prepend.md"), original).unwrap();
        engine.index_file("prepend.md", original).unwrap();
        engine.commit().unwrap();

        let result = registry
            .execute(
                "write_note",
                &mut engine,
                serde_json::json!({
                    "path": "prepend.md",
                    "content": "Prepended text.",
                    "mode": "prepend"
                }),
            )
            .unwrap();

        assert_eq!(result["written"], true);
        assert_eq!(result["mode"], "prepend");

        let file_content = fs::read_to_string(corpus_dir.join("prepend.md")).unwrap();
        // Prepended text should appear before original content.
        let prepend_pos = file_content.find("Prepended text.").unwrap();
        let original_pos = file_content.find("Original.").unwrap();
        assert!(prepend_pos < original_pos);
    }

    #[test]
    fn test_delete_note_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");
        let content = "# Delete Me\n\nGoing away.\n";
        fs::write(corpus_dir.join("delete-me.md"), content).unwrap();
        engine.index_file("delete-me.md", content).unwrap();
        engine.commit().unwrap();

        let result = registry
            .execute("delete_note", &mut engine, serde_json::json!({ "path": "delete-me.md" }))
            .unwrap();

        assert_eq!(result["deleted"], true);

        // File should be gone from disk.
        assert!(!corpus_dir.join("delete-me.md").exists());

        // Should not be found in search.
        let search_result = registry
            .execute(
                "search",
                &mut engine,
                serde_json::json!({ "query": "Going away", "mode": "bm25" }),
            )
            .unwrap();
        let resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(search_result).unwrap();
        let hits = resp.docs.map(|d| d.results).unwrap_or_default();
        assert!(hits.is_empty() || hits.iter().all(|h| h.path != "delete-me.md"));
    }

    #[test]
    fn test_delete_note_not_found() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry.execute(
            "delete_note",
            &mut engine,
            serde_json::json!({ "path": "nonexistent.md" }),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_move_note_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");

        // Create the note to move.
        let content = "# Alpha\n\nAlpha content.\n";
        fs::write(corpus_dir.join("alpha.md"), content).unwrap();
        engine.index_file("alpha.md", content).unwrap();

        // Create another note that links to alpha.
        let linker = "# Linker\n\nSee [[alpha]] for details.\n";
        fs::write(corpus_dir.join("linker.md"), linker).unwrap();
        engine.index_file("linker.md", linker).unwrap();
        engine.commit().unwrap();

        // Move alpha to beta.
        let result = registry
            .execute(
                "move_note",
                &mut engine,
                serde_json::json!({ "from": "alpha.md", "to": "beta.md" }),
            )
            .unwrap();

        assert_eq!(result["moved"], true);
        assert_eq!(result["from"], "alpha.md");
        assert_eq!(result["to"], "beta.md");
        assert_eq!(result["links_rewritten"], 1);

        // Old file should be gone, new file should exist.
        assert!(!corpus_dir.join("alpha.md").exists());
        assert!(corpus_dir.join("beta.md").exists());

        // Linker file should now reference [[beta]].
        let linker_content = fs::read_to_string(corpus_dir.join("linker.md")).unwrap();
        assert!(linker_content.contains("[[beta]]"));
        assert!(!linker_content.contains("[[alpha]]"));
    }

    #[test]
    fn test_move_note_to_subdirectory() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");
        let content = "# Move Me\n\nContent.\n";
        fs::write(corpus_dir.join("movable.md"), content).unwrap();
        engine.index_file("movable.md", content).unwrap();
        engine.commit().unwrap();

        let result = registry
            .execute(
                "move_note",
                &mut engine,
                serde_json::json!({ "from": "movable.md", "to": "archive/movable.md" }),
            )
            .unwrap();

        assert_eq!(result["moved"], true);
        assert!(!corpus_dir.join("movable.md").exists());
        assert!(corpus_dir.join("archive/movable.md").exists());
    }

    // ─── Multi-Corpus Routing Tests ────────────────────────────────────

    #[test]
    fn test_multi_corpus_registry_has_status() {
        let registry = MultiCorpusToolRegistry::new();
        let tools = registry.list();

        assert_eq!(tools.len(), 17, "Expected 17 tools in multi-corpus registry");
        assert!(
            registry.registry().get("status").is_some(),
            "consolidated status tool should be registered"
        );
        // The old status tools/aliases are gone.
        assert!(registry.registry().get("get_status").is_none());
        assert!(registry.registry().get("get_corpus_stats").is_none());
        assert!(registry.registry().get("get_indexing_status").is_none());
    }

    #[test]
    fn test_multi_corpus_routing_default() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let mut manager = ctxvault_core::corpus_manager::CorpusManager::new();
        let config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Full,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };
        manager.add_corpus(config).unwrap();

        // Index a file in wiki.
        {
            let engine = manager.get_engine_mut("wiki").unwrap();
            let content = "# Wiki Note\n\nWiki content here.\n";
            fs::write(wiki_dir.join("note.md"), content).unwrap();
            engine.index_file("note.md", content).unwrap();
            engine.commit().unwrap();
        }

        let registry = MultiCorpusToolRegistry::new();

        // Search without corpus param — should use default (wiki).
        let result = registry
            .execute(
                "search",
                &mut manager,
                serde_json::json!({ "query": "wiki content", "mode": "bm25" }),
            )
            .unwrap();

        let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(result).unwrap();
        let results = resp.docs.unwrap().results;
        assert!(!results.is_empty(), "Should find wiki note via default corpus");
        assert_eq!(results[0].path, "note.md");
    }

    #[test]
    fn test_multi_corpus_routing_explicit() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        let docs_dir = tmp.path().join("docs");
        fs::create_dir_all(&wiki_dir).unwrap();
        fs::create_dir_all(&docs_dir).unwrap();

        let mut manager = ctxvault_core::corpus_manager::CorpusManager::new();

        let wiki_config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Full,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };
        let docs_config = CorpusConfig {
            name: "docs".to_string(),
            path: docs_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Full,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };

        manager.add_corpus(wiki_config).unwrap();
        manager.add_corpus(docs_config).unwrap();

        // Index different content in each corpus.
        {
            let engine = manager.get_engine_mut("wiki").unwrap();
            let content = "# Rust Wiki\n\nRust programming language notes.\n";
            fs::write(wiki_dir.join("rust.md"), content).unwrap();
            engine.index_file("rust.md", content).unwrap();
            engine.commit().unwrap();
        }
        {
            let engine = manager.get_engine_mut("docs").unwrap();
            let content = "# Python Docs\n\nPython documentation guide.\n";
            fs::write(docs_dir.join("python.md"), content).unwrap();
            engine.index_file("python.md", content).unwrap();
            engine.commit().unwrap();
        }

        let registry = MultiCorpusToolRegistry::new();

        // Search in wiki corpus explicitly.
        let result = registry
            .execute(
                "search",
                &mut manager,
                serde_json::json!({ "query": "programming", "mode": "bm25", "corpus": "wiki" }),
            )
            .unwrap();
        let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(result).unwrap();
        let results = resp.docs.unwrap().results;
        assert!(!results.is_empty(), "Should find rust.md in wiki");
        assert_eq!(results[0].path, "rust.md");

        // Search in docs corpus explicitly.
        let result = registry
            .execute(
                "search",
                &mut manager,
                serde_json::json!({ "query": "documentation", "mode": "bm25", "corpus": "docs" }),
            )
            .unwrap();
        let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(result).unwrap();
        let results = resp.docs.unwrap().results;
        assert!(!results.is_empty(), "Should find python.md in docs");
        assert_eq!(results[0].path, "python.md");

        // Verify isolation: searching wiki for python returns nothing.
        let result = registry
            .execute(
                "search",
                &mut manager,
                serde_json::json!({ "query": "python documentation", "mode": "bm25", "corpus": "wiki" }),
            )
            .unwrap();
        let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(result).unwrap();
        let results = resp.docs.map(|d| d.results).unwrap_or_default();
        assert!(
            results.is_empty() || results.iter().all(|r| r.path != "python.md"),
            "Wiki corpus should not contain python.md"
        );
    }

    #[test]
    fn test_multi_corpus_fan_out_tags_by_corpus() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        let docs_dir = tmp.path().join("docs");
        fs::create_dir_all(&wiki_dir).unwrap();
        fs::create_dir_all(&docs_dir).unwrap();

        let mut manager = ctxvault_core::corpus_manager::CorpusManager::new();
        for (name, dir) in [("wiki", &wiki_dir), ("docs", &docs_dir)] {
            let config = CorpusConfig {
                name: name.to_string(),
                path: dir.to_string_lossy().to_string(),
                mode: CorpusMode::ReadWrite,
                index_mode: IndexMode::Full,
                chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
                embedding: EmbeddingConfig::default(),
                graph: GraphConfig { edge_types: Vec::new() },
                templates_dir: ".templates".to_string(),
            };
            manager.add_corpus(config).unwrap();
        }

        // Both corpora contain a doc mentioning "shared" (BM25-only; no embedder).
        {
            let engine = manager.get_engine_mut("wiki").unwrap();
            let content = "# Wiki\n\nshared knowledge lives here in the wiki.\n";
            fs::write(wiki_dir.join("shared.md"), content).unwrap();
            engine.index_file("shared.md", content).unwrap();
            engine.commit().unwrap();
        }
        {
            let engine = manager.get_engine_mut("docs").unwrap();
            let content = "# Docs\n\nshared documentation lives here in the docs.\n";
            fs::write(docs_dir.join("shared.md"), content).unwrap();
            engine.index_file("shared.md", content).unwrap();
            engine.commit().unwrap();
        }

        let registry = MultiCorpusToolRegistry::new();

        // Fan out across both corpora with corpora = "all".
        let result = registry
            .execute_read(
                "search",
                &manager,
                serde_json::json!({ "query": "shared", "mode": "bm25", "corpora": "all" }),
            )
            .unwrap();

        let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(result).unwrap();
        let docs = resp.docs.unwrap();
        assert_eq!(docs.results.len(), 2, "both corpora should contribute a hit");

        // Same path, distinct corpora → two tagged hits.
        let corpora: HashSet<String> =
            docs.results.iter().filter_map(|r| r.corpus.clone()).collect();
        assert!(corpora.contains("wiki"), "a hit must be tagged 'wiki'");
        assert!(corpora.contains("docs"), "a hit must be tagged 'docs'");
        assert!(docs.results.iter().all(|r| r.path == "shared.md"));

        // Single-corpus read via corpus="wiki" also tags its hit.
        let single = registry
            .execute_read(
                "search",
                &manager,
                serde_json::json!({ "query": "shared", "mode": "bm25", "corpus": "wiki" }),
            )
            .unwrap();
        let single_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(single).unwrap();
        let single_docs = single_resp.docs.unwrap();
        assert!(!single_docs.results.is_empty());
        assert!(single_docs.results.iter().all(|r| r.corpus.as_deref() == Some("wiki")));
    }

    #[test]
    fn test_multi_corpus_get_status() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let mut manager = ctxvault_core::corpus_manager::CorpusManager::new();
        let config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Full,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };
        manager.add_corpus(config).unwrap();

        let registry = MultiCorpusToolRegistry::new();

        let result = registry.execute("status", &mut manager, serde_json::json!({})).unwrap();

        assert_eq!(result["corpus_count"], 1);
        assert_eq!(result["default_corpus"], "wiki");
        let corpora = result["corpora"].as_array().unwrap();
        assert_eq!(corpora.len(), 1);
        assert_eq!(corpora[0]["name"], "wiki");
    }

    #[test]
    fn test_multi_corpus_invalid_corpus_returns_error() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let mut manager = ctxvault_core::corpus_manager::CorpusManager::new();
        let config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Full,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };
        manager.add_corpus(config).unwrap();

        let registry = MultiCorpusToolRegistry::new();

        // Non-existent corpus should error.
        let result = registry.execute(
            "search",
            &mut manager,
            serde_json::json!({ "query": "test", "mode": "bm25", "corpus": "nonexistent" }),
        );
        assert!(result.is_err());
    }

    // ─── Graph Match Tool Tests ────────────────────────────────────────

    #[test]
    fn test_graph_match_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);

        // Add nodes and lineage edge directly to graph and SQLite
        engine.graph_mut().add_edge(
            "docs/adrs/002.md",
            "docs/adrs/001.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "graph_match",
                &mut engine,
                serde_json::json!({
                    "pattern": "(a)-[:supersedes]->(b)"
                }),
            )
            .unwrap();

        let match_res: ctxvault_common::types::GraphMatchResult =
            serde_json::from_value(result).unwrap();
        assert_eq!(match_res.total_matches, 1);
        assert_eq!(match_res.matches[0].node, "docs/adrs/001.md");
    }

    #[test]
    fn test_validate_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);

        // Add broken link: valid.md -> missing.md
        engine.graph_mut().add_edge(
            "valid.md",
            "missing.md",
            "Wikilink",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );

        // Add circular dependency: A -> B -> A
        engine.graph_mut().add_edge(
            "A.md",
            "B.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        engine.graph_mut().add_edge(
            "B.md",
            "A.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute("validate", &mut engine, serde_json::json!({ "check_taxonomy": true }))
            .unwrap();

        assert_eq!(result["valid"], false);
        assert!(result["taxonomy"]["broken_links_count"].as_u64().unwrap() >= 1);
        assert!(result["taxonomy"]["circular_dependencies_count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn test_code_intelligence_mcp_tools() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        // Write polyglot code files
        let rust_code = r#"
pub struct QueryParser;

impl QueryParser {
    pub fn parse_query(&self, raw: &str) -> Vec<String> {
        tokenize(raw)
    }
}

pub fn tokenize(input: &str) -> Vec<String> {
    vec![input.to_string()]
}
"#;
        fs::write(corpus_dir.join("parser.rs"), rust_code).unwrap();
        engine.index_file("parser.rs", rust_code).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // 1. Test get_snippet (absorbed get_symbol_definition)
        let def_res = registry
            .execute_read("get_snippet", &engine, serde_json::json!({ "name": "parse_query" }))
            .unwrap();

        assert_eq!(def_res["kind"], "code_symbol");
        assert_eq!(def_res["name"], "parse_query");
        assert_eq!(def_res["path"], "parser.rs");
        assert!(def_res["source"].as_str().unwrap().contains("tokenize(raw)"));

        // 2. Test callers via graph_match
        let callers_res = registry
            .execute_read(
                "graph_match",
                &engine,
                serde_json::json!({ "pattern": "(caller)-[:calls]->(target {name: \"tokenize\"})" }),
            )
            .unwrap();
        let match_res: ctxvault_common::types::GraphMatchResult =
            serde_json::from_value(callers_res).unwrap();
        assert_eq!(match_res.total_matches, 1);
        assert_eq!(match_res.matches[0].node, "tokenize");

        // 3. Test graph_communities view='architecture' (absorbed get_architecture)
        let arch_res = registry
            .execute_read(
                "graph_communities",
                &engine,
                serde_json::json!({ "view": "architecture" }),
            )
            .unwrap();

        assert!(arch_res["component_count"].as_u64().unwrap() >= 1);
        assert!(!arch_res["components"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_read_file_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        let md = "# Design Note\n\nSome markdown content here.\n";
        fs::write(corpus_dir.join("design.md"), md).unwrap();
        engine.index_file("design.md", md).unwrap();

        let rust = "pub fn helper() -> u32 { 42 }\n";
        fs::write(corpus_dir.join("lib.rs"), rust).unwrap();
        engine.index_file("lib.rs", rust).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // Single file read
        let single = registry
            .execute_read("read_file", &engine, serde_json::json!({ "path": "design.md" }))
            .unwrap();
        assert_eq!(single["kind"], "markdown_note");
        assert_eq!(single["title"], "Design Note");
        assert!(single["content"].as_str().unwrap().contains("markdown content"));

        // Batch file read: Two existing files + one missing -> 3 entries, one carrying an error.
        let res = registry
            .execute_read(
                "read_file",
                &engine,
                serde_json::json!({ "paths": ["design.md", "lib.rs", "nope.md"] }),
            )
            .unwrap();

        assert_eq!(res["count"], 3);
        let results = res["results"].as_array().unwrap();

        let note = results.iter().find(|r| r["path"] == "design.md").unwrap();
        assert_eq!(note["kind"], "markdown_note");
        assert_eq!(note["title"], "Design Note");
        assert!(note["content"].as_str().unwrap().contains("markdown content"));
        assert!(note.get("error").is_none());

        let code = results.iter().find(|r| r["path"] == "lib.rs").unwrap();
        assert_eq!(code["kind"], "code_file");
        assert_eq!(code["language"], "rust");
        assert!(code["content"].as_str().unwrap().contains("helper"));

        let missing = results.iter().find(|r| r["path"] == "nope.md").unwrap();
        assert!(missing.get("error").is_some(), "missing path must carry an error entry");
    }

    #[test]
    fn test_status_coverage_scope() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        let rust = r#"
pub fn indexed_fn() -> u32 {
    7
}
"#;
        fs::write(corpus_dir.join("covered.rs"), rust).unwrap();
        engine.index_file("covered.rs", rust).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        let res = registry
            .execute_read(
                "status",
                &engine,
                serde_json::json!({ "scope": "coverage", "paths": ["covered.rs", "does_not_exist.rs"] }),
            )
            .unwrap();

        let reports = res["reports"].as_array().unwrap();
        assert_eq!(reports.len(), 2);

        let covered = reports.iter().find(|r| r["path"] == "covered.rs").unwrap();
        assert_eq!(covered["indexed"], true);
        assert!(covered["chunk_count"].as_u64().unwrap() > 0);
        assert_eq!(covered["parsed"], true);

        let bogus = reports.iter().find(|r| r["path"] == "does_not_exist.rs").unwrap();
        assert_eq!(bogus["indexed"], false);
        assert_eq!(bogus["parsed"], false);

        assert_eq!(res["summary"]["total"], 2);
        assert_eq!(res["summary"]["covered"], 1);
        assert_eq!(res["summary"]["uncovered"], 1);
    }

    #[test]
    fn test_fast_mode_mcp_tools() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("fast_corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        fs::write(
            corpus_dir.join("guide.md"),
            "# Architecture Guide\nFast mode provides instant BM25 and graph search without vector models.\n",
        )
        .unwrap();

        let mut config = test_config(&corpus_dir);
        config.index_mode = IndexMode::Fast;

        let index_dir = tmp.path().join(".index");
        let mut engine = Engine::open(config, &index_dir).unwrap();
        let files_indexed = engine.full_reindex().unwrap();
        assert_eq!(files_indexed, 1);
        assert!(engine.is_fast_mode());
        assert!(!engine.has_vector_index());

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // 1. Semantic search must fail with the exact fast mode error message
        let sem_err = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({ "query": "architecture guide", "mode": "semantic" }),
            )
            .unwrap_err();
        assert!(
            sem_err.to_string().contains(
                "Semantic search is unavailable in fast mode. Re-index with index_mode = 'full' to enable vector search."
            ),
            "Unexpected error: {sem_err}"
        );

        // 2. Hybrid search must cleanly fall back to BM25+Graph
        let hyb_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({ "query": "architecture", "mode": "hybrid" }),
            )
            .unwrap();
        let hyb_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(hyb_res).unwrap();
        let hyb_docs = hyb_resp.docs.unwrap();
        assert_eq!(hyb_docs.results.len(), 1);
        assert_eq!(hyb_docs.results[0].path, "guide.md");

        // 3. Sync corpus with fast: true maintains fast mode
        let sync_res = registry
            .execute("sync_corpus", &mut engine, serde_json::json!({ "fast": true }))
            .unwrap();
        assert_eq!(sync_res["status"], "complete");
        assert!(engine.is_fast_mode());
    }

    #[test]
    fn test_docs_embed_mode_mcp_tools() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("docs_embed_corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        fs::write(
            corpus_dir.join("guide.md"),
            "# Architecture Guide\nDocsEmbed provides vector search for documentation.\n",
        )
        .unwrap();
        fs::write(
            corpus_dir.join("service.rs"),
            "pub struct SearchPipeline;\npub fn execute_pipeline() {}\n",
        )
        .unwrap();

        let mut config = test_config(&corpus_dir);
        config.index_mode = IndexMode::DocsEmbed;

        let index_dir = tmp.path().join(".index");
        let mut engine = Engine::open(config, &index_dir).unwrap();
        let files_indexed = engine.full_reindex().unwrap();
        assert_eq!(files_indexed, 2);
        assert!(engine.is_docs_embed_mode());
        assert!(!engine.is_fast_mode());
        assert!(engine.has_vector_index());

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // 1. Status tool reflects DocsEmbed mode
        let status_res = registry
            .execute_read("status", &engine, serde_json::json!({ "scope": "corpus" }))
            .unwrap();
        assert_eq!(status_res["index_mode"], "DocsEmbed");

        // 2. BM25 search finds both doc and code
        let bm25_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({ "query": "SearchPipeline", "mode": "bm25" }),
            )
            .unwrap();
        let bm25_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(bm25_res).unwrap();
        assert!(bm25_resp.code.is_some());

        // 3. Hybrid search executes cleanly
        let hyb_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({ "query": "Architecture Guide", "mode": "hybrid" }),
            )
            .unwrap();
        let hyb_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(hyb_res).unwrap();
        assert!(hyb_resp.docs.is_some());

        // 4. Reindex with index_mode override preserves DocsEmbed
        let reindex_res = registry
            .execute(
                "sync_corpus",
                &mut engine,
                serde_json::json!({ "mode": "full", "index_mode": "docs-embed" }),
            )
            .unwrap();
        assert_eq!(reindex_res["status"], "complete");
        assert!(engine.is_docs_embed_mode());
    }

    // ─── Progressive Disclosure Tests (Tier 1 → 2 → 3) ─────────────────

    #[test]
    fn test_progressive_disclosure_handle_fetch_full() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        // A markdown note with two headings → two chunks.
        let md = "# Alpha Section\n\nAlpha talks about retrieval and ranking.\n\n\
                  # Beta Section\n\nBeta talks about graph traversal and edges.\n";
        fs::write(corpus_dir.join("notes.md"), md).unwrap();
        engine.index_file("notes.md", md).unwrap();

        // A Rust file with a caller/callee pair.
        let rust = r#"
pub struct Router;

impl Router {
    pub fn dispatch(&self, q: &str) -> Vec<String> {
        normalize(q)
    }
}

pub fn normalize(input: &str) -> Vec<String> {
    vec![input.to_lowercase()]
}
"#;
        fs::write(corpus_dir.join("router.rs"), rust).unwrap();
        engine.index_file("router.rs", rust).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // Tier 1: a search with detail="ids" returns handles with snippet == None.
        let ids_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({ "query": "retrieval ranking", "mode": "bm25", "detail": "ids" }),
            )
            .unwrap();
        let ids_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(ids_res).unwrap();
        let ids_results = ids_resp.docs.unwrap().results;
        assert!(!ids_results.is_empty(), "detail=ids should still return handles");
        assert!(
            ids_results.iter().all(|r| r.snippet.is_none()),
            "detail=ids must strip snippets (bare handles only)"
        );

        // Default detail keeps a short snippet.
        let default_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({ "query": "retrieval ranking", "mode": "bm25" }),
            )
            .unwrap();
        let default_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(default_res).unwrap();
        let default_results = default_resp.docs.unwrap().results;
        assert!(default_results.iter().any(|r| r.snippet.is_some()), "default keeps a snippet");

        // Tier 2 (doc): fetch exactly one chunk by path + chunk_index, bounded.
        let chunk_res = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "path": "notes.md", "chunk_index": 0, "max_lines": 100 }),
            )
            .unwrap();
        assert_eq!(chunk_res["kind"], "doc_chunk");
        assert_eq!(chunk_res["chunk_index"], 0);
        assert!(chunk_res["text"].as_str().unwrap().contains("Alpha"));

        // Tier 2 (doc) neighbor expansion: adjacent chunk is returned.
        let chunk_nb = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({
                    "path": "notes.md",
                    "chunk_index": 0,
                    "include_neighbors": true
                }),
            )
            .unwrap();
        assert_eq!(chunk_nb["previous"], Value::Null, "chunk 0 has no previous");
        assert!(chunk_nb["next"].is_object(), "chunk 0 should have a next neighbor");
        assert!(chunk_nb["next"]["text"].as_str().unwrap().contains("Beta"));

        // Tier 2 (code): fetch one symbol's source by qualified_name.
        let sym_res = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "Router > dispatch" }),
            )
            .unwrap();
        assert_eq!(sym_res["kind"], "code_symbol");
        assert_eq!(sym_res["path"], "router.rs");
        assert!(sym_res["source"].as_str().unwrap().contains("normalize(q)"));
        assert!(sym_res["start_line"].as_u64().unwrap() >= 1);
        assert!(
            sym_res["end_line"].as_u64().unwrap() >= sym_res["start_line"].as_u64().unwrap(),
            "line range must be well-formed"
        );

        // Tier 2 (code) neighbor expansion: callees include the called symbol.
        let sym_nb = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({
                    "qualified_name": "Router > dispatch",
                    "include_neighbors": true
                }),
            )
            .unwrap();
        let callees = sym_nb["callees"].as_array().unwrap();
        assert!(
            callees.iter().any(|c| c["name"] == "normalize" || c["scope_path"] == "normalize"),
            "dispatch should list normalize as a callee handle"
        );
        // Callees are HANDLES only — no body field.
        assert!(
            callees.iter().all(|c| c.get("source").is_none()),
            "neighbors are handles, not bodies"
        );

        // Callers of normalize should include dispatch.
        let normalize_nb = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "normalize", "include_neighbors": true }),
            )
            .unwrap();
        let callers = normalize_nb["callers"].as_array().unwrap();
        assert!(
            callers.iter().any(|c| c["scope_path"] == "Router > dispatch"),
            "normalize should list Router > dispatch as a caller handle"
        );

        // Tier 2 bounding: max_lines truncates the body.
        let capped = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "Router > dispatch", "max_lines": 1 }),
            )
            .unwrap();
        assert_eq!(capped["truncated"], true, "max_lines=1 must truncate a multi-line symbol");
        assert_eq!(capped["source"].as_str().unwrap().lines().count(), 1);

        // Tier 3 (code): read the whole file raw.
        let file_res = registry
            .execute_read("read_file", &engine, serde_json::json!({ "path": "router.rs" }))
            .unwrap();
        assert_eq!(file_res["language"], "rust");
        assert!(file_res["content"].as_str().unwrap().contains("pub struct Router;"));
        assert!(file_res["content"].as_str().unwrap().contains("pub fn normalize"));
        assert!(file_res["total_lines"].as_u64().unwrap() >= 5);

        // A bare path (no chunk_index / qualified_name) is redirected to Tier 3.
        let hint = registry.execute_read(
            "get_snippet",
            &engine,
            serde_json::json!({ "path": "router.rs" }),
        );
        assert!(hint.is_err(), "bare path must hint toward Tier 3");
    }

    #[test]
    fn test_search_detail_ids_stripping_and_explain() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        let md_old = "# Legacy\n\nLegacy architecture and design.\n";
        let md_new = "# Modern\n\nModern architecture and design.\n";
        fs::write(corpus_dir.join("legacy.md"), md_old).unwrap();
        fs::write(corpus_dir.join("modern.md"), md_new).unwrap();
        engine.index_file("legacy.md", md_old).unwrap();
        engine.index_file("modern.md", md_new).unwrap();

        let rust = r#"
pub struct Service;

impl Service {
    pub fn process(&self) -> bool {
        true
    }
}
"#;
        fs::write(corpus_dir.join("service.rs"), rust).unwrap();
        engine.index_file("service.rs", rust).unwrap();

        // Add structural edge so legacy.md has lineage: modern.md supersedes legacy.md
        engine.graph_mut().add_edge(
            "modern.md",
            "legacy.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            ctxvault_common::config::EdgeClass::Structural,
        );
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // 1. detail="ids" on code search: snippet, lineage, and score_components must all be None
        let code_ids_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({
                    "query": "Service process",
                    "modality": "code",
                    "mode": "bm25",
                    "detail": "ids"
                }),
            )
            .unwrap();
        let code_ids_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(code_ids_res).unwrap();
        let code_ids_results = code_ids_resp.code.unwrap().results;
        assert!(!code_ids_results.is_empty(), "expected hits for Service process");
        for r in &code_ids_results {
            assert!(r.snippet.is_none(), "code hit snippet must be None with detail=ids");
            assert!(r.lineage.is_none(), "code hit lineage must be None with detail=ids");
            assert!(
                r.score_components.is_none(),
                "code hit score_components must be None with detail=ids"
            );
        }

        // 2. detail="ids" on doc search with lineage: snippet, lineage, and score_components must all be None
        let doc_ids_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({
                    "query": "Legacy architecture",
                    "modality": "docs",
                    "mode": "bm25",
                    "detail": "ids"
                }),
            )
            .unwrap();
        let doc_ids_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(doc_ids_res).unwrap();
        let doc_ids_results = doc_ids_resp.docs.unwrap().results;
        assert!(!doc_ids_results.is_empty(), "expected hits for Legacy architecture");
        for r in &doc_ids_results {
            assert!(r.snippet.is_none(), "doc hit snippet must be None with detail=ids");
            assert!(r.lineage.is_none(), "doc hit lineage must be None with detail=ids");
            assert!(
                r.score_components.is_none(),
                "doc hit score_components must be None with detail=ids"
            );
        }

        // 3. detail="default" preserves snippet, lineage, and score_components
        let default_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({
                    "query": "Legacy architecture",
                    "modality": "docs",
                    "mode": "bm25",
                    "detail": "default"
                }),
            )
            .unwrap();
        let default_resp: ctxvault_common::types::SearchResponse =
            serde_json::from_value(default_res).unwrap();
        let default_results = default_resp.docs.unwrap().results;
        assert!(!default_results.is_empty());
        let legacy_hit = default_results.iter().find(|r| r.path.contains("legacy.md")).unwrap();
        assert!(legacy_hit.snippet.is_some(), "snippet must be preserved with detail=default");
        assert!(legacy_hit.lineage.is_some(), "lineage must be preserved with detail=default");
        assert!(
            legacy_hit.score_components.is_some(),
            "score_components must be preserved with detail=default"
        );

        // 4. mode="explain" preserves score breakdown even with detail="ids"
        let explain_res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({
                    "query": "Legacy architecture",
                    "mode": "explain",
                    "detail": "ids"
                }),
            )
            .unwrap();
        let explanations: Vec<ctxvault_common::types::SearchExplanation> =
            serde_json::from_value(explain_res).unwrap();
        assert!(!explanations.is_empty(), "explain should return explanations");
        for exp in &explanations {
            assert!(exp.snippet.is_none(), "snippet must be None when detail=ids in explain");
            assert!(exp.final_score > 0.0, "final_score must be preserved in explain");
            assert!(exp.bm25.raw_score > 0.0, "bm25 score component must be preserved in explain");
        }
    }

    #[test]
    fn test_generic_normalized_scope_resolution() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        let rust_code = r#"
pub struct EarlyBinder<'tcx, T> {
    value: T,
    _marker: std::marker::PhantomData<&'tcx ()>,
}

impl<'tcx, T> EarlyBinder<'tcx, T> {
    pub fn instantiate(&self) -> &T {
        &self.value
    }
}

pub struct OtherBinder<'a, A> {
    item: A,
    _life: &'a str,
}

impl<'a, A> OtherBinder<'a, A> {
    pub fn instantiate(&self) -> &A {
        &self.item
    }
}
"#;
        fs::write(corpus_dir.join("binder.rs"), rust_code).unwrap();
        engine.index_file("binder.rs", rust_code).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // 1. Resolve EarlyBinder > instantiate when defined as EarlyBinder<'tcx, T> > instantiate
        let res = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "EarlyBinder > instantiate" }),
            )
            .unwrap();
        assert_eq!(res["kind"], "code_symbol");
        assert_eq!(res["name"], "instantiate");
        assert!(res["scope_path"].as_str().unwrap().contains("EarlyBinder"));
        assert!(res["source"].as_str().unwrap().contains("&self.value"));

        // 2. Nonexistent symbol returns clean 404 Not Found error
        let err = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "Nonexistent > missing" }),
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("not found") || err.to_string().contains("no code symbol")
        );

        // 3. Ambiguous method: two EarlyBinder > instantiate in different files
        let rust_code_2 = r#"
pub struct EarlyBinder<'a, T> {
    alt: T,
}

impl<'a, T> EarlyBinder<'a, T> {
    pub fn instantiate(&self) -> &T {
        &self.alt
    }
}
"#;
        fs::write(corpus_dir.join("binder2.rs"), rust_code_2).unwrap();
        engine.index_file("binder2.rs", rust_code_2).unwrap();
        engine.commit().unwrap();

        let amb_res = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "EarlyBinder > instantiate" }),
            )
            .unwrap();
        assert_eq!(amb_res["kind"], "ambiguous");
        let candidates = amb_res["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().any(|c| c["file_path"] == "binder.rs"));
        assert!(candidates.iter().any(|c| c["file_path"] == "binder2.rs"));
    }

    #[test]
    fn test_get_snippet_suggestions_and_enrichment() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        let rust_code = r#"
/// Compute hash of input data.
pub fn compute_hash(data: &[u8]) -> u64 {
    42
}
"#;
        fs::write(corpus_dir.join("hash.rs"), rust_code).unwrap();
        engine.index_file("hash.rs", rust_code).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // 1. Context enrichment: check scope_path, signature, docstring, language, path
        let res = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "compute_hash", "include_neighbors": true }),
            )
            .unwrap();
        assert_eq!(res["kind"], "code_symbol");
        assert_eq!(res["path"], "hash.rs");
        assert_eq!(res["scope_path"], "compute_hash");
        assert_eq!(res["language"], "rust");
        assert!(res["signature"].as_str().unwrap().contains("pub fn compute_hash"));
        assert!(res["docstring"].as_str().unwrap().contains("Compute hash of input data."));
        // Empty neighbors serialized cleanly without crash
        assert_eq!(res["callers"].as_array().unwrap().len(), 0);
        assert_eq!(res["callees"].as_array().unwrap().len(), 0);

        // 2. Candidate suggestions on near-miss: query with wrong container "CryptoEngine > compute_hash"
        let sugg_res = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "CryptoEngine > compute_hash" }),
            )
            .unwrap();
        assert_eq!(sugg_res["kind"], "candidate_suggestions");
        let candidates = sugg_res["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["name"], "compute_hash");
        assert_eq!(candidates[0]["scope_path"], "compute_hash");
        assert!(candidates[0]["signature"].as_str().unwrap().contains("compute_hash"));

        // 3. Complete miss returns 404
        let err = registry
            .execute_read(
                "get_snippet",
                &engine,
                serde_json::json!({ "qualified_name": "CryptoEngine > unknown_fn" }),
            )
            .unwrap_err();
        assert!(err.to_string().contains("no code symbol"));
    }

    #[test]
    fn test_search_inlines_top_snippets() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");
        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        let doc_content =
            "# Architecture\n\nCtxvault is a high performance semantic context server.\n";
        fs::write(corpus_dir.join("arch.md"), doc_content).unwrap();
        engine.index_file("arch.md", doc_content).unwrap();

        let rust_code = "pub fn execute_search() -> bool { true }\n";
        fs::write(corpus_dir.join("search.rs"), rust_code).unwrap();
        engine.index_file("search.rs", rust_code).unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // Search with snippets = 2
        let res = registry
            .execute_read(
                "search",
                &engine,
                serde_json::json!({ "query": "semantic context server", "mode": "bm25", "snippets": 2 }),
            )
            .unwrap();

        let resp: ctxvault_common::types::SearchResponse = serde_json::from_value(res).unwrap();
        let docs = resp.docs.unwrap();
        assert!(!docs.results.is_empty());
        let top_hit = &docs.results[0];
        assert_eq!(top_hit.path, "arch.md");
        assert!(top_hit.snippet.is_some(), "Turn 1 snippet must be populated");
        assert!(top_hit.snippet.as_ref().unwrap().contains("high performance semantic"));
    }
}
