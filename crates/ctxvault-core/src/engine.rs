//! Engine: coordinates persistence (SQLite), BM25 index (Tantivy), knowledge graph (petgraph),
//! and vector index (HNSW) with optional embedding support.
//!
//! The [`Engine`] is the top-level orchestrator for a single corpus. It manages
//! indexing, delta scanning, and provides unified access to all subsystems.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use ctxvault_common::config::CorpusConfig;
use ctxvault_common::types::{ChunkEmbedPolicy, Document, EntityKind};
use ctxvault_common::{Error, Result};

use crate::embedding::Embedder;
use crate::graph::KnowledgeGraph;
use crate::index::{pipeline::AsyncEmbeddingPipeline, BM25Index};
use crate::parser;
use crate::parser::chunker;
use crate::persistence::{ChunkRecord, EdgeTypeRecord, IndexingState, IndexingStatus, Store};
use crate::vector_index::VectorIndex;

/// Detailed indexing status response for client queries and monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingStatusResponse {
    /// Corpus identifier/name.
    pub corpus_id: String,
    /// Current status (idle, indexing, paused, error, completed).
    pub status: IndexingStatus,
    /// Total markdown files discovered.
    pub total_files: usize,
    /// Count of markdown files successfully committed.
    pub indexed_files: usize,
    /// Progress as a percentage (0.0 - 100.0).
    pub progress_percent: f64,
    /// Relative path of last committed file.
    pub last_processed_path: Option<String>,
    /// When indexing started (Unix timestamp seconds).
    pub started_at: i64,
    /// When status was last updated (Unix timestamp seconds).
    pub updated_at: i64,
    /// Total elapsed time in seconds.
    pub elapsed_seconds: i64,
    /// Estimated indexing throughput in documents per second.
    pub estimated_throughput_docs_per_sec: f64,
    /// Estimated time remaining in seconds until completion.
    pub estimated_time_remaining_seconds: f64,
    /// Error message if status is Error.
    pub error_message: Option<String>,
}

/// A text chunk staged for vectorized batch embedding.
#[derive(Debug, Clone)]
pub struct PendingChunk {
    /// Document relative path.
    pub doc_path: String,
    /// Chunk index within document.
    pub chunk_index: usize,
    /// Prepared context-prefixed text for embedding.
    pub text: String,
    /// Policy determining if this chunk receives a dense vector embedding.
    pub embed_policy: ChunkEmbedPolicy,
    /// Coarse modality tag ("code" / "docs") carried into the vector index.
    pub modality: String,
}

/// Coordinates persistence, full-text index, knowledge graph, and vector index for a corpus.
pub struct Engine {
    config: CorpusConfig,
    store: Store,
    bm25: BM25Index,
    graph: KnowledgeGraph,
    vector_index: Option<VectorIndex>,
    embedder: RwLock<Option<Arc<Embedder>>>,
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
    /// Initializes SQLite store, Tantivy BM25 index, knowledge graph, and vector index (if in Full mode).
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

