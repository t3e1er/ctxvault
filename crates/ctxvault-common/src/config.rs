//! Configuration types for corpus, chunking, graph, and templates.
//!
//! These are deserialized from TOML files. Each corpus has its own config.

use serde::{Deserialize, Serialize};

/// Top-level corpus configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusConfig {
    /// Human-readable corpus name.
    pub name: String,
    /// Path to the markdown directory.
    pub path: String,
    /// Access mode for this corpus.
    #[serde(default)]
    pub mode: CorpusMode,
    /// Chunking strategy and parameters.
    #[serde(default)]
    pub chunking: ChunkingConfig,
    /// Embedding model configuration.
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    /// Graph edge type definitions.
    #[serde(default)]
    pub graph: GraphConfig,
    /// Path to templates directory (relative to corpus root).
    #[serde(default = "default_templates_dir")]
    pub templates_dir: String,
}

fn default_templates_dir() -> String {
    ".templates".to_string()
}

/// Corpus access mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CorpusMode {
    /// Full read and write access.
    #[default]
    ReadWrite,
    /// Search and read only — write tools are suppressed.
    ReadOnly,
}

/// Chunking strategy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    /// Chunking strategy.
    #[serde(default = "default_strategy")]
    pub strategy: ChunkingStrategy,
    /// Target chunk size in tokens.
    #[serde(default = "default_target_tokens")]
    pub target_tokens: usize,
    /// Maximum chunk size in tokens.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Overlap between chunks in tokens.
    #[serde(default = "default_overlap")]
    pub overlap_tokens: usize,
    /// Never split across heading boundaries.
    #[serde(default = "default_true")]
    pub respect_headings: bool,
    /// Discard chunks smaller than this.
    #[serde(default = "default_min_tokens")]
    pub min_chunk_tokens: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            strategy: ChunkingStrategy::Heading,
            target_tokens: 512,
            max_tokens: 1024,
            overlap_tokens: 64,
            respect_headings: true,
            min_chunk_tokens: 50,
        }
    }
}

fn default_strategy() -> ChunkingStrategy {
    ChunkingStrategy::Heading
}
fn default_target_tokens() -> usize {
    512
}
fn default_max_tokens() -> usize {
    1024
}
fn default_overlap() -> usize {
    64
}
fn default_min_tokens() -> usize {
    50
}
fn default_true() -> bool {
    true
}

/// Available chunking strategies.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ChunkingStrategy {
    /// Fixed character count split.
    Fixed,
    /// Split at paragraph boundaries (double newlines), merge to target size.
    Paragraph,
    /// Line-based accumulation to target size (legacy default).
    Semantic,
    /// Each heading section is one chunk (best for documentation).
    #[default]
    Heading,
    /// Tree-sitter AST-guided syntactic node chunking (for polyglot source code).
    CodeAst,
}

/// Embedding model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// fastembed model identifier (e.g., "BAAI/bge-small-en-v1.5").
    #[serde(default = "default_embedding_model")]
    pub model: String,
}

fn default_embedding_model() -> String {
    "BAAI/bge-small-en-v1.5".to_string()
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self { model: default_embedding_model() }
    }
}

/// Graph configuration: user-defined edge types.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphConfig {
    /// Registered edge types for this corpus.
    #[serde(default)]
    pub edge_types: Vec<EdgeTypeConfig>,
}

/// Configuration for a single edge type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeTypeConfig {
    /// Name of the edge type (e.g., "ParentChild", "Supersedes").
    pub name: String,
    /// Source of this edge.
    pub source: EdgeSource,
    /// Weight applied to edges of this type.
    #[serde(default = "default_weight")]
    pub weight: f32,
    /// Whether edges are created in both directions.
    #[serde(default)]
    pub bidirectional: bool,
    /// Frontmatter field name (required if source is "frontmatter").
    pub field: Option<String>,
    /// Direction for frontmatter-derived edges.
    pub direction: Option<EdgeDirection>,
    /// Maximum tag frequency (for tag-based edges).
    pub max_frequency: Option<usize>,
    /// Edge class: semantic (discovery), structural (navigation), or hybrid (both).
    /// If absent, inferred from source type.
    pub class: Option<EdgeClass>,
    /// Human-readable description of this edge relationship.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Allowed templates for source notes (optional constraint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_source_templates: Option<Vec<String>>,
    /// Allowed templates for target notes (optional constraint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_target_templates: Option<Vec<String>>,
}

