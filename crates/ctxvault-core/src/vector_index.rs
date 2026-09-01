//! Vector index: HNSW-based approximate nearest neighbor search.
//!
//! Wraps `hnsw_rs` to provide add/remove/search/save/load operations
//! for embedding vectors. Supports both chunk-level and document-level vectors.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use ctxvault_common::{Error, Result};
use hnsw_rs::prelude::*;
use serde::{Deserialize, Serialize};

/// Default number of dimensions for Jina embeddings (768).
pub const DEFAULT_DIMENSIONS: usize = 768;

/// Metadata about a stored vector, mapping HNSW internal IDs to documents/chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMeta {
    /// Document path this vector belongs to.
    pub doc_path: String,
    /// Chunk index within the document (None for document-level embeddings).
    pub chunk_index: Option<usize>,
    /// Whether this is a document-level embedding (vs chunk-level).
    pub is_doc_level: bool,
}

/// A stored vector entry for persistence (metadata + raw vector data).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredVector {
    id: usize,
    meta: VectorMeta,
    vector: Vec<f32>,
}

/// HNSW-based vector index for approximate nearest neighbor search.
pub struct VectorIndex {
    /// The HNSW graph structure.
    hnsw: Hnsw<'static, f32, DistCosine>,
    /// Mapping from external data ID to vector metadata.
    meta: HashMap<usize, VectorMeta>,
    /// Stored vectors for persistence and rebuild.
    vectors: HashMap<usize, Vec<f32>>,
    /// Next available external ID.
    next_id: usize,
    /// Number of dimensions per vector.
    dimensions: usize,
    /// HNSW construction parameters for rebuild.
    max_nb_connection: usize,
    ef_construction: usize,
    /// Model version that produced these embeddings.
    model_version: Option<String>,
    /// Whether vectors are stale (model version mismatch detected).
    stale: bool,
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
}

/// Persistence format for the entire vector index.
#[derive(Serialize, Deserialize)]
struct PersistenceData {
    entries: Vec<StoredVector>,
    next_id: usize,
    dimensions: usize,
    max_nb_connection: usize,
    ef_construction: usize,
    /// Model version that produced these embeddings (added in 3.10).
    #[serde(default)]
    model_version: Option<String>,
}

impl VectorIndex {
    /// Create a new in-memory vector index.
    ///
    /// - `dimensions`: vector dimensionality (e.g., 384 for MiniLM-L6-v2)
    /// - `max_elements`: estimated maximum number of vectors (can grow)
    /// - `ef_construction`: HNSW construction parameter (higher = more accurate, slower build)
    /// - `max_nb_connection`: max neighbors per node in HNSW graph
    pub fn new(
        dimensions: usize,
        max_elements: usize,
        ef_construction: usize,
        max_nb_connection: usize,
    ) -> Self {
        let hnsw = Hnsw::<f32, DistCosine>::new(
            max_nb_connection,
            max_elements,
            16, // max_layer
            ef_construction,
            DistCosine,
        );

        Self {
            hnsw,
            meta: HashMap::new(),
            vectors: HashMap::new(),
            next_id: 0,
            dimensions,
            max_nb_connection,
            ef_construction,
            model_version: None,
            stale: false,
        }
    }

    /// Create a new vector index with default parameters suitable for small-medium corpora.
    pub fn new_default(dimensions: usize) -> Self {
        Self::new(dimensions, 10_000, 200, 16)
    }

    /// Get the number of vectors currently in the index.
    pub fn len(&self) -> usize {
        self.meta.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.meta.is_empty()
    }

    /// Get the dimensionality of vectors in this index.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get the model version stored with this index.
    pub fn model_version(&self) -> Option<&str> {
        self.model_version.as_deref()
    }

    /// Set the model version for this index.
    pub fn set_model_version(&mut self, version: &str) {
        self.model_version = Some(version.to_string());
    }

    /// Check whether vectors are marked as stale (model version mismatch).
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// Mark vectors as stale (model version mismatch detected).
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Clear the stale flag (after re-embedding completes).
    pub fn clear_stale(&mut self) {
        self.stale = false;
    }

