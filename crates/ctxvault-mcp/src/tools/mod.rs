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

use ctxvault_common::config::{CorpusMode, EdgeClass};
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
const SCOUT_TOOLS: [&str; 9] = [
    "search",
    "search_related",
    "get_snippet",
    "read_note",
    "read_code_file",
    "read_multiple",
    "list_notes",
    "get_frontmatter",
    "status",
];

/// Read-only tools added by the `analysis` profile on top of `scout`.
const ANALYSIS_ONLY_TOOLS: [&str; 21] = [
    "backlinks",
    "forwardlinks",
    "graph_path",
    "graph_stats",
    "graph_subgraph",
    "graph_communities",
    "list_edge_types",
    "traverse_lineage",
    "get_symbol_definition",
    "find_callers",
    "get_architecture",
    "validate_note",
    "validate_corpus",
    "list_templates",
    "validate_taxonomy",
    "analyze_density",
    "find_semantic_gaps",
    "suggest_splits",
    "coverage_report",
    "check_index_coverage",
    "corpus_list",
];

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
    const NON_DISCRIMINATED_READ_TOOLS: [&'static str; 2] = ["status", "corpus_list"];

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
                ToolHandler::ReadWrite(_) => {
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
            "read_note",
            "Tier 3 (last resort): full-file read of a markdown note's content and frontmatter. Prefer search → get_snippet first; only read the whole note when you truly need full document context.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note within the corpus" }
                },
                "required": ["path"]
            }),
            handle_read_note,
        );

        self.register_read(
            "get_snippet",
            "Tier 2 fetch: retrieve exactly one code symbol's source (by qualified_name) or one doc chunk (by path+chunk_index), bounded by max_lines. Call this for the specific handles a search returned — do NOT read whole files unless necessary.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path — for a DOC chunk fetch (with chunk_index) or a code FILE hint" },
                    "chunk_index": { "type": "integer", "description": "With path, fetch that specific doc chunk (zero-based)" },
                    "qualified_name": { "type": "string", "description": "Code symbol scope_path (exact) or name (fuzzy) to fetch one symbol's source" },
                    "max_lines": { "type": "integer", "description": "Hard cap on returned lines (default 500)" },
                    "include_neighbors": { "type": "boolean", "description": "Include neighbor context: code callers/callees as handles, or adjacent doc chunks (default false)" }
                },
                "required": []
            }),
            handle_get_snippet,
        );

        self.register_read(
            "read_code_file",
            "Tier 3 (last resort): read a whole source file (or a line range). Prefer search → get_snippet first.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the source file within the corpus" },
                    "start_line": { "type": "integer", "description": "Optional 1-based start line to bound the read" },
                    "end_line": { "type": "integer", "description": "Optional 1-based end line (inclusive) to bound the read" },
                    "max_lines": { "type": "integer", "description": "Hard cap on returned lines (default 1000)" }
                },
                "required": ["path"]
            }),
            handle_read_code_file,
        );

        self.register_read(
            "read_multiple",
            "Batch Tier-3 read of multiple files in one call (token-efficient). For markdown returns parsed note; for source returns raw content. Prefer search + get_snippet first.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Relative paths to files within the corpus"
                    },
                    "max_lines_per_file": { "type": "integer", "description": "Optional hard cap on returned lines per file" }
                },
                "required": ["paths"]
            }),
            handle_read_multiple,
        );

        self.register_read(
            "list_notes",
            "List all indexed notes with metadata (path, title, template, content_hash).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "Maximum number of notes to return (default 100)" },
                    "offset": { "type": "number", "description": "Offset for pagination (default 0)" }
                },
                "required": []
            }),
            handle_list_notes,
        );

        self.register_read(
            "get_frontmatter",
            "Get the parsed YAML frontmatter of a note as JSON.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note within the corpus" }
                },
                "required": ["path"]
            }),
            handle_get_frontmatter,
        );

        // Search tools
        self.register_read(
            "search",
            "Tier 1 retrieval: returns handles (paths/qualified names + line ranges), not bodies; fetch source with get_snippet, read whole files only as a last resort. One search tool with a `mode` param: bm25 (exact identifiers/tokens), semantic (dense vector, natural-language intent), hybrid (default; BM25 + vector + graph RRF fusion), graph (typed graph traversal from query matches), explain (hybrid with a per-result BM25/vector/graph score breakdown).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "mode": { "type": "string", "enum": ["bm25", "semantic", "hybrid", "graph", "explain"], "description": "Retrieval mode (default: hybrid). bm25 = keyword; semantic = dense vector; hybrid = 3-way RRF fusion; graph = typed traversal; explain = hybrid + score breakdown." },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" },
                    "depth": { "type": "string", "enum": ["precise", "broad", "adaptive"], "description": "Semantic mode only: retrieval depth — precise (chunk-level, default), broad (doc-level), adaptive (both + RRF)" },
                    "graph_depth": { "type": "number", "description": "hybrid/graph/explain modes: max graph traversal depth (default 2 for hybrid/explain, 3 for graph)" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "hybrid/graph/explain modes: filter graph traversal by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "hybrid/graph/explain modes: filter graph traversal by edge class (default: semantic for hybrid/explain, structural for graph)" },
                    "decompose": { "type": "boolean", "description": "hybrid mode only: enable query decomposition for multi-hop queries (default: false)" },
                    "modality": { "type": "string", "enum": ["docs", "code", "both"], "description": "Restrict results to documentation, code, or both (default)." },
                    "detail": { "type": "string", "enum": ["ids", "default"], "description": "ids = bare handles (path/qualified_name + line range + metadata, no snippet) for wide sweeps; default = handle plus a short snippet. Never returns full bodies — use get_snippet to fetch source." }
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
                    "detail": { "type": "string", "enum": ["ids", "default"], "description": "ids = bare handles (path/qualified_name + line range + metadata, no snippet) for wide sweeps; default = handle plus a short snippet. Never returns full bodies — use get_snippet to fetch source." }
                },
                "required": ["seeds"]
            }),
            handle_search_related,
        );

        // Graph tools
        self.register_read(
            "backlinks",
            "Get all notes that link TO a given note, grouped by edge type.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the target note" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter by edge class" }
                },
                "required": ["path"]
            }),
            handle_backlinks,
        );

        self.register_read(
            "forwardlinks",
            "Get all notes that a given note links TO, grouped by edge type.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the source note" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter by edge class" }
                },
                "required": ["path"]
            }),
            handle_forwardlinks,
        );

        self.register_read(
            "graph_path",
            "Find the shortest path between two notes in the knowledge graph.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Source note path" },
                    "to": { "type": "string", "description": "Target note path" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter path search by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter path search by edge class" }
                },
                "required": ["from", "to"]
            }),
            handle_graph_path,
        );

        self.register_read(
            "graph_stats",
            "Get graph statistics: node count, edge count, orphans, most connected nodes, edge type distribution.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handle_graph_stats,
        );

        self.register_read(
            "graph_subgraph",
            "Get the N-hop neighborhood (subgraph) around a node via BFS traversal.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Starting node path" },
                    "depth": { "type": "number", "description": "Maximum traversal depth (default 2)" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter traversal by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter traversal by edge class" }
                },
                "required": ["path"]
            }),
            handle_graph_subgraph,
        );

        self.register_read(
            "graph_communities",
            "Detect communities in the knowledge graph. Defaults to Leiden (Louvain partition refined so every community is internally connected); pass algorithm='louvain' for the raw modularity partition. Returns community assignments with modularity scores.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "include_density": { "type": "boolean", "description": "Include per-community density statistics (default false)" },
                    "algorithm": { "type": "string", "enum": ["leiden", "louvain"], "description": "Community detection algorithm (default: leiden)" }
                },
                "required": []
            }),
            handle_graph_communities,
        );

        self.register_read(
            "list_edge_types",
            "List all configured edge types in the taxonomy with their classes, sources, descriptions, template constraints, and live edge counts.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "edge_class": { "type": "string", "enum": ["structural", "semantic", "all", "hybrid"], "description": "Filter by edge class (default: all)" }
                },
                "required": []
            }),
            handle_list_edge_types,
        );

        self.register_read(
            "traverse_lineage",
            "Deterministically traverse the knowledge graph along a structural edge type (e.g. supersedes, implements, depends_on) in outgoing, incoming, or both directions.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "start_path": { "type": "string", "description": "Starting document path" },
                    "edge_type": { "type": "string", "description": "Structural edge type to traverse (e.g. 'supersedes', 'implements', 'depends_on')" },
                    "direction": { "type": "string", "enum": ["outgoing", "incoming", "both"], "description": "Direction of traversal (default: 'outgoing')" },
                    "max_depth": { "type": "number", "description": "Maximum traversal depth hops (default 3)" }
                },
                "required": ["start_path", "edge_type"]
            }),
            handle_traverse_lineage,
        );

        // Write tools
        self.register_write(
            "create_note",
            "Create a new note with optional frontmatter and content.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path for the new note (e.g. 'projects/my-note.md')" },
                    "content": { "type": "string", "description": "Body content of the note (markdown)" },
                    "frontmatter": { "type": "object", "description": "YAML frontmatter fields as key-value pairs" },
                    "template": { "type": "string", "description": "Template name to set in frontmatter" }
                },
                "required": ["path"]
            }),
            handle_create_note,
        );

        self.register_write(
            "update_note",
            "Update an existing note's content (overwrite, append, or prepend).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note to update" },
                    "content": { "type": "string", "description": "New content to apply" },
                    "mode": { "type": "string", "enum": ["overwrite", "append", "prepend"], "description": "How to apply the content (default: overwrite)" }
                },
                "required": ["path", "content"]
            }),
            handle_update_note,
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

        self.register_write(
            "promote_concept",
            "Crystallize fluid memory notes into a consolidated, templated concept note. Validates schema before writing and indexing, with atomic rollback on validation failure.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "source_notes": { "type": "array", "items": { "type": "string" }, "description": "Source note paths being consolidated" },
                    "target_path": { "type": "string", "description": "Relative path for the new consolidated concept note" },
                    "template": { "type": "string", "description": "Template schema to apply (e.g. 'concept', 'adr')" },
                    "frontmatter": { "type": "object", "description": "YAML frontmatter fields as key-value pairs" },
                    "content": { "type": "string", "description": "Markdown body content of the consolidated note" },
                    "archive_sources": { "type": "boolean", "description": "Whether to archive the source notes (default false)" }
                },
                "required": ["source_notes", "target_path", "content"]
            }),
            handle_promote_concept,
        );

        // Validation tools
        self.register_read(
            "validate_note",
            "Validate a note against its declared template (checks required fields, types, sections).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note within the corpus" }
                },
                "required": ["path"]
            }),
            handle_validate_note,
        );

        self.register_read(
            "validate_corpus",
            "Validate all templated notes in the corpus, returning only those with issues.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "Maximum number of results to return (default: all)" }
                },
                "required": []
            }),
            handle_validate_corpus,
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

        self.register_read(
            "validate_taxonomy",
            "Ontology & structural graph integrity linter: checks for broken wikilinks, circular dependencies in DAG relations (supersedes, depends_on), orphan ADRs, and template constraints.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "check_broken": { "type": "boolean", "description": "Check for broken links (default true)" },
                    "check_cycles": { "type": "boolean", "description": "Check for circular dependencies in DAG relations (default true)" },
                    "check_orphans": { "type": "boolean", "description": "Check for orphan ADR notes (default true)" }
                },
                "required": []
            }),
            handle_validate_taxonomy,
        );

        // Analytics tools
        self.register_read(
            "analyze_density",
            "Analyze graph density: orphans, hubs, edge distribution, overall connectivity.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "top_hubs": { "type": "number", "description": "Number of top hub nodes to return (default 10)" }
                },
                "required": []
            }),
            handle_analyze_density,
        );

        self.register_read(
            "find_semantic_gaps",
            "Find queries where BM25 and vector search disagree — potential embedding blind spots.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "queries": { "type": "array", "items": { "type": "string" }, "description": "Test queries to evaluate" },
                    "top_k": { "type": "number", "description": "Number of results to compare per query (default 10)" }
                },
                "required": ["queries"]
            }),
            handle_find_semantic_gaps,
        );

        self.register_read(
            "suggest_splits",
            "Identify chunks with low coherence that may benefit from splitting.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "max_chunk_chars": { "type": "number", "description": "Character threshold for 'too long' chunks (default 2000)" }
                },
                "required": []
            }),
            handle_suggest_splits,
        );

        self.register_read(
            "coverage_report",
            "For a set of test queries, identify which notes are never retrieved (dead zones).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "queries": { "type": "array", "items": { "type": "string" }, "description": "Test queries to evaluate coverage" },
                    "top_k": { "type": "number", "description": "Number of results per query (default 10)" }
                },
                "required": ["queries"]
            }),
            handle_coverage_report,
        );

        self.register_read(
            "check_index_coverage",
            "Report index coverage + parse status for the given paths or path prefixes: which are indexed, chunk/symbol counts, and parse gaps (indexed but empty).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Relative paths or path prefixes/scopes to check for index coverage"
                    }
                },
                "required": ["paths"]
            }),
            handle_check_index_coverage,
        );

        // System tools
        self.register_read(
            "corpus_list",
            "List all configured corpora with their modes, file counts, and index stats.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handle_corpus_list,
        );

        self.register_write(
            "reembed_corpus",
            "Re-embed all chunks with the current embedding model. Use after changing models to update vectors without losing data.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handle_reembed_corpus,
        );

        self.register_write(
            "sync_corpus",
            "Delta sync: compare filesystem against the index, add new files, update modified files, remove deleted files in configurable batches.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "batch_size": { "type": "number", "description": "Batch size for commits (default 50)" },
                    "fast": { "type": "boolean", "description": "Enable Fast Mode: skip dense embedding and vector indexing for instant indexing" }
                },
                "required": []
            }),
            handle_sync_corpus,
        );

        self.register_write(
            "reindex_corpus",
            "Full reindex: re-index corpus files in configurable batches with automatic resumption support.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "batch_size": { "type": "number", "description": "Batch size for intermediate checkpoints (default 50)" },
                    "resume": { "type": "boolean", "description": "Resume from last indexing checkpoint if available (default true)" },
                    "fast": { "type": "boolean", "description": "Enable Fast Mode: skip dense embedding and vector indexing for instant indexing" }
                },
                "required": []
            }),
            handle_reindex_corpus,
        );

        self.register_read(
            "status",
            "Corpus + indexing status in one tool via `scope`: corpus = per-corpus statistics, document counts, and configuration; indexing = current indexing progress, throughput, and estimated time remaining; all (default) = both combined. When no specific corpus is targeted the multi-corpus overview (all configured corpora) is included.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["corpus", "indexing", "all"], "description": "corpus = per-corpus stats/config; indexing = indexing progress; all (default) = both combined." },
                    "corpus": { "type": "string", "description": "Target a single corpus by name for per-corpus stats/indexing. Omit for the multi-corpus overview across all configured corpora." }
                },
                "required": []
            }),
            handle_status,
        );

        // Code Intelligence & Architecture Tools
        self.register_read(
            "get_symbol_definition",
            "Find code symbol definition (function, method, struct, class, trait, interface) with exact source lines, docstrings, and incoming caller count.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Symbol name to look up" },
                    "file_path": { "type": "string", "description": "Optional file path to disambiguate symbols with the same name" }
                },
                "required": ["name"]
            }),
            handle_get_symbol_definition,
        );

        self.register_read(
            "find_callers",
            "Find all inbound call sites and callers for a given code symbol or method across polyglot source files.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol_name": { "type": "string", "description": "Symbol name to find callers for" }
                },
                "required": ["symbol_name"]
            }),
            handle_find_callers,
        );

        self.register_read(
            "get_architecture",
            "Get high-level architectural component overview via Louvain community clustering across the cross-modal knowledge graph.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "resolution": { "type": "number", "description": "Community detection resolution parameter (default 1.0)" }
                },
                "required": []
            }),
            handle_get_architecture,
        );

        self.register_write(
            "detect_changes",
            "Detect modified files and calculate their impact radius (impacted symbols and upstream callers).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "since": { "type": "string", "description": "Optional revision/reference" }
                },
                "required": []
            }),
            handle_detect_changes,
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
    if !output.is_array() {
        return output;
    }
    match serde_json::from_value::<Vec<ctxvault_common::types::SearchResult>>(output.clone()) {
        Ok(results) => {
            let tagged: Vec<ctxvault_common::types::SearchResult> =
                results.into_iter().map(|r| r.with_corpus(Some(corpus_name.to_string()))).collect();
            serde_json::to_value(tagged).unwrap_or(output)
        }
        // A non-SearchResult array (e.g. search_explain) is returned as-is.
        Err(_) => output,
    }
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