        // 5. Load or create vector index (skipped entirely in Fast Mode).
        let vector_index = match config.index_mode {
            ctxvault_common::config::IndexMode::Fast => {
                info!(corpus = %config.name, "Fast Mode enabled: skipping vector index allocation and ONNX embedder initialization");
                None
            }
            ctxvault_common::config::IndexMode::Full => {
                let configured_model_name =
                    crate::embedding::ModelName::from_str_name(&config.embedding.model)
                        .unwrap_or_default();
                let configured_dimensions = configured_model_name.dimensions();
                let configured_model_version = configured_model_name.version_string();

                let vector_path = index_dir.join("vectors.json");
                let mut vi = if vector_path.exists() {
                    VectorIndex::load(&vector_path).unwrap_or_else(|e| {
                        warn!("Failed to load vector index from disk, starting fresh: {}", e);
                        VectorIndex::new_default(configured_dimensions)
                    })
                } else {
                    VectorIndex::new_default(configured_dimensions)
                };

                // Check model version staleness and dimension match.
                if vi.dimensions() != configured_dimensions {
                    warn!(
                        "Vector index dimension mismatch: stored={}, configured={}. Recreating index with {} dimensions and marking stale.",
                        vi.dimensions(),
                        configured_dimensions,
                        configured_dimensions
                    );
                    vi = VectorIndex::new_default(configured_dimensions);
                    vi.set_model_version(&configured_model_version);
                    vi.mark_stale();
                } else if let Some(stored_version) = vi.model_version() {
                    let is_compatible = stored_version == configured_model_version
                        || (stored_version.starts_with("jina-embeddings-v2-base-code")
                            && configured_model_version
                                .starts_with("jina-embeddings-v2-base-code"));
                    if !is_compatible {
                        warn!(
                            "Embedding model version mismatch: stored='{}', configured='{}'. Vectors marked as stale.",
                            stored_version, configured_model_version
                        );
                        vi.mark_stale();
                    } else {
                        vi.clear_stale();
                    }
                } else if !vi.is_empty() {
                    warn!(
                        "Vector index has no model_version metadata. Marking as stale for safety."
                    );
                    vi.mark_stale();
                } else {
                    vi.set_model_version(&configured_model_version);
                }

                Some(vi)
            }
        };

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
            embedder: RwLock::new(None), // Lazily initialized
            index_dir: index_dir.to_path_buf(),
        })
    }

    /// Ensure the embedder is initialized. Returns Ok(true) if available, Ok(false) if skipped.
    ///
    /// In Fast Mode, this immediately returns Ok(false) without loading the model.
    /// In Full Mode, the embedder is lazily created to avoid model download during tests or when
    /// vector indexing is not needed.
    pub fn ensure_embedder(&self) -> Result<bool> {
        if self.config.index_mode == ctxvault_common::config::IndexMode::Fast {
            return Ok(false);
        }
        {
            let guard = self.embedder.read().unwrap();
            if guard.is_some() {
                return Ok(true);
            }
        }

        let model_str = &self.config.embedding.model;
        match Embedder::from_config(model_str) {
            Ok(embedder) => {
                let arc = Arc::new(embedder);
                let mut guard = self.embedder.write().unwrap();
                if guard.is_none() {
                    *guard = Some(arc);
                }
                Ok(true)
            }
            Err(e) => {
                warn!("Could not initialize embedder, vector indexing disabled: {}", e);
                Ok(false)
            }
        }
    }

    /// Staged file indexing: parses, chunks, updates persistence, BM25, vector removal,
    /// and graph edges without immediately triggering embedding inference.
    ///
    /// Returns pending chunks ready for batched embedding, along with the parsed markdown
    /// document (if markdown) for constructing global tag edges without re-parsing.
    pub fn index_file_staged(
        &mut self,
        rel_path: &str,
        content: &str,
    ) -> Result<(Vec<PendingChunk>, Option<Document>)> {
        let path = Path::new(rel_path);
        let modified_at = now_unix();

        if crate::parser::code::is_code_file(path) {
            let parse_res = crate::parser::code::chunker::CodeChunker::parse_and_chunk(
                path,
                content,
                &self.config.chunking,
            );

            let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let file_title =
                path.file_name().and_then(|n| n.to_str()).unwrap_or(rel_path).to_string();

            // 1. Store file record in persistence
            self.store.insert_file(
                rel_path,
                &content_hash,
                modified_at,
                None,
                Some(&file_title),
            )?;

            let mut pending = Vec::new();

            // 2. Chunks and symbols
            if let Some(res) = parse_res {
                self.store.delete_chunks_for_file(rel_path)?;
                let chunk_records: Vec<ChunkRecord> = res
                    .chunks
                    .iter()
                    .map(|c| ChunkRecord {
                        chunk_index: c.chunk_index,
                        start_byte: c.start_byte,
                        end_byte: c.end_byte,
                        text: c.text.clone(),
                    })
                    .collect();
                self.store.insert_chunks(rel_path, &chunk_records)?;
                self.store.save_code_symbols(rel_path, &res.symbols)?;

                // 3. BM25
                self.bm25.remove_document(rel_path)?;
                self.bm25.add_document(rel_path, Some(&file_title), &[], &res.chunks)?;

                // 4. Vector index: clear existing vectors for this doc
                if let Some(ref mut vi) = self.vector_index {
                    vi.remove_document(rel_path);
                }

                // Build pending chunks for embedding
                for c in &res.chunks {
                    let modality = c
                        .entity_kind
                        .as_ref()
                        .map(EntityKind::modality_tag)
                        .unwrap_or("docs")
                        .to_string();
                    pending.push(PendingChunk {
                        doc_path: rel_path.to_string(),
                        chunk_index: c.chunk_index,
                        text: c.text.clone(),
                        embed_policy: c.embed_policy,
                        modality,
                    });
                }

                // 5. Code Graph
                self.graph.remove_edges_for_node(rel_path);
                let edges = crate::graph::code::CodeGraphExtractor::extract_edges_for_file(
                    path,
                    content,
                    &res.symbols,
                    &res.symbols,
                );
                for edge in &edges {
                    self.graph.add_code_edge(edge);
                }
            }

            debug!("Staged code file: {}", rel_path);
            return Ok((pending, None));
        }

        // 1. Parse document.
        let doc = parser::parse_document(Path::new(rel_path), content)?;

        // 2. Chunk document.
        let chunks = chunker::chunk_document(rel_path, &doc.content, &self.config.chunking);

        // 3. Store file record in persistence.
        self.store.insert_file(
            rel_path,
            &doc.content_hash,
            modified_at,
            doc.template.as_deref(),
            doc.title.as_deref(),
        )?;

        // 4. Delete old chunks and insert new ones.
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

        // 5. Remove old document from BM25, add new.
        self.bm25.remove_document(rel_path)?;
        self.bm25.add_document(rel_path, doc.title.as_deref(), &doc.tags, &chunks)?;

        // 6. Vector index: clear existing vectors for this doc
        if let Some(ref mut vi) = self.vector_index {
            vi.remove_document(rel_path);
        }

        // Build context-prefixed text for embedding
        let doc_title = doc.title.as_deref().unwrap_or("").trim();
        let pending: Vec<PendingChunk> = chunks
            .iter()
            .map(|c| {
                let section = c.heading_chain.as_deref().unwrap_or("").trim();
                let text = if !doc_title.is_empty() && !section.is_empty() {
                    format!("{} > {}: {}", doc_title, section, c.text)
                } else if !doc_title.is_empty() {
                    format!("{}: {}", doc_title, c.text)
                } else if !section.is_empty() {
                    format!("{}: {}", section, c.text)
                } else {
                    c.text.clone()
                };
                let modality = c
                    .entity_kind
                    .as_ref()
                    .map(EntityKind::modality_tag)
                    .unwrap_or("docs")
                    .to_string();
                PendingChunk {
                    doc_path: rel_path.to_string(),
                    chunk_index: c.chunk_index,
                    text,
                    embed_policy: c.embed_policy,
                    modality,
                }
            })
            .collect();

        // 7. Remove old edges and rebuild from document.
        self.graph.remove_edges_for_node(rel_path);
        self.graph.build_edges_for_document(&doc, &self.config.graph.edge_types, &[]);

        debug!("Staged markdown file: {}", rel_path);
        Ok((pending, Some(doc)))
    }

    /// Flush a batch of pending chunks into the vector index in a single vectorized forward pass.
    /// Only anchor chunks receive dense vector embeddings; graph-only chunks are skipped.
    pub fn flush_chunk_buffer(&mut self, buffer: &[PendingChunk]) -> Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        // Partition buffer into anchor chunks and graph-only chunks
        let anchor_chunks: Vec<&PendingChunk> =
            buffer.iter().filter(|c| c.embed_policy == ChunkEmbedPolicy::Anchor).collect();

        tracing::debug!(
            total = buffer.len(),
            anchors = anchor_chunks.len(),
            graph_only = buffer.len() - anchor_chunks.len(),
            "flush_chunk_buffer: partitioned by embed policy"
        );

        if anchor_chunks.is_empty() {
            return Ok(());
        }

        let embedder = match self.embedder_ref() {
            Some(emb) => emb,
            None => return Ok(()),
        };

        let texts: Vec<&str> = anchor_chunks.iter().map(|c| c.text.as_str()).collect();
        let embeddings = match embedder.embed_batch(&texts) {
            Ok(embs) => embs,
            Err(e) => {
                warn!(
                    "Failed to generate embeddings for batch of {} anchor chunks: {}",
                    anchor_chunks.len(),
                    e
                );
                return Ok(());
            }
        };

        if embeddings.len() != anchor_chunks.len() {
            warn!(
                "Embedding count mismatch: expected {}, got {}",
                anchor_chunks.len(),
                embeddings.len()
            );
            return Ok(());
        }

        // Group by contiguous document slices (zero allocation, zero hash map overhead)
        let mut start = 0;
        while start < anchor_chunks.len() {
            let doc_path = &anchor_chunks[start].doc_path;
            let mut end = start + 1;
            while end < anchor_chunks.len() && anchor_chunks[end].doc_path == *doc_path {
                end += 1;
            }

            let file_chunks = &anchor_chunks[start..end];
            let file_embeddings = &embeddings[start..end];

            let chunk_indices: Vec<Option<usize>> =
                file_chunks.iter().map(|c| Some(c.chunk_index)).collect();
            // All chunks for a doc_path share the same file, hence the same modality.
            let modality = file_chunks[0].modality.as_str();

            if let Some(ref mut vi) = self.vector_index {
                let _ = vi.add_batch(file_embeddings, doc_path, &chunk_indices, false, modality);

                if let Some(doc_embedding) = Embedder::average_embeddings(file_embeddings) {
                    let _ = vi.add(&doc_embedding, doc_path, None, true, modality);
                }
            }

            start = end;
        }

        Ok(())
    }

    /// Index a single file. Parses, chunks, stores metadata, indexes in Tantivy,
    /// embeds in vector index (if embedder available), and builds graph edges.
    pub fn index_file(&mut self, rel_path: &str, content: &str) -> Result<()> {
        let (pending, _doc) = self.index_file_staged(rel_path, content)?;
        if !pending.is_empty() {
            self.flush_chunk_buffer(&pending)?;
        }
        Ok(())
    }

    /// Remove a file from all indices (persistence, BM25, vector, graph).
    pub fn remove_file(&mut self, rel_path: &str) -> Result<()> {
        // 1. Delete from persistence (cascades chunks).
        self.store.delete_file(rel_path)?;

        // 2. Remove from BM25.
        self.bm25.remove_document(rel_path)?;

        // 3. Remove from vector index.
        if let Some(ref mut vi) = self.vector_index {
            vi.remove_document(rel_path);
        }

        // 4. Remove edges from graph.
        self.graph.remove_edges_for_node(rel_path);

        // 5. Remove node from graph (ignore error if node doesn't exist).
        let _ = self.graph.remove_node(rel_path);

        debug!("Removed file: {}", rel_path);
        Ok(())
    }

    /// Perform a delta scan with default batch size.
    pub fn delta_scan(&mut self) -> Result<DeltaScanResult> {
        self.delta_scan_paginated(50)
    }

    /// Perform a paginated delta scan: compare filesystem against stored file records.
    ///
    /// Automatically re-indexes changed files and removes deleted ones with intermediate commits.
    /// Returns a summary of what changed.
    pub fn delta_scan_paginated(&mut self, batch_size: usize) -> Result<DeltaScanResult> {
        let batch_size = if batch_size == 0 { 50 } else { batch_size };
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
        let mut uncommitted_count = 0usize;
        let embedding_pipeline = self.embedder_ref().map(AsyncEmbeddingPipeline::new);

        for (rel_path, full_path) in &disk_files {
            let content = fs::read_to_string(full_path).map_err(|e| {
                Error::Io(std::io::Error::new(e.kind(), format!("{}: {}", rel_path, e)))
            })?;
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            let _ = seen_on_disk.insert(rel_path.clone(), ());

            match stored_map.get(rel_path) {
                None => {
                    // New file.
                    let (chunks, _) = self.index_file_staged(rel_path, &content)?;
                    if let Some(ref pipeline) = embedding_pipeline {
                        for chunk in chunks {
                            if chunk.embed_policy == ChunkEmbedPolicy::Anchor {
                                pipeline.send(chunk)?;
                            }
                        }
                        if let Some(ref mut vi) = self.vector_index {
                            pipeline.try_recv_completed(vi)?;
                        }
                    }
                    new_files.push(rel_path.clone());
                    uncommitted_count += 1;
                }
                Some(stored_hash) if *stored_hash != hash => {
                    // Modified file.
                    let (chunks, _) = self.index_file_staged(rel_path, &content)?;
                    if let Some(ref pipeline) = embedding_pipeline {
                        for chunk in chunks {
                            if chunk.embed_policy == ChunkEmbedPolicy::Anchor {
                                pipeline.send(chunk)?;
                            }
                        }
                        if let Some(ref mut vi) = self.vector_index {
                            pipeline.try_recv_completed(vi)?;
                        }
                    }
                    modified_files.push(rel_path.clone());
                    uncommitted_count += 1;
                }
                _ => {
                    // Unchanged, skip.
                }
            }

            if uncommitted_count >= batch_size {
                if let Some(ref pipeline) = embedding_pipeline {
                    if let Some(ref mut vi) = self.vector_index {
                        pipeline.try_recv_completed(vi)?;
                    }
                }
                self.commit()?;
                uncommitted_count = 0;
            }
        }

        // 3. Find deleted files (in store but not on disk).
        let mut deleted_files = Vec::new();
        for path in stored_map.keys() {
            if !seen_on_disk.contains_key(path) {
                self.remove_file(path)?;
                deleted_files.push(path.clone());
                uncommitted_count += 1;
            }
            if uncommitted_count >= batch_size {
                if let Some(ref pipeline) = embedding_pipeline {
                    if let Some(ref mut vi) = self.vector_index {
                        pipeline.try_recv_completed(vi)?;
                    }
                }
                self.commit()?;
                uncommitted_count = 0;
            }
        }

        // 4. Finish embedding pipeline and commit remaining changes.
        if let Some(mut pipeline) = embedding_pipeline {
            if let Some(ref mut vi) = self.vector_index {
                pipeline.finish(vi)?;
            }
        }
        self.commit()?;

        info!(
            "Delta scan complete: {} new, {} modified, {} deleted",
            new_files.len(),
            modified_files.len(),
            deleted_files.len()
        );

        Ok(DeltaScanResult { new_files, modified_files, deleted_files })
    }

    /// Full reindex with default parameters (batch_size=50, resume=false).
    ///
    /// Returns the number of files indexed.
    pub fn full_reindex(&mut self) -> Result<usize> {
        self.full_reindex_paginated(50, false)
    }

    /// Paginated, resumable full reindex: scans corpus directory in configurable batches.
    ///
    /// - `batch_size`: Number of documents processed before flushing/checkpointing (default 50).
    /// - `resume`: If true, skips files already committed with identical content hash.
    ///
    /// Performs intermediate commits of SQLite, Tantivy, Vectors, Graph, and updates `indexing_state`.
    pub fn full_reindex_paginated(&mut self, batch_size: usize, resume: bool) -> Result<usize> {
        let batch_size = if batch_size == 0 { 50 } else { batch_size };
        let corpus_id = self.config.name.clone();
        let corpus_path = PathBuf::from(&self.config.path);
        let mut disk_files = walk_markdown_files(&corpus_path)?;
        disk_files.sort_by(|a, b| a.0.cmp(&b.0));
        let total_files = disk_files.len();

        // Ensure embedder is available for indexing.
        let _ = self.ensure_embedder();

        let mut stored_map: HashMap<String, String> = HashMap::new();

        if !resume {
            // Fresh rebuild: clear store, Tantivy, graph, vector index, and reset state
            let existing = self.store.list_files()?;
            for file in &existing {
                self.store.delete_file(&file.path)?;
                self.bm25.remove_document(&file.path)?;
            }
            self.graph = KnowledgeGraph::new();
            if let Some(ref mut vi) = self.vector_index {
                *vi = VectorIndex::new_default(vi.dimensions());
            }
            self.store.reset_indexing_state(&corpus_id)?;
        } else {
            // Resuming: load existing indexed files and their content hashes
            let existing = self.store.list_files()?;
            for file in existing {
                let _ = stored_map.insert(file.path, file.content_hash);
            }
        }

        let started_at = now_unix();
        let mut state = IndexingState {
            corpus_id: corpus_id.clone(),
            status: IndexingStatus::Indexing,
            total_files,
            indexed_files: 0,
            last_processed_path: None,
            started_at,
            updated_at: started_at,
            error_message: None,
        };

        // If resuming, calculate already-indexed matching files
        if resume {
            let mut matched_count = 0usize;
            for (rel_path, full_path) in &disk_files {
                if let Some(stored_hash) = stored_map.get(rel_path) {
                    if let Ok(content) = fs::read_to_string(full_path) {
                        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
                        if hash == *stored_hash {
                            matched_count += 1;
                        }
                    }
                }
            }
            state.indexed_files = matched_count;
        }

        self.store.update_indexing_state(&state)?;

        let mut all_docs: Vec<Document> = Vec::new();
        let embedding_pipeline = self.embedder_ref().map(AsyncEmbeddingPipeline::new);
        let mut processed_in_current_batch = 0usize;

        let mut newly_indexed_count = 0usize;

        for (rel_path, full_path) in &disk_files {
            let content = match fs::read_to_string(full_path) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read {}: {}", rel_path, e);
                    continue;
                }
            };
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();

            // If resume is enabled and file is already indexed with matching hash, skip parsing/indexing!
            if resume {
                if let Some(stored_hash) = stored_map.get(rel_path) {
                    if *stored_hash == hash {
                        // Document already indexed and unchanged
                        continue;
                    }
                }
            }

            // Staged indexing (handles both markdown and polyglot code files, returning chunks for batched embedding)
            let (chunks, maybe_doc) = self.index_file_staged(rel_path, &content)?;
            if let Some(doc) = maybe_doc {
                all_docs.push(doc);
            }

            // Stream anchor chunks to the async GPU pipeline and poll any completed batches
            if let Some(ref pipeline) = embedding_pipeline {
                for chunk in chunks {
                    if chunk.embed_policy == ChunkEmbedPolicy::Anchor {
                        pipeline.send(chunk)?;
                    }
                }
                if let Some(ref mut vi) = self.vector_index {
                    pipeline.try_recv_completed(vi)?;
                }
            }

            processed_in_current_batch += 1;
            newly_indexed_count += 1;
            state.indexed_files += 1;
            state.last_processed_path = Some(rel_path.clone());

            // Check if batch is full -> commit checkpoint!
            // Notice: SQLite and Tantivy commit immediately without blocking on GPU forward pass!
            if processed_in_current_batch >= batch_size {
                if let Some(ref pipeline) = embedding_pipeline {
                    if let Some(ref mut vi) = self.vector_index {
                        pipeline.try_recv_completed(vi)?;
                    }
                }
                self.commit()?;
                state.updated_at = now_unix();
                self.store.update_indexing_state(&state)?;
                debug!(
                    "Committed batch of {} files ({}/{} total)",
                    processed_in_current_batch, state.indexed_files, total_files
                );
                processed_in_current_batch = 0;
            }
        }

        // Finish embedding pipeline: drains all remaining in-flight batches, joins threads,
        // and inserts completed embeddings into self.vector_index.
        if let Some(mut pipeline) = embedding_pipeline {
            if let Some(ref mut vi) = self.vector_index {
                pipeline.finish(vi)?;
            }
        }

        // Commit any remaining files in final batch
        if processed_in_current_batch > 0 {
            self.commit()?;
        }

        // Second pass: build tag-based edges with all documents available.
        let tag_configs: Vec<_> = self
            .config
            .graph
            .edge_types
            .iter()
            .filter(|et| et.source == ctxvault_common::config::EdgeSource::Tag)
            .cloned()
            .collect();

        if !tag_configs.is_empty() && !all_docs.is_empty() {
            self.graph.build_all_tag_edges(&tag_configs, &all_docs);
        }

        // Final commit and update state to Completed
        self.commit()?;
        state.status = IndexingStatus::Completed;
        state.updated_at = now_unix();
        state.indexed_files = total_files;
        self.store.update_indexing_state(&state)?;

        info!(
            "Paginated indexing complete: {} new/updated files ({} total files)",
            newly_indexed_count, total_files
        );

        Ok(total_files)
    }

    /// Commit all pending changes (Tantivy commit, graph save, vector index save).
    pub fn commit(&mut self) -> Result<()> {
        self.bm25.commit()?;
        self.graph.save(&self.index_dir.join("graph.bin"))?;
        // Save vector index (only if it has data and has unpersisted changes).
        if let Some(ref vi) = self.vector_index {
            if vi.is_dirty() && !vi.is_empty() {
                vi.save(&self.index_dir.join("vectors.json")).unwrap_or_else(|e| {
                    warn!("Failed to save vector index: {}", e);
                });
            }
        }
        Ok(())
    }

    /// Get a reference to the BM25 index for searching.
    pub fn bm25(&self) -> &BM25Index {
        &self.bm25
    }

    /// Get a reference to the vector index for semantic search (None if in Fast Mode).
    pub fn vector_index(&self) -> Option<&VectorIndex> {
        self.vector_index.as_ref()
    }

    /// Check whether the engine is running in Fast Mode.
    pub fn is_fast_mode(&self) -> bool {
        self.config.index_mode == ctxvault_common::config::IndexMode::Fast
    }

    /// Get an Arc reference to the embedder (if initialized).
    pub fn embedder_ref(&self) -> Option<Arc<Embedder>> {
        self.embedder.read().unwrap().clone()
    }

    /// Get current indexing progress and throughput statistics.
    pub fn get_indexing_status(&self) -> Result<IndexingStatusResponse> {
        let corpus_id = &self.config.name;
        let stored = self.store.get_indexing_state(corpus_id)?;
        let now = now_unix();

        if let Some(state) = stored {
            let total = state.total_files;
            let indexed = state.indexed_files;
            let progress_percent = if total > 0 {
                ((indexed as f64 / total as f64) * 100.0).min(100.0)
            } else if state.status == IndexingStatus::Completed {
                100.0
            } else {
                0.0
            };

            let elapsed = if state.status == IndexingStatus::Indexing {
                now.saturating_sub(state.started_at)
            } else {
                state.updated_at.saturating_sub(state.started_at)
            };

            let throughput = if elapsed > 0 { indexed as f64 / elapsed as f64 } else { 0.0 };

            let remaining_files = total.saturating_sub(indexed);
            let time_remaining = if throughput > 0.0 && state.status == IndexingStatus::Indexing {
                remaining_files as f64 / throughput
            } else {
                0.0
            };

            Ok(IndexingStatusResponse {
                corpus_id: state.corpus_id,
                status: state.status,
                total_files: total,
                indexed_files: indexed,
                progress_percent: (progress_percent * 100.0).round() / 100.0,
                last_processed_path: state.last_processed_path,
                started_at: state.started_at,
                updated_at: state.updated_at,
                elapsed_seconds: elapsed,
                estimated_throughput_docs_per_sec: (throughput * 100.0).round() / 100.0,
                estimated_time_remaining_seconds: (time_remaining * 100.0).round() / 100.0,
                error_message: state.error_message,
            })
        } else {
            // No indexing state recorded yet: check store files
            let count = self.store.list_files().map(|f| f.len()).unwrap_or(0);
            Ok(IndexingStatusResponse {
                corpus_id: corpus_id.clone(),
                status: if count > 0 { IndexingStatus::Completed } else { IndexingStatus::Idle },
                total_files: count,
                indexed_files: count,
                progress_percent: if count > 0 { 100.0 } else { 0.0 },
                last_processed_path: None,
                started_at: 0,
                updated_at: 0,
                elapsed_seconds: 0,
                estimated_throughput_docs_per_sec: 0.0,
                estimated_time_remaining_seconds: 0.0,
                error_message: None,
            })
        }
    }

    /// Get a reference to the graph for traversal/queries.
    pub fn graph(&self) -> &KnowledgeGraph {
        &self.graph
    }

    /// Get a mutable reference to the graph for manipulation.
    pub fn graph_mut(&mut self) -> &mut KnowledgeGraph {
        &mut self.graph
    }

    /// Build the set of graph node keys that represent code entities.
    ///
    /// Used by the search layer to classify a result path as code vs docs for
    /// modality filtering. Keys include each code symbol's `scope_path`, its
    /// defining file path, and the `<corpus>::scope_path` cross-corpus form.
    /// Returns an empty set on catalog read error (all paths then classify as
    /// docs, which is the safe default).
    pub fn code_paths_set(&self) -> std::collections::HashSet<String> {
        let mut set = std::collections::HashSet::new();
        let corpus = &self.config.name;
        if let Ok(symbols) = self.store.get_all_code_symbols() {
            for sym in symbols {
                let _ = set.insert(sym.scope_path.clone());
                let _ = set.insert(sym.file_path.clone());
                let _ = set.insert(format!("{}::{}", corpus, sym.scope_path));
            }
        }
        set
    }

    /// Get a reference to the store for metadata queries.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Get the corpus config.
    pub fn config(&self) -> &CorpusConfig {
        &self.config
    }

    /// Get a mutable reference to the corpus config.
    pub fn config_mut(&mut self) -> &mut CorpusConfig {
        &mut self.config
    }

    /// Check whether vectors are stale (model version mismatch).
    pub fn vectors_stale(&self) -> bool {
        self.vector_index.as_ref().map(|vi| vi.is_stale()).unwrap_or(false)
    }

    /// Check whether the corpus has been indexed (has any files in the store).
    pub fn is_indexed(&self) -> bool {
        self.store.list_files().map(|f| !f.is_empty()).unwrap_or(false)
    }

    /// Get the model version stored in the vector index.
    pub fn stored_model_version(&self) -> Option<&str> {
        self.vector_index.as_ref().and_then(|vi| vi.model_version())
    }

    /// Get a mutable reference to the vector index.
    pub fn vector_index_mut(&mut self) -> Option<&mut VectorIndex> {
        self.vector_index.as_mut()
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
        if self.is_fast_mode() || self.vector_index.is_none() {
            return Err(Error::Index(
                "re-embedding is unavailable in fast mode. Re-index with index_mode = 'full'"
                    .to_string(),
            ));
        }

        // 1. Ensure embedder is available.
        let available = self.ensure_embedder()?;
        if !available {
            return Err(Error::Index("embedder not available — cannot re-embed".to_string()));
        }
        let embedder = self.embedder_ref().unwrap();

        // 2. Get all files and their chunks from the store.
        let files = self.store.list_files()?;

        // 3. Reset vector index (preserve dimensions and params).
        let dims = self.vector_index.as_ref().unwrap().dimensions();
        self.vector_index = Some(VectorIndex::new_default(dims));

        // 4. Re-embed all chunks.
        let mut total_chunks = 0usize;
        let mut chunk_buffer: Vec<PendingChunk> = Vec::new();

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
                    .map(|cr| {
                        ctxvault_common::types::Chunk::new(
                            file.path.clone(),
                            cr.chunk_index,
                            cr.text,
                            cr.start_byte,
                            cr.end_byte,
                        )
                    })
                    .collect();
                (chunks, file.title.clone())
            };

            if chunks.is_empty() {
                continue;
            }

            let doc_title = title.as_deref().unwrap_or("").trim();
            for c in &chunks {
                let section = c.heading_chain.as_deref().unwrap_or("").trim();
                let text = if !doc_title.is_empty() && !section.is_empty() {
                    format!("{} > {}: {}", doc_title, section, c.text)
                } else if !doc_title.is_empty() {
                    format!("{}: {}", doc_title, c.text)
                } else if !section.is_empty() {
                    format!("{}: {}", section, c.text)
                } else {
                    c.text.clone()
                };
                let modality = c
                    .entity_kind
                    .as_ref()
                    .map(EntityKind::modality_tag)
                    .unwrap_or("docs")
                    .to_string();
                chunk_buffer.push(PendingChunk {
                    doc_path: file.path.clone(),
                    chunk_index: c.chunk_index,
                    text,
                    embed_policy: c.embed_policy,
                    modality,
                });
            }

            if chunk_buffer.len() >= 64 {
                self.flush_chunk_buffer(&chunk_buffer)?;
                chunk_buffer.clear();
            }

            total_chunks += chunks.len();
        }

        // Flush any remaining buffered chunks
        if !chunk_buffer.is_empty() {
            self.flush_chunk_buffer(&chunk_buffer)?;
            chunk_buffer.clear();
        }

        // 5. Update model version metadata.
        let model_version = embedder.model_name().version_string().to_string();
        if let Some(ref mut vi) = self.vector_index {
            vi.set_model_version(&model_version);
            vi.clear_stale();
        }

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
            // Skip hidden directories and common build/dependency artifacts
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.')
                    || name == "target"
                    || name == "node_modules"
                    || name == "dist"
                    || name == "build"
                    || name == "venv"
                    || name == ".venv"
                {
                    continue;
                }
            }
            walk_dir_recursive(root, &path, results)?;
        } else {
            let is_md = path.extension().and_then(|e| e.to_str()) == Some("md");
            let is_code = crate::parser::code::is_code_file(&path);
            if is_md || is_code {
                let rel = path.strip_prefix(root).map_err(|e| {
                    Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                })?;
                // Normalize path separators to forward slashes.
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                results.push((rel_str, path.clone()));
            }
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
    use ctxvault_common::types::CodeSymbolType;
    use tempfile::TempDir;

    /// Create a minimal corpus config pointing at the given path.
    fn test_config(corpus_path: &Path) -> CorpusConfig {
        CorpusConfig {
            name: "test".to_string(),
            path: corpus_path.to_string_lossy().to_string(),
            mode: ctxvault_common::config::CorpusMode::ReadWrite,
            index_mode: ctxvault_common::config::IndexMode::Full,
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
    fn test_fast_mode_skips_vectors_and_embedder() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        fs::write(corpus_dir.join("file1.md"), "# File 1\nSome test markdown content").unwrap();

        let mut config = test_config(&corpus_dir);
        config.index_mode = ctxvault_common::config::IndexMode::Fast;

        let index_dir = tmp.path().join("index");
        let mut engine = Engine::open(config, &index_dir).unwrap();
        assert!(engine.is_fast_mode());
        assert!(engine.vector_index().is_none());
        assert_eq!(engine.ensure_embedder().unwrap(), false);

        let files = engine.full_reindex_paginated(10, false).unwrap();
        assert_eq!(files, 1);
        assert!(engine.vector_index().is_none());
        assert_eq!(engine.store().list_files().unwrap().len(), 1);
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
                "vector": vec![0.1f32; 768]
            }],
            "next_id": 1,
            "dimensions": 768,
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
                "vector": vec![0.1f32; 768]
            }],
            "next_id": 1,
            "dimensions": 768,
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

    #[test]
    fn test_paginated_reindex_and_status() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        // Write 15 files
        for i in 0..15 {
            fs::write(
                corpus_dir.join(format!("doc_{:02}.md", i)),
                format!("# Document {}\n\nContent for note {}\n", i, i),
            )
            .unwrap();
        }

        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        // Index in batches of 5
        let count = engine.full_reindex_paginated(5, false).unwrap();
        assert_eq!(count, 15);

        // Check indexing status
        let status = engine.get_indexing_status().unwrap();
        assert_eq!(status.corpus_id, "test");
        assert_eq!(status.status, IndexingStatus::Completed);
        assert_eq!(status.total_files, 15);
        assert_eq!(status.indexed_files, 15);
        assert_eq!(status.progress_percent, 100.0);
    }

    #[test]
    fn test_indexing_resumption() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(&corpus_dir).unwrap();
        let index_dir = tmp.path().join("index");

        // Write 10 files
        for i in 0..10 {
            fs::write(
                corpus_dir.join(format!("doc_{:02}.md", i)),
                format!("# Document {}\n\nContent for note {}\n", i, i),
            )
            .unwrap();
        }

        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config.clone(), &index_dir).unwrap();

        // 1. First index all 10 files
        let count = engine.full_reindex_paginated(4, false).unwrap();
        assert_eq!(count, 10);

        // 2. Add 5 more files to corpus
        for i in 10..15 {
            fs::write(
                corpus_dir.join(format!("doc_{:02}.md", i)),
                format!("# Document {}\n\nContent for note {}\n", i, i),
            )
            .unwrap();
        }

        // 3. Open fresh engine instance and resume indexing
        let mut resumed_engine = Engine::open(config, &index_dir).unwrap();
        let resumed_count = resumed_engine.full_reindex_paginated(4, true).unwrap();
        assert_eq!(resumed_count, 15);

        let files = resumed_engine.store().list_files().unwrap();
        assert_eq!(files.len(), 15);
    }

    #[test]
    fn test_polyglot_codebase_indexing_and_cross_modal_search() {
        let tmp = TempDir::new().unwrap();
        let corpus_dir = tmp.path().join("corpus");
        fs::create_dir_all(corpus_dir.join("docs/adr")).unwrap();
        fs::create_dir_all(corpus_dir.join("src")).unwrap();
        fs::create_dir_all(corpus_dir.join("scripts")).unwrap();
        let index_dir = tmp.path().join("index");

        // 1. Write markdown ADR
        let adr_content = r#"---
title: ADR-0001 Hybrid Search
tags: [search, rrf, architecture]
---
# ADR-0001: Reciprocal Rank Fusion Search

We implement 4-way RRF hybrid search combining BM25, embeddings, and graph traversal.
"#;
        fs::write(corpus_dir.join("docs/adr/0001-hybrid-search.md"), adr_content).unwrap();

        // 2. Write Rust file
        let rust_code = r#"
/// Search engine implementation
pub struct Engine;

impl Engine {
    /// Execute hybrid search across all modalities
    pub fn search_hybrid(&self, query: &str) -> Vec<String> {
        let results = execute_rrf(query);
        results
    }
}

pub fn execute_rrf(q: &str) -> Vec<String> {
    vec![q.to_string()]
}
"#;
        fs::write(corpus_dir.join("src/search.rs"), rust_code).unwrap();

        // 3. Write TypeScript file
        let ts_code = r#"
export interface UserProfile {
    id: string;
    email: string;
}

export class UserService {
    /** Fetch user by ID */
    async getUser(id: string): Promise<UserProfile> {
        return { id, email: "user@example.com" };
    }
}
"#;
        fs::write(corpus_dir.join("src/user.ts"), ts_code).unwrap();

        // 4. Write Python script
        let py_code = r#"
class DataIngest:
    """Batch data ingestion pipeline."""
    def run_pipeline(self, batch):
        return len(batch)
"#;
        fs::write(corpus_dir.join("scripts/process.py"), py_code).unwrap();

        let config = test_config(&corpus_dir);
        let mut engine = Engine::open(config, &index_dir).unwrap();

        // Perform full reindex
        let count = engine.full_reindex().unwrap();
        assert_eq!(count, 4, "Should index 1 markdown file + 3 polyglot code files");

        // Verify BM25 search across modalities
        let adr_hits = engine.bm25().search("Reciprocal Rank Fusion", 5).unwrap();
        assert!(!adr_hits.is_empty());
        assert_eq!(adr_hits[0].path, "docs/adr/0001-hybrid-search.md");

        let rust_hits = engine.bm25().search("search_hybrid modalities", 5).unwrap();
        assert!(!rust_hits.is_empty());
        assert_eq!(rust_hits[0].path, "src/search.rs");

        let ts_hits = engine.bm25().search("UserProfile getUser", 5).unwrap();
        assert!(!ts_hits.is_empty());
        assert_eq!(ts_hits[0].path, "src/user.ts");

        // Verify SQLite code_symbols catalog
        let rust_symbols = engine.store().get_code_symbols_for_file("src/search.rs").unwrap();
        assert!(rust_symbols
            .iter()
            .any(|s| s.name == "Engine" && s.symbol_type == CodeSymbolType::Struct));
        assert!(rust_symbols
            .iter()
            .any(|s| s.name == "search_hybrid" && s.symbol_type == CodeSymbolType::Function));
        assert!(rust_symbols
            .iter()
            .any(|s| s.name == "execute_rrf" && s.symbol_type == CodeSymbolType::Function));

        let ts_symbols = engine.store().get_code_symbols_for_file("src/user.ts").unwrap();
        assert!(ts_symbols
            .iter()
            .any(|s| s.name == "UserService" && s.symbol_type == CodeSymbolType::Class));
        assert!(ts_symbols
            .iter()
            .any(|s| s.name == "getUser" && s.symbol_type == CodeSymbolType::Method));

        // Verify graph edges (defines and calls)
        let edges = engine.graph().get_all_edges();
        assert!(edges.iter().any(|e| e.edge_type == "defines"
            && e.source == "src/search.rs"
            && e.target == "Engine"));
        assert!(edges.iter().any(|e| e.edge_type == "calls"
            && e.source == "Engine > search_hybrid"
            && e.target == "execute_rrf"));
    }
}
