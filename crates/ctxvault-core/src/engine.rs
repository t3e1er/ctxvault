//! Engine: coordinates persistence (SQLite), BM25 index (Tantivy), knowledge graph (petgraph),
//! and vector index (HNSW) with optional embedding support.
//!
//! The [`Engine`] is the top-level orchestrator for a single corpus. It manages
//! indexing, delta scanning, and provides unified access to all subsystems.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{debug, info, warn};

use ctxvault_common::config::CorpusConfig;
use ctxvault_common::types::Document;
use ctxvault_common::{Error, Result};

use crate::embedding::Embedder;
use crate::graph::KnowledgeGraph;
use crate::index::BM25Index;
use crate::parser;
use crate::parser::chunker;
use crate::persistence::{ChunkRecord, EdgeTypeRecord, Store};
use crate::vector_index::VectorIndex;

/// Coordinates persistence, full-text index, knowledge graph, and vector index for a corpus.
pub struct Engine {
    config: CorpusConfig,
    store: Store,
    bm25: BM25Index,
    graph: KnowledgeGraph,
    vector_index: VectorIndex,
    embedder: Option<Embedder>,
    index_dir: PathBuf,
}

/// Result of a delta scan comparing filesystem state against the index.
#[derive(Debug, Clone)]
pub struct DeltaScanResult {
    /// Files that exist on disk but were not previously indexed.
    pub new_files: Vec<String>,
    /// Files whose content hash changed since last indexing.
    pub modified_files: Vec<String>,
    /// Files that were indexed but no longer exist on disk.
    pub deleted_files: Vec<String>,
}

impl Engine {
    /// Create or open an engine for a corpus.
    ///
    /// - `config`: Corpus configuration (includes path, chunking settings, graph edge types).
    /// - `index_dir`: Path to the `.index/` directory. Will be created if it doesn't exist.
    ///
    /// Initializes SQLite store, Tantivy BM25 index, knowledge graph, and vector index.
    /// The embedder is initialized lazily on first use via `ensure_embedder()`.
    pub fn open(config: CorpusConfig, index_dir: &Path) -> Result<Self> {
        // 1. Create index directory if needed.
        fs::create_dir_all(index_dir)?;

        // 2. Open SQLite store.
        let db_path = index_dir.join("meta.db");
        let store = Store::open(&db_path)?;

        // 3. Open BM25 index.
        let tantivy_path = index_dir.join("tantivy");
        let bm25 = BM25Index::open(&tantivy_path)?;

        // 4. Load or create knowledge graph.
        let graph_path = index_dir.join("graph.bin");
        let graph = if graph_path.exists() {
            KnowledgeGraph::load(&graph_path).unwrap_or_else(|e| {
                warn!("Failed to load graph from disk, starting fresh: {}", e);
                KnowledgeGraph::new()
            })
        } else {
            KnowledgeGraph::new()
        };

        // 5. Load or create vector index.
        let vector_path = index_dir.join("vectors.json");
        let mut vector_index = if vector_path.exists() {
            VectorIndex::load(&vector_path).unwrap_or_else(|e| {
                warn!("Failed to load vector index from disk, starting fresh: {}", e);
                VectorIndex::new_default(crate::vector_index::DEFAULT_DIMENSIONS)
            })
        } else {
            VectorIndex::new_default(crate::vector_index::DEFAULT_DIMENSIONS)
        };

        // 5b. Check model version staleness.
        // Use the configured model from corpus config.
        let configured_model_name =
            crate::embedding::ModelName::from_str_name(&config.embedding.model).unwrap_or_default();
        let configured_model_version = configured_model_name.version_string();
        if let Some(stored_version) = vector_index.model_version() {
            if stored_version != configured_model_version {
                warn!(
                    "Embedding model version mismatch: stored='{}', configured='{}'. Vectors marked as stale.",
                    stored_version, configured_model_version
                );
                vector_index.mark_stale();
            }
        } else if !vector_index.is_empty() {
            // Vectors exist but no model version stored — legacy data, mark stale for safety.
            warn!("Vector index has no model_version metadata. Marking as stale for safety.");
            vector_index.mark_stale();
        }

        // 6. Register edge type configs in the persistence store.
        let edge_type_records: Vec<EdgeTypeRecord> = config
            .graph
            .edge_types
            .iter()
            .map(|et| EdgeTypeRecord {
                name: et.name.clone(),
                source: format!("{:?}", et.source).to_lowercase(),
                weight: et.weight,
                bidirectional: et.bidirectional,
                field: et.field.clone(),
                config: None,
            })
            .collect();
        if !edge_type_records.is_empty() {
            store.insert_edge_types(&edge_type_records)?;
        }

        Ok(Self {
            config,
            store,
            bm25,
            graph,
            vector_index,
            embedder: None, // Lazily initialized
            index_dir: index_dir.to_path_buf(),
        })
    }