// ---------------------------------------------------------------------------
// Parameter structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ReadNoteParams {
    path: String,
}

#[derive(Deserialize)]
struct ReadMultipleParams {
    paths: Vec<String>,
    max_lines_per_file: Option<usize>,
}

#[derive(Deserialize)]
struct CheckIndexCoverageParams {
    paths: Vec<String>,
}

#[derive(Deserialize)]
struct ListNotesParams {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Deserialize)]
struct GetFrontmatterParams {
    path: String,
}

#[derive(Deserialize)]
struct GetSnippetParams {
    path: Option<String>,
    chunk_index: Option<usize>,
    qualified_name: Option<String>,
    max_lines: Option<usize>,
    #[serde(default)]
    include_neighbors: bool,
}

#[derive(Deserialize)]
struct ReadCodeFileParams {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
    max_lines: Option<usize>,
}

#[derive(Deserialize)]
struct SearchParams {
    query: String,
    #[serde(default)]
    mode: Option<String>,
    limit: Option<usize>,
    depth: Option<String>,
    graph_depth: Option<usize>,
    edge_types: Option<Vec<String>>,
    edge_class: Option<String>,
    decompose: Option<bool>,
    modality: Option<String>,
    detail: Option<String>,
}

#[derive(Deserialize)]
struct SearchRelatedParams {
    seeds: Vec<String>,
    limit: Option<usize>,
    modality: Option<String>,
    detail: Option<String>,
}

