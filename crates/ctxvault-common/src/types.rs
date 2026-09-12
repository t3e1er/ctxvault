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

impl EntityKind {
    /// Whether this entity is source code (anything other than [`EntityKind::Documentation`]).
    pub fn is_code(&self) -> bool {
        !matches!(self, EntityKind::Documentation)
    }

    /// Coarse modality tag for indexing/filtering: `"code"` for any code entity,
    /// `"docs"` for documentation.
    pub fn modality_tag(&self) -> &'static str {
        if self.is_code() {
            "code"
        } else {
            "docs"
        }
    }
}

/// Restricts search results to documentation, code, or both.
///
/// Applied consistently across BM25, vector, graph, and the fused hybrid path.
/// [`Modality::Both`] (the default) returns every entity kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    /// Only documentation entities ([`EntityKind::Documentation`]).
    Docs,
    /// Only code entities (`CodeFile`, `CodeSymbol`, `CodeChunk`).
    Code,
    /// Both documentation and code (no restriction).
    Both,
}

impl Default for Modality {
    fn default() -> Self {
        Modality::Both
    }
}

impl Modality {
    /// Parse from a string (case-insensitive): `"docs"`, `"code"`, or `"both"`.
    pub fn from_str_name(s: &str) -> Option<Modality> {
        match s.to_lowercase().as_str() {
            "docs" => Some(Modality::Docs),
            "code" => Some(Modality::Code),
            "both" => Some(Modality::Both),
            _ => None,
        }
    }

    /// Whether an [`EntityKind`] passes this modality filter.
    ///
    /// [`Modality::Both`] matches all kinds; [`Modality::Docs`] matches only
    /// [`EntityKind::Documentation`]; [`Modality::Code`] matches every code kind.
    pub fn matches_kind(self, kind: &EntityKind) -> bool {
        match self {
            Modality::Both => true,
            Modality::Docs => !kind.is_code(),
            Modality::Code => kind.is_code(),
        }
    }

    /// Whether a coarse modality tag (`"code"` / `"docs"`) passes this filter.
    pub fn matches_tag(self, tag: &str) -> bool {
        match self {
            Modality::Both => true,
            Modality::Docs => tag == "docs",
            Modality::Code => tag == "code",
        }
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
    /// Signature or declaration snippet (e.g. `pub fn search_hybrid(&self, ...) -> Result<Vec<SearchResult>>`).
    pub signature: String,
    /// Docstring or preceding documentation comments if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based end line.
    pub end_line: usize,
}

/// Status of an indexing operation for a corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexingStatus {
    /// No indexing job in progress.
    Idle,
    /// Actively indexing files in batches.
    Indexing,
    /// Indexing is paused.
    Paused,
    /// Indexing encountered an error.
    Error,
    /// All discovered files successfully indexed.
    Completed,
}

impl std::fmt::Display for IndexingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Indexing => write!(f, "indexing"),
            Self::Paused => write!(f, "paused"),
            Self::Error => write!(f, "error"),
            Self::Completed => write!(f, "completed"),
        }
    }
}

impl std::str::FromStr for IndexingStatus {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "idle" => Ok(Self::Idle),
            "indexing" => Ok(Self::Indexing),
            "paused" => Ok(Self::Paused),
            "error" => Ok(Self::Error),
            "completed" => Ok(Self::Completed),
            _ => Ok(Self::Idle),
        }
    }
}

/// State tracking record for resumable paginated indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingState {
    /// Corpus identifier/name.
    pub corpus_id: String,
    /// Current indexing status.
    pub status: IndexingStatus,
    /// Total markdown files discovered in corpus.
    pub total_files: usize,
    /// Count of markdown files successfully committed.
    pub indexed_files: usize,
    /// Relative path of the last committed file.
    pub last_processed_path: Option<String>,
    /// When indexing started (Unix timestamp seconds).
    pub started_at: i64,
    /// When state was last updated (Unix timestamp seconds).
    pub updated_at: i64,
    /// Error message if status is Error.
    pub error_message: Option<String>,
}

