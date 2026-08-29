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

/// A text chunk ready for embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// The document this chunk belongs to.
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
}

/// A typed, weighted, directed edge in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source document path.
    pub source: String,
    /// Target document path.
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
    /// Target notes this ADR is for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adr_for: Vec<String>,
    /// Notes that have ADRs for this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub has_adr: Vec<String>,
    /// Parent notes of this note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_of: Vec<String>,
    /// Child notes under this note.
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
    /// Document path.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchDepth {
    /// Chunk-level only — best for specific factual queries.
    Precise,
    /// Document-level only — best for "what do we know about X?" sensemaking.
    Broad,
    /// Both chunk and doc-level, merged with RRF — default.
    Adaptive,
}

impl Default for SearchDepth {
    fn default() -> Self {
        Self::Precise
    }
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