#[derive(Deserialize)]
struct StatusParams {
    scope: Option<String>,
}

#[derive(Deserialize)]
struct BacklinksParams {
    path: String,
    edge_class: Option<String>,
}

#[derive(Deserialize)]
struct ForwardlinksParams {
    path: String,
    edge_class: Option<String>,
}

#[derive(Deserialize)]
struct GraphPathParams {
    from: String,
    to: String,
    edge_types: Option<Vec<String>>,
    edge_class: Option<String>,
}

#[derive(Deserialize)]
struct GraphSubgraphParams {
    path: String,
    depth: Option<usize>,
    edge_types: Option<Vec<String>>,
    edge_class: Option<String>,
}

#[derive(Deserialize)]
struct GraphCommunitiesParams {
    include_density: Option<bool>,
    /// Community detection algorithm: `leiden` (default, connectivity-refined)
    /// or `louvain` (raw modularity partition).
    algorithm: Option<String>,
}

// Write tool params

#[derive(Deserialize)]
struct CreateNoteParams {
    path: String,
    content: Option<String>,
    frontmatter: Option<Value>,
    template: Option<String>,
}

#[derive(Deserialize)]
struct UpdateNoteParams {
    path: String,
    content: String,
    mode: Option<String>,
}

#[derive(Deserialize)]
struct DeleteNoteParams {
    path: String,
}

#[derive(Deserialize)]
struct MoveNoteParams {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct ValidateNoteParams {
    path: String,
}

#[derive(Deserialize)]
struct ValidateCorpusParams {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct ListEdgeTypesParams {
    edge_class: Option<String>,
}

#[derive(Deserialize)]
struct TraverseLineageParams {
    start_path: String,
    edge_type: String,
    direction: Option<String>,
    max_depth: Option<usize>,
}

#[derive(Deserialize)]
struct PromoteConceptParams {
    source_notes: Vec<String>,
    target_path: String,
    template: Option<String>,
    frontmatter: Option<Value>,
    content: String,
    archive_sources: Option<bool>,
}

#[derive(Deserialize)]
struct ValidateTaxonomyParams {
    check_broken: Option<bool>,
    check_cycles: Option<bool>,
    check_orphans: Option<bool>,
}

// Analytics tool params

#[derive(Deserialize)]
struct AnalyzeDensityParams {
    top_hubs: Option<usize>,
}

#[derive(Deserialize)]
struct FindSemanticGapsParams {
    queries: Vec<String>,
    top_k: Option<usize>,
}

#[derive(Deserialize)]
struct SuggestSplitsParams {
    max_chunk_chars: Option<usize>,
}

#[derive(Deserialize)]
struct CoverageReportParams {
    queries: Vec<String>,
    top_k: Option<usize>,
}

#[derive(Deserialize)]
struct SyncCorpusParams {
    batch_size: Option<usize>,
    fast: Option<bool>,
}

#[derive(Deserialize)]
struct ReindexCorpusParams {
    batch_size: Option<usize>,
    resume: Option<bool>,
    fast: Option<bool>,
}

// ---------------------------------------------------------------------------
// Response structs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct NoteResponse {
    path: String,
    title: Option<String>,
    frontmatter: Option<Value>,
    content: String,
    content_hash: String,
}

#[derive(Serialize)]
struct NoteListItem {
    path: String,
    title: Option<String>,
    template: Option<String>,
    content_hash: String,
}

#[derive(Serialize)]
struct SubgraphNode {
    path: String,
    depth: usize,
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

/// Read a note's full content + frontmatter.
fn handle_read_note(engine: &Engine, args: Value) -> Result<Value> {
    let params: ReadNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", params.path, e)))?;

    let doc = ctxvault_core::parser::parse_document(Path::new(&params.path), &content)?;

    let response = NoteResponse {
        path: params.path,
        title: doc.title,
        frontmatter: doc.frontmatter,
        content: doc.content,
        content_hash: doc.content_hash,
    };

    serde_json::to_value(response).map_err(|e| Error::Config(format!("serialize error: {}", e)))
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

/// Tier 2 fetch: return exactly one code symbol's source or one doc chunk,
/// bounded by `max_lines`, with optional neighbor expansion.
fn handle_get_snippet(engine: &Engine, args: Value) -> Result<Value> {
    let params: GetSnippetParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let max_lines = params.max_lines.unwrap_or(500).max(1);
    let corpus_root = Path::new(&engine.config().path);

    if let Some(qualified_name) = params.qualified_name.as_deref() {
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
             For a whole file use Tier 3: read_note (docs) or read_code_file (code).",
        )));
    }

    Err(Error::Config(
        "get_snippet requires either `qualified_name` (code) or `path`+`chunk_index` (doc)."
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

    let text_lines: Vec<&str> = chunk.text.lines().collect();
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
            chunks.iter().find(|c| c.chunk_index == target).map(|c| {
                let nlines: Vec<&str> = c.text.lines().collect();
                let (ntext, ntrunc) = cap_lines(&nlines, neighbor_cap);
                serde_json::json!({
                    "chunk_index": c.chunk_index,
                    "start_byte": c.start_byte,
                    "end_byte": c.end_byte,
                    "text": ntext,
                    "truncated": ntrunc,
                })
            })
        };

        out["previous"] = chunk_index.checked_sub(1).and_then(neighbor).unwrap_or(Value::Null);
        out["next"] = neighbor(chunk_index + 1).unwrap_or(Value::Null);
    }

    Ok(out)
}

/// Tier 3 fetch: read a whole source file (or a bounded line range) as raw text.
fn handle_read_code_file(engine: &Engine, args: Value) -> Result<Value> {
    let params: ReadCodeFileParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_root = Path::new(&engine.config().path);
    let full_path = corpus_root.join(&params.path);
    let content = fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", params.path, e)))?;

    let all_lines: Vec<&str> = content.lines().collect();
    let total_line_count = all_lines.len();
    let max_lines = params.max_lines.unwrap_or(1000).max(1);

    // Resolve an optional 1-based inclusive line window.
    let start_idx =
        params.start_line.map(|s| s.saturating_sub(1).min(total_line_count)).unwrap_or(0);
    let end_idx =
        params.end_line.map(|e| e.min(total_line_count)).unwrap_or(total_line_count).max(start_idx);

    let windowed = &all_lines[start_idx..end_idx];
    let (body, truncated) = cap_lines(windowed, max_lines);

    Ok(serde_json::json!({
        "path": params.path,
        "language": language_from_path(&params.path),
        "total_line_count": total_line_count,
        "start_line": start_idx + 1,
        "end_line": start_idx + windowed.len().min(max_lines),
        "content": body,
        "truncated": truncated,
    }))
}

/// Batch Tier-3 read of multiple files in one call.
///
/// For markdown files each result mirrors [`handle_read_note`]'s shape
/// (`path`, `title`, `frontmatter`, `content`, `content_hash`); for source
/// files it returns raw `content` (like `read_code_file`). Per-path failures
/// become an entry with an `error` field rather than aborting the whole call.
fn handle_read_multiple(engine: &Engine, args: Value) -> Result<Value> {
    let params: ReadMultipleParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_root = PathBuf::from(&engine.config().path);
    let results: Vec<Value> = params
        .paths
        .iter()
        .map(|path| read_one_file(&corpus_root, path, params.max_lines_per_file))
        .collect();

    Ok(serde_json::json!({
        "count": results.len(),
        "results": results,
    }))
}

/// Read a single file for [`handle_read_multiple`], returning either the
/// file payload or an `{ "path", "error" }` entry on failure.
fn read_one_file(corpus_root: &Path, path: &str, max_lines: Option<usize>) -> Value {
    match read_one_file_inner(corpus_root, path, max_lines) {
        Ok(value) => value,
        Err(e) => serde_json::json!({ "path": path, "error": e.to_string() }),
    }
}

/// Fallible core of [`read_one_file`].
fn read_one_file_inner(corpus_root: &Path, path: &str, max_lines: Option<usize>) -> Result<Value> {
    let full_path = corpus_root.join(path);
    let content = fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", path, e)))?;

    let is_markdown = matches!(language_from_path(path), "markdown");

    if is_markdown {
        let doc = ctxvault_core::parser::parse_document(Path::new(path), &content)?;
        let body = match max_lines {
            Some(cap) => {
                let lines: Vec<&str> = doc.content.lines().collect();
                cap_lines(&lines, cap.max(1)).0
            }
            None => doc.content,
        };
        Ok(serde_json::json!({
            "path": path,
            "kind": "note",
            "title": doc.title,
            "frontmatter": doc.frontmatter,
            "content": body,
            "content_hash": doc.content_hash,
        }))
    } else {
        let all_lines: Vec<&str> = content.lines().collect();
        let total_line_count = all_lines.len();
        let (body, truncated) = match max_lines {
            Some(cap) => cap_lines(&all_lines, cap.max(1)),
            None => (content, false),
        };
        Ok(serde_json::json!({
            "path": path,
            "kind": "code",
            "language": language_from_path(path),
            "total_line_count": total_line_count,
            "content": body,
            "truncated": truncated,
        }))
    }
}

