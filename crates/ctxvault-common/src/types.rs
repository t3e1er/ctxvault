//! Core domain types shared across crates.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A unique identifier for a document (note) within a corpus.
pub type DocId = String;

/// A parsed markdown document with extracted metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Relative path within the corpus.
    pub path: String,
    /// Parsed YAML frontmatter (if present).
    pub frontmatter: Option<serde_json::Value>,
    /// Document title (from frontmatter or first heading).
    pub title: Option<String>,
    /// Extracted tags (from frontmatter and inline #tags).
    pub tags: Vec<String>,
    /// Wikilinks found in the content.
    pub wikilinks: Vec<WikiLink>,
    /// The template this note declares (from frontmatter `template:` field).
    pub template: Option<String>,
    /// Raw markdown content (without frontmatter block).
    pub content: String,
    /// Content hash for change detection.
    pub content_hash: String,
}

/// A wikilink reference found in a document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiLink {
    /// The target path or name (what's inside the `[[...]]`).
    pub target: String,
    /// Optional display alias (from `[[target|alias]]`).
    pub alias: Option<String>,
}

/// Discriminates between documentation notes and polyglot source code entities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// Markdown documentation note, RFC, or ADR.
    Documentation,
    /// Whole source code file (e.g. `src/engine.rs`).
    CodeFile {
        /// Programming language of the source file.
        language: String,
    },
    /// Distinct code symbol (function, struct, class, trait, interface, etc.).
    CodeSymbol {
        /// Programming language of the symbol.
        language: String,
        /// Classification of the symbol.
        symbol_type: CodeSymbolType,
        /// Hierarchical scope path (e.g. `crate::search::Engine`).
        scope_path: String,
        /// Full signature or declaration line.
        signature: String,
    },
    /// Syntactically coherent AST chunk for vector and BM25 indexing.
    CodeChunk {
        /// Programming language of the chunk.
        language: String,
        /// Hierarchical scope path breadcrumb.
        scope_path: String,
        /// 1-based start line number in original file.
        start_line: usize,
        /// 1-based end line number in original file.
        end_line: usize,
    },
}

impl Default for EntityKind {
    fn default() -> Self {
        Self::Documentation
    }
}

/// The specific classification of a code symbol.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CodeSymbolType {
    /// Standalone function.
    Function,
    /// Method associated with a struct, class, or trait.
    Method,
    /// Struct data structure.
    Struct,
    /// Object-oriented class.
    Class,
    /// Rust trait definition.
    Trait,
    /// Interface definition.
    Interface,
    /// Enum type definition.
    Enum,
    /// Module or namespace declaration.
    Module,
    /// Constant or static value.
    Constant,
    /// Type alias definition.
    TypeAlias,
}

/// A structured code symbol record extracted via AST analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSymbol {
    /// File path where the symbol is defined.
    pub file_path: String,
    /// Identifier name (e.g. "search_hybrid").
    pub name: String,
    /// Fully qualified hierarchical scope (e.g. "ctxvault_core::search::Engine").
    pub scope_path: String,
    /// Symbol classification.
    pub symbol_type: CodeSymbolType,
    /// Source code language (e.g. "rust", "typescript", "python", "go").
    pub language: String,
    /// Signature or declaration snippet (e.g. "pub fn search_hybrid(&self, ...) -> Result<Vec<SearchResult>>").
    pub signature: String,
    /// Docstring or preceding documentation comments if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based end line.
    pub end_line: usize,
}

/// A text chunk ready for embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// The document or code file this chunk belongs to.
    pub doc_path: String,
    /// Zero-based index of this chunk within the document.
    pub chunk_index: usize,
    /// The text content of this chunk.
    pub text: String,
    /// Byte offset of chunk start in original content.
    pub start_byte: usize,
    /// Byte offset of chunk end in original content.
    pub end_byte: usize,
    /// Heading hierarchy for this chunk (e.g., "Setup > Prerequisites").
    /// Populated by the heading-aware chunker; None for other strategies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_chain: Option<String>,
    /// Source code language if this is an AST code chunk (e.g. "rust", "typescript").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Hierarchical AST scope breadcrumb for code chunks (e.g. "crate::search::Engine > search_hybrid").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,
    /// Entity kind for this chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_kind: Option<EntityKind>,
}