    /// Ensure the embedder is initialized. Returns Ok(true) if available, Ok(false) if skipped.
    ///
    /// The embedder is lazily created to avoid model download during tests or when
    /// vector indexing is not needed. Uses the model specified in corpus config.
    pub fn ensure_embedder(&mut self) -> Result<bool> {
        if self.embedder.is_some() {
            return Ok(true);
        }

        let model_str = &self.config.embedding.model;
        match Embedder::from_config(model_str) {
            Ok(embedder) => {
                // Set model version on vector index if not already set.
                if self.vector_index.model_version().is_none() {
                    self.vector_index.set_model_version(embedder.model_name().version_string());
                }
                self.embedder = Some(embedder);
                Ok(true)
            }
            Err(e) => {
                warn!("Could not initialize embedder, vector indexing disabled: {}", e);
                Ok(false)
            }
        }
    }

    /// Index a single file. Parses, chunks, stores metadata, indexes in Tantivy,
    /// embeds in vector index (if embedder available), and builds graph edges.
    pub fn index_file(&mut self, rel_path: &str, content: &str) -> Result<()> {
        // 1. Parse document.
        let doc = parser::parse_document(Path::new(rel_path), content)?;

        // 2. Chunk document.
        let chunks = chunker::chunk_document(rel_path, &doc.content, &self.config.chunking);

        // 3. Compute modified_at timestamp.
        let modified_at = now_unix();

        // 4. Store file record in persistence.
        self.store.insert_file(
            rel_path,
            &doc.content_hash,
            modified_at,
            doc.template.as_deref(),
            doc.title.as_deref(),
        )?;

        // 5. Delete old chunks and insert new ones.
        self.store.delete_chunks_for_file(rel_path)?;
        let chunk_records: Vec<ChunkRecord> = chunks
            .iter()
            .map(|c| ChunkRecord {
                chunk_index: c.chunk_index,
                start_byte: c.start_byte,
                end_byte: c.end_byte,
                text: c.text.clone(),
            })
            .collect();
        self.store.insert_chunks(rel_path, &chunk_records)?;

        // 6. Remove old document from BM25, add new.
        self.bm25.remove_document(rel_path)?;
        self.bm25.add_document(rel_path, doc.title.as_deref(), &doc.tags, &chunks)?;

        // 7. Embed chunks and add to vector index (if embedder is available).
        self.vector_index.remove_document(rel_path);
        if let Some(ref embedder) = self.embedder {
            // Build context-prefixed text for embedding (original text preserved for BM25/snippets).
            let doc_title = doc.title.as_deref().unwrap_or("").trim();
            let texts_for_embedding: Vec<String> = chunks
                .iter()
                .map(|c| {
                    let section = c.heading_chain.as_deref().unwrap_or("").trim();
                    if !doc_title.is_empty() && !section.is_empty() {
                        format!("{} > {}: {}", doc_title, section, c.text)
                    } else if !doc_title.is_empty() {
                        format!("{}: {}", doc_title, c.text)
                    } else if !section.is_empty() {
                        format!("{}: {}", section, c.text)
                    } else {
                        c.text.clone()
                    }
                })
                .collect();
            let texts: Vec<&str> = texts_for_embedding.iter().map(|s| s.as_str()).collect();
            if !texts.is_empty() {
                match embedder.embed_batch(&texts) {
                    Ok(embeddings) => {
                        // Add chunk-level embeddings.
                        let chunk_indices: Vec<Option<usize>> =
                            chunks.iter().map(|c| Some(c.chunk_index)).collect();
                        let _ = self.vector_index.add_batch(
                            &embeddings,
                            rel_path,
                            &chunk_indices,
                            false,
                        );

                        // Add document-level embedding (average of chunks).
                        if let Some(doc_embedding) = Embedder::average_embeddings(&embeddings) {
                            let _ = self.vector_index.add(&doc_embedding, rel_path, None, true);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to embed chunks for {}: {}", rel_path, e);
                    }
                }
            }
        }

        // 8. Remove old edges and rebuild from document.
        //    Note: pass empty slice for all_docs — tag edges are only built during full_reindex.
        self.graph.remove_edges_for_node(rel_path);
        self.graph.build_edges_for_document(&doc, &self.config.graph.edge_types, &[]);

        debug!("Indexed file: {}", rel_path);
        Ok(())
    }

    /// Remove a file from all indices (persistence, BM25, vector, graph).
    pub fn remove_file(&mut self, rel_path: &str) -> Result<()> {
        // 1. Delete from persistence (cascades chunks).
        self.store.delete_file(rel_path)?;

        // 2. Remove from BM25.
        self.bm25.remove_document(rel_path)?;

        // 3. Remove from vector index.
        self.vector_index.remove_document(rel_path);

        // 4. Remove edges from graph.
        self.graph.remove_edges_for_node(rel_path);

        // 5. Remove node from graph (ignore error if node doesn't exist).
        let _ = self.graph.remove_node(rel_path);

        debug!("Removed file: {}", rel_path);
        Ok(())
    }

    /// Perform a delta scan: compare filesystem against stored file records.
    ///
    /// Automatically re-indexes changed files and removes deleted ones.
    /// Returns a summary of what changed.
    pub fn delta_scan(&mut self) -> Result<DeltaScanResult> {
        // Ensure embedder is available for indexing.
        let _ = self.ensure_embedder();

        // 1. List all files currently in persistence.
        let stored_files = self.store.list_files()?;
        let stored_map: HashMap<String, String> =
            stored_files.into_iter().map(|f| (f.path.clone(), f.content_hash.clone())).collect();

        // 2. Walk the corpus directory.
        let corpus_path = PathBuf::from(&self.config.path);
        let disk_files = walk_markdown_files(&corpus_path)?;

        let mut new_files = Vec::new();
        let mut modified_files = Vec::new();
        let mut seen_on_disk = HashMap::new();

        for (rel_path, full_path) in &disk_files {
            let content = fs::read_to_string(full_path).map_err(|e| {
                Error::Io(std::io::Error::new(e.kind(), format!("{}: {}", rel_path, e)))
            })?;
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let _ = seen_on_disk.insert(rel_path.clone(), ());

            match stored_map.get(rel_path) {
                None => {
                    // New file.
                    self.index_file(rel_path, &content)?;
                    new_files.push(rel_path.clone());
                }
                Some(stored_hash) if *stored_hash != hash => {
                    // Modified file.
                    self.index_file(rel_path, &content)?;
                    modified_files.push(rel_path.clone());
                }
                _ => {
                    // Unchanged, skip.
                }
            }
        }

        // 3. Find deleted files (in store but not on disk).
        let mut deleted_files = Vec::new();
        for path in stored_map.keys() {
            if !seen_on_disk.contains_key(path) {
                self.remove_file(path)?;
                deleted_files.push(path.clone());
            }
        }

        // 4. Commit.
        self.commit()?;

        info!(
            "Delta scan complete: {} new, {} modified, {} deleted",
            new_files.len(),
            modified_files.len(),
            deleted_files.len()
        );

        Ok(DeltaScanResult { new_files, modified_files, deleted_files })
    }

    /// Full reindex: clear all indices, scan entire corpus, re-index everything.
    ///
    /// Returns the number of files indexed.
    pub fn full_reindex(&mut self) -> Result<usize> {
        // Ensure embedder is available for indexing.
        let _ = self.ensure_embedder();

        // 1. Delete all files from store (cascades chunks).
        let existing = self.store.list_files()?;
        for file in &existing {
            self.store.delete_file(&file.path)?;
        }

        // 2. Clear BM25 index by removing all known documents.
        for file in &existing {
            self.bm25.remove_document(&file.path)?;
        }

        // 3. Reset graph.
        self.graph = KnowledgeGraph::new();

        // 3b. Reset vector index.
        self.vector_index = VectorIndex::new_default(self.vector_index.dimensions());

        // 4. Walk corpus directory and index every .md file.
        let corpus_path = PathBuf::from(&self.config.path);
        let disk_files = walk_markdown_files(&corpus_path)?;

        let mut all_docs: Vec<Document> = Vec::new();

        for (rel_path, full_path) in &disk_files {
            let content = fs::read_to_string(full_path).map_err(|e| {
                Error::Io(std::io::Error::new(e.kind(), format!("{}: {}", rel_path, e)))
            })?;

            // Parse and store.
            let doc = parser::parse_document(Path::new(rel_path.as_str()), &content)?;
            let chunks = chunker::chunk_document(rel_path, &doc.content, &self.config.chunking);

            let modified_at = now_unix();
            self.store.insert_file(
                rel_path,
                &doc.content_hash,
                modified_at,
                doc.template.as_deref(),
                doc.title.as_deref(),
            )?;

            let chunk_records: Vec<ChunkRecord> = chunks
                .iter()
                .map(|c| ChunkRecord {
                    chunk_index: c.chunk_index,
                    start_byte: c.start_byte,
                    end_byte: c.end_byte,
                    text: c.text.clone(),
                })
                .collect();
            self.store.insert_chunks(rel_path, &chunk_records)?;
            self.bm25.add_document(rel_path, doc.title.as_deref(), &doc.tags, &chunks)?;

            // Embed chunks in vector index (if embedder available).
            if let Some(embedder) = &self.embedder {
                let doc_title = doc.title.as_deref().unwrap_or(rel_path);
                let texts_for_embedding: Vec<String> = chunks
                    .iter()
                    .map(|c| {
                        if let Some(ref chain) = c.heading_chain {
                            format!("{} > {}: {}", doc_title, chain, c.text)
                        } else if let Some(ref title) = doc.title {
                            format!("{}: {}", title, c.text)
                        } else {
                            c.text.clone()
                        }
                    })
                    .collect();
                let texts: Vec<&str> = texts_for_embedding.iter().map(|s| s.as_str()).collect();
                if !texts.is_empty() {
                    match embedder.embed_batch(&texts) {
                        Ok(embeddings) => {
                            let chunk_indices: Vec<Option<usize>> =
                                chunks.iter().map(|c| Some(c.chunk_index)).collect();
                            let _ = self.vector_index.add_batch(
                                &embeddings,
                                rel_path,
                                &chunk_indices,
                                false,
                            );

                            if let Some(doc_embedding) = Embedder::average_embeddings(&embeddings) {
                                let _ = self.vector_index.add(&doc_embedding, rel_path, None, true);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to embed chunks for {}: {}", rel_path, e);
                        }
                    }
                }
            }

            // Build non-tag edges (wikilink, frontmatter).
            self.graph.build_edges_for_document(&doc, &self.config.graph.edge_types, &[]);

            all_docs.push(doc);
        }

        // 5. Second pass: build tag-based edges with all documents available.
        let tag_configs: Vec<_> = self
            .config
            .graph
            .edge_types
            .iter()
            .filter(|et| et.source == ctxvault_common::config::EdgeSource::Tag)
            .cloned()
            .collect();

        if !tag_configs.is_empty() {
            for doc in &all_docs {
                self.graph.build_edges_for_document(doc, &tag_configs, &all_docs);
            }
        }

        // 6. Commit.
        self.commit()?;

        let count = all_docs.len();
        info!("Full reindex complete: {} files indexed", count);
        Ok(count)
    }

    /// Commit all pending changes (Tantivy commit, graph save, vector index save).
    pub fn commit(&mut self) -> Result<()> {
        self.bm25.commit()?;
        self.graph.save(&self.index_dir.join("graph.bin"))?;
        // Save vector index (only if it has data).
        if !self.vector_index.is_empty() {
            self.vector_index.save(&self.index_dir.join("vectors.json")).unwrap_or_else(|e| {
                warn!("Failed to save vector index: {}", e);
            });
        }
        Ok(())
    }

    /// Get a reference to the BM25 index for searching.
    pub fn bm25(&self) -> &BM25Index {
        &self.bm25
    }

    /// Get a reference to the vector index for semantic search.
    pub fn vector_index(&self) -> &VectorIndex {
        &self.vector_index
    }

    /// Get a reference to the embedder (if initialized).
    pub fn embedder_ref(&self) -> Option<&Embedder> {
        self.embedder.as_ref()
    }

    /// Get a reference to the graph for traversal/queries.
    pub fn graph(&self) -> &KnowledgeGraph {
        &self.graph
    }

    /// Get a mutable reference to the graph for manipulation.
    pub fn graph_mut(&mut self) -> &mut KnowledgeGraph {
        &mut self.graph
    }

    /// Get a reference to the store for metadata queries.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Get the corpus config.
    pub fn config(&self) -> &CorpusConfig {
        &self.config
    }

    /// Check whether vectors are stale (model version mismatch).
    pub fn vectors_stale(&self) -> bool {
        self.vector_index.is_stale()
    }

    /// Check whether the corpus has been indexed (has any files in the store).
    pub fn is_indexed(&self) -> bool {
        self.store.list_files().map(|f| !f.is_empty()).unwrap_or(false)
    }

    /// Get the model version stored in the vector index.
    pub fn stored_model_version(&self) -> Option<&str> {
        self.vector_index.model_version()
    }

    /// Get a mutable reference to the vector index.
    pub fn vector_index_mut(&mut self) -> &mut VectorIndex {
        &mut self.vector_index
    }

    /// Re-embed all chunks with the current model, replacing old vectors.
    ///
    /// This performs a full re-embedding of all stored chunks:
    /// 1. Ensures embedder is initialized
    /// 2. Reads all chunks from the persistence store
    /// 3. Clears the vector index
    /// 4. Re-embeds all chunks and adds them to the vector index
    /// 5. Updates model_version metadata
    /// 6. Commits changes to disk
    ///
    /// Returns the number of chunks re-embedded.
    pub fn reembed(&mut self) -> Result<usize> {
        // 1. Ensure embedder is available.
        let available = self.ensure_embedder()?;
        if !available {
            return Err(Error::Index("embedder not available — cannot re-embed".to_string()));
        }

        // 2. Get all files and their chunks from the store.
        let files = self.store.list_files()?;

        // 3. Reset vector index (preserve dimensions and params).
        let dims = self.vector_index.dimensions();
        self.vector_index = VectorIndex::new_default(dims);

        // 4. Re-embed all chunks.
        let mut total_chunks = 0usize;

        let corpus_path = std::path::PathBuf::from(&self.config.path);
        for file in &files {
            let full_path = corpus_path.join(&file.path);
            let parsed_chunks: Option<(Vec<ctxvault_common::types::Chunk>, Option<String>)> =
                std::fs::read_to_string(&full_path).ok().and_then(|content| {
                    let doc =
                        crate::parser::parse_document(std::path::Path::new(&file.path), &content)
                            .ok()?;
                    let chunks = crate::parser::chunker::chunk_document(
                        &file.path,
                        &doc.content,
                        &self.config.chunking,
                    );
                    Some((chunks, doc.title))
                });

            let (chunks, title) = if let Some((c, t)) = parsed_chunks {
                (c, t.or_else(|| file.title.clone()))
            } else {
                let chunk_records = self.store.get_chunks_for_file(&file.path)?;
                let chunks: Vec<ctxvault_common::types::Chunk> = chunk_records
                    .into_iter()
                    .map(|cr| ctxvault_common::types::Chunk {
                        doc_path: file.path.clone(),
                        chunk_index: cr.chunk_index,
                        text: cr.text,
                        start_byte: cr.start_byte,
                        end_byte: cr.end_byte,
                        heading_chain: None,
                    })
                    .collect();
                (chunks, file.title.clone())
            };

            if chunks.is_empty() {
                continue;
            }

            // Build context-prefixed text for embedding.
            let doc_title = title.as_deref().unwrap_or("").trim();
            let texts_for_embedding: Vec<String> = chunks
                .iter()
                .map(|c| {
                    let section = c.heading_chain.as_deref().unwrap_or("").trim();
                    if !doc_title.is_empty() && !section.is_empty() {
                        format!("{} > {}: {}", doc_title, section, c.text)
                    } else if !doc_title.is_empty() {
                        format!("{}: {}", doc_title, c.text)
                    } else if !section.is_empty() {
                        format!("{}: {}", section, c.text)
                    } else {
                        c.text.clone()
                    }
                })
                .collect();
            let texts: Vec<&str> = texts_for_embedding.iter().map(|s| s.as_str()).collect();

            // Embed using the current embedder.
            let embeddings = self.embedder.as_ref().unwrap().embed_batch(&texts)?;

            // Add chunk-level embeddings.
            let chunk_indices: Vec<Option<usize>> =
                chunks.iter().map(|c| Some(c.chunk_index)).collect();
            let _ = self.vector_index.add_batch(&embeddings, &file.path, &chunk_indices, false);

            // Add document-level embedding (average of chunks).
            if let Some(doc_embedding) = crate::embedding::Embedder::average_embeddings(&embeddings)
            {
                let _ = self.vector_index.add(&doc_embedding, &file.path, None, true);
            }

            total_chunks += chunks.len();
        }

        // 5. Update model version metadata.
        let model_version =
            self.embedder.as_ref().unwrap().model_name().version_string().to_string();
        self.vector_index.set_model_version(&model_version);
        self.vector_index.clear_stale();

        // 6. Store model version in persistence for audit trail.
        self.store.set_config("embedding_model", &model_version)?;

        // 7. Commit to disk.
        self.commit()?;

        info!(
            "Re-embedding complete: {} chunks re-embedded with model '{}'",
            total_chunks, model_version
        );

        Ok(total_chunks)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Current Unix timestamp in seconds.
fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("system time before epoch").as_secs() as i64
}

/// Recursively walk a directory and collect all `.md` files.
/// Returns `(relative_path, absolute_path)` pairs.
fn walk_markdown_files(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut results = Vec::new();
    if !root.exists() {
        return Ok(results);
    }
    walk_dir_recursive(root, root, &mut results)?;
    Ok(results)
}

fn walk_dir_recursive(
    root: &Path,
    current: &Path,
    results: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let entries = fs::read_dir(current)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories (e.g., .index, .templates, .git).
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }
            walk_dir_recursive(root, &path, results)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = path.strip_prefix(root).map_err(|e| {
                Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            })?;
            // Normalize path separators to forward slashes.
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            results.push((rel_str, path.clone()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a minimal corpus config pointing at the given path.
    fn test_config(corpus_path: &Path) -> CorpusConfig {
        CorpusConfig {
            name: "test".to_string(),
            path: corpus_path.to_string_lossy().to_string(),
            mode: ctxvault_common::config::CorpusMode::ReadWrite,
            chunking: ctxvault_common::config::ChunkingConfig {
                min_chunk_tokens: 1, // very low for tests
                ..Default::default()
            },
            embedding: ctxvault_common::config::EmbeddingConfig::default(),
            graph: ctxvault_common::config::GraphConfig {
                edge_types: vec![ctxvault_common::config::EdgeTypeConfig {
                    name: "Wikilink".to_string(),
                    source: ctxvault_common::config::EdgeSource::Wikilink,
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

    #[test]
    fn test_open_creates_index_dir() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        let config = test_config(&corpus_dir);
        let _engine = Engine::open(config, &index_dir).unwrap();

        // Verify index directory structure was created.
        assert!(index_dir.exists());
        assert!(index_dir.join("meta.db").exists());
        assert!(index_dir.join("tantivy").exists());
    }

    #[test]
    fn test_index_and_search() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        let content =
            "# Rust Programming\n\nRust is a systems programming language focused on safety.\n";
        engine.index_file("rust.md", content).unwrap();
        engine.commit().unwrap();

        // Verify searchable via BM25.
        let results = engine.bm25().search("systems programming", 10).unwrap();
        assert!(!results.is_empty(), "Should find indexed file via search");
        assert_eq!(results[0].path, "rust.md");

        // Verify stored in persistence.
        let file = engine.store().get_file("rust.md").unwrap();
        assert!(file.is_some());
        assert_eq!(file.unwrap().title.as_deref(), Some("Rust Programming"));
    }

    #[test]
    fn test_remove_file() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        let content = "# To Remove\n\nThis note will be removed.\n";
        engine.index_file("remove_me.md", content).unwrap();
        engine.commit().unwrap();

        // Confirm it's indexed.
        assert!(engine.store().get_file("remove_me.md").unwrap().is_some());

        // Remove it.
        engine.remove_file("remove_me.md").unwrap();
        engine.commit().unwrap();

        // Verify gone from all stores.
        assert!(engine.store().get_file("remove_me.md").unwrap().is_none());
        let results = engine.bm25().search("removed", 10).unwrap();
        assert!(results.iter().all(|r| r.path != "remove_me.md"), "File should be gone from BM25");
        assert!(engine.graph().get_node("remove_me.md").is_none());
    }

    #[test]
    fn test_delta_scan() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        // Create initial files.
        fs::write(corpus_dir.join("existing.md"), "# Existing\n\nOriginal content.\n").unwrap();
        fs::write(corpus_dir.join("will_modify.md"), "# Will Modify\n\nOriginal.\n").unwrap();
        fs::write(corpus_dir.join("will_delete.md"), "# Will Delete\n\nGoing away.\n").unwrap();

        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        // Initial full index.
        let count = engine.full_reindex().unwrap();
        assert_eq!(count, 3);

        // Now modify one file, delete one, and add a new one.
        fs::write(
            corpus_dir.join("will_modify.md"),
            "# Will Modify\n\nUpdated content that is different.\n",
        )
        .unwrap();
        fs::remove_file(corpus_dir.join("will_delete.md")).unwrap();
        fs::write(corpus_dir.join("new_file.md"), "# New File\n\nBrand new.\n").unwrap();

        // Run delta scan.
        let result = engine.delta_scan().unwrap();

        assert_eq!(result.new_files, vec!["new_file.md"]);
        assert_eq!(result.modified_files, vec!["will_modify.md"]);
        assert_eq!(result.deleted_files, vec!["will_delete.md"]);

        // Verify the new file is searchable.
        let search = engine.bm25().search("brand new", 10).unwrap();
        assert!(search.iter().any(|r| r.path == "new_file.md"));

        // Verify deleted file is gone.
        assert!(engine.store().get_file("will_delete.md").unwrap().is_none());
    }

    #[test]
    fn test_full_reindex() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        fs::write(corpus_dir.join("alpha.md"), "# Alpha\n\nFirst note.\n").unwrap();
        fs::write(corpus_dir.join("beta.md"), "# Beta\n\nSecond note.\n").unwrap();
        fs::write(corpus_dir.join("gamma.md"), "# Gamma\n\nThird note with [[alpha]] link.\n")
            .unwrap();

        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        let count = engine.full_reindex().unwrap();
        assert_eq!(count, 3);

        // All files should be in the store.
        let files = engine.store().list_files().unwrap();
        assert_eq!(files.len(), 3);

        // Graph should have the wikilink edge from gamma to alpha.
        let fwd = engine.graph().forwardlinks("gamma.md", None);
        let targets = fwd.get("Wikilink").unwrap_or(&Vec::new()).clone();
        assert!(
            targets.contains(&"alpha".to_string()),
            "Expected wikilink edge from gamma to alpha"
        );

        // Verify search works.
        let results = engine.bm25().search("Second note", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "beta.md");
    }

    #[test]
    fn test_model_version_set_on_new_index() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        let config = test_config(&corpus_dir);
        let engine = Engine::open(config, &index_dir).unwrap();

        // New empty vector index should not be stale.
        assert!(!engine.vectors_stale());
    }

    #[test]
    fn test_model_version_mismatch_marks_stale() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        // Create a vector index file with a different model version.
        fs::create_dir_all(&index_dir).unwrap();
        let fake_data = serde_json::json!({
            "entries": [{
                "id": 0,
                "meta": {"doc_path": "test.md", "chunk_index": 0, "is_doc_level": false},
                "vector": vec![0.1f32; 384]
            }],
            "next_id": 1,
            "dimensions": 384,
            "max_nb_connection": 16,
            "ef_construction": 200,
            "model_version": "some-other-model-v99"
        });
        fs::write(index_dir.join("vectors.json"), serde_json::to_string(&fake_data).unwrap())
            .unwrap();

        let config = test_config(&corpus_dir);
        let engine = Engine::open(config, &index_dir).unwrap();

        // Should be marked stale due to version mismatch.
        assert!(engine.vectors_stale());
        assert_eq!(engine.stored_model_version(), Some("some-other-model-v99"));
    }

    #[test]
    fn test_model_version_no_version_marks_stale() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        // Create a vector index file WITHOUT model_version (legacy format).
        fs::create_dir_all(&index_dir).unwrap();
        let fake_data = serde_json::json!({
            "entries": [{
                "id": 0,
                "meta": {"doc_path": "test.md", "chunk_index": 0, "is_doc_level": false},
                "vector": vec![0.1f32; 384]
            }],
            "next_id": 1,
            "dimensions": 384,
            "max_nb_connection": 16,
            "ef_construction": 200
        });
        fs::write(index_dir.join("vectors.json"), serde_json::to_string(&fake_data).unwrap())
            .unwrap();

        let config = test_config(&corpus_dir);
        let engine = Engine::open(config, &index_dir).unwrap();

        // Legacy vectors (no model_version) with data should be marked stale.
        assert!(engine.vectors_stale());
    }

    #[test]
    fn test_corpus_config_persistence() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        let config = test_config(&corpus_dir);
        let engine = Engine::open(config, &index_dir).unwrap();

        // Set and get config.
        engine.store().set_config("embedding_model", "all-minilm-l6-v2").unwrap();
        let value = engine.store().get_config("embedding_model").unwrap();
        assert_eq!(value, Some("all-minilm-l6-v2".to_string()));

        // Non-existent key returns None.
        let missing = engine.store().get_config("nonexistent").unwrap();
        assert_eq!(missing, None);
    }
}
