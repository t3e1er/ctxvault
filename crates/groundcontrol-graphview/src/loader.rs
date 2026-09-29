//! Read-only snapshot loader for `groundcontrol` corpus indices on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use groundcontrol_common::config::{get_corpora_cache_dir, get_corpus_index_dir};
use groundcontrol_core::graph::KnowledgeGraph;
use tracing::{info, warn};

use crate::error::{GraphViewError, Result};

/// In-memory read-only snapshot of a single corpus graph.
pub struct CorpusSnapshot {
    /// Name of the corpus.
    pub name: String,
    /// Absolute path to corpus index directory.
    pub index_dir: PathBuf,
    /// Absolute path to the original source root directory on disk, if known.
    pub root_dir: Option<PathBuf>,
    /// Loaded petgraph knowledge graph.
    pub graph: KnowledgeGraph,
    /// Authoritative AST-derived symbol types from meta.db (key -> symbol_type).
    pub ast_types: Arc<HashMap<String, String>>,
    /// Pre-computed path → community-id mapping from Leiden detection at load time.
    /// Used by Tier 2 ego subgraph to inherit full community coloring without re-running detection.
    pub community_map: Arc<HashMap<String, u32>>,
    /// Modification time of `graph.bin` when this snapshot was loaded.
    /// Surfaced in `/api/status` for cache-staleness detection.
    pub graph_mtime: SystemTime,
}

impl CorpusSnapshot {
    /// Load a corpus snapshot from disk in read-only mode.
    pub fn load_from_dir(name: &str, index_dir: &Path) -> Result<Self> {
        let graph_path = index_dir.join("graph.bin");
        if !graph_path.exists() {
            return Err(GraphViewError::NotFound(format!(
                "No graph.bin found in {}",
                index_dir.display()
            )));
        }

        let graph = KnowledgeGraph::load(&graph_path)
            .map_err(|e| GraphViewError::GraphLoad(format!("{}: {}", name, e)))?;

        // Extract AST-derived entity classes and source root from SQLite catalog meta.db
        let mut ast_types = HashMap::new();
        let mut root_dir = None;
        let meta_path = index_dir.join("meta.db");
        if meta_path.exists() {
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &meta_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            ) {
                if let Ok(mut stmt) = conn.prepare("SELECT value FROM corpus_config WHERE key = 'corpus_config'") {
                    if let Ok(mut rows) = stmt.query([]) {
                        if let Ok(Some(row)) = rows.next() {
                            if let Ok(val_str) = row.get::<_, String>(0) {
                                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&val_str) {
                                    if let Some(p) = parsed.get("path").and_then(|v| v.as_str()) {
                                        let clean = p.trim_start_matches("//?/").trim_start_matches(r"\\?\");
                                        root_dir = Some(PathBuf::from(clean));
                                    }
                                }
                            }
                        }
                    }
                }

                if let Ok(mut stmt) = conn
                    .prepare("SELECT name, scope_path, file_path, symbol_type FROM code_symbols")
                {
                    if let Ok(rows) = stmt.query_map([], |row| {
                        let name: String = row.get(0)?;
                        let scope: String = row.get(1)?;
                        let file: String = row.get(2)?;
                        let sym_type: String = row.get(3)?;
                        Ok((name, scope, file, sym_type))
                    }) {
                        for row in rows.flatten() {
                            let (name, scope, file, sym_type) = row;
                            let norm_file = file.replace('\\', "/");
                            if !name.is_empty() {
                                ast_types.insert(name.clone(), sym_type.clone());
                            }
                            if !scope.is_empty() {
                                let full_scope = format!("{}::{}", scope, name);
                                ast_types.insert(scope.clone(), sym_type.clone());
                                ast_types.insert(full_scope.clone(), sym_type.clone());
                                ast_types.insert(
                                    format!("{}#{}", norm_file, full_scope),
                                    sym_type.clone(),
                                );
                                ast_types
                                    .insert(format!("{}#{}", norm_file, scope), sym_type.clone());
                                ast_types
                                    .insert(format!("{}:{}", norm_file, scope), sym_type.clone());
                            }
                            ast_types.insert(format!("{}#{}", norm_file, name), sym_type.clone());
                            ast_types.insert(format!("{}:{}", norm_file, name), sym_type.clone());
                            ast_types.insert(norm_file.clone(), "Module".to_string());
                        }
                    }
                }
                if let Ok(mut doc_stmt) = conn.prepare("SELECT path FROM documents") {
                    if let Ok(rows) = doc_stmt.query_map([], |row| row.get::<_, String>(0)) {
                        for path in rows.flatten() {
                            let norm = path.replace('\\', "/");
                            ast_types.insert(norm.clone(), "DocNode".to_string());
                            let base = norm.split('/').next_back().unwrap_or(&norm);
                            ast_types.insert(base.to_string(), "DocNode".to_string());
                        }
                    }
                }
            }
        }