fn default_weight() -> f32 {
    1.0
}

/// Where an edge comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeSource {
    /// Derived from `[[wikilinks]]` in content.
    Wikilink,
    /// Derived from shared `#tags`.
    Tag,
    /// Derived from a specific frontmatter field.
    Frontmatter,
    /// Derived from standard `[markdown](links)`.
    Reference,
    /// Derived from code AST analysis (e.g. calls, defines, imports, implements).
    Code,
}

/// Direction of a frontmatter-derived edge relative to the current note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeDirection {
    /// Edge points FROM this note TO the target.
    Outbound,
    /// Edge points FROM the target TO this note.
    Inbound,
}

/// Classification of an edge's purpose in the knowledge graph.
/// Semantic edges support discovery/boosting; structural edges support navigation/lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EdgeClass {
    /// Discovery-oriented: shared tags, vector similarity, co-occurrence.
    /// Used by hybrid search for graph boosting, search_related, graph_communities.
    Semantic,
    /// Navigation-oriented: wikilinks, frontmatter relationships, schema-declared links.
    /// Used by search_graph, graph_path, backlinks/forwardlinks.
    Structural,
    /// Both purposes: intentional link that also signals topical proximity.
    #[default]
    Hybrid,
}

impl EdgeClass {
    /// Infer class from edge source when not explicitly configured.
    pub fn infer_from_source(source: &EdgeSource) -> Self {
        match source {
            EdgeSource::Tag => EdgeClass::Semantic,
            EdgeSource::Wikilink => EdgeClass::Structural,
            EdgeSource::Frontmatter => EdgeClass::Structural,
            EdgeSource::Reference => EdgeClass::Structural,
            EdgeSource::Code => EdgeClass::Structural,
        }
    }

    /// Parse from a string (case-insensitive).
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "semantic" => Some(Self::Semantic),
            "structural" => Some(Self::Structural),
            "hybrid" => Some(Self::Hybrid),
            _ => None,
        }
    }

    /// Check if this class matches a filter. Hybrid matches both semantic and structural filters.
    pub fn matches(&self, filter: EdgeClass) -> bool {
        match filter {
            EdgeClass::Hybrid => true, // Hybrid filter matches everything
            EdgeClass::Semantic => *self == EdgeClass::Semantic || *self == EdgeClass::Hybrid,
            EdgeClass::Structural => *self == EdgeClass::Structural || *self == EdgeClass::Hybrid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_corpus_config() {
        let toml_str = r#"
            name = "test-wiki"
            path = "./wiki"
        "#;
        let config: CorpusConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.name, "test-wiki");
        assert_eq!(config.mode, CorpusMode::ReadWrite);
        assert_eq!(config.chunking.target_tokens, 512);
    }

    #[test]
    fn parse_full_corpus_config() {
        let toml_str = r#"
            name = "engineering"
            path = "./docs"
            mode = "read-only"
            templates_dir = ".schemas"

            [chunking]
            strategy = "heading"
            target_tokens = 1024
            max_tokens = 2048

            [embedding]
            model = "BAAI/bge-small-en-v1.5"

            [[graph.edge_types]]
            name = "Wikilink"
            source = "wikilink"
            weight = 1.0

            [[graph.edge_types]]
            name = "Implements"
            source = "frontmatter"
            field = "implements"
            weight = 0.8
            direction = "outbound"
        "#;
        let config: CorpusConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, CorpusMode::ReadOnly);
        assert_eq!(config.chunking.strategy, ChunkingStrategy::Heading);
        assert_eq!(config.embedding.model, "BAAI/bge-small-en-v1.5");
        assert_eq!(config.graph.edge_types.len(), 2);
        assert_eq!(config.graph.edge_types[1].name, "Implements");
    }
}