    /// Add a single vector to the index.
    ///
    /// Returns the internal ID assigned to this vector.
    pub fn add(
        &mut self,
        vector: &[f32],
        doc_path: &str,
        chunk_index: Option<usize>,
        is_doc_level: bool,
    ) -> Result<usize> {
        if vector.len() != self.dimensions {
            return Err(Error::Index(format!(
                "vector dimension mismatch: expected {}, got {}",
                self.dimensions,
                vector.len()
            )));
        }

        let id = self.next_id;
        self.next_id += 1;

        // Insert into HNSW. The insert_slice method takes a tuple (&[T], DataId).
        self.hnsw.insert_slice((&vector, id));

        let meta = VectorMeta { doc_path: doc_path.to_string(), chunk_index, is_doc_level };

        // Store metadata and vector data.
        let _ = self.meta.insert(id, meta);
        let _ = self.vectors.insert(id, vector.to_vec());

        Ok(id)
    }

    /// Add multiple vectors in batch (more efficient than individual adds).
    ///
    /// Returns the internal IDs assigned.
    pub fn add_batch(
        &mut self,
        vectors: &[Vec<f32>],
        doc_path: &str,
        chunk_indices: &[Option<usize>],
        is_doc_level: bool,
    ) -> Result<Vec<usize>> {
        if vectors.len() != chunk_indices.len() {
            return Err(Error::Index(
                "vectors and chunk_indices must have same length".to_string(),
            ));
        }

        let mut ids = Vec::with_capacity(vectors.len());

        for (vec, &chunk_idx) in vectors.iter().zip(chunk_indices.iter()) {
            let id = self.add(vec, doc_path, chunk_idx, is_doc_level)?;
            ids.push(id);
        }

        Ok(ids)
    }

    /// Remove all vectors for a given document path.
    ///
    /// Note: HNSW doesn't support true deletion, so we remove from metadata
    /// and stored vectors. The HNSW graph entries become stale but are filtered
    /// out during search. A rebuild (save+load) compacts the index.
    pub fn remove_document(&mut self, doc_path: &str) {
        let ids_to_remove: Vec<usize> =
            self.meta.iter().filter(|(_, m)| m.doc_path == doc_path).map(|(&id, _)| id).collect();

        for id in ids_to_remove {
            let _ = self.meta.remove(&id);
            let _ = self.vectors.remove(&id);
        }
    }

    /// Search for the K nearest neighbors to a query vector.
    ///
    /// - `query`: the query embedding vector
    /// - `k`: number of results to return
    /// - `doc_level_only`: if true, only return document-level embeddings
    ///
    /// Returns results sorted by descending similarity score.
    pub fn search(
        &self,
        query: &[f32],
        k: usize,
        doc_level_only: bool,
    ) -> Result<Vec<VectorSearchResult>> {
        if query.len() != self.dimensions {
            return Err(Error::Index(format!(
                "query dimension mismatch: expected {}, got {}",
                self.dimensions,
                query.len()
            )));
        }

        if self.meta.is_empty() || k == 0 {
            return Ok(Vec::new());
        }

        // Over-fetch neighbors to account for multiple chunks per document and filtered entries.
        let fetch_k = (k * 10).max(64);
        let ef_search = (k * 10).max(64);

        let neighbours = self.hnsw.search(query, fetch_k, ef_search);

        let mut candidate_results: Vec<VectorSearchResult> = neighbours
            .into_iter()
            .filter_map(|neighbour| {
                let id = neighbour.d_id;
                let meta = self.meta.get(&id)?;

                // Filter by level if requested.
                if doc_level_only && !meta.is_doc_level {
                    return None;
                }

                // DistCosine returns 1 - cos(a,b), so similarity = 1 - distance.
                let similarity = 1.0 - neighbour.distance as f64;

                Some(VectorSearchResult {
                    doc_path: meta.doc_path.clone(),
                    chunk_index: meta.chunk_index,
                    score: similarity,
                    is_doc_level: meta.is_doc_level,
                })
            })
            .collect();

        // Sort by descending score (highest similarity first).
        candidate_results
            .sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Document-level max pooling: retain only the highest scoring chunk per document.
        let mut seen_docs = HashSet::new();
        let mut results = Vec::with_capacity(k);
        for item in candidate_results {
            if seen_docs.insert(item.doc_path.clone()) {
                results.push(item);
                if results.len() >= k {
                    break;
                }
            }
        }

        Ok(results)
    }

    /// Save the index to disk as a JSON file with all vectors and metadata.
    ///
    /// On reload, the HNSW graph is rebuilt from stored vectors.
    /// This approach avoids lifetime complications with `HnswIo`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|e| Error::Index(format!("cannot create vector index dir: {}", e)))?;