/// Report index coverage + parse status for the given paths or path prefixes.
///
/// For each requested path or prefix, consults the catalog to report whether
/// any file record matches (`indexed`), the chunk and symbol counts, and
/// whether it parsed (indexed but zero chunks signals a parse gap).
fn handle_check_index_coverage(engine: &Engine, args: Value) -> Result<Value> {
    let params: CheckIndexCoverageParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let all_files = engine.store().list_files()?;

    let mut reports = Vec::with_capacity(params.paths.len());
    let mut covered = 0usize;

    for scope in &params.paths {
        // A scope matches a file record either exactly or as a path prefix.
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

        // Parsed means the scope produced content: indexed but zero chunks and
        // zero symbols is a parse gap.
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

    let total = params.paths.len();
    Ok(serde_json::json!({
        "reports": reports,
        "summary": {
            "total": total,
            "covered": covered,
            "uncovered": total - covered,
        },
    }))
}

/// List all indexed notes with metadata.
fn handle_list_notes(engine: &Engine, args: Value) -> Result<Value> {
    let params: ListNotesParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

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

/// Get parsed frontmatter as JSON.
fn handle_get_frontmatter(engine: &Engine, args: Value) -> Result<Value> {
    let params: GetFrontmatterParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", params.path, e)))?;

    let doc = ctxvault_core::parser::parse_document(Path::new(&params.path), &content)?;

    let frontmatter = doc.frontmatter.unwrap_or(Value::Null);

    Ok(frontmatter)
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
        serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
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

/// All notes linking TO a note, grouped by edge type.
fn handle_backlinks(engine: &Engine, args: Value) -> Result<Value> {
    let params: BacklinksParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let edge_class_filter = params.edge_class.as_deref().and_then(EdgeClass::from_str_name);
    let backlinks = engine.graph().backlinks(&params.path, edge_class_filter);

    serde_json::to_value(backlinks).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// All notes a note links TO, grouped by edge type.
fn handle_forwardlinks(engine: &Engine, args: Value) -> Result<Value> {
    let params: ForwardlinksParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let edge_class_filter = params.edge_class.as_deref().and_then(EdgeClass::from_str_name);
    let forwardlinks = engine.graph().forwardlinks(&params.path, edge_class_filter);

    serde_json::to_value(forwardlinks).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Shortest path between two notes.
fn handle_graph_path(engine: &Engine, args: Value) -> Result<Value> {
    let params: GraphPathParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let edge_type_filter = params.edge_types;
    let edge_class_filter = params.edge_class.as_deref().and_then(EdgeClass::from_str_name);

    let path = engine.graph().shortest_path(
        &params.from,
        &params.to,
        edge_type_filter.as_deref(),
        edge_class_filter,
    );

    match path {
        Some(p) => {
            serde_json::to_value(p).map_err(|e| Error::Config(format!("serialize error: {}", e)))
        }
        None => Ok(Value::Null),
    }
}

/// Graph statistics.
fn handle_graph_stats(engine: &Engine, _args: Value) -> Result<Value> {
    let stats = engine.graph().stats();

    serde_json::to_value(stats).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// N-hop neighborhood around a node.
fn handle_graph_subgraph(engine: &Engine, args: Value) -> Result<Value> {
    let params: GraphSubgraphParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let depth = params.depth.unwrap_or(2);
    let edge_type_filter = params.edge_types;
    let edge_class_filter = params.edge_class.as_deref().and_then(EdgeClass::from_str_name);

    let neighbors = engine.graph().traverse_bfs(
        &params.path,
        depth,
        edge_type_filter.as_deref(),
        edge_class_filter,
    );

    let nodes: Vec<SubgraphNode> =
        neighbors.into_iter().map(|(path, d)| SubgraphNode { path, depth: d }).collect();

    serde_json::to_value(nodes).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Detect communities via Louvain algorithm.
fn handle_graph_communities(engine: &Engine, args: Value) -> Result<Value> {
    let params: GraphCommunitiesParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let include_density = params.include_density.unwrap_or(false);

    // Default to the connectivity-refined Leiden partition; `louvain` selects the
    // raw modularity partition.
    let result = match params.algorithm.as_deref() {
        Some("louvain") => engine.graph().detect_communities(),
        _ => engine.graph().detect_communities_leiden(),
    };

    if include_density {
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

// ---------------------------------------------------------------------------
// Analytics tool handlers
// ---------------------------------------------------------------------------

/// Corpus list: reports on the current engine as a corpus.
fn handle_corpus_list(engine: &Engine, _args: Value) -> Result<Value> {
    let file_count = engine.store().list_files().map(|f| f.len()).unwrap_or(0);
    let info = serde_json::json!([{
        "name": engine.config().name,
        "path": engine.config().path,
        "mode": format!("{:?}", engine.config().mode),
        "file_count": file_count,
        "embedder_active": engine.embedder_active(),
        "vector_count": engine.vector_count(),
        "graph_node_count": engine.graph().node_count(),
    }]);

    Ok(info)
}

/// Re-embed all chunks with the current embedding model.
fn handle_reembed_corpus(engine: &mut Engine, _args: Value) -> Result<Value> {
    let was_stale = engine.vectors_stale();
    let old_version = engine.stored_model_version().map(|s| s.to_string());

    let chunks_reembedded = engine.reembed()?;

    let new_version = engine.stored_model_version().unwrap_or("unknown").to_string();

    Ok(serde_json::json!({
        "status": "complete",
        "chunks_reembedded": chunks_reembedded,
        "was_stale": was_stale,
        "previous_model_version": old_version,
        "current_model_version": new_version,
    }))
}

/// Delta sync: index new/modified files, remove deleted files in configurable batches.
fn handle_sync_corpus(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SyncCorpusParams =
        serde_json::from_value(args).unwrap_or(SyncCorpusParams { batch_size: None, fast: None });
    if let Some(fast) = params.fast {
        engine.config_mut().index_mode = if fast {
            ctxvault_common::config::IndexMode::Fast
        } else {
            ctxvault_common::config::IndexMode::Full
        };
    }
    let batch_size = params.batch_size.unwrap_or(50);
    let result = engine.delta_scan_paginated(batch_size)?;

    Ok(serde_json::json!({
        "status": "complete",
        "new_files": result.new_files.len(),
        "modified_files": result.modified_files.len(),
        "deleted_files": result.deleted_files.len(),
        "new": result.new_files,
        "modified": result.modified_files,
        "deleted": result.deleted_files,
    }))
}

/// Full reindex: clear all indices and rebuild from scratch or resume in configurable batches.
fn handle_reindex_corpus(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: ReindexCorpusParams = serde_json::from_value(args).unwrap_or(ReindexCorpusParams {
        batch_size: None,
        resume: None,
        fast: None,
    });
    if let Some(fast) = params.fast {
        engine.config_mut().index_mode = if fast {
            ctxvault_common::config::IndexMode::Fast
        } else {
            ctxvault_common::config::IndexMode::Full
        };
    }
    let batch_size = params.batch_size.unwrap_or(50);
    let resume = params.resume.unwrap_or(true);
    let count = engine.full_reindex_paginated(batch_size, resume)?;

    Ok(serde_json::json!({
        "status": "complete",
        "files_indexed": count,
        "batch_size": batch_size,
        "resumed": resume,
    }))
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
        "chunking": format!("{:?}", engine.config().chunking.strategy),
        "embedding_model": engine.config().embedding.model,
    }))
}

/// Consolidated status tool (engine-level): combines per-corpus statistics and
/// indexing progress, selected by `scope` (`corpus` | `indexing` | `all`,
/// default `all`).
fn handle_status(engine: &Engine, args: Value) -> Result<Value> {
    let params: StatusParams = serde_json::from_value(args).unwrap_or(StatusParams { scope: None });
    let scope = params.scope.as_deref().unwrap_or("all");

    match scope {
        "corpus" => corpus_stats(engine),
        "indexing" => {
            let status = engine.get_indexing_status()?;
            serde_json::to_value(status)
                .map_err(|e| Error::Config(format!("serialize error: {}", e)))
        }
        _ => {
            let corpus = corpus_stats(engine)?;
            let indexing = serde_json::to_value(engine.get_indexing_status()?)
                .map_err(|e| Error::Config(format!("serialize error: {}", e)))?;
            Ok(serde_json::json!({
                "corpus": corpus,
                "indexing": indexing,
            }))
        }
    }
}

/// Graph density analysis.
fn handle_analyze_density(engine: &Engine, args: Value) -> Result<Value> {
    let params: AnalyzeDensityParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let top_hubs = params.top_hubs.unwrap_or(10);
    let report = engine.analyze_density(top_hubs);

    serde_json::to_value(report).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Find semantic gaps between BM25 and vector search.
fn handle_find_semantic_gaps(engine: &Engine, args: Value) -> Result<Value> {
    if engine.is_fast_mode() || !engine.has_vector_index() {
        return Err(Error::Index(
            "Semantic gap analysis is unavailable in fast mode. Re-index with index_mode = 'full' to enable vector search.".to_string(),
        ));
    }

    let params: FindSemanticGapsParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let top_k = params.top_k.unwrap_or(10);
    let query_refs: Vec<&str> = params.queries.iter().map(|s| s.as_str()).collect();

    // The engine runs the analysis over its own backends. `Ok(None)` signals the
    // embedder was unavailable or some queries failed to embed.
    match engine.find_semantic_gaps(&query_refs, top_k)? {
        Some(gaps) => {
            serde_json::to_value(gaps).map_err(|e| Error::Config(format!("serialize error: {}", e)))
        }
        None => Ok(serde_json::json!({
            "error": "embedder not available or some queries failed to embed",
            "gaps": []
        })),
    }
}

/// Suggest chunks that may benefit from splitting.
fn handle_suggest_splits(engine: &Engine, args: Value) -> Result<Value> {
    let params: SuggestSplitsParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let max_chunk_chars = params.max_chunk_chars.unwrap_or(2000);
    let suggestions = engine.suggest_splits(max_chunk_chars)?;

    serde_json::to_value(suggestions).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Coverage report: which notes are never retrieved.
fn handle_coverage_report(engine: &Engine, args: Value) -> Result<Value> {
    let params: CoverageReportParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let top_k = params.top_k.unwrap_or(10);

    let query_refs: Vec<&str> = params.queries.iter().map(|s| s.as_str()).collect();
    let report = engine.coverage_report(&query_refs, top_k)?;

    serde_json::to_value(report).map_err(|e| Error::Config(format!("serialize error: {}", e)))
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

/// Create a new note on disk and index it.
fn handle_create_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: CreateNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    // Don't overwrite existing files.
    if full_path.exists() {
        return Err(Error::Config(format!("file already exists: {}", params.path)));
    }

    // Ensure parent directory exists.
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("cannot create directory for {}: {}", params.path, e),
            ))
        })?;
    }

    let body = params.content.as_deref().unwrap_or("");
    let content = build_note_content(params.frontmatter.as_ref(), params.template.as_deref(), body);

    // Write file atomically-ish (write then sync).
    fs::write(&full_path, &content).map_err(|e| {
        Error::Io(std::io::Error::new(e.kind(), format!("cannot write {}: {}", params.path, e)))
    })?;

    // Index the new file.
    engine.index_file(&params.path, &content)?;
    engine.commit()?;

    debug!("Created note: {}", params.path);

    Ok(serde_json::json!({
        "path": params.path,
        "created": true
    }))
}

/// Update an existing note's content.
fn handle_update_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: UpdateNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    if !full_path.exists() {
        return Err(Error::NotFound(format!("file not found: {}", params.path)));
    }

    let existing = fs::read_to_string(&full_path).map_err(|e| {
        Error::Io(std::io::Error::new(e.kind(), format!("cannot read {}: {}", params.path, e)))
    })?;

    let mode = params.mode.as_deref().unwrap_or("overwrite");
    let new_content = match mode {
        "append" => {
            let mut s = existing;
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&params.content);
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s
        }
        "prepend" => {
            let mut s = params.content.clone();
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&existing);
            s
        }
        _ => {
            // "overwrite" or any unrecognized mode defaults to overwrite.
            let mut s = params.content.clone();
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s
        }
    };

    fs::write(&full_path, &new_content).map_err(|e| {
        Error::Io(std::io::Error::new(e.kind(), format!("cannot write {}: {}", params.path, e)))
    })?;

    // Re-index.
    engine.index_file(&params.path, &new_content)?;
    engine.commit()?;

    debug!("Updated note: {} (mode={})", params.path, mode);

    Ok(serde_json::json!({
        "path": params.path,
        "updated": true
    }))
}