impl Chunk {
    /// Create a standard document chunk with default metadata.
    pub fn new(
        doc_path: impl Into<String>,
        chunk_index: usize,
        text: impl Into<String>,
        start_byte: usize,
        end_byte: usize,
    ) -> Self {
        Self {
            doc_path: doc_path.into(),
            chunk_index,
            text: text.into(),
            start_byte,
            end_byte,
            heading_chain: None,
            language: None,
            scope_path: None,
            entity_kind: Some(EntityKind::Documentation),
        }
    }

    /// Set heading chain.
    pub fn with_heading_chain(mut self, heading_chain: Option<String>) -> Self {
        self.heading_chain = heading_chain;
        self
    }

    /// Set code AST metadata.
    pub fn with_code_metadata(
        mut self,
        language: impl Into<String>,
        scope_path: impl Into<String>,
        start_line: usize,
        end_line: usize,
    ) -> Self {
        let lang = language.into();
        let scope = scope_path.into();
        self.language = Some(lang.clone());
        self.scope_path = Some(scope.clone());
        self.entity_kind =
            Some(EntityKind::CodeChunk { language: lang, scope_path: scope, start_line, end_line });
        self
    }
}

/// A typed, weighted, directed edge in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source document or code entity path.
    pub source: String,
    /// Target document or code entity path.
    pub target: String,
    /// Edge type name (must match a registered `EdgeTypeConfig.name`).
    pub edge_type: String,
    /// Weight of this edge.
    pub weight: f32,
    /// How this edge was created.
    pub provenance: EdgeProvenance,
}

/// How an edge came into existence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeProvenance {
    /// Parsed from explicit wikilink in content.
    Wikilink,
    /// Derived from shared tag.
    SharedTag,
    /// Declared in frontmatter field.
    Frontmatter,
    /// Parsed from standard markdown link.
    MarkdownLink,
    /// AST symbol definition (file defines symbol).
    CodeDefines,
    /// Import / use dependency across files.
    CodeImports,
    /// Function/method call site invocation.
    CodeCalls,
    /// Trait or interface implementation.
    CodeImplementsTrait,
    /// Markdown documentation specifies or documents code symbol.
    DocumentsCode,
    /// Code entity implements an architecture decision record (ADR).
    ImplementsAdr,
    /// Inferred by LLM extraction (future: tiered ontological model).
    Inferred,
}

/// Structural lineage metadata annotations for a retrieved document.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct LineageAnnotation {
    /// Notes that supersede this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded_by: Vec<String>,
    /// Notes that this note supersedes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
    /// Notes implemented by this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
    /// Notes that implement this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implemented_by: Vec<String>,
    /// Notes that this note depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Notes that depend on this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depended_on_by: Vec<String>,
    /// Decisions that this note serves as an ADR for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adr_for: Vec<String>,
    /// ADRs that document this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub has_adr: Vec<String>,
    /// Parent notes in hierarchy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_of: Vec<String>,
    /// Child notes in hierarchy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_of: Vec<String>,
    /// All other active incoming structural links grouped by edge type.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub incoming: HashMap<String, Vec<String>>,
    /// All other active outgoing structural links grouped by edge type.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub outgoing: HashMap<String, Vec<String>>,
}

impl LineageAnnotation {
    /// Whether any lineage relations are present.
    pub fn is_empty(&self) -> bool {
        self.superseded_by.is_empty()
            && self.supersedes.is_empty()
            && self.implements.is_empty()
            && self.implemented_by.is_empty()
            && self.depends_on.is_empty()
            && self.depended_on_by.is_empty()
            && self.adr_for.is_empty()
            && self.has_adr.is_empty()
            && self.parent_of.is_empty()
            && self.child_of.is_empty()
            && self.incoming.is_empty()
            && self.outgoing.is_empty()
    }
}

