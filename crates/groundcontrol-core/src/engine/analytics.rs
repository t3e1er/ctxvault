//! Analysis, graph query, template resolution, status metrics, and SCIP ingestion.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tracing::info;

use groundcontrol_common::types::{Document, IndexingStatus};
use groundcontrol_common::{Error, Result};

use crate::template::Template;

use super::state::Engine;
use super::types::{now_unix, IndexingStatusResponse};

impl Engine {
    /// Ingest a pre-computed SCIP (Source Code Intelligence Protocol) index into the knowledge graph.
    pub fn ingest_scip(&mut self, scip_path: &Path) -> Result<crate::graph::scip::ScipIngestStats> {
        self.ingest_scip_with_prefix(scip_path, None)
    }

    /// Ingest a pre-computed SCIP index with an optional repository-relative base directory prefix.
    pub fn ingest_scip_with_prefix(
        &mut self,
        scip_path: &Path,
        base_prefix: Option<&str>,
    ) -> Result<crate::graph::scip::ScipIngestStats> {
        info!("Ingesting SCIP index from: {} (prefix: {:?})", scip_path.display(), base_prefix);
        let (edges, stats) = crate::graph::scip::ScipIngester::extract_edges_from_file_with_prefix(
            scip_path,
            base_prefix,
        )?;

        for edge in &edges {
            self.graph.add_code_edge(edge);
        }

        // Reconcile SCIP monikers with existing SQLite code symbols
        for edge in &edges {
            if edge.edge_type == "defines" {
                if let Some(leaf) = crate::graph::scip::moniker_leaf(&edge.target) {
                    if let Ok(mut symbols) = self.store.get_code_symbols_for_file(&edge.source) {
                        let mut changed = false;
                        for sym in &mut symbols {
                            if sym.name == leaf
                                && sym.canonical_name.as_deref() != Some(&edge.target)
                            {
                                sym.canonical_name = Some(edge.target.clone());
                                changed = true;
                            }
                        }
                        if changed {
                            let _ = self.store.save_code_symbols(&edge.source, &symbols);
                        }
                    }
                }
            }
        }

        self.graph.save(&self.index_dir.join("graph.bin"))?;

        info!(
            "SCIP ingestion complete: {} documents, {} definitions, {} calls, {} edges added",
            stats.documents_processed,
            stats.definitions_extracted,
            stats.calls_extracted,
            stats.edges_added
        );

        Ok(stats)
    }