/// A tracked file in the index.
#[derive(Debug, Clone)]
pub struct FileRecord {
    /// Relative path within the corpus.
    pub path: String,
    /// BLAKE3 content hash.
    pub content_hash: String,
    /// File modification time as Unix timestamp (seconds).
    pub modified_at: i64,
    /// Template declared in frontmatter.
    pub template: Option<String>,
    /// Document title.
    pub title: Option<String>,
    /// When this file was last indexed (Unix timestamp seconds).
    pub indexed_at: i64,
}

/// A text chunk coordinate record stored for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkRecord {
    /// Zero-based index within the parent document.
    pub chunk_index: usize,
    /// Byte offset of chunk start in original content.
    pub start_byte: usize,
    /// Byte offset of chunk end in original content.
    pub end_byte: usize,
    /// 1-based line number of chunk start.
    pub start_line: usize,
    /// 1-based line number of chunk end.
    pub end_line: usize,
}

/// A registered edge type configuration persisted to the database.
#[derive(Debug, Clone)]
pub struct EdgeTypeRecord {
    /// Name of the edge type.
    pub name: String,
    /// Source kind (e.g. "wikilink", "tag", "frontmatter", "reference").
    pub source: String,
    /// Weight for scoring.
    pub weight: f32,
    /// Whether edges of this type are created in both directions.
    pub bidirectional: bool,
    /// Frontmatter field name (if source is frontmatter).
    pub field: Option<String>,
    /// Additional config serialized as JSON.
    pub config: Option<String>,
}

/// Policy for whether a chunk should receive a dense vector embedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkEmbedPolicy {
    /// Always embed: documentation, ADRs, exported interfaces, public API surfaces.
    Anchor,
    /// Never embed: internal helpers, private methods, leaf implementations.
    /// Searchable via BM25 lexical index and navigable via AST graph.
    GraphOnly,
}

impl Default for ChunkEmbedPolicy {
    fn default() -> Self {
        Self::Anchor
    }
}

fn default_line() -> usize {
    1
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
    /// 1-based line number of chunk start.
    #[serde(default = "default_line")]
    pub start_line: usize,
    /// 1-based line number of chunk end.
    #[serde(default = "default_line")]
    pub end_line: usize,
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
    /// Embedding policy for this chunk (anchor vs graph-only).
    #[serde(default)]
    pub embed_policy: ChunkEmbedPolicy,
    /// Compact skeleton text (e.g. signature + docstring + scope) for embedding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skeleton_text: Option<String>,
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
            start_line: 1,
            end_line: 1,
            heading_chain: None,
            language: None,
            scope_path: None,
            entity_kind: Some(EntityKind::Documentation),
            embed_policy: ChunkEmbedPolicy::Anchor,
            skeleton_text: None,
        }
    }

    /// Set line span.
    pub fn with_lines(mut self, start_line: usize, end_line: usize) -> Self {
        self.start_line = start_line;
        self.end_line = end_line;
        self
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
        self.start_line = start_line;
        self.end_line = end_line;
        self.entity_kind =
            Some(EntityKind::CodeChunk { language: lang, scope_path: scope, start_line, end_line });
        self
    }

    /// Set embedding policy.
    pub fn with_embed_policy(mut self, embed_policy: ChunkEmbedPolicy) -> Self {
        self.embed_policy = embed_policy;
        self
    }

    /// Set skeleton text for compact embedding.
    pub fn with_skeleton_text(mut self, skeleton_text: impl Into<String>) -> Self {
        self.skeleton_text = Some(skeleton_text.into());
        self
    }
}

/// Confidence band for a resolved cross-corpus symbol link.
///
/// Resolution across independent corpora is inherently ambiguous, so each
/// cross-corpus edge carries a provenance band describing how it was resolved:
///
/// - [`ResolutionConfidence::High`] — exactly one exact `scope_path` match across
///   all corpora (unambiguous).
/// - [`ResolutionConfidence::Medium`] — unique only after a tie-break heuristic
///   (e.g. matching language).
/// - [`ResolutionConfidence::Speculative`] — a weaker, best-effort match.
///
/// This phase only ever emits `High` edges; `Medium`/`Speculative` exist for the
/// confidence-band API and are populated by later resolution phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionConfidence {
    /// Exactly one exact `scope_path` match across all corpora.
    High,
    /// Unique only after a tie-break heuristic (e.g. same language).
    Medium,
    /// A weaker, best-effort match.
    Speculative,
}

