//! Multi-corpus support: manages multiple independent Engine instances.
//!
//! Each corpus is an independent unit with its own BM25 index, vector index,
//! knowledge graph, and SQLite store. The [`CorpusManager`] provides a unified
//! interface for routing operations to the correct engine.

use std::collections::HashMap;
use std::path::PathBuf;

use ctxvault_common::config::{CorpusConfig, EdgeClass};
use ctxvault_common::ports::{GraphStore, MetadataCatalog};
use ctxvault_common::types::{CodeSymbol, EdgeProvenance, ResolutionConfidence};
use ctxvault_common::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::engine::Engine;

/// Status information for a single corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusInfo {
    /// Corpus name.
    pub name: String,
    /// Path to the corpus directory.
    pub path: String,
    /// Access mode (read-write or read-only).
    pub mode: String,
    /// Number of indexed files.
    pub file_count: usize,
    /// Whether the embedder is active for this corpus.
    pub embedder_active: bool,
    /// Number of vectors in the index.
    pub vector_count: usize,
    /// Number of nodes in the knowledge graph.
    pub graph_node_count: usize,
}

/// Manages multiple independent Engine instances, one per corpus.
///
/// Provides routing by corpus name and a default corpus for backwards compatibility.
pub struct CorpusManager {
    /// Engines keyed by corpus name.
    engines: HashMap<String, Engine>,
    /// Name of the default corpus (first one registered, or explicitly set).
    default_corpus: Option<String>,
}

impl CorpusManager {
    /// Create an empty corpus manager.
    pub fn new() -> Self {
        Self { engines: HashMap::new(), default_corpus: None }
    }

    /// Add a corpus to the manager.
    ///
    /// Opens or creates the engine for the given corpus config.
    /// If this is the first corpus added, it becomes the default.
    pub fn add_corpus(&mut self, config: CorpusConfig) -> Result<()> {
        let name = config.name.clone();

        // Each corpus stores its index at `<corpus_path>/.index`.
        let index_dir = PathBuf::from(&config.path).join(".index");

        let engine = crate::engine_builder::EngineBuilder::open(config, &index_dir)?;

        if self.default_corpus.is_none() {
            self.default_corpus = Some(name.clone());
        }

        let _ = self.engines.insert(name, engine);
        Ok(())
    }

    /// Set the default corpus by name.
    pub fn set_default(&mut self, name: &str) -> Result<()> {
        if !self.engines.contains_key(name) {
            return Err(Error::NotFound(format!("corpus not found: {}", name)));
        }
        self.default_corpus = Some(name.to_string());
        Ok(())
    }

    /// Get the default corpus name.
    pub fn default_corpus_name(&self) -> Option<&str> {
        self.default_corpus.as_deref()
    }

    /// Get a mutable reference to an engine by corpus name.
    pub fn get_engine_mut(&mut self, name: &str) -> Result<&mut Engine> {
        self.engines
            .get_mut(name)
            .ok_or_else(|| Error::NotFound(format!("corpus not found: {}", name)))
    }

    /// Get an immutable reference to an engine by corpus name.
    pub fn get_engine(&self, name: &str) -> Result<&Engine> {
        self.engines.get(name).ok_or_else(|| Error::NotFound(format!("corpus not found: {}", name)))
    }

    /// Get a mutable reference to the default engine.
    pub fn default_engine_mut(&mut self) -> Result<&mut Engine> {
        let name = self
            .default_corpus
            .as_ref()
            .ok_or_else(|| Error::NotFound("no default corpus configured".to_string()))?
            .clone();
        self.get_engine_mut(&name)
    }

    /// Get an immutable reference to the default engine.
    pub fn default_engine(&self) -> Result<&Engine> {
        let name = self
            .default_corpus
            .as_ref()
            .ok_or_else(|| Error::NotFound("no default corpus configured".to_string()))?;
        self.get_engine(name)
    }

    /// Resolve a corpus name: if provided, use it; otherwise use default (immutable).
    pub fn resolve_engine(&self, corpus: Option<&str>) -> Result<&Engine> {
        match corpus {
            Some(name) => self.get_engine(name),
            None => self.default_engine(),
        }
    }

    /// Resolve a corpus name: if provided, use it; otherwise use default (mutable).
    pub fn resolve_engine_mut(&mut self, corpus: Option<&str>) -> Result<&mut Engine> {
        match corpus {
            Some(name) => self.get_engine_mut(name),
            None => self.default_engine_mut(),
        }
    }