    /// Recursively discover and ingest all precomputed SCIP index files (`index.scip`, `*.scip`)
    /// located within the corpus root or any subproject directory.
    pub fn ingest_all_scip_indices(&mut self, corpus_root: &Path) -> Result<usize> {
        let mut count = 0;
        let scip_files = find_scip_files(corpus_root);

        for (scip_file, prefix) in scip_files {
            if let Ok(stats) = self.ingest_scip_with_prefix(&scip_file, prefix.as_deref()) {
                if stats.edges_added > 0 {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Second-pass cross-file AST call graph resolution.
    ///
    /// Resolves unresolved external call references in memory against the complete
    /// workspace symbol catalog with zero redundant disk I/O and zero re-parsing.
    pub fn resolve_cross_file_code_edges(&mut self) -> Result<usize> {
        let all_symbols = self.store.get_all_code_symbols()?;
        if all_symbols.is_empty() || self.external_refs.is_empty() {
            return Ok(0);
        }

        let symbol_index = crate::graph::code::CodeGraphExtractor::build_symbol_index(&all_symbols);
        let pending_refs = std::mem::take(&mut self.external_refs);
        let mut remaining_refs = Vec::with_capacity(pending_refs.len());
        let mut edges_added = 0usize;
        let mut visited_calls = std::collections::HashSet::new();

        for ext_ref in pending_refs {
            if ext_ref.kind == groundcontrol_common::types::ExternalRefKind::Call {
                let clean_name =
                    ext_ref.raw_target.rsplit("::").next().unwrap_or(&ext_ref.raw_target);
                let clean_name = clean_name.rsplit('.').next().unwrap_or(clean_name);

                let caller_file = ext_ref.caller_scope_path.split('#').next().unwrap_or("");
                let caller_dir = Path::new(caller_file).parent().unwrap_or_else(|| Path::new(""));

                if let Some(candidates) = symbol_index.get(clean_name) {
                    let cross_candidates: Vec<&&groundcontrol_common::types::CodeSymbol> =
                        candidates.iter().filter(|s| s.file_path != caller_file).collect();

                    if !cross_candidates.is_empty() {
                        let (target_sym, confidence) = if cross_candidates.len() == 1 {
                            (
                                *cross_candidates[0],
                                groundcontrol_common::types::ResolutionConfidence::High,
                            )
                        } else if let Some(dir_match) = cross_candidates.iter().find(|c| {
                            Path::new(&c.file_path).parent().unwrap_or_else(|| Path::new(""))
                                == caller_dir
                        }) {
                            (**dir_match, groundcontrol_common::types::ResolutionConfidence::Medium)
                        } else {
                            (
                                *cross_candidates[0],
                                groundcontrol_common::types::ResolutionConfidence::Speculative,
                            )
                        };

                        let call_key =
                            (ext_ref.caller_scope_path.clone(), target_sym.scope_path.clone());
                        if visited_calls.insert(call_key) {
                            let is_test = ext_ref.caller_scope_path.to_lowercase().contains("test")
                                || ext_ref.caller_scope_path.to_lowercase().contains("spec");

                            self.graph.add_code_edge(&groundcontrol_common::types::Edge {
                                source: ext_ref.caller_scope_path.clone(),
                                target: target_sym.scope_path.clone(),
                                edge_type: "calls".to_string(),
                                weight: 0.8,
                                provenance: groundcontrol_common::types::EdgeProvenance::CodeCalls,
                                target_corpus: None,
                                confidence: Some(confidence),
                                target_path: None,
                                target_symbol: None,
                                target_kind: None,
                            });
                            edges_added += 1;

                            if is_test {
                                self.graph.add_code_edge(&groundcontrol_common::types::Edge {
                                    source: ext_ref.caller_scope_path.clone(),
                                    target: target_sym.scope_path.clone(),
                                    edge_type: "tests".to_string(),
                                    weight: 0.9,
                                    provenance:
                                        groundcontrol_common::types::EdgeProvenance::CodeTests,
                                    target_corpus: None,
                                    confidence: Some(confidence),
                                    target_path: None,
                                    target_symbol: None,
                                    target_kind: None,
                                });
                                edges_added += 1;
                            }
                        }
                        continue;
                    }
                }
            }

            remaining_refs.push(ext_ref);
        }

        self.external_refs = remaining_refs;
        info!(
            "In-memory cross-file code call resolution complete: {} edges added/updated",
            edges_added
        );
        Ok(edges_added)
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

    /// Execute a Cypher-Lite linear path pattern match across code and doc entities.
    pub fn graph_match(
        &self,
        pattern: &str,
        edge_class: Option<&str>,
        where_clause: Option<&str>,
        limit: usize,
        max_depth: usize,
    ) -> Result<groundcontrol_common::types::GraphMatchResult> {
        let parsed = crate::graph::query::parse_path_pattern(pattern)?;
        let qe = crate::graph::query::QueryEngine::new(&self.graph, &self.store);
        qe.execute_match(&parsed, edge_class, where_clause, limit, max_depth)
    }

    /// Compute direct degree affordances for a node.
    pub fn compute_affordances(&self, path: &str) -> groundcontrol_common::types::GraphAffordances {
        self.graph.compute_affordances(path)
    }

    /// Format immediate 1-hop graph neighborhood as a compact Cypher-Lite ASCII expression.
    pub fn format_cypher_affordances(&self, path: &str, max_neighbors: usize) -> Option<String> {
        self.graph.format_cypher_affordances(path, max_neighbors)
    }

    /// Return the total in-degree of a node directly without allocating affordance maps.
    pub fn in_degree(&self, path: &str) -> usize {
        self.graph.in_degree(path)
    }

    /// Return all active distinct edge types present in the graph, optionally filtered by EdgeClass.
    pub fn active_edge_types(
        &self,
        class_filter: Option<groundcontrol_common::config::EdgeClass>,
    ) -> Vec<String> {
        self.graph.active_edge_types(class_filter)
    }

    /// Build the set of graph node keys that represent code entities.
    pub fn code_paths_set(&self) -> HashSet<String> {
        let mut set = HashSet::new();
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

    /// Load all markdown templates defined in the corpus templates directory.
    pub fn load_templates(&self) -> Result<HashMap<String, Template>> {
        let corpus_path = Path::new(&self.config.path);
        let (_resolved, templates) =
            Template::discover_and_load(corpus_path, self.config.templates_dir.as_deref())?;
        Ok(templates)
    }

    /// Discover and load all markdown templates along with the resolved relative directory.
    pub fn discover_templates(&self) -> Result<(Option<PathBuf>, HashMap<String, Template>)> {
        let corpus_path = Path::new(&self.config.path);
        Template::discover_and_load(corpus_path, self.config.templates_dir.as_deref())
    }

    /// Compute the effective edge type configurations for a document.
    pub fn effective_edge_configs_for_document(
        &self,
        doc: &Document,
        templates: Option<&HashMap<String, Template>>,
    ) -> Vec<groundcontrol_common::config::EdgeTypeConfig> {
        let mut configs = self.config.graph.edge_types.clone();
        if let Some(ref tmpl_name) = doc.template {
            let loaded = if templates.is_none() { self.load_templates().ok() } else { None };
            let tmpl_map = templates.or(loaded.as_ref());
            if let Some(tmpl) = tmpl_map.and_then(|m| m.get(tmpl_name)) {
                for edge in &tmpl.edges {
                    configs.push(edge.to_edge_type_config());
                }
            }
        }
        configs
    }

    /// Analyze graph density, identifying hubs and orphans.
    pub fn analyze_density(&self, top_hubs: usize) -> crate::analytics::DensityReport {
        crate::analytics::analyze_density(&self.graph, top_hubs)
    }

    /// Find queries where BM25 and vector search disagree.
    pub fn find_semantic_gaps(
        &self,
        queries: &[&str],
        top_k: usize,
    ) -> Result<Option<Vec<crate::analytics::SemanticGap>>> {
        let vector_index = match self.vector_index() {
            Some(vi) => vi,
            None => {
                return Err(Error::Index(
                    "Semantic gap analysis is unavailable in fast mode. Re-index with index_mode = 'full' to enable vector search.".to_string(),
                ));
            }
        };

        let query_embeddings: Vec<Vec<f32>> = if let Some(embedder) = self.embedder_arc() {
            queries.iter().filter_map(|q| embedder.embed_query(q).ok()).collect()
        } else {
            Vec::new()
        };

        if query_embeddings.len() != queries.len() {
            return Ok(None);
        }

        let gaps = crate::analytics::find_semantic_gaps(
            &self.bm25,
            vector_index,
            queries,
            &query_embeddings,
            top_k,
        )?;
        Ok(Some(gaps))
    }

    /// Suggest chunks that may benefit from splitting.
    pub fn suggest_splits(
        &self,
        max_chunk_chars: usize,
    ) -> Result<Vec<crate::analytics::SplitSuggestion>> {
        crate::analytics::suggest_splits(
            &self.store,
            Some(Path::new(&self.config.path)),
            max_chunk_chars,
        )
    }

    /// Generate a coverage report over the given test queries.
    pub fn coverage_report(
        &self,
        queries: &[&str],
        top_k: usize,
    ) -> Result<crate::analytics::CoverageReport> {
        let files = self.store.list_files()?;
        let all_paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
        crate::analytics::coverage_report(&self.bm25, queries, &all_paths, top_k)
    }
}

/// Helper to scan for all SCIP index files in a corpus directory.
/// Returns a list of (absolute_path_to_scip_file, optional_relative_prefix).
fn find_scip_files(root: &Path) -> Vec<(PathBuf, Option<String>)> {
    let mut results = Vec::new();
    if !root.is_dir() {
        return results;
    }

    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                !matches!(
                    name.as_ref(),
                    ".git"
                        | "target"
                        | "node_modules"
                        | "build"
                        | "dist"
                        | ".index"
                        | ".agents"
                        | ".gemini"
                )
            } else {
                true
            }
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_file() {
            let is_scip = path
                .file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n == "index.scip" || n.ends_with(".scip"));

            if is_scip {
                let parent = path.parent().unwrap_or(root);
                let subproject_dir = if parent.file_name().and_then(|n| n.to_str()) == Some(".scip")
                {
                    parent.parent().unwrap_or(root)
                } else {
                    parent
                };

                let prefix = if subproject_dir == root {
                    None
                } else if let Ok(rel) = subproject_dir.strip_prefix(root) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if rel_str.is_empty() {
                        None
                    } else {
                        Some(rel_str)
                    }
                } else {
                    None
                };

                results.push((path.to_path_buf(), prefix));
            }
        }
    }

    results
}