/// Delete a note from disk and all indices.
fn handle_delete_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: DeleteNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

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
fn handle_validate_note(engine: &Engine, args: Value) -> Result<Value> {
    let params: ValidateNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    let content = fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", params.path, e)))?;

    let doc = ctxvault_core::parser::parse_document(Path::new(&params.path), &content)?;

    // Determine which template the note declares.
    let template_name = doc.template.clone();

    let (valid, issues, tmpl_name) = if let Some(ref name) = template_name {
        let templates = load_corpus_templates(engine)?;
        if let Some(tmpl) = templates.get(name) {
            let issues = tmpl.validate(&doc.frontmatter, &doc.content);
            let valid =
                !issues.iter().any(|i| i.severity == ctxvault_core::template::Severity::Error);
            (valid, issues, Some(name.clone()))
        } else {
            // Template declared but not found — report as warning.
            let issues = vec![ctxvault_core::template::ValidationIssue {
                severity: ctxvault_core::template::Severity::Warning,
                message: format!("template '{}' not found in templates directory", name),
                field: Some("template".to_string()),
            }];
            (true, issues, Some(name.clone()))
        }
    } else {
        // No template declared — nothing to validate.
        (true, Vec::new(), None)
    };

    let result = ctxvault_core::template::ValidationResult {
        path: params.path,
        template: tmpl_name,
        valid,
        issues,
    };

    serde_json::to_value(result).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Validate all templated notes in the corpus.
fn handle_validate_corpus(engine: &Engine, args: Value) -> Result<Value> {
    let params: ValidateCorpusParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let templates = load_corpus_templates(engine)?;
    let files = engine.store().list_files()?;
    let corpus_path = PathBuf::from(&engine.config().path);

    let mut results: Vec<ctxvault_core::template::ValidationResult> = Vec::new();

    for file in &files {
        // Only validate files that declare a template.
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

        // Only include notes that have issues.
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

        // Respect limit if set.
        if let Some(limit) = params.limit {
            if results.len() >= limit {
                break;
            }
        }
    }

    serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// List all available templates.
fn handle_list_templates(engine: &Engine, _args: Value) -> Result<Value> {
    let templates = load_corpus_templates(engine)?;

    let mut list: Vec<&Template> = templates.values().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));

    serde_json::to_value(list).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// List all edge types in the taxonomy.
fn handle_list_edge_types(engine: &Engine, args: Value) -> Result<Value> {
    let params: ListEdgeTypesParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let filter_class = params.edge_class.as_deref().and_then(|s| {
        if s.eq_ignore_ascii_case("all") {
            None
        } else {
            EdgeClass::from_str_name(s)
        }
    });

    let stats = engine.graph().stats();
    let config_edge_types = &engine.config().graph.edge_types;

    let mut edge_types_info = Vec::new();
    for et in config_edge_types {
        let class = et.class.unwrap_or_else(|| EdgeClass::infer_from_source(&et.source));
        if let Some(filter) = filter_class {
            if !class.matches(filter) {
                continue;
            }
        }

        let live_count = *stats.edge_type_distribution.get(&et.name).unwrap_or(&0);
        edge_types_info.push(serde_json::json!({
            "name": et.name,
            "class": format!("{:?}", class).to_lowercase(),
            "source": format!("{:?}", et.source).to_lowercase(),
            "weight": et.weight,
            "bidirectional": et.bidirectional,
            "field": et.field,
            "direction": et.direction.as_ref().map(|d| format!("{:?}", d).to_lowercase()),
            "description": et.description,
            "allowed_source_templates": et.allowed_source_templates,
            "allowed_target_templates": et.allowed_target_templates,
            "live_edge_count": live_count,
        }));
    }

    Ok(serde_json::json!({
        "edge_types": edge_types_info,
        "total_edge_types": edge_types_info.len(),
        "total_live_edges": stats.edge_count,
    }))
}

/// Traverse knowledge graph along a structural edge type.
fn handle_traverse_lineage(engine: &Engine, args: Value) -> Result<Value> {
    let params: TraverseLineageParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let direction = params.direction.as_deref().unwrap_or("outgoing");
    let max_depth = params.max_depth.unwrap_or(3);

    let nodes = engine.graph().traverse_lineage(
        &params.start_path,
        &params.edge_type,
        direction,
        max_depth,
    );

    let total_hops = nodes.iter().map(|n| n.depth).max().unwrap_or(0);

    Ok(serde_json::json!({
        "start_path": params.start_path,
        "edge_type": params.edge_type,
        "direction": direction,
        "max_depth": max_depth,
        "nodes": nodes,
        "node_count": nodes.len(),
        "total_hops": total_hops,
    }))
}

/// Promote fluid notes into a consolidated, schema-validated concept note.
fn handle_promote_concept(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: PromoteConceptParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    if engine.config().mode == CorpusMode::ReadOnly {
        return Err(Error::Config(format!("corpus '{}' is read-only", engine.config().name)));
    }

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_target_path = corpus_path.join(&params.target_path);

    if full_target_path.exists() {
        return Err(Error::Config(format!("target file already exists: {}", params.target_path)));
    }

    let mut fm_map = match &params.frontmatter {
        Some(Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };

    if let Some(ref tmpl) = params.template {
        let _ = fm_map.insert("template".to_string(), Value::String(tmpl.clone()));
    }

    let fm_value = Value::Object(fm_map);

    let mut validation_issues = Vec::new();
    if let Some(ref tmpl_name) = params.template {
        let templates = load_corpus_templates(engine)?;
        if let Some(template) = templates.get(tmpl_name) {
            let issues = template.validate(&Some(fm_value.clone()), &params.content);
            let has_error =
                issues.iter().any(|i| i.severity == ctxvault_core::template::Severity::Error);
            validation_issues = issues;
            if has_error {
                let error_msgs: Vec<String> = validation_issues
                    .iter()
                    .filter(|i| i.severity == ctxvault_core::template::Severity::Error)
                    .map(|i| format!("{}: {}", i.field.as_deref().unwrap_or("general"), i.message))
                    .collect();
                return Err(Error::Config(format!(
                    "concept promotion failed template schema validation for '{}': {}",
                    tmpl_name,
                    error_msgs.join("; ")
                )));
            }
        }
    }

    let content = build_note_content(Some(&fm_value), params.template.as_deref(), &params.content);

    if let Some(parent) = full_target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("cannot create directory for {}: {}", params.target_path, e),
            ))
        })?;
    }

    fs::write(&full_target_path, &content).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("cannot write {}: {}", params.target_path, e),
        ))
    })?;

    engine.index_file(&params.target_path, &content)?;

    let mut archived = Vec::new();
    if params.archive_sources == Some(true) {
        for src in &params.source_notes {
            let src_full = corpus_path.join(src);
            if src_full.exists() {
                if let Ok(src_content) = fs::read_to_string(&src_full) {
                    if let Ok(doc) =
                        ctxvault_core::parser::parse_document(Path::new(src), &src_content)
                    {
                        let mut src_fm = match doc.frontmatter {
                            Some(Value::Object(map)) => map,
                            _ => serde_json::Map::new(),
                        };
                        let _ = src_fm
                            .insert("status".to_string(), Value::String("archived".to_string()));
                        let _ = src_fm.insert(
                            "superseded_by".to_string(),
                            Value::String(params.target_path.clone()),
                        );

                        let updated_src_content = build_note_content(
                            Some(&Value::Object(src_fm)),
                            doc.template.as_deref(),
                            &doc.content,
                        );
                        if fs::write(&src_full, &updated_src_content).is_ok() {
                            let _ = engine.index_file(src, &updated_src_content);
                            archived.push(src.clone());
                        }
                    }
                }
            }
        }
    }

    engine.commit()?;

    Ok(serde_json::json!({
        "status": "promoted",
        "target_path": params.target_path,
        "source_notes": params.source_notes,
        "template_applied": params.template,
        "validation_issues": validation_issues,
        "archived_sources": archived,
    }))
}

