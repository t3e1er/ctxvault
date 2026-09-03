//! Multi-corpus support: manages multiple independent Engine instances.
//!
//! Each corpus is an independent unit with its own BM25 index, vector index,
//! knowledge graph, and SQLite store. The [`CorpusManager`] provides a unified
//! interface for routing operations to the correct engine.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ctxvault_common::config::CorpusConfig;
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
    /// Base directory for index storage.
    index_base_dir: PathBuf,
}

impl CorpusManager {
    /// Create an empty corpus manager.
    ///
    /// - `index_base_dir`: Base directory where per-corpus index directories are created.
    pub fn new(index_base_dir: &Path) -> Self {
        Self {
            engines: HashMap::new(),
            default_corpus: None,
            index_base_dir: index_base_dir.to_path_buf(),
        }
    }

    /// Add a corpus to the manager.
    ///
    /// Opens or creates the engine for the given corpus config.
    /// If this is the first corpus added, it becomes the default.
    pub fn add_corpus(&mut self, config: CorpusConfig) -> Result<()> {
        let name = config.name.clone();

        // Create per-corpus index directory.
        let index_dir = self.index_base_dir.join(&name);

        let engine = Engine::open(config, &index_dir)?;

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
                    embedder_active: engine.embedder_ref().is_some(),
                    vector_count: engine.vector_index().map(|vi| vi.len()).unwrap_or(0),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctxvault_common::config::{ChunkingConfig, CorpusMode, EmbeddingConfig, GraphConfig, IndexMode};
    use std::fs;
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
        let tmp = TempDir::new().unwrap();
        let manager = CorpusManager::new(tmp.path());
        assert_eq!(manager.corpus_count(), 0);
        assert!(manager.default_corpus_name().is_none());
    }

    #[test]
    fn test_add_corpus_sets_default() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("wiki");
        fs::create_dir_all(&corpus_dir).unwrap();

        let mut manager = CorpusManager::new(&tmp.path().join("indices"));
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

        let mut manager = CorpusManager::new(&tmp.path().join("indices"));
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

        let mut manager = CorpusManager::new(&tmp.path().join("indices"));
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

        let mut manager = CorpusManager::new(&tmp.path().join("indices"));
        manager.add_corpus(test_config("wiki", &wiki_dir)).unwrap();

        let list = manager.list_corpora();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "wiki");
        assert_eq!(list[0].file_count, 0);
    }
}