    /// List all configured corpora with their status.
    pub fn list_corpora(&self) -> Vec<CorpusInfo> {
        self.engines
            .iter()
            .map(|(name, engine)| {
                let file_count = engine.store().list_files().map(|f| f.len()).unwrap_or(0);
                let mode = format!("{:?}", engine.config().mode);

                CorpusInfo {
                    name: name.clone(),
                    path: engine.config().path.clone(),
                    mode,
                    file_count,
                    embedder_active: engine.embedder_active(),
                    vector_count: engine.vector_count(),
                    graph_node_count: engine.graph().node_count(),
                }
            })
            .collect()
    }

    /// Number of corpora managed.
    pub fn corpus_count(&self) -> usize {
        self.engines.len()
    }

    /// Check if a corpus exists by name.
    pub fn has_corpus(&self, name: &str) -> bool {
        self.engines.contains_key(name)
    }

    /// Get all corpus names.
    pub fn corpus_names(&self) -> Vec<&str> {
        self.engines.keys().map(|s| s.as_str()).collect()
    }

    // ─── Cross-corpus symbol linking ─────────────────────────────────────────

    /// Resolve a fully qualified symbol name across every managed corpus.
    ///
    /// Queries each engine's store for an exact `scope_path` match and returns
    /// `(corpus_name, symbol)` for every match found across all corpora. An empty
    /// result means the name is unknown; more than one result means the name is
    /// ambiguous and must NOT be linked.
    pub fn resolve_symbol_across_corpora(&self, qualified_name: &str) -> Vec<(String, CodeSymbol)> {
        let mut matches = Vec::new();
        for (corpus_name, engine) in &self.engines {
            match engine.store().find_symbols_by_qualified_name(qualified_name) {
                Ok(symbols) => {
                    for sym in symbols {
                        matches.push((corpus_name.clone(), sym));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        corpus = %corpus_name,
                        qualified_name,
                        error = %e,
                        "cross-corpus symbol lookup failed"
                    );
                }
            }
        }
        matches
    }

    /// Post-index linking pass that injects cross-corpus doc→code edges.
    ///
    /// For each corpus (the "doc side"), every document's outgoing
    /// frontmatter-provenance edge targets are treated as candidate doc→code
    /// links. A candidate is linked only when it:
    ///
    /// 1. does NOT already resolve to a node within the same corpus's graph, and
    /// 2. resolves to EXACTLY ONE `(corpus, symbol)` across all corpora.
    ///
    /// When both hold, an edge is injected into the doc's corpus graph pointing at
    /// a distinct cross-corpus node keyed `"<corpus>::<scope_path>"`, tagged with
    /// [`EdgeProvenance::DocumentsCode`], `target_corpus`, and
    /// [`ResolutionConfidence::High`]. Ambiguous (>1) or unresolved (0) candidates
    /// produce no edge, so no false or dangling edges are ever created.
    ///
    /// The pass is idempotent: re-running relies on the graph's same-type edge
    /// de-duplication, so repeated invocations neither duplicate edges nor grow
    /// the graph unbounded. Returns the number of cross-corpus edges created.
    pub fn link_cross_corpus_symbols(&mut self) -> Result<usize> {
        // Phase 1: gather link decisions using immutable access (no borrow conflict).
        // Each decision: (doc_corpus, doc_path, edge_type, target_corpus, node_key, title).
        struct CrossLink {
            doc_corpus: String,
            doc_path: String,
            edge_type: String,
            target_corpus: String,
            node_key: String,
            title: Option<String>,
        }

        let mut decisions: Vec<CrossLink> = Vec::new();

        for (doc_corpus, engine) in &self.engines {
            let graph = engine.graph();
            for doc_path in graph.node_paths() {
                for (edge_type, raw_target) in graph.outgoing_frontmatter_targets(&doc_path) {
                    // Skip candidates that already resolve within the same corpus.
                    if graph.contains_node(&raw_target)
                        && raw_target != doc_path
                        && Self::is_intra_corpus_symbol(engine, &raw_target)
                    {
                        continue;
                    }

                    let resolved = self.resolve_symbol_across_corpora(&raw_target);
                    // Only unambiguous, single, cross-corpus matches are linked.
                    if resolved.len() != 1 {
                        continue;
                    }
                    let (target_corpus, symbol) = &resolved[0];
                    // Must be a DIFFERENT corpus (intra-corpus already handled by key match).
                    if target_corpus == doc_corpus {
                        continue;
                    }

                    let node_key = format!("{}::{}", target_corpus, symbol.scope_path);
                    decisions.push(CrossLink {
                        doc_corpus: doc_corpus.clone(),
                        doc_path: doc_path.clone(),
                        edge_type,
                        target_corpus: target_corpus.clone(),
                        node_key,
                        title: Some(symbol.name.clone()),
                    });
                }
            }
        }

        // Phase 2: apply decisions with mutable access to each doc corpus graph.
        let mut created = 0usize;
        for link in decisions {
            let engine = self.get_engine_mut(&link.doc_corpus)?;
            let graph = engine.graph_mut();
            graph.add_node(&link.node_key, link.title.as_deref());
            graph.add_edge_full(
                &link.doc_path,
                &link.node_key,
                &link.edge_type,
                1.0,
                EdgeProvenance::DocumentsCode,
                EdgeClass::Structural,
                Some(link.target_corpus),
                Some(ResolutionConfidence::High),
            );
            created += 1;
        }

        Ok(created)
    }