        info!(
            corpus = %name,
            nodes = graph.node_count(),
            edges = graph.edge_count(),
            ast_symbols = ast_types.len(),
            "Loaded read-only corpus graph snapshot"
        );

        // Pre-compute community map via Leiden detection (run once at load, cached for Tier 2).
        let community_res = graph.detect_communities_leiden();
        let mut community_map = HashMap::with_capacity(community_res.communities.len() * 8);
        for (comm_id, comm) in community_res.communities.iter().enumerate() {
            for member in &comm.members {
                community_map.insert(member.clone(), comm_id as u32);
            }
        }

        // Record graph.bin mtime for staleness detection.
        let graph_mtime = std::fs::metadata(&graph_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        Ok(Self {
            name: name.to_string(),
            index_dir: index_dir.to_path_buf(),
            root_dir,
            graph,
            ast_types: Arc::new(ast_types),
            community_map: Arc::new(community_map),
            graph_mtime,
        })
    }
}

/// Catalog holding read-only snapshots across all detected corpora.
pub struct CorpusCatalog {
    base_dir: PathBuf,
    snapshots: HashMap<String, Arc<CorpusSnapshot>>,
}

impl CorpusCatalog {
    /// Scan and load all corpora from the default or specified cache directory.
    pub fn load_all(custom_dir: Option<PathBuf>) -> Result<Self> {
        let base_dir = custom_dir.unwrap_or_else(get_corpora_cache_dir);
        let mut snapshots = HashMap::new();

        if base_dir.exists() && base_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&base_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let graph_path = path.join("graph.bin");
                        if graph_path.exists() {
                            match CorpusSnapshot::load_from_dir(&name, &path) {
                                Ok(snapshot) => {
                                    snapshots.insert(name, Arc::new(snapshot));
                                }
                                Err(err) => {
                                    warn!(corpus = %name, error = %err, "Failed to load corpus snapshot");
                                }
                            }
                        }
                    }
                }
            }
        }

        info!(
            corpora_count = snapshots.len(),
            base_dir = %base_dir.display(),
            "Corpus catalog initialized"
        );

        Ok(Self { base_dir, snapshots })
    }

    /// Load or reload a single specific corpus by name.
    pub fn reload_corpus(&mut self, name: &str) -> Result<Arc<CorpusSnapshot>> {
        let index_dir = get_corpus_index_dir(name);
        let snapshot = Arc::new(CorpusSnapshot::load_from_dir(name, &index_dir)?);
        self.snapshots.insert(name.to_string(), snapshot.clone());
        Ok(snapshot)
    }

    /// Get an in-memory snapshot of a corpus.
    pub fn get_corpus(&self, name: &str) -> Option<Arc<CorpusSnapshot>> {
        self.snapshots.get(name).cloned()
    }

    /// List all loaded corpus names.
    pub fn corpus_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.snapshots.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of loaded corpora.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Whether any corpora are loaded.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Reference to base directory.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}