/// The kind of unresolved reference captured as an [`ExternalRef`].
///
/// Both variants capture ordinary intra-language references that failed to
/// resolve locally and are handed to the cross-corpus reconciliation pass for
/// qualified-name symbol matching.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExternalRefKind {
    /// A call site whose callee did not resolve to any in-corpus symbol.
    Call,
    /// An import/use whose target did not resolve to any in-corpus symbol.
    Import,
}

/// A call or import target that failed to resolve against the full corpus
/// symbol index.
///
/// Single-repo indexing intentionally still emits a low-confidence intra-repo
/// edge for these (see the graph code extractor); an `ExternalRef` is captured
/// *in addition*, as a durable record of the unresolved target so a later
/// cross-corpus reconciliation pass can attempt to resolve it against other
/// corpora. Capturing these does not alter intra-repo edge output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalRef {
    /// Scope path of the caller/importer (the edge source) that referenced the
    /// unresolved target.
    pub caller_scope_path: String,
    /// The raw, unresolved target string as it appeared in the source (e.g. a
    /// bare callee name, `receiver.method`, or an import path).
    pub raw_target: String,
    /// Whether the unresolved reference is a call or an import.
    pub kind: ExternalRefKind,
    /// Resolution confidence band for the reference (always
    /// [`ResolutionConfidence::Speculative`] at capture time).
    pub confidence: ResolutionConfidence,
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
    /// Name of the corpus the target lives in, for cross-corpus links.
    ///
    /// `None` for intra-corpus edges; `Some(corpus)` when the target symbol was
    /// resolved in a different corpus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_corpus: Option<String>,
    /// Confidence band for a resolved cross-corpus link.
    ///
    /// `None` for intra-corpus edges; `Some(_)` for cross-corpus links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<ResolutionConfidence>,
    /// Path of the resolved target within its (possibly remote) corpus.
    ///
    /// `None` for intra-corpus edges. For cross-corpus edges this is the
    /// remote-endpoint payload that lets a federated query report a hop and
    /// continue traversal into the target corpus without re-resolving.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    /// Qualified symbol name of the resolved cross-corpus target (`None` for
    /// intra-corpus edges).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_symbol: Option<String>,
    /// Kind of the resolved cross-corpus target (e.g. code symbol type or
    /// document kind); `None` for intra-corpus edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
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
    /// Language decorator or annotation (e.g. @decorator, #[derive]).
    CodeDecorates,
    /// Class inheritance / extension (e.g. class A extends B).
    CodeExtends,
    /// Macro expansion invocation (e.g. println!, vec![]).
    CodeMacroExpands,
    /// Embedded struct field composition (e.g. Go anonymous struct fields).
    CodeStructEmbeds,
    /// Relational foreign key constraint (e.g. SQL REFERENCES).
    CodeForeignKey,
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
    /// Name of the corpus this result originated from.
    ///
    /// Populated by the multi-corpus routing layer; `None` for results that
    /// have not been tagged with a source corpus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus: Option<String>,
    /// Graph degree affordances for Turn 2 expansion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_affordances: Option<GraphAffordances>,
    /// Immediate 1-hop neighborhood in Cypher-Lite ASCII notation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<String>,
    /// Code symbol identifier (provided when snippet is omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
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
            corpus: None,
            graph_affordances: None,
            graph: None,
            symbol: None,
        }
    }

    /// Set Cypher-Lite graph representation.
    pub fn with_graph(mut self, graph: Option<String>) -> Self {
        self.graph = graph;
        self
    }

    /// Set symbol identifier.
    pub fn with_symbol(mut self, symbol: Option<String>) -> Self {
        self.symbol = symbol;
        self
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

    /// Set the source corpus tag.
    pub fn with_corpus(mut self, corpus: Option<String>) -> Self {
        self.corpus = corpus;
        self
    }

    /// Set graph affordances.
    pub fn with_graph_affordances(mut self, affordances: GraphAffordances) -> Self {
        self.graph_affordances = Some(affordances);
        self
    }
}

/// Metadata about a stored vector, mapping HNSW internal IDs to documents/chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMeta {
    /// Document path this vector belongs to.
    pub doc_path: String,
    /// Chunk index within the document (None for document-level embeddings).
    pub chunk_index: Option<usize>,
    /// Whether this is a document-level embedding (vs chunk-level).
    pub is_doc_level: bool,
    /// Coarse modality tag ("code" / "docs") for modality-filtered search.
    #[serde(default = "default_modality")]
    pub modality: String,
}

