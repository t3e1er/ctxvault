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

use cxtvault_common::config::{CorpusMode, EdgeClass};
use cxtvault_common::{Error, Result};
use cxtvault_core::engine::Engine;
use cxtvault_core::search;
use cxtvault_core::template::Template;

// ---------------------------------------------------------------------------
// Registry types
// ---------------------------------------------------------------------------

/// MCP tool handler function signature.
pub type ToolHandler = fn(&mut Engine, Value) -> Result<Value>;

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

/// Registry of all available MCP tools.
pub struct ToolRegistry {
    tools: HashMap<String, ToolInfo>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    /// Register all available tools.
    pub fn register_all(&mut self) {
        // Read tools
        self.register(ToolInfo {
            name: "read_note".to_string(),
            description: "Read a note's full content and frontmatter metadata.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note within the corpus" }
                },
                "required": ["path"]
            }),
            handler: handle_read_note,
        });

        self.register(ToolInfo {
            name: "list_notes".to_string(),
            description: "List all indexed notes with metadata (path, title, template, content_hash).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "Maximum number of notes to return (default 100)" },
                    "offset": { "type": "number", "description": "Offset for pagination (default 0)" }
                },
                "required": []
            }),
            handler: handle_list_notes,
        });

        self.register(ToolInfo {
            name: "get_frontmatter".to_string(),
            description: "Get the parsed YAML frontmatter of a note as JSON.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note within the corpus" }
                },
                "required": ["path"]
            }),
            handler: handle_get_frontmatter,
        });

        // Search tools
        self.register(ToolInfo {
            name: "search_bm25".to_string(),
            description: "Full-text BM25 keyword search across all indexed notes.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" }
                },
                "required": ["query"]
            }),
            handler: handle_search_bm25,
        });

        self.register(ToolInfo {
            name: "search_semantic".to_string(),
            description: "Vector similarity search using embedding cosine distance. Supports dual-level retrieval via depth parameter.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language search query" },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" },
                    "depth": { "type": "string", "enum": ["precise", "broad", "adaptive"], "description": "Retrieval depth: precise (chunk-level, default), broad (doc-level), adaptive (both + RRF)" }
                },
                "required": ["query"]
            }),
            handler: handle_search_semantic,
        });

        self.register(ToolInfo {
            name: "search_hybrid".to_string(),
            description: "BM25 + graph-boosted hybrid search combining keyword relevance with graph proximity.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" },
                    "graph_depth": { "type": "number", "description": "Max graph traversal depth for boosting (default 2)" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter graph traversal by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter graph boost traversal by edge class (default: semantic)" },
                    "decompose": { "type": "boolean", "description": "Enable query decomposition for multi-hop queries (default: false)" }
                },
                "required": ["query"]
            }),
            handler: handle_search_hybrid,
        });

        self.register(ToolInfo {
            name: "search_graph".to_string(),
            description: "Typed graph traversal search: finds nodes reachable from query matches via graph edges.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query to find seed nodes" },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" },
                    "max_depth": { "type": "number", "description": "Maximum traversal depth (default 3)" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter traversal by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter traversal by edge class (default: structural)" }
                },
                "required": ["query"]
            }),
            handler: handle_search_graph,
        });

        self.register(ToolInfo {
            name: "search_related".to_string(),
            description: "Find related documents via graph-based Personalized PageRank approximation.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "seeds": { "type": "array", "items": { "type": "string" }, "description": "Seed document paths to find related notes for" },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" }
                },
                "required": ["seeds"]
            }),
            handler: handle_search_related,
        });

        self.register(ToolInfo {
            name: "search_explain".to_string(),
            description: "Returns full scoring breakdown for a query: BM25, vector, and graph components with rank and RRF contribution per result.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "number", "description": "Maximum results to return (default 10)" },
                    "graph_depth": { "type": "number", "description": "Max graph traversal depth (default 2)" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter graph traversal by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter graph traversal by edge class" }
                },
                "required": ["query"]
            }),
            handler: handle_search_explain,
        });

        // Graph tools
        self.register(ToolInfo {
            name: "backlinks".to_string(),
            description: "Get all notes that link TO a given note, grouped by edge type.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the target note" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter by edge class" }
                },
                "required": ["path"]
            }),
            handler: handle_backlinks,
        });

        self.register(ToolInfo {
            name: "forwardlinks".to_string(),
            description: "Get all notes that a given note links TO, grouped by edge type.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the source note" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter by edge class" }
                },
                "required": ["path"]
            }),
            handler: handle_forwardlinks,
        });

        self.register(ToolInfo {
            name: "graph_path".to_string(),
            description: "Find the shortest path between two notes in the knowledge graph.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Source note path" },
                    "to": { "type": "string", "description": "Target note path" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter path search by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter path search by edge class" }
                },
                "required": ["from", "to"]
            }),
            handler: handle_graph_path,
        });

        self.register(ToolInfo {
            name: "graph_stats".to_string(),
            description: "Get graph statistics: node count, edge count, orphans, most connected nodes, edge type distribution.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handler: handle_graph_stats,
        });

        self.register(ToolInfo {
            name: "graph_subgraph".to_string(),
            description: "Get the N-hop neighborhood (subgraph) around a node via BFS traversal.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Starting node path" },
                    "depth": { "type": "number", "description": "Maximum traversal depth (default 2)" },
                    "edge_types": { "type": "array", "items": { "type": "string" }, "description": "Filter traversal by edge types" },
                    "edge_class": { "type": "string", "enum": ["semantic", "structural", "hybrid"], "description": "Filter traversal by edge class" }
                },
                "required": ["path"]
            }),
            handler: handle_graph_subgraph,
        });

        self.register(ToolInfo {
            name: "graph_communities".to_string(),
            description: "Detect communities in the knowledge graph using the Louvain modularity algorithm. Returns community assignments with modularity scores.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "include_density": { "type": "boolean", "description": "Include per-community density statistics (default false)" }
                },
                "required": []
            }),
            handler: handle_graph_communities,
        });

        self.register(ToolInfo {
            name: "list_edge_types".to_string(),
            description: "List all configured edge types in the taxonomy with their classes, sources, descriptions, template constraints, and live edge counts.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "edge_class": { "type": "string", "enum": ["structural", "semantic", "all", "hybrid"], "description": "Filter by edge class (default: all)" }
                },
                "required": []
            }),
            handler: handle_list_edge_types,
        });

        self.register(ToolInfo {
            name: "traverse_lineage".to_string(),
            description: "Deterministically traverse the knowledge graph along a structural edge type (e.g. supersedes, implements, depends_on) in outgoing, incoming, or both directions.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "start_path": { "type": "string", "description": "Starting document path" },
                    "edge_type": { "type": "string", "description": "Structural edge type to traverse (e.g. 'supersedes', 'implements', 'depends_on')" },
                    "direction": { "type": "string", "enum": ["outgoing", "incoming", "both"], "description": "Direction of traversal (default: 'outgoing')" },
                    "max_depth": { "type": "number", "description": "Maximum traversal depth hops (default 3)" }
                },
                "required": ["start_path", "edge_type"]
            }),
            handler: handle_traverse_lineage,
        });

        // Write tools
        self.register(ToolInfo {
            name: "create_note".to_string(),
            description: "Create a new note with optional frontmatter and content.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path for the new note (e.g. 'projects/my-note.md')" },
                    "content": { "type": "string", "description": "Body content of the note (markdown)" },
                    "frontmatter": { "type": "object", "description": "YAML frontmatter fields as key-value pairs" },
                    "template": { "type": "string", "description": "Template name to set in frontmatter" }
                },
                "required": ["path"]
            }),
            handler: handle_create_note,
        });

        self.register(ToolInfo {
            name: "update_note".to_string(),
            description: "Update an existing note's content (overwrite, append, or prepend).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note to update" },
                    "content": { "type": "string", "description": "New content to apply" },
                    "mode": { "type": "string", "enum": ["overwrite", "append", "prepend"], "description": "How to apply the content (default: overwrite)" }
                },
                "required": ["path", "content"]
            }),
            handler: handle_update_note,
        });

        self.register(ToolInfo {
            name: "delete_note".to_string(),
            description: "Delete a note from disk and remove it from all indices.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note to delete" }
                },
                "required": ["path"]
            }),
            handler: handle_delete_note,
        });

        self.register(ToolInfo {
            name: "move_note".to_string(),
            description: "Move/rename a note, updating wikilinks in other notes that reference it.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Current relative path of the note" },
                    "to": { "type": "string", "description": "New relative path for the note" }
                },
                "required": ["from", "to"]
            }),
            handler: handle_move_note,
        });

        self.register(ToolInfo {
            name: "promote_concept".to_string(),
            description: "Crystallize fluid memory notes into a consolidated, templated concept note. Validates schema before writing and indexing, with atomic rollback on validation failure.".to_string(),
            input_schema: serde_json::json!({
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
            handler: handle_promote_concept,
        });

        // Validation tools
        self.register(ToolInfo {
            name: "validate_note".to_string(),
            description: "Validate a note against its declared template (checks required fields, types, sections).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to the note within the corpus" }
                },
                "required": ["path"]
            }),
            handler: handle_validate_note,
        });

        self.register(ToolInfo {
            name: "validate_corpus".to_string(),
            description: "Validate all templated notes in the corpus, returning only those with issues.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "Maximum number of results to return (default: all)" }
                },
                "required": []
            }),
            handler: handle_validate_corpus,
        });

        self.register(ToolInfo {
            name: "list_templates".to_string(),
            description: "List all available templates with their field schemas and content rules."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handler: handle_list_templates,
        });

        self.register(ToolInfo {
            name: "validate_taxonomy".to_string(),
            description: "Ontology & structural graph integrity linter: checks for broken wikilinks, circular dependencies in DAG relations (supersedes, depends_on), orphan ADRs, and template constraints.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "check_broken": { "type": "boolean", "description": "Check for broken links (default true)" },
                    "check_cycles": { "type": "boolean", "description": "Check for circular dependencies in DAG relations (default true)" },
                    "check_orphans": { "type": "boolean", "description": "Check for orphan ADR notes (default true)" }
                },
                "required": []
            }),
            handler: handle_validate_taxonomy,
        });

        // Analytics tools
        self.register(ToolInfo {
            name: "analyze_density".to_string(),
            description: "Analyze graph density: orphans, hubs, edge distribution, overall connectivity.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "top_hubs": { "type": "number", "description": "Number of top hub nodes to return (default 10)" }
                },
                "required": []
            }),
            handler: handle_analyze_density,
        });

        self.register(ToolInfo {
            name: "find_semantic_gaps".to_string(),
            description: "Find queries where BM25 and vector search disagree — potential embedding blind spots.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "queries": { "type": "array", "items": { "type": "string" }, "description": "Test queries to evaluate" },
                    "top_k": { "type": "number", "description": "Number of results to compare per query (default 10)" }
                },
                "required": ["queries"]
            }),
            handler: handle_find_semantic_gaps,
        });

        self.register(ToolInfo {
            name: "suggest_splits".to_string(),
            description: "Identify chunks with low coherence that may benefit from splitting.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "max_chunk_chars": { "type": "number", "description": "Character threshold for 'too long' chunks (default 2000)" }
                },
                "required": []
            }),
            handler: handle_suggest_splits,
        });

        self.register(ToolInfo {
            name: "coverage_report".to_string(),
            description: "For a set of test queries, identify which notes are never retrieved (dead zones).".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "queries": { "type": "array", "items": { "type": "string" }, "description": "Test queries to evaluate coverage" },
                    "top_k": { "type": "number", "description": "Number of results per query (default 10)" }
                },
                "required": ["queries"]
            }),
            handler: handle_coverage_report,
        });

        // System tools
        self.register(ToolInfo {
            name: "corpus_list".to_string(),
            description:
                "List all configured corpora with their modes, file counts, and index stats."
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handler: handle_corpus_list,
        });

        self.register(ToolInfo {
            name: "reembed_corpus".to_string(),
            description: "Re-embed all chunks with the current embedding model. Use after changing models to update vectors without losing data.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handler: handle_reembed_corpus,
        });

        self.register(ToolInfo {
            name: "sync_corpus".to_string(),
            description: "Delta sync: compare filesystem against the index, add new files, update modified files, remove deleted files. Lightweight — only processes changes since last sync.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handler: handle_sync_corpus,
        });

        self.register(ToolInfo {
            name: "reindex_corpus".to_string(),
            description: "Full reindex: clear all indices and re-index every file in the corpus from scratch. Use when the index is corrupted or after major changes to chunking/config.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            handler: handle_reindex_corpus,
        });
    }

    /// Register a single tool.
    fn register(&mut self, info: ToolInfo) {
        let _ = self.tools.insert(info.name.clone(), info);
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

    /// Execute a tool by name with given arguments.
    pub fn execute(&self, name: &str, engine: &mut Engine, args: Value) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| Error::NotFound(format!("tool not found: {}", name)))?;
        (tool.handler)(engine, args)
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