/// Validate ontology and graph integrity.
fn handle_validate_taxonomy(engine: &Engine, args: Value) -> Result<Value> {
    let params: ValidateTaxonomyParams =
        serde_json::from_value(args).unwrap_or(ValidateTaxonomyParams {
            check_broken: Some(true),
            check_cycles: Some(true),
            check_orphans: Some(true),
        });

    let check_broken = params.check_broken.unwrap_or(true);
    let check_cycles = params.check_cycles.unwrap_or(true);
    let check_orphans = params.check_orphans.unwrap_or(true);

    let files = engine.store().list_files()?;
    let existing_paths: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();

    let broken_links =
        if check_broken { engine.graph().detect_broken_links(&existing_paths) } else { Vec::new() };

    let circular_dependencies = if check_cycles {
        engine.graph().detect_circular_dependencies(&[
            "supersedes",
            "depends_on",
            "implements",
            "parent_of",
        ])
    } else {
        Vec::new()
    };

    let orphan_adrs = if check_orphans {
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
        engine.graph().detect_orphan_adrs(&adr_paths)
    } else {
        Vec::new()
    };

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

// ---------------------------------------------------------------------------
// Structural Code Tools Handlers
// ---------------------------------------------------------------------------

fn handle_get_symbol_definition(engine: &Engine, params: Value) -> Result<Value> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| Error::Config("missing required parameter: name".to_string()))?;
    let file_path_filter = params["file_path"].as_str();

    let symbols = engine.store().find_symbols_by_name(name)?;
    let filtered: Vec<_> = symbols
        .into_iter()
        .filter(|s| {
            if let Some(fp) = file_path_filter {
                s.file_path == fp
            } else {
                s.name == name || s.scope_path.ends_with(name)
            }
        })
        .collect();

    let mut results = Vec::new();
    let corpus_root = Path::new(&engine.config().path);

    for sym in &filtered {
        let full_path = corpus_root.join(&sym.file_path);
        let snippet = if let Ok(content) = fs::read_to_string(&full_path) {
            let lines: Vec<&str> = content.lines().collect();
            if sym.start_line > 0 && sym.start_line <= lines.len() {
                let start_idx = sym.start_line - 1;
                let end_idx = sym.end_line.min(lines.len());
                Some(lines[start_idx..end_idx].join("\n"))
            } else {
                None
            }
        } else {
            None
        };

        let edges = engine.graph().get_all_edges();
        let callers_count = edges
            .iter()
            .filter(|e| {
                e.edge_type == "calls" && (e.target == sym.scope_path || e.target == sym.name)
            })
            .count();

        results.push(serde_json::json!({
            "name": sym.name,
            "scope_path": sym.scope_path,
            "symbol_type": sym.symbol_type,
            "language": sym.language,
            "file_path": sym.file_path,
            "start_line": sym.start_line,
            "end_line": sym.end_line,
            "signature": sym.signature,
            "docstring": sym.docstring,
            "snippet": snippet,
            "incoming_callers_count": callers_count,
        }));
    }

    Ok(serde_json::json!({
        "symbol_name": name,
        "matches_count": results.len(),
        "definitions": results,
    }))
}

fn handle_find_callers(engine: &Engine, params: Value) -> Result<Value> {
    let symbol_name = params["symbol_name"]
        .as_str()
        .ok_or_else(|| Error::Config("missing required parameter: symbol_name".to_string()))?;

    let edges = engine.graph().get_all_edges();
    let caller_edges: Vec<_> = edges
        .into_iter()
        .filter(|e| {
            e.edge_type == "calls"
                && (e.target == symbol_name || e.target.ends_with(&format!(" > {}", symbol_name)))
        })
        .collect();

    let all_symbols = engine.store().get_all_code_symbols().unwrap_or_default();
    let mut callers = Vec::new();

    for edge in &caller_edges {
        let sym = all_symbols.iter().find(|s| s.scope_path == edge.source || s.name == edge.source);
        callers.push(serde_json::json!({
            "caller_symbol": edge.source,
            "target_symbol": edge.target,
            "file_path": sym.map(|s| s.file_path.clone()),
            "start_line": sym.map(|s| s.start_line),
            "signature": sym.map(|s| s.signature.clone()),
            "docstring": sym.and_then(|s| s.docstring.clone()),
            "confidence": edge.confidence,
        }));
    }

    Ok(serde_json::json!({
        "target_symbol": symbol_name,
        "callers_count": callers.len(),
        "callers": callers,
    }))
}

fn handle_get_architecture(engine: &Engine, _params: Value) -> Result<Value> {
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
        let mut key_nodes: Vec<_> =
            nodes.iter().filter_map(|n| node_degree.get(n).map(|deg| (n.clone(), *deg))).collect();
        key_nodes.sort_by(|a, b| b.1.cmp(&a.1));
        let top_key_nodes: Vec<String> = key_nodes.into_iter().take(5).map(|(n, _)| n).collect();

        clusters.push(serde_json::json!({
            "community_id": comm_id,
            "node_count": nodes.len(),
            "density": density,
            "key_nodes": top_key_nodes,
            "nodes": nodes,
        }));
    }

    clusters.sort_by(|a, b| {
        b["node_count"].as_u64().unwrap_or(0).cmp(&a["node_count"].as_u64().unwrap_or(0))
    });

    Ok(serde_json::json!({
        "total_nodes": engine.graph().node_count(),
        "total_edges": engine.graph().edge_count(),
        "modularity": result.modularity,
        "clusters_count": clusters.len(),
        "clusters": clusters,
    }))
}

