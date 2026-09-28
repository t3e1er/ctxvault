//! Edge domain types.

use serde::{Deserialize, Serialize};

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

use std::sync::Arc;

/// Canonical universal semantic edge types across code and documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UniversalEdge {
    /// Source defines target symbol.
    Defines,
    /// Source calls target function/method.
    Calls,
    /// Source imports target module/package.
    Imports,
    /// Source implements target interface/trait.
    Implements,
    /// Source inherits target class/type.
    Inherits,
    /// Source extends target class/type (e.g. TypeScript/Java class extension).
    Extends,
    /// Source test function tests target symbol.
    Tests,
    /// Source documentation documents target symbol.
    Documents,
    /// Wikilink reference between documents/code.
    Wikilink,
    /// Shared tag link.
    SharedTag,
    /// Frontmatter field link.
    Frontmatter,
    /// Route endpoint handles target function/method.
    Handles,
}

impl UniversalEdge {
    /// Return the canonical string identifier for this universal edge.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Defines => "defines",
            Self::Calls => "calls",
            Self::Imports => "imports",
            Self::Implements => "implements",
            Self::Inherits => "inherits",
            Self::Extends => "extends",
            Self::Tests => "tests",
            Self::Documents => "documents",
            Self::Wikilink => "wikilink",
            Self::SharedTag => "tag",
            Self::Frontmatter => "frontmatter",
            Self::Handles => "handles",
        }
    }
}

/// Generalized relational edge kind: universal semantic edge or dynamic grammar-extracted relation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// Universal standard semantic edge.
    Universal(UniversalEdge),
    /// Dynamic grammar-extracted edge (e.g. "embeds_struct", "jsx_embeds", "type_alias", "macro_expands").
    Grammar(Arc<str>),
}

impl EdgeKind {
    /// Create a new dynamic grammar edge kind.
    pub fn grammar(s: impl AsRef<str>) -> Self {
        Self::Grammar(Arc::from(s.as_ref()))
    }

    /// Return the string representation of this edge kind.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Universal(u) => u.as_str(),
            Self::Grammar(s) => s.as_ref(),
        }
    }

    /// Parse an edge kind from a string.
    pub fn from_str(s: &str) -> Self {
        match s {
            "defines" => Self::Universal(UniversalEdge::Defines),
            "calls" => Self::Universal(UniversalEdge::Calls),
            "imports" => Self::Universal(UniversalEdge::Imports),
            "implements" | "implements_trait" => Self::Universal(UniversalEdge::Implements),
            "inherits" => Self::Universal(UniversalEdge::Inherits),
            "extends" => Self::Universal(UniversalEdge::Extends),
            "tests" => Self::Universal(UniversalEdge::Tests),
            "documents" | "documents_code" => Self::Universal(UniversalEdge::Documents),
            "wikilink" => Self::Universal(UniversalEdge::Wikilink),
            "tag" | "shared_tag" => Self::Universal(UniversalEdge::SharedTag),
            "frontmatter" => Self::Universal(UniversalEdge::Frontmatter),
            "handles" => Self::Universal(UniversalEdge::Handles),
            other => Self::Grammar(Arc::from(other)),
        }
    }
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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

impl Edge {
    /// Create a standard intra-corpus edge.
    pub fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        edge_type: impl Into<String>,
        weight: f32,
        provenance: EdgeProvenance,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            edge_type: edge_type.into(),
            weight,
            provenance,
            target_corpus: None,
            confidence: None,
            target_path: None,
            target_symbol: None,
            target_kind: None,
        }
    }

    /// Retrieve the typed [`EdgeKind`] of this edge.
    pub fn kind(&self) -> EdgeKind {
        EdgeKind::from_str(&self.edge_type)
    }

    /// Create an edge with a typed [`EdgeKind`].
    pub fn with_kind(
        source: impl Into<String>,
        target: impl Into<String>,
        kind: EdgeKind,
        weight: f32,
        provenance: EdgeProvenance,
    ) -> Self {
        Self::new(source, target, kind.as_str(), weight, provenance)
    }
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
    /// Test function verifies target symbol.
    CodeTests,
    /// Route endpoint handles backend function or handler method.
    CodeHandlesRoute,
    /// Markdown documentation specifies or documents code symbol.
    DocumentsCode,
    /// Code entity implements an architecture decision record (ADR).
    ImplementsAdr,
    /// Inferred by LLM extraction (future: tiered ontological model).
    Inferred,
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