/// A search result from any search strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Document or code entity path.
    pub path: String,
    /// Combined relevance score (0.0 - 1.0).
    pub score: f64,
    /// Text snippet showing the relevant passage.
    pub snippet: Option<String>,
    /// Which chunk matched (if applicable).
    pub chunk_index: Option<usize>,
    /// Score breakdown for explainability.
    pub score_components: Option<ScoreBreakdown>,
    /// Structural lineage annotations (e.g. superseded_by, implements).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<LineageAnnotation>,
    /// Entity kind for this search hit (e.g. Documentation, CodeSymbol, CodeChunk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_kind: Option<EntityKind>,
    /// Programming language if this result is from code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl SearchResult {
    /// Create a basic search result.
    pub fn new(path: impl Into<String>, score: f64) -> Self {
        Self {
            path: path.into(),
            score,
            snippet: None,
            chunk_index: None,
            score_components: None,
            lineage: None,
            entity_kind: None,
            language: None,
        }
    }

    /// Set snippet text.
    pub fn with_snippet(mut self, snippet: Option<String>) -> Self {
        self.snippet = snippet;
        self
    }

    /// Set chunk index.
    pub fn with_chunk_index(mut self, chunk_index: Option<usize>) -> Self {
        self.chunk_index = chunk_index;
        self
    }

    /// Set score components breakdown.
    pub fn with_score_components(mut self, components: ScoreBreakdown) -> Self {
        self.score_components = Some(components);
        self
    }

    /// Set entity kind.
    pub fn with_entity_kind(mut self, entity_kind: EntityKind) -> Self {
        self.entity_kind = Some(entity_kind);
        self
    }

    /// Set language.
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }
}

/// Breakdown of how a search score was computed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// BM25 component (0.0 if not applicable).
    pub bm25: f64,
    /// Vector cosine similarity component.
    pub vector: f64,
    /// Graph proximity boost.
    pub graph_boost: f64,
    /// Number of hops from seed in graph traversal.
    pub graph_hops: Option<usize>,
}

/// Depth level for dual-level retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchDepth {
    /// Chunk-level only — best for specific factual queries.
    #[default]
    Precise,
    /// Document-level only — best for "what do we know about X?" sensemaking.
    Broad,
    /// Both chunk and doc-level, merged with RRF — default.
    Adaptive,
}

impl SearchDepth {
    /// Parse from a string (case-insensitive).
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "precise" => Some(Self::Precise),
            "broad" => Some(Self::Broad),
            "adaptive" => Some(Self::Adaptive),
            _ => None,
        }
    }
}

/// Detailed explanation of how a search result was scored.
/// Richer than `ScoreBreakdown` — includes per-signal rank and RRF contributions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchExplanation {
    /// Document path.
    pub path: String,
    /// Final fused score (RRF total).
    pub final_score: f64,
    /// BM25 component details.
    pub bm25: SignalExplanation,
    /// Vector similarity component details.
    pub vector: SignalExplanation,
    /// Graph proximity component details.
    pub graph: GraphExplanation,
    /// Text snippet from the matched chunk.
    pub snippet: Option<String>,
    /// Which chunk matched (if applicable).
    pub chunk_index: Option<usize>,
}

/// Explanation of a single signal (BM25 or vector).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalExplanation {
    /// Raw score from this signal (BM25 score or cosine similarity).
    pub raw_score: f64,
    /// Rank position in this signal's result list (1-based, 0 if not present).
    pub rank: usize,
    /// RRF contribution from this signal: 1/(k + rank).
    pub rrf_contribution: f64,
}

/// Explanation of graph proximity signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphExplanation {
    /// Graph boost score (1/hop_distance accumulated).
    pub boost: f64,
    /// Minimum hops from any seed node.
    pub min_hops: Option<usize>,
    /// Rank position in graph signal's result list (1-based, 0 if not present).
    pub rank: usize,
    /// RRF contribution from graph signal.
    pub rrf_contribution: f64,
}