use cxtvault_core::corpus_manager::CorpusManager;

/// Multi-corpus tool registry: wraps a `CorpusManager` and routes tool calls
/// to the correct engine based on an optional `corpus` parameter in tool arguments.
///
/// When `corpus` is provided, routes to that specific corpus engine.
/// When omitted, routes to the default corpus (backward compatible).
pub struct MultiCorpusToolRegistry {
    registry: ToolRegistry,
}

impl MultiCorpusToolRegistry {
    /// Create a new multi-corpus registry with all tools registered.
    pub fn new() -> Self {
        let mut registry = ToolRegistry::new();
        registry.register_all();

        // Register the get_status tool (only available in multi-corpus mode).
        registry.register(ToolInfo {
            name: "get_status".to_string(),
            description: "Get overall system status: corpora, index states, watcher status, embedding model info.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            // Placeholder handler — actual logic is in execute_multi.
            handler: handle_get_status_placeholder,
        });

        Self { registry }
    }

    /// List all registered tools (for MCP `tools/list`).
    pub fn list(&self) -> Vec<&ToolInfo> {
        self.registry.list()
    }

    /// Execute a tool call, routing to the correct corpus engine.
    ///
    /// Extracts the optional `corpus` field from arguments, resolves the engine
    /// from the CorpusManager, and dispatches to the tool handler.
    pub fn execute(&self, name: &str, manager: &mut CorpusManager, args: Value) -> Result<Value> {
        // Special handling for get_status — needs the whole CorpusManager.
        if name == "get_status" {
            return handle_get_status(manager);
        }

        // Extract and remove the `corpus` param from arguments.
        let (corpus_name, clean_args) = extract_corpus_param(args);

        // Resolve the engine.
        let engine = manager.resolve_engine_mut(corpus_name.as_deref())?;

        // Execute the tool.
        self.registry.execute(name, engine, clean_args)
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

/// Extract the optional `corpus` field from tool arguments, returning
/// the corpus name and the arguments with `corpus` removed.
fn extract_corpus_param(args: Value) -> (Option<String>, Value) {
    match args {
        Value::Object(mut map) => {
            let corpus = map.remove("corpus").and_then(|v| v.as_str().map(|s| s.to_string()));
            (corpus, Value::Object(map))
        }
        other => (None, other),
    }
}

/// Placeholder handler for get_status (not actually called via this path).
fn handle_get_status_placeholder(_engine: &mut Engine, _args: Value) -> Result<Value> {
    Ok(serde_json::json!({
        "error": "get_status must be called via MultiCorpusToolRegistry"
    }))
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
struct ListNotesParams {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Deserialize)]
struct GetFrontmatterParams {
    path: String,
}

#[derive(Deserialize)]
struct SearchBm25Params {
    query: String,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct SearchSemanticParams {
    query: String,
    limit: Option<usize>,
    depth: Option<String>,
}

#[derive(Deserialize)]
struct SearchHybridParams {
    query: String,
    limit: Option<usize>,
    graph_depth: Option<usize>,
    edge_types: Option<Vec<String>>,
    edge_class: Option<String>,
    decompose: Option<bool>,
}

#[derive(Deserialize)]
struct SearchGraphParams {
    query: String,
    limit: Option<usize>,
    max_depth: Option<usize>,
    edge_types: Option<Vec<String>>,
    edge_class: Option<String>,
}

#[derive(Deserialize)]
struct SearchRelatedParams {
    seeds: Vec<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct SearchExplainParams {
    query: String,
    limit: Option<usize>,
    graph_depth: Option<usize>,
    edge_types: Option<Vec<String>>,
    edge_class: Option<String>,
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
fn handle_read_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: ReadNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", params.path, e)))?;

    let doc = cxtvault_core::parser::parse_document(Path::new(&params.path), &content)?;

    let response = NoteResponse {
        path: params.path,
        title: doc.title,
        frontmatter: doc.frontmatter,
        content: doc.content,
        content_hash: doc.content_hash,
    };

    serde_json::to_value(response).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// List all indexed notes with metadata.
fn handle_list_notes(engine: &mut Engine, args: Value) -> Result<Value> {
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
fn handle_get_frontmatter(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: GetFrontmatterParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", params.path, e)))?;

    let doc = cxtvault_core::parser::parse_document(Path::new(&params.path), &content)?;

    let frontmatter = doc.frontmatter.unwrap_or(Value::Null);

    Ok(frontmatter)
}

/// Full-text BM25 keyword search.
fn handle_search_bm25(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SearchBm25Params = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(10);
    let mut results = search::search_bm25(engine.bm25(), &params.query, limit)?;
    search::enrich_results_with_lineage(&mut results, engine.graph());

    serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Semantic vector search using embedding similarity.
fn handle_search_semantic(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SearchSemanticParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(10);
    let depth = params
        .depth
        .as_deref()
        .and_then(cxtvault_common::types::SearchDepth::from_str_name)
        .unwrap_or_default();

    // Ensure the embedder is initialized.
    let _ = engine.ensure_embedder()?;

    let embedder = match engine.embedder_ref() {
        Some(e) => e,
        None => {
            return Err(Error::Index(
                "embedder not available — cannot perform semantic search".to_string(),
            ));
        }
    };

    let mut results =
        search::search_semantic_dual(engine.vector_index(), embedder, &params.query, limit, depth)?;
    search::enrich_results_with_lineage(&mut results, engine.graph());

    serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// BM25 + vector + graph hybrid search (true 3-signal fusion via RRF).
fn handle_search_hybrid(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SearchHybridParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(10);
    let graph_depth = params.graph_depth.unwrap_or(2);
    let edge_type_filter = params.edge_types;

    // Default to Semantic class filter for hybrid search graph boost.
    let edge_class_filter = match params.edge_class.as_deref() {
        Some(s) => EdgeClass::from_str_name(s),
        None => Some(EdgeClass::Semantic),
    };

    // Try to get a query embedding for full 3-signal hybrid.
    // If embedder is not available, fall back to BM25+graph only.
    let query_embedding =
        engine.embedder_ref().and_then(|embedder| embedder.embed_query(&params.query).ok());

    let results = if params.decompose == Some(true) {
        // Multi-hop query decomposition mode.
        search::search_multihop(
            engine.bm25(),
            engine.vector_index(),
            engine.graph(),
            engine.embedder_ref(),
            &params.query,
            query_embedding.as_deref(),
            limit,
            graph_depth,
            edge_type_filter.as_deref(),
        )?
    } else {
        search::search_hybrid_full(
            engine.bm25(),
            engine.vector_index(),
            engine.graph(),
            &params.query,
            query_embedding.as_deref(),
            limit,
            graph_depth,
            edge_type_filter.as_deref(),
            edge_class_filter,
        )?
    };

    serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Typed graph traversal search.
fn handle_search_graph(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SearchGraphParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(10);
    let max_depth = params.max_depth.unwrap_or(3);
    let edge_type_filter = params.edge_types;

    // Default to Structural class filter for graph traversal search.
    let edge_class_filter = match params.edge_class.as_deref() {
        Some(s) => EdgeClass::from_str_name(s),
        None => Some(EdgeClass::Structural),
    };

    let results = search::search_graph(
        engine.bm25(),
        engine.graph(),
        &params.query,
        limit,
        max_depth,
        edge_type_filter.as_deref(),
        edge_class_filter,
    )?;

    serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Find related documents via PPR approximation.
fn handle_search_related(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SearchRelatedParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(10);

    let results = search::search_related(engine.graph(), &params.seeds, limit, 0.85, 20)?;

    serde_json::to_value(results).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Full scoring breakdown — returns detailed per-result explanation.
fn handle_search_explain(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SearchExplainParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let limit = params.limit.unwrap_or(10);
    let graph_depth = params.graph_depth.unwrap_or(2);
    let edge_type_filter = params.edge_types;
    let edge_class_filter = params.edge_class.as_deref().and_then(EdgeClass::from_str_name);

    // Try to get a query embedding for full 3-signal explanation.
    let query_embedding =
        engine.embedder_ref().and_then(|embedder| embedder.embed_query(&params.query).ok());

    let explanations = search::search_explain(
        engine.bm25(),
        engine.vector_index(),
        engine.graph(),
        &params.query,
        query_embedding.as_deref(),
        limit,
        graph_depth,
        edge_type_filter.as_deref(),
        edge_class_filter,
    )?;

    serde_json::to_value(explanations).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// All notes linking TO a note, grouped by edge type.
fn handle_backlinks(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: BacklinksParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let edge_class_filter = params.edge_class.as_deref().and_then(EdgeClass::from_str_name);
    let backlinks = engine.graph().backlinks(&params.path, edge_class_filter);

    serde_json::to_value(backlinks).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// All notes a note links TO, grouped by edge type.
fn handle_forwardlinks(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: ForwardlinksParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let edge_class_filter = params.edge_class.as_deref().and_then(EdgeClass::from_str_name);
    let forwardlinks = engine.graph().forwardlinks(&params.path, edge_class_filter);

    serde_json::to_value(forwardlinks).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Shortest path between two notes.
fn handle_graph_path(engine: &mut Engine, args: Value) -> Result<Value> {
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
fn handle_graph_stats(engine: &mut Engine, _args: Value) -> Result<Value> {
    let stats = engine.graph().stats();

    serde_json::to_value(stats).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// N-hop neighborhood around a node.
fn handle_graph_subgraph(engine: &mut Engine, args: Value) -> Result<Value> {
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
fn handle_graph_communities(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: GraphCommunitiesParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let include_density = params.include_density.unwrap_or(false);

    let result = engine.graph().detect_communities();

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
fn handle_corpus_list(engine: &mut Engine, _args: Value) -> Result<Value> {
    let file_count = engine.store().list_files().map(|f| f.len()).unwrap_or(0);
    let info = serde_json::json!([{
        "name": engine.config().name,
        "path": engine.config().path,
        "mode": format!("{:?}", engine.config().mode),
        "file_count": file_count,
        "embedder_active": engine.embedder_ref().is_some(),
        "vector_count": engine.vector_index().len(),
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

/// Delta sync: index new/modified files, remove deleted files.
fn handle_sync_corpus(engine: &mut Engine, _args: Value) -> Result<Value> {
    let result = engine.delta_scan()?;

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

/// Full reindex: clear all indices and rebuild from scratch.
fn handle_reindex_corpus(engine: &mut Engine, _args: Value) -> Result<Value> {
    let count = engine.full_reindex()?;

    Ok(serde_json::json!({
        "status": "complete",
        "files_indexed": count,
    }))
}

/// Graph density analysis.
fn handle_analyze_density(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: AnalyzeDensityParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let top_hubs = params.top_hubs.unwrap_or(10);
    let report = cxtvault_core::analytics::analyze_density(engine.graph(), top_hubs);

    serde_json::to_value(report).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Find semantic gaps between BM25 and vector search.
fn handle_find_semantic_gaps(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: FindSemanticGapsParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let top_k = params.top_k.unwrap_or(10);

    // Embed each query (requires embedder).
    let query_embeddings: Vec<Vec<f32>> = if let Some(embedder) = engine.embedder_ref() {
        params.queries.iter().filter_map(|q| embedder.embed_query(q).ok()).collect()
    } else {
        Vec::new()
    };

    // If no embeddings available, return empty gaps (can't compare).
    if query_embeddings.len() != params.queries.len() {
        return Ok(serde_json::json!({
            "error": "embedder not available or some queries failed to embed",
            "gaps": []
        }));
    }

    let query_refs: Vec<&str> = params.queries.iter().map(|s| s.as_str()).collect();
    let gaps = cxtvault_core::analytics::find_semantic_gaps(
        engine.bm25(),
        engine.vector_index(),
        &query_refs,
        &query_embeddings,
        top_k,
    )?;

    serde_json::to_value(gaps).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Suggest chunks that may benefit from splitting.
fn handle_suggest_splits(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: SuggestSplitsParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let max_chunk_chars = params.max_chunk_chars.unwrap_or(2000);
    let suggestions = cxtvault_core::analytics::suggest_splits(engine.store(), max_chunk_chars)?;

    serde_json::to_value(suggestions).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Coverage report: which notes are never retrieved.
fn handle_coverage_report(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: CoverageReportParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let top_k = params.top_k.unwrap_or(10);

    // Get all known note paths from the store.
    let files = engine.store().list_files()?;
    let all_paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();

    let query_refs: Vec<&str> = params.queries.iter().map(|s| s.as_str()).collect();
    let report =
        cxtvault_core::analytics::coverage_report(engine.bm25(), &query_refs, &all_paths, top_k)?;

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
fn handle_validate_note(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: ValidateNoteParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let corpus_path = PathBuf::from(&engine.config().path);
    let full_path = corpus_path.join(&params.path);

    let content = fs::read_to_string(&full_path)
        .map_err(|e| Error::NotFound(format!("cannot read {}: {}", params.path, e)))?;

    let doc = cxtvault_core::parser::parse_document(Path::new(&params.path), &content)?;

    // Determine which template the note declares.
    let template_name = doc.template.clone();

    let (valid, issues, tmpl_name) = if let Some(ref name) = template_name {
        let templates = load_corpus_templates(engine)?;
        if let Some(tmpl) = templates.get(name) {
            let issues = tmpl.validate(&doc.frontmatter, &doc.content);
            let valid =
                !issues.iter().any(|i| i.severity == cxtvault_core::template::Severity::Error);
            (valid, issues, Some(name.clone()))
        } else {
            // Template declared but not found — report as warning.
            let issues = vec![cxtvault_core::template::ValidationIssue {
                severity: cxtvault_core::template::Severity::Warning,
                message: format!("template '{}' not found in templates directory", name),
                field: Some("template".to_string()),
            }];
            (true, issues, Some(name.clone()))
        }
    } else {
        // No template declared — nothing to validate.
        (true, Vec::new(), None)
    };

    let result = cxtvault_core::template::ValidationResult {
        path: params.path,
        template: tmpl_name,
        valid,
        issues,
    };

    serde_json::to_value(result).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// Validate all templated notes in the corpus.
fn handle_validate_corpus(engine: &mut Engine, args: Value) -> Result<Value> {
    let params: ValidateCorpusParams = serde_json::from_value(args)
        .map_err(|e| Error::Config(format!("invalid params: {}", e)))?;

    let templates = load_corpus_templates(engine)?;
    let files = engine.store().list_files()?;
    let corpus_path = PathBuf::from(&engine.config().path);

    let mut results: Vec<cxtvault_core::template::ValidationResult> = Vec::new();

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

        let doc = match cxtvault_core::parser::parse_document(Path::new(&file.path), &content) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let issues = if let Some(tmpl) = templates.get(&tmpl_name) {
            tmpl.validate(&doc.frontmatter, &doc.content)
        } else {
            vec![cxtvault_core::template::ValidationIssue {
                severity: cxtvault_core::template::Severity::Warning,
                message: format!("template '{}' not found in templates directory", tmpl_name),
                field: Some("template".to_string()),
            }]
        };

        // Only include notes that have issues.
        if !issues.is_empty() {
            let valid =
                !issues.iter().any(|i| i.severity == cxtvault_core::template::Severity::Error);
            results.push(cxtvault_core::template::ValidationResult {
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
fn handle_list_templates(engine: &mut Engine, _args: Value) -> Result<Value> {
    let templates = load_corpus_templates(engine)?;

    let mut list: Vec<&Template> = templates.values().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));

    serde_json::to_value(list).map_err(|e| Error::Config(format!("serialize error: {}", e)))
}

/// List all edge types in the taxonomy.
fn handle_list_edge_types(engine: &mut Engine, args: Value) -> Result<Value> {
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
fn handle_traverse_lineage(engine: &mut Engine, args: Value) -> Result<Value> {
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
                issues.iter().any(|i| i.severity == cxtvault_core::template::Severity::Error);
            validation_issues = issues;
            if has_error {
                let error_msgs: Vec<String> = validation_issues
                    .iter()
                    .filter(|i| i.severity == cxtvault_core::template::Severity::Error)
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
                        cxtvault_core::parser::parse_document(Path::new(src), &src_content)
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
fn handle_validate_taxonomy(engine: &mut Engine, args: Value) -> Result<Value> {
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use cxtvault_common::config::{
        ChunkingConfig, CorpusConfig, CorpusMode, EdgeSource, EdgeTypeConfig, EmbeddingConfig,
        GraphConfig,
    };
    use cxtvault_common::types::EdgeProvenance;
    use std::fs;
    use tempfile::TempDir;

    /// Create a minimal corpus config pointing at the given path.
    fn test_config(corpus_path: &std::path::Path) -> CorpusConfig {
        CorpusConfig {
            name: "test".to_string(),
            path: corpus_path.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
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
        assert_eq!(tools.len(), 34, "Expected 34 tools registered");

        // Verify each expected tool exists.
        let expected = [
            "read_note",
            "list_notes",
            "get_frontmatter",
            "search_bm25",
            "search_semantic",
            "search_hybrid",
            "search_graph",
            "search_related",
            "search_explain",
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
            "corpus_list",
            "reembed_corpus",
            "sync_corpus",
            "reindex_corpus",
        ];

        for name in expected {
            assert!(registry.get(name).is_some(), "Tool '{}' should be registered", name);
        }
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
                "search_bm25",
                &mut engine,
                serde_json::json!({ "query": "systems programming" }),
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
            .execute("search_bm25", &mut engine, serde_json::json!({ "query": "new note" }))
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
            .execute("search_bm25", &mut engine, serde_json::json!({ "query": "Going away" }))
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
    fn test_multi_corpus_registry_has_get_status() {
        let registry = MultiCorpusToolRegistry::new();
        let tools = registry.list();

        // Should have 35 tools (34 base + get_status).
        assert_eq!(tools.len(), 35, "Expected 35 tools in multi-corpus registry");
        assert!(registry.registry().get("get_status").is_some(), "get_status should be registered");
    }

    #[test]
    fn test_multi_corpus_routing_default() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let mut manager =
            cxtvault_core::corpus_manager::CorpusManager::new(&tmp.path().join("indices"));
        let config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
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
            .execute("search_bm25", &mut manager, serde_json::json!({ "query": "wiki content" }))
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

        let mut manager =
            cxtvault_core::corpus_manager::CorpusManager::new(&tmp.path().join("indices"));

        let wiki_config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };
        let docs_config = CorpusConfig {
            name: "docs".to_string(),
            path: docs_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
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
                "search_bm25",
                &mut manager,
                serde_json::json!({ "query": "programming", "corpus": "wiki" }),
            )
            .unwrap();
        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(!results.is_empty(), "Should find rust.md in wiki");
        assert_eq!(results[0]["path"], "rust.md");

        // Search in docs corpus explicitly.
        let result = registry
            .execute(
                "search_bm25",
                &mut manager,
                serde_json::json!({ "query": "documentation", "corpus": "docs" }),
            )
            .unwrap();
        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(!results.is_empty(), "Should find python.md in docs");
        assert_eq!(results[0]["path"], "python.md");

        // Verify isolation: searching wiki for python returns nothing.
        let result = registry
            .execute(
                "search_bm25",
                &mut manager,
                serde_json::json!({ "query": "python documentation", "corpus": "wiki" }),
            )
            .unwrap();
        let results: Vec<Value> = serde_json::from_value(result).unwrap();
        assert!(
            results.is_empty() || results.iter().all(|r| r["path"] != "python.md"),
            "Wiki corpus should not contain python.md"
        );
    }

    #[test]
    fn test_multi_corpus_get_status() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let mut manager =
            cxtvault_core::corpus_manager::CorpusManager::new(&tmp.path().join("indices"));
        let config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };
        manager.add_corpus(config).unwrap();

        let registry = MultiCorpusToolRegistry::new();

        let result = registry.execute("get_status", &mut manager, serde_json::json!({})).unwrap();

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

        let mut manager =
            cxtvault_core::corpus_manager::CorpusManager::new(&tmp.path().join("indices"));
        let config = CorpusConfig {
            name: "wiki".to_string(),
            path: wiki_dir.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        };
        manager.add_corpus(config).unwrap();

        let registry = MultiCorpusToolRegistry::new();

        // Non-existent corpus should error.
        let result = registry.execute(
            "search_bm25",
            &mut manager,
            serde_json::json!({ "query": "test", "corpus": "nonexistent" }),
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
}