    /// Whether a graph node keyed by `scope_path` corresponds to a code symbol
    /// defined in this engine's own corpus (as opposed to a bare doc target that
    /// merely happens to share the string).
    fn is_intra_corpus_symbol(engine: &Engine, scope_path: &str) -> bool {
        engine
            .store()
            .find_symbols_by_qualified_name(scope_path)
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }
}

impl Default for CorpusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctxvault_common::config::{
        ChunkingConfig, CorpusMode, EmbeddingConfig, GraphConfig, IndexMode,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn test_config(name: &str, corpus_path: &Path) -> CorpusConfig {
        CorpusConfig {
            name: name.to_string(),
            path: corpus_path.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Full,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: Vec::new() },
            templates_dir: ".templates".to_string(),
        }
    }

    #[test]
    fn test_create_empty_manager() {
        let manager = CorpusManager::new();
        assert_eq!(manager.corpus_count(), 0);
        assert!(manager.default_corpus_name().is_none());
    }

    #[test]
    fn test_add_corpus_sets_default() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("wiki");
        fs::create_dir_all(&corpus_dir).unwrap();

        let mut manager = CorpusManager::new();
        let config = test_config("wiki", &corpus_dir);
        manager.add_corpus(config).unwrap();

        assert_eq!(manager.corpus_count(), 1);
        assert_eq!(manager.default_corpus_name(), Some("wiki"));
        assert!(manager.has_corpus("wiki"));
    }

    #[test]
    fn test_multiple_corpora_isolation() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        let docs_dir = tmp.path().join("docs");
        fs::create_dir_all(&wiki_dir).unwrap();
        fs::create_dir_all(&docs_dir).unwrap();

        let mut manager = CorpusManager::new();
        manager.add_corpus(test_config("wiki", &wiki_dir)).unwrap();
        manager.add_corpus(test_config("docs", &docs_dir)).unwrap();

        assert_eq!(manager.corpus_count(), 2);

        // Index a file in "wiki".
        {
            let wiki_engine = manager.get_engine_mut("wiki").unwrap();
            wiki_engine.index_file("test.md", "# Wiki Note\n\nContent for wiki.\n").unwrap();
            wiki_engine.commit().unwrap();
        }

        // Index a different file in "docs".
        {
            let docs_engine = manager.get_engine_mut("docs").unwrap();
            docs_engine.index_file("guide.md", "# Guide\n\nDocumentation guide.\n").unwrap();
            docs_engine.commit().unwrap();
        }

        // Wiki should have test.md but not guide.md.
        let wiki_engine = manager.get_engine("wiki").unwrap();
        assert!(wiki_engine.store().get_file("test.md").unwrap().is_some());
        assert!(wiki_engine.store().get_file("guide.md").unwrap().is_none());

        // Docs should have guide.md but not test.md.
        let docs_engine = manager.get_engine("docs").unwrap();
        assert!(docs_engine.store().get_file("guide.md").unwrap().is_some());
        assert!(docs_engine.store().get_file("test.md").unwrap().is_none());
    }

    #[test]
    fn test_resolve_engine_with_corpus_param() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        let docs_dir = tmp.path().join("docs");
        fs::create_dir_all(&wiki_dir).unwrap();
        fs::create_dir_all(&docs_dir).unwrap();

        let mut manager = CorpusManager::new();
        manager.add_corpus(test_config("wiki", &wiki_dir)).unwrap();
        manager.add_corpus(test_config("docs", &docs_dir)).unwrap();

        // None resolves to default (wiki, since it was added first).
        {
            let engine = manager.resolve_engine_mut(None).unwrap();
            assert_eq!(engine.config().name, "wiki");
        }

        // Explicit name resolves correctly.
        {
            let engine = manager.resolve_engine_mut(Some("docs")).unwrap();
            assert_eq!(engine.config().name, "docs");
        }

        // Non-existent corpus returns error.
        assert!(manager.resolve_engine_mut(Some("nope")).is_err());
    }

    #[test]
    fn test_list_corpora() {
        let tmp = TempDir::new().unwrap();
        let wiki_dir = tmp.path().join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let mut manager = CorpusManager::new();
        manager.add_corpus(test_config("wiki", &wiki_dir)).unwrap();

        let list = manager.list_corpora();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "wiki");
        assert_eq!(list[0].file_count, 0);
    }

    // ─── Cross-corpus symbol linking ─────────────────────────────────────────

    use ctxvault_common::config::{EdgeSource, EdgeTypeConfig};

    /// Fast-mode corpus config with a single frontmatter `implements` edge type.
    /// Fast mode skips embeddings, so no ONNX model is required.
    fn linking_config(name: &str, corpus_path: &Path) -> CorpusConfig {
        let implements = EdgeTypeConfig {
            name: "implements".to_string(),
            source: EdgeSource::Frontmatter,
            weight: 1.0,
            bidirectional: false,
            field: Some("implements".to_string()),
            direction: None,
            max_frequency: None,
            class: None,
            description: None,
            allowed_source_templates: None,
            allowed_target_templates: None,
        };
        CorpusConfig {
            name: name.to_string(),
            path: corpus_path.to_string_lossy().to_string(),
            mode: CorpusMode::ReadWrite,
            index_mode: IndexMode::Fast,
            chunking: ChunkingConfig { min_chunk_tokens: 1, ..Default::default() },
            embedding: EmbeddingConfig::default(),
            graph: GraphConfig { edge_types: vec![implements] },
            templates_dir: ".templates".to_string(),
        }
    }

    /// Rust source defining exactly one top-level symbol with the given name.
    /// The extracted `scope_path` for a top-level function equals its bare name.
    fn rust_symbol_source(name: &str) -> String {
        format!("pub fn {name}() -> u32 {{\n    42\n}}\n")
    }

    /// Markdown doc whose frontmatter `implements` a code symbol scope_path.
    fn doc_implementing(target_scope: &str) -> String {
        format!(
            "---\nimplements: \"{target_scope}\"\n---\n\n# Design Note\n\nDescribes the impl.\n"
        )
    }

    fn add_fast_corpus(manager: &mut CorpusManager, name: &str, root: &Path) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        manager.add_corpus(linking_config(name, &dir)).unwrap();
    }

    #[test]
    fn test_cross_corpus_unique_match_links() {
        let tmp = TempDir::new().unwrap();
        let mut manager = CorpusManager::new();
        add_fast_corpus(&mut manager, "A", tmp.path());
        add_fast_corpus(&mut manager, "B", tmp.path());

        // Corpus B uniquely defines a symbol `WidgetEngine`.
        {
            let b = manager.get_engine_mut("B").unwrap();
            b.index_file("src/widget.rs", &rust_symbol_source("WidgetEngine")).unwrap();
            b.commit().unwrap();
        }
        // Corpus A has a doc whose frontmatter implements that scope_path.
        {
            let a = manager.get_engine_mut("A").unwrap();
            a.index_file("design.md", &doc_implementing("WidgetEngine")).unwrap();
            a.commit().unwrap();
        }

        let created = manager.link_cross_corpus_symbols().unwrap();
        assert_eq!(created, 1, "exactly one cross-corpus edge should be created");

        // The doc's forward links must include the cross-corpus node.
        let a = manager.get_engine("A").unwrap();
        let node_key = "B::WidgetEngine";
        assert!(a.graph().contains_node(node_key), "cross-corpus node must exist");

        let edge = a
            .graph()
            .get_all_edges()
            .into_iter()
            .find(|e| e.source == "design.md" && e.target == node_key)
            .expect("cross-corpus edge must exist");
        assert_eq!(edge.target_corpus.as_deref(), Some("B"));
        assert_eq!(edge.confidence, Some(ResolutionConfidence::High));
        assert_eq!(edge.provenance, EdgeProvenance::DocumentsCode);
        assert_eq!(edge.edge_type, "implements");

        // Idempotent: re-running creates no additional edges.
        let created_again = manager.link_cross_corpus_symbols().unwrap();
        assert_eq!(
            created_again, 1,
            "re-run resolves the same single candidate (deduped in graph)"
        );
        let a = manager.get_engine("A").unwrap();
        let dup_count = a
            .graph()
            .get_all_edges()
            .into_iter()
            .filter(|e| e.source == "design.md" && e.target == node_key)
            .count();
        assert_eq!(dup_count, 1, "no duplicate cross-corpus edge after re-run");
    }

    #[test]
    fn test_cross_corpus_ambiguous_match_no_edge() {
        let tmp = TempDir::new().unwrap();
        let mut manager = CorpusManager::new();
        add_fast_corpus(&mut manager, "A", tmp.path());
        add_fast_corpus(&mut manager, "B", tmp.path());
        add_fast_corpus(&mut manager, "C", tmp.path());

        // The SAME scope_path is defined in BOTH B and C => ambiguous.
        {
            let b = manager.get_engine_mut("B").unwrap();
            b.index_file("src/dup.rs", &rust_symbol_source("Dup")).unwrap();
            b.commit().unwrap();
        }
        {
            let c = manager.get_engine_mut("C").unwrap();
            c.index_file("src/dup.rs", &rust_symbol_source("Dup")).unwrap();
            c.commit().unwrap();
        }
        {
            let a = manager.get_engine_mut("A").unwrap();
            a.index_file("design.md", &doc_implementing("Dup")).unwrap();
            a.commit().unwrap();
        }

        // Resolution should find two matches across corpora.
        assert_eq!(manager.resolve_symbol_across_corpora("Dup").len(), 2);

        let created = manager.link_cross_corpus_symbols().unwrap();
        assert_eq!(created, 0, "ambiguous target must not create an edge");

        let a = manager.get_engine("A").unwrap();
        assert!(!a.graph().contains_node("B::Dup"));
        assert!(!a.graph().contains_node("C::Dup"));
    }

    #[test]
    fn test_cross_corpus_unresolved_no_edge() {
        let tmp = TempDir::new().unwrap();
        let mut manager = CorpusManager::new();
        add_fast_corpus(&mut manager, "A", tmp.path());
        add_fast_corpus(&mut manager, "B", tmp.path());

        // B defines something, but the doc implements a symbol nobody defines.
        {
            let b = manager.get_engine_mut("B").unwrap();
            b.index_file("src/other.rs", &rust_symbol_source("SomethingElse")).unwrap();
            b.commit().unwrap();
        }
        {
            let a = manager.get_engine_mut("A").unwrap();
            a.index_file("design.md", &doc_implementing("NoSuchSymbol")).unwrap();
            a.commit().unwrap();
        }

        assert!(manager.resolve_symbol_across_corpora("NoSuchSymbol").is_empty());

        let created = manager.link_cross_corpus_symbols().unwrap();
        assert_eq!(created, 0, "unresolved target must not create an edge");
    }

    #[test]
    fn test_intra_corpus_doc_to_code_edge_aligns_on_scope_path() {
        // A single corpus containing both the doc and the code symbol it implements.
        // The frontmatter edge target string equals the code symbol scope_path, so
        // build_frontmatter_edges lands the edge directly on the symbol node.
        let tmp = TempDir::new().unwrap();
        let mut manager = CorpusManager::new();
        add_fast_corpus(&mut manager, "mono", tmp.path());

        {
            let m = manager.get_engine_mut("mono").unwrap();
            m.index_file("src/thing.rs", &rust_symbol_source("Thing")).unwrap();
            m.index_file("design.md", &doc_implementing("Thing")).unwrap();
            m.commit().unwrap();
        }

        let m = manager.get_engine("mono").unwrap();
        // The symbol node exists (from the code `defines` pass) and the doc's
        // frontmatter `implements` edge points straight at it.
        assert!(m.graph().contains_node("Thing"));
        let fwd = m.graph().forwardlinks("design.md", None);
        let implements_targets = fwd.get("implements").expect("implements edge must exist");
        assert!(
            implements_targets.iter().any(|t| t == "Thing"),
            "intra-corpus doc->code edge must land on the symbol node"
        );

        // Single corpus => cross-corpus linking is a no-op.
        let created = manager.link_cross_corpus_symbols().unwrap();
        assert_eq!(created, 0);
    }
}