/// Default coarse modality tag ("docs") for round-tripping persisted vectors.
fn default_modality() -> String {
    "docs".to_string()
}

/// A single vector search result.
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    /// Document path.
    pub doc_path: String,
    /// Chunk index (None for document-level).
    pub chunk_index: Option<usize>,
    /// Cosine similarity score (0.0 to 1.0, higher = more similar).
    pub score: f64,
    /// Whether this came from a document-level embedding.
    pub is_doc_level: bool,
    /// Coarse modality tag ("code" / "docs") of the matched vector.
    pub modality: String,
}

fn is_zero_f64(v: &f64) -> bool {
    v.abs() < 1e-9
}

/// Breakdown of how a search score was computed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// BM25 component (0.0 if not applicable).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub bm25: f64,
    /// Vector cosine similarity component.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub vector: f64,
    /// Graph proximity boost.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub graph_boost: f64,
    /// Number of hops from seed in graph traversal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_hops: Option<usize>,
}

impl ScoreBreakdown {
    /// Returns true if all numerical components are zero and graph_hops is None.
    pub fn is_empty(&self) -> bool {
        is_zero_f64(&self.bm25)
            && is_zero_f64(&self.vector)
            && is_zero_f64(&self.graph_boost)
            && self.graph_hops.is_none()
    }
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

/// Graph statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total number of nodes.
    pub node_count: usize,
    /// Total number of edges.
    pub edge_count: usize,
    /// Nodes with zero edges (neither incoming nor outgoing).
    pub orphan_count: usize,
    /// Top 10 nodes by total degree (incoming + outgoing).
    pub most_connected: Vec<(String, usize)>,
    /// Count of edges per edge type.
    pub edge_type_distribution: HashMap<String, usize>,
}

/// A step or node in a lineage traversal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageNode {
    /// Document path.
    pub path: String,
    /// Document title if known.
    pub title: Option<String>,
    /// Hop distance from the start node (0 for start note).
    pub depth: usize,
    /// Edge type traversed to reach this note.
    pub edge_type: String,
    /// Direction traversed ("start", "outgoing", "incoming").
    pub direction: String,
}

/// Broken link detected in taxonomy validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrokenLink {
    /// Source document path containing the link.
    pub source: String,
    /// Target path or wikilink that could not be resolved.
    pub target: String,
    /// Edge type (e.g. "Wikilink", "supersedes", etc.).
    pub edge_type: String,
    /// Edge provenance.
    pub provenance: EdgeProvenance,
}

/// Circular dependency detected in a directed acyclic relation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CircularDependency {
    /// The edge type where the cycle exists (e.g. "supersedes").
    pub edge_type: String,
    /// The cycle path (e.g. ["A.md", "B.md", "A.md"]).
    pub cycle: Vec<String>,
}

/// Orphan ADR detected in taxonomy validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrphanAdr {
    /// Path to the orphan ADR note.
    pub path: String,
    /// Note title if known.
    pub title: Option<String>,
    /// Human-readable explanation.
    pub reason: String,
}

/// A detected community of nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    /// Community identifier.
    pub id: usize,
    /// Paths of nodes in this community.
    pub members: Vec<String>,
    /// Modularity contribution of this community to the overall partition.
    pub modularity_contribution: f64,
}

/// Result of community detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDetectionResult {
    /// Detected communities.
    pub communities: Vec<Community>,
    /// Overall modularity of the partition (Q ∈ [-0.5, 1.0]).
    pub modularity: f64,
    /// Number of iterations the algorithm ran.
    pub iterations: usize,
}

/// Per-community density statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDensity {
    /// Community id.
    pub community_id: usize,
    /// Number of nodes in the community.
    pub node_count: usize,
    /// Number of internal edges (edges within the community, treating as undirected).
    pub internal_edges: usize,
    /// Density: internal_edges / max_possible_internal_edges.
    pub density: f64,
}

// ---------------------------------------------------------------------------
// Adaptive Graph Expansion & Relational Graph Types (RFC)
// ---------------------------------------------------------------------------