        let entries: Vec<StoredVector> = self
            .meta
            .iter()
            .filter_map(|(&id, meta)| {
                let vector = self.vectors.get(&id)?.clone();
                Some(StoredVector { id, meta: meta.clone(), vector })
            })
            .collect();

        let data = PersistenceData {
            entries,
            next_id: self.next_id,
            dimensions: self.dimensions,
            max_nb_connection: self.max_nb_connection,
            ef_construction: self.ef_construction,
            model_version: self.model_version.clone(),
        };

        let json = serde_json::to_string(&data)
            .map_err(|e| Error::Index(format!("cannot serialize vector index: {}", e)))?;
        fs::write(path, json)
            .map_err(|e| Error::Index(format!("cannot write vector index: {}", e)))?;

        Ok(())
    }

    /// Load a previously saved index from disk and rebuild the HNSW graph.
    ///
    /// Returns a new `VectorIndex` with the restored state.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(Error::Index(format!("vector index file not found at {}", path.display())));
        }

        let json = fs::read_to_string(path)
            .map_err(|e| Error::Index(format!("cannot read vector index: {}", e)))?;
        let data: PersistenceData = serde_json::from_str(&json)
            .map_err(|e| Error::Index(format!("cannot parse vector index: {}", e)))?;

        let max_elements = data.entries.len().max(100);
        let hnsw = Hnsw::<f32, DistCosine>::new(
            data.max_nb_connection,
            max_elements,
            16,
            data.ef_construction,
            DistCosine,
        );

        let mut meta = HashMap::new();
        let mut vectors = HashMap::new();

        for entry in data.entries {
            hnsw.insert_slice((&entry.vector, entry.id));
            let _ = meta.insert(entry.id, entry.meta);
            let _ = vectors.insert(entry.id, entry.vector);
        }

        Ok(Self {
            hnsw,
            meta,
            vectors,
            next_id: data.next_id,
            dimensions: data.dimensions,
            max_nb_connection: data.max_nb_connection,
            ef_construction: data.ef_construction,
            model_version: data.model_version,
            stale: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a deterministic vector with a specific seed pattern, L2-normalized.
    fn make_vector(seed: usize, dims: usize) -> Vec<f32> {
        let v: Vec<f32> = (0..dims).map(|i| ((seed * 7 + i * 13) % 100) as f32 / 100.0).collect();
        // L2-normalize for cosine distance.
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter().map(|x| x / norm).collect()
        } else {
            v
        }
    }

    /// Generate a vector that's similar to another (small perturbation).
    fn make_similar_vector(base: &[f32], offset: f32) -> Vec<f32> {
        let v: Vec<f32> = base.iter().map(|&val| val + offset * 0.01).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter().map(|x| x / norm).collect()
        } else {
            v
        }
    }

    #[test]
    fn test_create_empty_index() {
        let index = VectorIndex::new_default(384);
        assert_eq!(index.len(), 0);
        assert!(index.is_empty());
        assert_eq!(index.dimensions(), 384);
    }

    #[test]
    fn test_add_single_vector() {
        let mut index = VectorIndex::new_default(384);
        let vec = make_vector(1, 384);

        let id = index.add(&vec, "notes/test.md", Some(0), false).unwrap();
        assert_eq!(id, 0);
        assert_eq!(index.len(), 1);
        assert!(!index.is_empty());
    }

    #[test]
    fn test_add_batch() {
        let mut index = VectorIndex::new_default(384);

        let vectors: Vec<Vec<f32>> = (0..5).map(|i| make_vector(i, 384)).collect();
        let chunk_indices: Vec<Option<usize>> = (0..5).map(Some).collect();

        let ids = index.add_batch(&vectors, "notes/multi.md", &chunk_indices, false).unwrap();

        assert_eq!(ids.len(), 5);
        assert_eq!(index.len(), 5);
    }

    #[test]
    fn test_dimension_mismatch_rejected() {
        let mut index = VectorIndex::new_default(384);
        let wrong_vec = make_vector(1, 256); // Wrong dimension.

        let result = index.add(&wrong_vec, "notes/bad.md", Some(0), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_finds_similar() {
        let mut index = VectorIndex::new(384, 100, 200, 16);

        // Add a base vector and some others.
        let base = make_vector(42, 384);
        let similar = make_similar_vector(&base, 1.0);
        let different = make_vector(999, 384);

        index.add(&base, "notes/base.md", Some(0), false).unwrap();
        index.add(&similar, "notes/similar.md", Some(0), false).unwrap();
        index.add(&different, "notes/different.md", Some(0), false).unwrap();

        // Search with the base vector — should find itself and similar.
        let results = index.search(&base, 3, false).unwrap();
        assert!(!results.is_empty());

        // The base vector should be the top result (exact match = highest similarity).
        assert_eq!(results[0].doc_path, "notes/base.md");

        // Scores should be in descending order.
        for window in results.windows(2) {
            assert!(window[0].score >= window[1].score);
        }
    }

    #[test]
    fn test_search_empty_index() {
        let index = VectorIndex::new_default(384);
        let query = make_vector(1, 384);

        let results = index.search(&query, 10, false).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_dimension_mismatch() {
        let mut index = VectorIndex::new_default(384);
        let vec = make_vector(1, 384);
        index.add(&vec, "notes/a.md", Some(0), false).unwrap();

        let bad_query = make_vector(1, 256);
        let result = index.search(&bad_query, 10, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_document() {
        let mut index = VectorIndex::new(384, 100, 200, 16);

        let v1 = make_vector(1, 384);
        let v2 = make_vector(2, 384);
        let v3 = make_vector(3, 384);

        index.add(&v1, "notes/keep.md", Some(0), false).unwrap();
        index.add(&v2, "notes/remove.md", Some(0), false).unwrap();
        index.add(&v3, "notes/remove.md", Some(1), false).unwrap();

        assert_eq!(index.len(), 3);

        index.remove_document("notes/remove.md");
        assert_eq!(index.len(), 1);

        // Search should not return removed documents.
        let results = index.search(&v2, 10, false).unwrap();
        for r in &results {
            assert_ne!(r.doc_path, "notes/remove.md");
        }
    }

    #[test]
    fn test_doc_level_filter() {
        let mut index = VectorIndex::new(384, 100, 200, 16);

        let v1 = make_vector(1, 384);
        let v2 = make_vector(2, 384);

        // Add chunk-level and doc-level vectors.
        index.add(&v1, "notes/a.md", Some(0), false).unwrap();
        index.add(&v2, "notes/a.md", None, true).unwrap();

        // Search with doc_level_only = true should only return doc-level.
        let results = index.search(&v2, 10, true).unwrap();
        for r in &results {
            assert!(r.is_doc_level);
        }
    }

    #[test]
    fn test_save_and_load() {
        let tmp = tempfile::TempDir::new().unwrap();
        let index_path = tmp.path().join("vectors.json");

        // Create and populate an index.
        let mut index = VectorIndex::new(384, 100, 200, 16);
        let v1 = make_vector(10, 384);
        let v2 = make_vector(20, 384);
        let v3 = make_vector(30, 384);

        index.add(&v1, "notes/alpha.md", Some(0), false).unwrap();
        index.add(&v2, "notes/beta.md", Some(0), false).unwrap();
        index.add(&v3, "notes/alpha.md", None, true).unwrap();

        // Save to disk.
        index.save(&index_path).unwrap();

        // Load from disk.
        let loaded = VectorIndex::load(&index_path).unwrap();

        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded.dimensions(), 384);

        // Search should work on loaded index.
        let results = loaded.search(&v1, 3, false).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_path, "notes/alpha.md");
    }

    #[test]
    fn test_search_document_deduplication() {
        let mut index = VectorIndex::new(384, 100, 200, 16);
        let base = make_vector(1, 384);
        let chunk0 = make_similar_vector(&base, 1.0);
        let chunk1 = make_similar_vector(&base, 0.1); // Closer to base
        let other = make_vector(20, 384);

        // Add 2 chunks for doc A and 1 for doc B
        index.add(&chunk0, "notes/doc_a.md", Some(0), false).unwrap();
        index.add(&chunk1, "notes/doc_a.md", Some(1), false).unwrap();
        index.add(&other, "notes/doc_b.md", Some(0), false).unwrap();

        let results = index.search(&base, 5, false).unwrap();

        // doc_a should appear only once (with chunk 1 which is closer)
        let doc_a_results: Vec<_> =
            results.iter().filter(|r| r.doc_path == "notes/doc_a.md").collect();
        assert_eq!(doc_a_results.len(), 1);
        assert_eq!(doc_a_results[0].chunk_index, Some(1));
    }
}