fn handle_detect_changes(engine: &mut Engine, _params: Value) -> Result<Value> {
    let delta = engine.delta_scan()?;
    let edges = engine.graph().get_all_edges();

    let mut impacted_symbols = Vec::new();
    for path in &delta.modified_files {
        let symbols = engine.store().get_code_symbols_for_file(path).unwrap_or_default();
        for sym in symbols {
            let callers: Vec<String> = edges
                .iter()
                .filter(|e| {
                    e.edge_type == "calls" && (e.target == sym.scope_path || e.target == sym.name)
                })
                .map(|e| e.source.clone())
                .collect();

            impacted_symbols.push(serde_json::json!({
                "symbol": sym.scope_path,
                "file_path": sym.file_path,
                "symbol_type": sym.symbol_type,
                "impacted_callers": callers,
            }));
        }
    }

    Ok(serde_json::json!({
        "new_files": delta.new_files,
        "modified_files": delta.modified_files,
        "deleted_files": delta.deleted_files,
        "impacted_symbols": impacted_symbols,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ctxvault_common::config::{
        ChunkingConfig, CorpusConfig, CorpusMode, EdgeSource, EdgeTypeConfig, EmbeddingConfig,
        GraphConfig, IndexMode,
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
        assert_eq!(tools.len(), 39, "Expected 39 tools registered");

        // Verify each expected tool exists.
        let expected = [
            "read_note",
            "get_snippet",
            "read_code_file",
            "read_multiple",
            "list_notes",
            "get_frontmatter",
            "search",
            "search_related",
            "backlinks",
            "forwardlinks",
            "graph_path",
            "graph_stats",
            "graph_subgraph",
            "graph_communities",
            "list_edge_types",
            "traverse_lineage",
            "create_note",
            "update_note",
            "delete_note",
            "move_note",
            "promote_concept",
            "validate_note",
            "validate_corpus",
            "list_templates",
            "validate_taxonomy",
            "analyze_density",
            "find_semantic_gaps",
            "suggest_splits",
            "coverage_report",
            "check_index_coverage",
            "corpus_list",
            "reembed_corpus",
            "sync_corpus",
            "reindex_corpus",
            "status",
            "get_symbol_definition",
            "find_callers",
            "get_architecture",
            "detect_changes",
        ];

        assert_eq!(expected.len(), 39, "expected-name list must match the 39-tool count");

        for name in expected {
            assert!(registry.get(name).is_some(), "Tool '{}' should be registered", name);
        }

        // The consolidated tools replace the old per-mode / per-status tools.
        for gone in [
            "search_bm25",
            "search_semantic",
            "search_hybrid",
            "search_graph",
            "search_explain",
            "get_status",
            "get_corpus_stats",
            "get_indexing_status",
        ] {
            assert!(registry.get(gone).is_none(), "Tool '{}' must no longer be registered", gone);
        }

        // Verify read-only classification
        assert!(registry.is_read_only("read_note"));
        assert!(registry.is_read_only("search"));
        assert!(registry.is_read_only("status"));
        assert!(registry.is_read_only("get_symbol_definition"));
        assert!(registry.is_read_only("find_callers"));
        assert!(registry.is_read_only("get_architecture"));
        assert!(registry.is_read_only("get_snippet"));
        assert!(registry.is_read_only("read_code_file"));
        assert!(registry.is_read_only("read_multiple"));
        assert!(registry.is_read_only("check_index_coverage"));
        assert!(!registry.is_read_only("detect_changes"));
        assert!(!registry.is_read_only("create_note"));
        assert!(!registry.is_read_only("reindex_corpus"));
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
        assert_eq!(all_count, 39, "all profile advertises every registered tool");
        assert_eq!(scout_count, 9, "scout profile advertises the minimal set");

        // scout includes core retrieval/fetch but not writes.
        let scout_names: HashSet<&str> = scout.list().iter().map(|t| t.name.as_str()).collect();
        assert!(scout_names.contains("search"));
        assert!(scout_names.contains("get_snippet"));
        assert!(scout_names.contains("status"));
        assert!(!scout_names.contains("create_note"));
        assert!(!scout_names.contains("backlinks"));

        // Hidden tools still execute (advertise-only filtering): create_note is
        // registered even though scout does not advertise it.
        assert!(scout.registry().get("create_note").is_some());

        // analysis adds read-only tools but still hides writes.
        let analysis_names: HashSet<&str> =
            analysis.list().iter().map(|t| t.name.as_str()).collect();
        assert!(analysis_names.contains("backlinks"));
        assert!(analysis_names.contains("corpus_list"));
        assert!(!analysis_names.contains("create_note"));
        assert!(!analysis_names.contains("reindex_corpus"));
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
        let err =
            registry.execute_read("create_note", &engine, serde_json::json!({ "path": "fail.md" }));
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

        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(!results.is_empty(), "Should find indexed file via search");
        assert_eq!(results[0]["path"], "rust.md");
    }

    #[test]
    fn test_create_note_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "create_note",
                &mut engine,
                serde_json::json!({
                    "path": "new-note.md",
                    "content": "# Hello\n\nThis is a new note.",
                    "frontmatter": { "tags": ["test", "demo"] }
                }),
            )
            .unwrap();

        assert_eq!(result["path"], "new-note.md");
        assert_eq!(result["created"], true);

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
        let hits: Vec<Value> = serde_json::from_value(search_result).unwrap();
        assert!(!hits.is_empty());
    }

    #[test]
    fn test_create_note_with_template() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "create_note",
                &mut engine,
                serde_json::json!({
                    "path": "templated.md",
                    "content": "Body text here.",
                    "template": "meeting"
                }),
            )
            .unwrap();

        assert_eq!(result["created"], true);

        let corpus_dir = tmp.path().join("corpus");
        let file_content = fs::read_to_string(corpus_dir.join("templated.md")).unwrap();
        assert!(file_content.contains("template: meeting"));
    }

    #[test]
    fn test_create_note_already_exists() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let corpus_dir = tmp.path().join("corpus");
        fs::write(corpus_dir.join("existing.md"), "# Existing").unwrap();

        let result = registry.execute(
            "create_note",
            &mut engine,
            serde_json::json!({ "path": "existing.md", "content": "overwrite?" }),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_update_note_overwrite() {
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
                "update_note",
                &mut engine,
                serde_json::json!({
                    "path": "update-me.md",
                    "content": "# Replaced\n\nNew content."
                }),
            )
            .unwrap();

        assert_eq!(result["updated"], true);

        let file_content = fs::read_to_string(corpus_dir.join("update-me.md")).unwrap();
        assert!(file_content.contains("New content"));
        assert!(!file_content.contains("Old content"));
    }

    #[test]
    fn test_update_note_append() {
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
                "update_note",
                &mut engine,
                serde_json::json!({
                    "path": "append.md",
                    "content": "Second line.",
                    "mode": "append"
                }),
            )
            .unwrap();

        assert_eq!(result["updated"], true);

        let file_content = fs::read_to_string(corpus_dir.join("append.md")).unwrap();
        assert!(file_content.contains("First line."));
        assert!(file_content.contains("Second line."));
    }

    #[test]
    fn test_update_note_prepend() {
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
                "update_note",
                &mut engine,
                serde_json::json!({
                    "path": "prepend.md",
                    "content": "Prepended text.",
                    "mode": "prepend"
                }),
            )
            .unwrap();

        assert_eq!(result["updated"], true);

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
        let hits: Vec<Value> = serde_json::from_value(search_result).unwrap();
        assert!(hits.is_empty() || hits.iter().all(|h| h["path"] != "delete-me.md"));
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

        // Should have 39 tools.
        assert_eq!(tools.len(), 39, "Expected 39 tools in multi-corpus registry");
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

        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(!results.is_empty(), "Should find wiki note via default corpus");
        assert_eq!(results[0]["path"], "note.md");
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
        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(!results.is_empty(), "Should find rust.md in wiki");
        assert_eq!(results[0]["path"], "rust.md");

        // Search in docs corpus explicitly.
        let result = registry
            .execute(
                "search",
                &mut manager,
                serde_json::json!({ "query": "documentation", "mode": "bm25", "corpus": "docs" }),
            )
            .unwrap();
        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(!results.is_empty(), "Should find python.md in docs");
        assert_eq!(results[0]["path"], "python.md");

        // Verify isolation: searching wiki for python returns nothing.
        let result = registry
            .execute(
                "search",
                &mut manager,
                serde_json::json!({ "query": "python documentation", "mode": "bm25", "corpus": "wiki" }),
            )
            .unwrap();
        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(
            results.is_empty() || results.iter().all(|r| r["path"] != "python.md"),
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

        let results: Vec<ctxvault_common::types::SearchResult> =
            serde_json::from_value(result).unwrap();
        assert_eq!(results.len(), 2, "both corpora should contribute a hit");

        // Same path, distinct corpora → two tagged hits.
        let corpora: HashSet<String> = results.iter().filter_map(|r| r.corpus.clone()).collect();
        assert!(corpora.contains("wiki"), "a hit must be tagged 'wiki'");
        assert!(corpora.contains("docs"), "a hit must be tagged 'docs'");
        assert!(results.iter().all(|r| r.path == "shared.md"));

        // Single-corpus read via corpus="wiki" also tags its hit.
        let single = registry
            .execute_read(
                "search",
                &manager,
                serde_json::json!({ "query": "shared", "mode": "bm25", "corpus": "wiki" }),
            )
            .unwrap();
        let single_results: Vec<ctxvault_common::types::SearchResult> =
            serde_json::from_value(single).unwrap();
        assert!(!single_results.is_empty());
        assert!(single_results.iter().all(|r| r.corpus.as_deref() == Some("wiki")));
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

    // ─── Structural Tools Tests ────────────────────────────────────────

    #[test]
    fn test_list_edge_types_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute("list_edge_types", &mut engine, serde_json::json!({ "edge_class": "all" }))
            .unwrap();

        assert_eq!(result["total_edge_types"], 1);
        let edge_types = result["edge_types"].as_array().unwrap();
        assert_eq!(edge_types[0]["name"], "Wikilink");
    }

    #[test]
    fn test_traverse_lineage_tool() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);

        // Add nodes and lineage edge directly to graph
        engine.graph_mut().add_edge(
            "docs/adrs/002.md",
            "docs/adrs/001.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "traverse_lineage",
                &mut engine,
                serde_json::json!({
                    "start_path": "docs/adrs/002.md",
                    "edge_type": "supersedes",
                    "direction": "outgoing"
                }),
            )
            .unwrap();

        assert_eq!(result["start_path"], "docs/adrs/002.md");
        assert_eq!(result["node_count"], 2);
        let nodes = result["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["path"], "docs/adrs/002.md");
        assert_eq!(nodes[1]["path"], "docs/adrs/001.md");
    }

    #[test]
    fn test_promote_concept_tool_success() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        // Create raw source notes
        fs::write(corpus_dir.join("scratch1.md"), "# Scratch 1\nIdea 1").unwrap();
        fs::write(corpus_dir.join("scratch2.md"), "# Scratch 2\nIdea 2").unwrap();
        engine.index_file("scratch1.md", "# Scratch 1\nIdea 1").unwrap();
        engine.index_file("scratch2.md", "# Scratch 2\nIdea 2").unwrap();
        engine.commit().unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        let result = registry
            .execute(
                "promote_concept",
                &mut engine,
                serde_json::json!({
                    "source_notes": ["scratch1.md", "scratch2.md"],
                    "target_path": "concepts/unified_architecture.md",
                    "content": "# Unified Architecture\n\nSynthesized content here.",
                    "frontmatter": { "status": "active", "confidence": "high" },
                    "archive_sources": true
                }),
            )
            .unwrap();

        assert_eq!(result["status"], "promoted");
        assert_eq!(result["target_path"], "concepts/unified_architecture.md");

        // Target note created
        assert!(corpus_dir.join("concepts/unified_architecture.md").exists());

        // Source notes archived
        let s1 = fs::read_to_string(corpus_dir.join("scratch1.md")).unwrap();
        assert!(s1.contains("status: archived"));
        assert!(s1.contains("superseded_by: concepts/unified_architecture.md"));
    }

    #[test]
    fn test_promote_concept_tool_rollback_on_schema_error() {
        let tmp = TempDir::new().unwrap();
        let mut engine = create_test_engine(&tmp);
        let corpus_dir = tmp.path().join("corpus");

        // Create a template directory with strict schema
        let templates_dir = corpus_dir.join(".templates");
        fs::create_dir_all(&templates_dir).unwrap();
        let tmpl_toml = r#"
            name = "adr"
            description = "Architecture Decision Record"

            [fields.status]
            type = "enum"
            required = true
            allowed_values = ["proposed", "accepted", "rejected"]
        "#;
        fs::write(templates_dir.join("adr.toml"), tmpl_toml).unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_all();

        // Attempt promotion with invalid status field value
        let result = registry.execute(
            "promote_concept",
            &mut engine,
            serde_json::json!({
                "source_notes": ["raw.md"],
                "target_path": "adrs/001.md",
                "template": "adr",
                "content": "# ADR 001\nDecision content",
                "frontmatter": { "status": "invalid_status" }
            }),
        );

        assert!(result.is_err(), "Schema error must reject promotion transaction");

        // Target note must NOT exist on disk (rolled back)
        assert!(!corpus_dir.join("adrs/001.md").exists());
    }

    #[test]
    fn test_validate_taxonomy_tool() {
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

        let result =
            registry.execute("validate_taxonomy", &mut engine, serde_json::json!({})).unwrap();

        assert_eq!(result["valid"], false);
        assert!(result["broken_links_count"].as_u64().unwrap() >= 1);
        assert!(result["circular_dependencies_count"].as_u64().unwrap() >= 1);
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

        // 1. Test get_symbol_definition
        let def_res = registry
            .execute_read(
                "get_symbol_definition",
                &engine,
                serde_json::json!({ "name": "parse_query" }),
            )
            .unwrap();

        assert_eq!(def_res["matches_count"], 1);
        let def = &def_res["definitions"][0];
        assert_eq!(def["name"], "parse_query");
        assert_eq!(def["file_path"], "parser.rs");
        assert!(def["snippet"].as_str().unwrap().contains("tokenize(raw)"));

        // 2. Test find_callers
        let callers_res = registry
            .execute_read("find_callers", &engine, serde_json::json!({ "symbol_name": "tokenize" }))
            .unwrap();

        assert_eq!(callers_res["callers_count"], 1);
        assert_eq!(callers_res["callers"][0]["caller_symbol"], "QueryParser > parse_query");

        // 3. Test get_architecture
        let arch_res =
            registry.execute_read("get_architecture", &engine, serde_json::json!({})).unwrap();

        assert!(arch_res["total_nodes"].as_u64().unwrap() >= 1);
        assert!(arch_res["clusters_count"].as_u64().unwrap() >= 1);

        // 4. Test detect_changes
        let change_res =
            registry.execute("detect_changes", &mut engine, serde_json::json!({})).unwrap();

        assert_eq!(change_res["new_files"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_read_multiple_tool() {
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

        // Two existing files + one missing -> 3 entries, one carrying an error.
        let res = registry
            .execute_read(
                "read_multiple",
                &engine,
                serde_json::json!({ "paths": ["design.md", "lib.rs", "nope.md"] }),
            )
            .unwrap();

        assert_eq!(res["count"], 3);
        let results = res["results"].as_array().unwrap();

        let note = results.iter().find(|r| r["path"] == "design.md").unwrap();
        assert_eq!(note["kind"], "note");
        assert_eq!(note["title"], "Design Note");
        assert!(note["content"].as_str().unwrap().contains("markdown content"));
        assert!(note.get("error").is_none());

        let code = results.iter().find(|r| r["path"] == "lib.rs").unwrap();
        assert_eq!(code["kind"], "code");
        assert_eq!(code["language"], "rust");
        assert!(code["content"].as_str().unwrap().contains("helper"));

        let missing = results.iter().find(|r| r["path"] == "nope.md").unwrap();
        assert!(missing.get("error").is_some(), "missing path must carry an error entry");
    }

    #[test]
    fn test_check_index_coverage_tool() {
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
                "check_index_coverage",
                &engine,
                serde_json::json!({ "paths": ["covered.rs", "does_not_exist.rs"] }),
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
        let hyb_array = hyb_res.as_array().unwrap();
        assert_eq!(hyb_array.len(), 1);
        assert_eq!(hyb_array[0]["path"], "guide.md");

        // 3. Find semantic gaps must fail in fast mode
        let gaps_err = registry
            .execute_read(
                "find_semantic_gaps",
                &engine,
                serde_json::json!({ "queries": ["architecture"] }),
            )
            .unwrap_err();
        assert!(gaps_err.to_string().contains("unavailable in fast mode"));

        // 4. Sync corpus with fast: true maintains fast mode
        let sync_res = registry
            .execute("sync_corpus", &mut engine, serde_json::json!({ "fast": true }))
            .unwrap();
        assert_eq!(sync_res["status"], "complete");
        assert!(engine.is_fast_mode());
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
        let ids_results: Vec<ctxvault_common::types::SearchResult> =
            serde_json::from_value(ids_res).unwrap();
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
        let default_results: Vec<ctxvault_common::types::SearchResult> =
            serde_json::from_value(default_res).unwrap();
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
            .execute_read("read_code_file", &engine, serde_json::json!({ "path": "router.rs" }))
            .unwrap();
        assert_eq!(file_res["language"], "rust");
        assert!(file_res["content"].as_str().unwrap().contains("pub struct Router;"));
        assert!(file_res["content"].as_str().unwrap().contains("pub fn normalize"));
        assert!(file_res["total_line_count"].as_u64().unwrap() >= 5);

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
        let code_ids_results: Vec<ctxvault_common::types::SearchResult> =
            serde_json::from_value(code_ids_res).unwrap();
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
        let doc_ids_results: Vec<ctxvault_common::types::SearchResult> =
            serde_json::from_value(doc_ids_res).unwrap();
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
        let default_results: Vec<ctxvault_common::types::SearchResult> =
            serde_json::from_value(default_res).unwrap();
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
}