/// Graph affordances for a search result node.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GraphAffordances {
    /// Inbound calls count (for code symbols).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calls_in: Option<usize>,
    /// Outbound calls count (for code symbols).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calls_out: Option<usize>,
    /// Implemented interfaces / traits count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implements: Option<usize>,
    /// Imported dependencies count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imports: Option<usize>,
    /// Inbound wikilinks count (for doc nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikilinks_in: Option<usize>,
    /// Outbound wikilinks count (for doc nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikilinks_out: Option<usize>,
    /// Connected code documentation links count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documents_code: Option<usize>,
    /// Dynamic counts for language-specific or extended edge types (e.g. decorates, extends, foreign_key).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub edge_counts: HashMap<String, usize>,
    /// Count of edges suppressed due to hub degree thresholds, keyed by edge type.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub suppressed_edges: HashMap<String, usize>,
}

impl GraphAffordances {
    /// Returns true if all affordance counts are None or zero, and edge_counts and suppressed_edges are empty.
    pub fn is_empty(&self) -> bool {
        self.calls_in.unwrap_or(0) == 0
            && self.calls_out.unwrap_or(0) == 0
            && self.implements.unwrap_or(0) == 0
            && self.imports.unwrap_or(0) == 0
            && self.wikilinks_in.unwrap_or(0) == 0
            && self.wikilinks_out.unwrap_or(0) == 0
            && self.documents_code.unwrap_or(0) == 0
            && self.edge_counts.is_empty()
            && self.suppressed_edges.is_empty()
    }
}

/// Contextual schema envelope returned with search partitions.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SchemaEnvelope {
    /// Distinct node labels/kinds relevant to this partition.
    pub node_labels: Vec<String>,
    /// Active edge types present in this partition.
    pub active_edges: Vec<String>,
}

/// A partitioned search result set (docs or code).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchPartition {
    /// Hits in this partition.
    pub results: Vec<SearchResult>,
    /// Total matches across the corpus in this modality.
    pub total_matches: usize,
    /// Number of hits returned in `results`.
    pub top_k_returned: usize,
    /// Schema envelope providing information scent for Turn 2 expansion.
    pub schema_envelope: SchemaEnvelope,
}

/// Partitioned bimodal search response.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchResponse {
    /// Documentation partition (if requested/available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<SearchPartition>,
    /// Source code partition (if requested/available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<SearchPartition>,
}

/// Durable relational edge record for SQLite persistence (`meta.db`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeRecord {
    /// Database row ID (None for unsaved records).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// Source document or code entity path.
    pub source: String,
    /// Target document or code entity path.
    pub target: String,
    /// Edge relationship type (e.g. "calls", "defines", "imports", "implements", "wikilink", "documents").
    pub edge_type: String,
    /// Edge classification class ("structural", "semantic", "hybrid").
    pub edge_class: String,
    /// Edge weight (default 1.0).
    pub weight: f32,
    /// Resolution confidence (0.0 - 1.0, default 1.0).
    pub confidence: f32,
    /// Optional metadata payload (e.g. line numbers, call AST context).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

/// Result of a `graph_match` path query.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphMatchResult {
    /// Matched paths from anchor to terminal node.
    pub matches: Vec<PathMatch>,
    /// Total matched path instances.
    pub total_matches: usize,
    /// Distinct nodes in the matched subgraph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<MatchedNode>,
    /// Distinct edges in the matched subgraph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<MatchedEdge>,
}

/// A single matched path traversal.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathMatch {
    /// Identifier or scope path of the terminal node.
    pub node: String,
    /// Traversal depth (hops) from the anchor.
    pub depth: usize,
    /// Linear path representation (e.g. "A -> B <- C").
    pub path: String,
    /// Symbol type or node label if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_type: Option<String>,
    /// File path where the terminal entity lives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Line number where the terminal entity starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

/// A node in the matched subgraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedNode {
    /// Node identifier (path or scope path).
    pub id: String,
    /// Node label (e.g. "CodeSymbol", "DocNode", "Interface").
    pub label: String,
    /// Node properties.
    pub properties: std::collections::HashMap<String, String>,
}

/// An edge in the matched subgraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedEdge {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Edge type (e.g. "calls", "implements").
    pub edge_type: String,
    /// Direction relative to traversal ("outgoing", "incoming").
    pub direction: String,
}
