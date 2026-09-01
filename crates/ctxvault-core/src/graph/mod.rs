//! Knowledge graph: typed directed edges, traversal, PPR, subgraph extraction.

pub mod code;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};

use ctxvault_common::config::{EdgeClass, EdgeDirection, EdgeSource, EdgeTypeConfig};
use ctxvault_common::types::{Document, EdgeProvenance};
use ctxvault_common::{Error, Result};

/// Node data stored in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Relative path within the corpus.
    pub path: String,
    /// Document title (if known).
    pub title: Option<String>,
}

/// Edge data stored in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Edge type name (must match a registered `EdgeTypeConfig.name`).
    pub edge_type: String,
    /// Weight of this edge.
    pub weight: f32,
    /// How this edge was created.
    pub provenance: EdgeProvenance,
    /// Edge class for filtering purposes.
    pub class: EdgeClass,
}

/// Graph statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total number of nodes.
    pub node_count: usize,
    /// Total number of edges.
    pub edge_count: usize,
    /// Nodes with zero edges (neither incoming nor outgoing).
    pub orphan_count: usize,
    /// Top 10 nodes by total degree (incoming + outgoing).
    pub most_connected: Vec<(String, usize)>,
    /// Count of edges per edge type.
    pub edge_type_distribution: HashMap<String, usize>,
}

/// Serializable wrapper for persistence.
#[derive(Serialize, Deserialize)]
struct GraphData {
    graph: DiGraph<GraphNode, GraphEdge>,
}

/// Knowledge graph with typed, weighted, directed edges.
pub struct KnowledgeGraph {
    graph: DiGraph<GraphNode, GraphEdge>,
    /// Map from document path to NodeIndex for O(1) lookup.
    node_map: HashMap<String, NodeIndex>,
}

impl KnowledgeGraph {
    /// Create an empty knowledge graph.
    pub fn new() -> Self {
        Self { graph: DiGraph::new(), node_map: HashMap::new() }
    }

    /// Add or update a node. Returns the NodeIndex.
    pub fn add_node(&mut self, path: &str, title: Option<&str>) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(path) {
            // Update existing node data.
            if let Some(node) = self.graph.node_weight_mut(idx) {
                node.title = title.map(|t| t.to_string());
            }
            idx
        } else {
            let node = GraphNode { path: path.to_string(), title: title.map(|t| t.to_string()) };
            let idx = self.graph.add_node(node);
            let _ = self.node_map.insert(path.to_string(), idx);
            idx
        }
    }

    /// Remove a node and all its edges.
    pub fn remove_node(&mut self, path: &str) -> Result<()> {
        let idx = self
            .node_map
            .remove(path)
            .ok_or_else(|| Error::Graph(format!("node not found: {}", path)))?;
        let _ = self.graph.remove_node(idx);

        // petgraph may swap the last node into the removed index.
        // Rebuild node_map to stay consistent.
        self.rebuild_node_map();
        Ok(())
    }

    /// Add a directed edge between two nodes. Creates target node if missing.
    pub fn add_edge(
        &mut self,
        source: &str,
        target: &str,
        edge_type: &str,
        weight: f32,
        provenance: EdgeProvenance,
        class: EdgeClass,
    ) {
        let src_idx = self.add_node(source, None);
        let tgt_idx = self.add_node(target, None);

        // De-duplicate parallel edges of the same type.
        if let Some(edge_ref) = self
            .graph
            .edges_connecting(src_idx, tgt_idx)
            .find(|e| e.weight().edge_type == edge_type)
        {
            let edge_id = edge_ref.id();
            if let Some(edge_mut) = self.graph.edge_weight_mut(edge_id) {
                edge_mut.weight = weight;
                edge_mut.provenance = provenance;
                edge_mut.class = class;
            }
            return;
        }

        let edge = GraphEdge { edge_type: edge_type.to_string(), weight, provenance, class };
        let _ = self.graph.add_edge(src_idx, tgt_idx, edge);
    }

    /// Add a code edge into the graph with appropriate EdgeClass.
    pub fn add_code_edge(&mut self, edge: &ctxvault_common::types::Edge) {
        self.add_edge(
            &edge.source,
            &edge.target,
            &edge.edge_type,
            edge.weight,
            edge.provenance.clone(),
            EdgeClass::Structural,
        );
    }

    /// Remove all edges where the given path is source or target.
    pub fn remove_edges_for_node(&mut self, path: &str) {
        let Some(&idx) = self.node_map.get(path) else {
            return;
        };

        // Collect edge indices to remove (both directions).
        let edge_indices: Vec<_> = self
            .graph
            .edges_directed(idx, Direction::Outgoing)
            .map(|e| e.id())
            .chain(self.graph.edges_directed(idx, Direction::Incoming).map(|e| e.id()))
            .collect();

        // Remove in reverse order to avoid index invalidation issues.
        // petgraph's remove_edge swaps with last edge, so collect all first.
        for edge_id in edge_indices.into_iter().rev() {
            let _ = self.graph.remove_edge(edge_id);
        }
    }

    /// Get the NodeIndex for a path.
    pub fn get_node(&self, path: &str) -> Option<NodeIndex> {
        self.node_map.get(path).copied()
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Retrieve all edges currently in the knowledge graph.
    pub fn get_all_edges(&self) -> Vec<ctxvault_common::types::Edge> {
        let mut edges = Vec::new();
        for edge in self.graph.edge_references() {
            let source_idx = edge.source();
            let target_idx = edge.target();
            if let (Some(source_node), Some(target_node)) =
                (self.graph.node_weight(source_idx), self.graph.node_weight(target_idx))
            {
                let weight_data = edge.weight();
                edges.push(ctxvault_common::types::Edge {
                    source: source_node.path.clone(),
                    target: target_node.path.clone(),
                    edge_type: weight_data.edge_type.clone(),
                    weight: weight_data.weight,
                    provenance: weight_data.provenance.clone(),
                });
            }
        }
        edges
    }

    // ─── Graph Builder ───────────────────────────────────────────────────────

    /// Build edges from a parsed Document based on the given edge type configurations.
    /// This processes wikilinks, shared tags, and frontmatter fields.
    pub fn build_edges_for_document(
        &mut self,
        doc: &Document,
        edge_configs: &[EdgeTypeConfig],
        all_docs: &[Document],
    ) {
        // Ensure this document is in the graph.
        let _ = self.add_node(&doc.path, doc.title.as_deref());

        for config in edge_configs {
            match config.source {
                EdgeSource::Wikilink => {
                    self.build_wikilink_edges(doc, config);
                }
                EdgeSource::Tag => {
                    self.build_tag_edges(doc, config, all_docs);
                }
                EdgeSource::Frontmatter => {
                    self.build_frontmatter_edges(doc, config);
                }
                EdgeSource::Reference => {
                    // Standard markdown link edges — similar to wikilinks but from
                    // markdown reference links. Not yet extracted in Document type.
                    // No-op for now.
                }
                EdgeSource::Code => {
                    // Code AST edges are populated via dedicated code graph extraction passes.
                }
            }
        }
    }

    fn build_wikilink_edges(&mut self, doc: &Document, config: &EdgeTypeConfig) {
        let class = config.class.unwrap_or_else(|| EdgeClass::infer_from_source(&config.source));
        for wikilink in &doc.wikilinks {
            self.add_edge(
                &doc.path,
                &wikilink.target,
                &config.name,
                config.weight,
                EdgeProvenance::Wikilink,
                class,
            );
            if config.bidirectional {
                self.add_edge(
                    &wikilink.target,
                    &doc.path,
                    &config.name,
                    config.weight,
                    EdgeProvenance::Wikilink,
                    class,
                );
            }
        }
    }

    fn build_tag_edges(&mut self, doc: &Document, config: &EdgeTypeConfig, all_docs: &[Document]) {
        if doc.tags.is_empty() || all_docs.is_empty() {
            return;
        }

        let class = config.class.unwrap_or_else(|| EdgeClass::infer_from_source(&config.source));
        let n = all_docs.len() as f32;

        // Always compute tag frequencies for IDF calculation.
        let mut tag_frequencies: HashMap<&str, usize> = HashMap::new();
        for d in all_docs {
            for tag in &d.tags {
                *tag_frequencies.entry(tag.as_str()).or_insert(0) += 1;
            }
        }

        let doc_tags: HashSet<&str> = doc.tags.iter().map(|t| t.as_str()).collect();

        for other in all_docs {
            if other.path == doc.path {
                continue;
            }

            // Compute IDF-weighted edge weight from all shared qualifying tags.
            let mut weight_sum: f32 = 0.0;
            for tag in &other.tags {
                if !doc_tags.contains(tag.as_str()) {
                    continue;
                }
                // Check max_frequency hard cutoff.
                if let Some(max_freq) = config.max_frequency {
                    if let Some(&count) = tag_frequencies.get(tag.as_str()) {
                        if count > max_freq {
                            continue;
                        }
                    }
                }
                // Compute IDF weight for this tag.
                if let Some(&doc_freq) = tag_frequencies.get(tag.as_str()) {
                    let idf = (n / doc_freq as f32).ln();
                    weight_sum += config.weight * idf;
                }
            }

            if weight_sum > 0.0 {
                self.add_edge(
                    &doc.path,
                    &other.path,
                    &config.name,
                    weight_sum,
                    EdgeProvenance::SharedTag,
                    class,
                );
            }
        }
    }

    fn build_frontmatter_edges(&mut self, doc: &Document, config: &EdgeTypeConfig) {
        let Some(ref frontmatter) = doc.frontmatter else {
            return;
        };
        let Some(ref field_name) = config.field else {
            return;
        };

        let targets = extract_frontmatter_targets(frontmatter, field_name);

        let direction = config.direction.as_ref();
        let class = config.class.unwrap_or_else(|| EdgeClass::infer_from_source(&config.source));

        for target in targets {
            match direction {
                Some(EdgeDirection::Inbound) => {
                    self.add_edge(
                        &target,
                        &doc.path,
                        &config.name,
                        config.weight,
                        EdgeProvenance::Frontmatter,
                        class,
                    );
                }
                _ => {
                    // Default to outbound.
                    self.add_edge(
                        &doc.path,
                        &target,
                        &config.name,
                        config.weight,
                        EdgeProvenance::Frontmatter,
                        class,
                    );
                }
            }
            if config.bidirectional {
                // Add reverse direction as well.
                match direction {
                    Some(EdgeDirection::Inbound) => {
                        self.add_edge(
                            &doc.path,
                            &target,
                            &config.name,
                            config.weight,
                            EdgeProvenance::Frontmatter,
                            class,
                        );
                    }
                    _ => {
                        self.add_edge(
                            &target,
                            &doc.path,
                            &config.name,
                            config.weight,
                            EdgeProvenance::Frontmatter,
                            class,
                        );
                    }
                }
            }
        }
    }

    // ─── Traversal ───────────────────────────────────────────────────────────

    /// BFS from a starting node, up to `max_depth` hops.
    /// Optionally filter by edge types.
    /// Optionally filter by edge class.
    /// Returns (path, hops_from_start).
    pub fn traverse_bfs(
        &self,
        start: &str,
        max_depth: usize,
        edge_type_filter: Option<&[String]>,
        edge_class_filter: Option<EdgeClass>,
    ) -> Vec<(String, usize)> {
        let Some(&start_idx) = self.node_map.get(start) else {
            return Vec::new();
        };

        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
        let mut results: Vec<(String, usize)> = Vec::new();

        let _ = visited.insert(start_idx);
        queue.push_back((start_idx, 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth > 0 {
                if let Some(node) = self.graph.node_weight(current) {
                    results.push((node.path.clone(), depth));
                }
            }

            if depth >= max_depth {
                continue;
            }

            for edge in self.graph.edges_directed(current, Direction::Outgoing) {
                let edge_data = edge.weight();
                if let Some(filter) = edge_type_filter {
                    if !filter.contains(&edge_data.edge_type) {
                        continue;
                    }
                }
                if let Some(class_filter) = edge_class_filter {
                    if !edge_data.class.matches(class_filter) {
                        continue;
                    }
                }

                let neighbor = edge.target();
                if !visited.contains(&neighbor) {
                    let _ = visited.insert(neighbor);
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }

        results
    }

    /// DFS from a starting node, up to `max_depth` hops.
    /// Optionally filter by edge types.
    /// Optionally filter by edge class.
    /// Returns (path, hops_from_start).
    pub fn traverse_dfs(
        &self,
        start: &str,
        max_depth: usize,
        edge_type_filter: Option<&[String]>,
        edge_class_filter: Option<EdgeClass>,
    ) -> Vec<(String, usize)> {
        let Some(&start_idx) = self.node_map.get(start) else {
            return Vec::new();
        };

        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut stack: Vec<(NodeIndex, usize)> = Vec::new();
        let mut results: Vec<(String, usize)> = Vec::new();

        let _ = visited.insert(start_idx);
        stack.push((start_idx, 0));

        while let Some((current, depth)) = stack.pop() {
            if depth > 0 {
                if let Some(node) = self.graph.node_weight(current) {
                    results.push((node.path.clone(), depth));
                }
            }

            if depth >= max_depth {
                continue;
            }

            for edge in self.graph.edges_directed(current, Direction::Outgoing) {
                let edge_data = edge.weight();
                if let Some(filter) = edge_type_filter {
                    if !filter.contains(&edge_data.edge_type) {
                        continue;
                    }
                }
                if let Some(class_filter) = edge_class_filter {
                    if !edge_data.class.matches(class_filter) {
                        continue;
                    }
                }

                let neighbor = edge.target();
                if !visited.contains(&neighbor) {
                    let _ = visited.insert(neighbor);
                    stack.push((neighbor, depth + 1));
                }
            }
        }

        results
    }

    // ─── Persistence ─────────────────────────────────────────────────────────

    /// Serialize the graph to a file using bincode.
    pub fn save(&self, path: &Path) -> Result<()> {
        let data = GraphData { graph: self.graph.clone() };
        let encoded =
            bincode::serialize(&data).map_err(|e| Error::Graph(format!("serialize: {}", e)))?;
        std::fs::write(path, encoded).map_err(|e| Error::Graph(format!("write: {}", e)))?;
        Ok(())
    }

    /// Deserialize a graph from a file.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|e| Error::Graph(format!("read: {}", e)))?;
        let data: GraphData = bincode::deserialize(&bytes)
            .map_err(|e| Error::Graph(format!("deserialize: {}", e)))?;

        let mut node_map = HashMap::new();
        for idx in data.graph.node_indices() {
            if let Some(node) = data.graph.node_weight(idx) {
                let _ = node_map.insert(node.path.clone(), idx);
            }
        }

        Ok(Self { graph: data.graph, node_map })
    }

    // ─── Query Helpers ───────────────────────────────────────────────────────

    /// Get all notes that link TO this note, grouped by edge type.
    pub fn backlinks(
        &self,
        path: &str,
        edge_class_filter: Option<EdgeClass>,
    ) -> HashMap<String, Vec<String>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();

        let Some(&idx) = self.node_map.get(path) else {
            return result;
        };

        for edge in self.graph.edges_directed(idx, Direction::Incoming) {
            let source_idx = edge.source();
            if let Some(source_node) = self.graph.node_weight(source_idx) {
                let edge_data = edge.weight();
                if let Some(class_filter) = edge_class_filter {
                    if !edge_data.class.matches(class_filter) {
                        continue;
                    }
                }
                result
                    .entry(edge_data.edge_type.clone())
                    .or_default()
                    .push(source_node.path.clone());
            }
        }

        result
    }

    /// Get all notes this note links TO, grouped by edge type.
    pub fn forwardlinks(
        &self,
        path: &str,
        edge_class_filter: Option<EdgeClass>,
    ) -> HashMap<String, Vec<String>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();

        let Some(&idx) = self.node_map.get(path) else {
            return result;
        };

        for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
            let target_idx = edge.target();
            if let Some(target_node) = self.graph.node_weight(target_idx) {
                let edge_data = edge.weight();
                if let Some(class_filter) = edge_class_filter {
                    if !edge_data.class.matches(class_filter) {
                        continue;
                    }
                }
                result
                    .entry(edge_data.edge_type.clone())
                    .or_default()
                    .push(target_node.path.clone());
            }
        }

        result
    }

    /// Find shortest path between two nodes (optionally filtered by edge types).
    /// Returns the path as a list of document paths, or None if no path exists.
    pub fn shortest_path(
        &self,
        from: &str,
        to: &str,
        edge_type_filter: Option<&[String]>,
        edge_class_filter: Option<EdgeClass>,
    ) -> Option<Vec<String>> {
        let start_idx = *self.node_map.get(from)?;
        let end_idx = *self.node_map.get(to)?;

        // BFS to find shortest path (unweighted).
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<NodeIndex> = VecDeque::new();
        let mut parents: HashMap<NodeIndex, NodeIndex> = HashMap::new();

        let _ = visited.insert(start_idx);
        queue.push_back(start_idx);

        let mut found = false;

        while let Some(current) = queue.pop_front() {
            if current == end_idx {
                found = true;
                break;
            }

            for edge in self.graph.edges_directed(current, Direction::Outgoing) {
                let edge_data = edge.weight();
                if let Some(filter) = edge_type_filter {
                    if !filter.contains(&edge_data.edge_type) {
                        continue;
                    }
                }
                if let Some(class_filter) = edge_class_filter {
                    if !edge_data.class.matches(class_filter) {
                        continue;
                    }
                }

                let neighbor = edge.target();
                if !visited.contains(&neighbor) {
                    let _ = visited.insert(neighbor);
                    let _ = parents.insert(neighbor, current);
                    queue.push_back(neighbor);
                }
            }
        }

        if !found {
            return None;
        }

        // Reconstruct path.
        let mut path_indices = Vec::new();
        let mut current = end_idx;
        while current != start_idx {
            path_indices.push(current);
            current = *parents.get(&current)?;
        }
        path_indices.push(start_idx);
        path_indices.reverse();

        let path: Vec<String> = path_indices
            .iter()
            .filter_map(|&idx| self.graph.node_weight(idx).map(|n| n.path.clone()))
            .collect();

        Some(path)
    }

    /// Get graph statistics.
    pub fn stats(&self) -> GraphStats {
        let node_count = self.graph.node_count();
        let edge_count = self.graph.edge_count();

        // Count orphans (nodes with no edges in either direction).
        let mut orphan_count = 0;
        let mut degree_map: Vec<(String, usize)> = Vec::new();

        for idx in self.graph.node_indices() {
            let in_degree = self.graph.edges_directed(idx, Direction::Incoming).count();
            let out_degree = self.graph.edges_directed(idx, Direction::Outgoing).count();
            let total_degree = in_degree + out_degree;

            if total_degree == 0 {
                orphan_count += 1;
            }

            if let Some(node) = self.graph.node_weight(idx) {
                degree_map.push((node.path.clone(), total_degree));
            }
        }

        // Top 10 most connected.
        degree_map.sort_by(|a, b| b.1.cmp(&a.1));
        let most_connected: Vec<(String, usize)> = degree_map.into_iter().take(10).collect();

        // Edge type distribution.
        let mut edge_type_distribution: HashMap<String, usize> = HashMap::new();
        for edge in self.graph.edge_weights() {
            *edge_type_distribution.entry(edge.edge_type.clone()).or_insert(0) += 1;
        }

        GraphStats { node_count, edge_count, orphan_count, most_connected, edge_type_distribution }
    }

    /// Get paths of all orphan nodes (nodes with no incoming or outgoing edges).
    pub fn orphan_paths(&self) -> Vec<String> {
        use petgraph::Direction;
        let mut orphans = Vec::new();
        for idx in self.graph.node_indices() {
            let in_degree = self.graph.edges_directed(idx, Direction::Incoming).count();
            let out_degree = self.graph.edges_directed(idx, Direction::Outgoing).count();
            if in_degree + out_degree == 0 {
                if let Some(node) = self.graph.node_weight(idx) {
                    orphans.push(node.path.clone());
                }
            }
        }
        orphans
    }

    /// Get per-node degree information: (path, in_degree, out_degree).
    pub fn node_degree_list(&self) -> Vec<(String, usize, usize)> {
        use petgraph::Direction;
        let mut degrees = Vec::new();
        for idx in self.graph.node_indices() {
            let in_degree = self.graph.edges_directed(idx, Direction::Incoming).count();
            let out_degree = self.graph.edges_directed(idx, Direction::Outgoing).count();
            if let Some(node) = self.graph.node_weight(idx) {
                degrees.push((node.path.clone(), in_degree, out_degree));
            }
        }
        degrees
    }

    /// Ensure a node exists (add it if not present). Used for testing.
    pub fn ensure_node(&mut self, path: &str) {
        let _ = self.add_node(path, None);
    }

    // ─── Structural Lineage & Taxonomy ───────────────────────────────────────

    /// Deterministically traverse the graph along a specified structural edge type.
    ///
    /// - `start`: The starting document path.
    /// - `edge_type`: The structural relationship name (e.g. "supersedes", "implements").
    /// - `direction`: "outgoing", "incoming", or "both".
    /// - `max_depth`: Maximum traversal hops.
    ///
    /// Returns the ordered lineage chain starting with the start node at depth 0.
    pub fn traverse_lineage(
        &self,
        start: &str,
        edge_type: &str,
        direction: &str,
        max_depth: usize,
    ) -> Vec<LineageNode> {
        let mut results = Vec::new();

        let Some(&start_idx) = self.node_map.get(start) else {
            return results;
        };

        let start_title = self.graph.node_weight(start_idx).and_then(|n| n.title.clone());
        results.push(LineageNode {
            path: start.to_string(),
            title: start_title,
            depth: 0,
            edge_type: edge_type.to_string(),
            direction: "start".to_string(),
        });

        if max_depth == 0 {
            return results;
        }

        let dir_lower = direction.to_lowercase();
        let allow_outgoing = dir_lower == "outgoing" || dir_lower == "both";
        let allow_incoming = dir_lower == "incoming" || dir_lower == "both";

        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let _ = visited.insert(start_idx);

        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
        queue.push_back((start_idx, 0));

        while let Some((curr, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            if allow_outgoing {
                for edge in self.graph.edges_directed(curr, Direction::Outgoing) {
                    if edge.weight().edge_type.eq_ignore_ascii_case(edge_type) {
                        let neighbor = edge.target();
                        if !visited.contains(&neighbor) {
                            let _ = visited.insert(neighbor);
                            let title =
                                self.graph.node_weight(neighbor).and_then(|n| n.title.clone());
                            let path = self
                                .graph
                                .node_weight(neighbor)
                                .map(|n| n.path.clone())
                                .unwrap_or_default();
                            results.push(LineageNode {
                                path,
                                title,
                                depth: depth + 1,
                                edge_type: edge.weight().edge_type.clone(),
                                direction: "outgoing".to_string(),
                            });
                            queue.push_back((neighbor, depth + 1));
                        }
                    }
                }
            }

            if allow_incoming {
                for edge in self.graph.edges_directed(curr, Direction::Incoming) {
                    if edge.weight().edge_type.eq_ignore_ascii_case(edge_type) {
                        let neighbor = edge.source();
                        if !visited.contains(&neighbor) {
                            let _ = visited.insert(neighbor);
                            let title =
                                self.graph.node_weight(neighbor).and_then(|n| n.title.clone());
                            let path = self
                                .graph
                                .node_weight(neighbor)
                                .map(|n| n.path.clone())
                                .unwrap_or_default();
                            results.push(LineageNode {
                                path,
                                title,
                                depth: depth + 1,
                                edge_type: edge.weight().edge_type.clone(),
                                direction: "incoming".to_string(),
                            });
                            queue.push_back((neighbor, depth + 1));
                        }
                    }
                }
            }
        }

        results
    }

    /// Extract active structural lineage metadata for a node.
    pub fn extract_lineage_for_node(
        &self,
        path: &str,
    ) -> Option<ctxvault_common::types::LineageAnnotation> {
        let &idx = self.node_map.get(path)?;

        let mut ann = ctxvault_common::types::LineageAnnotation::default();

        for edge in self.graph.edges_directed(idx, Direction::Incoming) {
            let src_idx = edge.source();
            if let Some(src_node) = self.graph.node_weight(src_idx) {
                let et = edge.weight().edge_type.to_lowercase();
                match et.as_str() {
                    "supersedes" => ann.superseded_by.push(src_node.path.clone()),
                    "implements" => ann.implemented_by.push(src_node.path.clone()),
                    "depends_on" | "dependson" | "depends-on" => {
                        ann.depended_on_by.push(src_node.path.clone())
                    }
                    "adr_for" | "adrfor" | "adr-for" => ann.has_adr.push(src_node.path.clone()),
                    "parent_of" | "parentof" | "parent-of" => {
                        ann.child_of.push(src_node.path.clone())
                    }
                    _ => {
                        if edge.weight().class.matches(EdgeClass::Structural) {
                            ann.incoming
                                .entry(edge.weight().edge_type.clone())
                                .or_default()
                                .push(src_node.path.clone());
                        }
                    }
                }
            }
        }

        for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
            let tgt_idx = edge.target();
            if let Some(tgt_node) = self.graph.node_weight(tgt_idx) {
                let et = edge.weight().edge_type.to_lowercase();
                match et.as_str() {
                    "supersedes" => ann.supersedes.push(tgt_node.path.clone()),
                    "implements" => ann.implements.push(tgt_node.path.clone()),
                    "depends_on" | "dependson" | "depends-on" => {
                        ann.depends_on.push(tgt_node.path.clone())
                    }
                    "adr_for" | "adrfor" | "adr-for" => ann.adr_for.push(tgt_node.path.clone()),
                    "parent_of" | "parentof" | "parent-of" => {
                        ann.parent_of.push(tgt_node.path.clone())
                    }
                    _ => {
                        if edge.weight().class.matches(EdgeClass::Structural) {
                            ann.outgoing
                                .entry(edge.weight().edge_type.clone())
                                .or_default()
                                .push(tgt_node.path.clone());
                        }
                    }
                }
            }
        }

        // Sort and deduplicate vectors for determinism
        ann.superseded_by.sort();
        ann.superseded_by.dedup();
        ann.supersedes.sort();
        ann.supersedes.dedup();
        ann.implements.sort();
        ann.implements.dedup();
        ann.implemented_by.sort();
        ann.implemented_by.dedup();
        ann.depends_on.sort();
        ann.depends_on.dedup();
        ann.depended_on_by.sort();
        ann.depended_on_by.dedup();
        ann.adr_for.sort();
        ann.adr_for.dedup();
        ann.has_adr.sort();
        ann.has_adr.dedup();
        ann.parent_of.sort();
        ann.parent_of.dedup();
        ann.child_of.sort();
        ann.child_of.dedup();

        for list in ann.incoming.values_mut() {
            list.sort();
            list.dedup();
        }
        for list in ann.outgoing.values_mut() {
            list.sort();
            list.dedup();
        }

        if ann.is_empty() {
            None
        } else {
            Some(ann)
        }
    }

    /// Detect broken structural links (wikilinks or frontmatter references pointing to non-existent notes).
    pub fn detect_broken_links(&self, existing_paths: &HashSet<String>) -> Vec<BrokenLink> {
        let mut broken = Vec::new();
        let mut seen = HashSet::new();

        for edge in self.graph.edge_references() {
            if edge.weight().class.matches(EdgeClass::Structural) {
                let src_node = self.graph.node_weight(edge.source());
                let tgt_node = self.graph.node_weight(edge.target());

                if let (Some(src), Some(tgt)) = (src_node, tgt_node) {
                    let target_str = &tgt.path;
                    let exists = existing_paths.contains(target_str)
                        || existing_paths.contains(&format!("{}.md", target_str))
                        || (target_str.ends_with(".md")
                            && existing_paths.contains(&target_str[..target_str.len() - 3]));

                    if !exists {
                        let key = format!("{}->{}:{}", src.path, tgt.path, edge.weight().edge_type);
                        if !seen.contains(&key) {
                            let _ = seen.insert(key);
                            broken.push(BrokenLink {
                                source: src.path.clone(),
                                target: tgt.path.clone(),
                                edge_type: edge.weight().edge_type.clone(),
                                provenance: edge.weight().provenance.clone(),
                            });
                        }
                    }
                }
            }
        }

        broken
    }

    /// Detect circular dependencies in specified directed acyclic relations (e.g. "supersedes", "depends_on").
    pub fn detect_circular_dependencies(&self, edge_types: &[&str]) -> Vec<CircularDependency> {
        let mut cycles = Vec::new();
        let mut seen_cycles: HashSet<String> = HashSet::new();

        for &edge_type in edge_types {
            let mut adj: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
            for edge in self.graph.edge_references() {
                if edge.weight().edge_type.eq_ignore_ascii_case(edge_type) {
                    adj.entry(edge.source()).or_default().push(edge.target());
                }
            }

            let mut visited: HashSet<NodeIndex> = HashSet::new();
            let mut rec_stack: Vec<NodeIndex> = Vec::new();

            for &start_idx in adj.keys() {
                if !visited.contains(&start_idx) {
                    self.dfs_find_cycles(
                        start_idx,
                        &adj,
                        &mut visited,
                        &mut rec_stack,
                        edge_type,
                        &mut cycles,
                        &mut seen_cycles,
                    );
                }
            }
        }

        cycles
    }

    fn dfs_find_cycles(
        &self,
        node: NodeIndex,
        adj: &HashMap<NodeIndex, Vec<NodeIndex>>,
        visited: &mut HashSet<NodeIndex>,
        rec_stack: &mut Vec<NodeIndex>,
        edge_type: &str,
        cycles: &mut Vec<CircularDependency>,
        seen_cycles: &mut HashSet<String>,
    ) {
        let _ = visited.insert(node);
        rec_stack.push(node);

        if let Some(neighbors) = adj.get(&node) {
            for &next in neighbors {
                if let Some(pos) = rec_stack.iter().position(|&n| n == next) {
                    let mut cycle_nodes = Vec::new();
                    for &n in &rec_stack[pos..] {
                        if let Some(w) = self.graph.node_weight(n) {
                            cycle_nodes.push(w.path.clone());
                        }
                    }
                    if let Some(w) = self.graph.node_weight(next) {
                        cycle_nodes.push(w.path.clone());
                    }

                    let cycle_key = format!("{}:{:?}", edge_type, cycle_nodes);
                    if !seen_cycles.contains(&cycle_key) {
                        let _ = seen_cycles.insert(cycle_key);
                        cycles.push(CircularDependency {
                            edge_type: edge_type.to_string(),
                            cycle: cycle_nodes,
                        });
                    }
                } else if !visited.contains(&next) {
                    self.dfs_find_cycles(
                        next,
                        adj,
                        visited,
                        rec_stack,
                        edge_type,
                        cycles,
                        seen_cycles,
                    );
                }
            }
        }

        let _ = rec_stack.pop();
    }

    /// Detect unattached orphan ADR notes (ADR notes with no inbound or outbound structural links).
    pub fn detect_orphan_adrs(&self, adr_paths: &[String]) -> Vec<OrphanAdr> {
        let mut orphans = Vec::new();

        for path in adr_paths {
            if let Some(&idx) = self.node_map.get(path) {
                let structural_in = self
                    .graph
                    .edges_directed(idx, Direction::Incoming)
                    .filter(|e| e.weight().class.matches(EdgeClass::Structural))
                    .count();
                let structural_out = self
                    .graph
                    .edges_directed(idx, Direction::Outgoing)
                    .filter(|e| e.weight().class.matches(EdgeClass::Structural))
                    .count();

                if structural_in + structural_out == 0 {
                    let title = self.graph.node_weight(idx).and_then(|n| n.title.clone());
                    orphans.push(OrphanAdr {
                        path: path.clone(),
                        title,
                        reason: "ADR note has no inbound or outbound structural links".to_string(),
                    });
                }
            } else {
                orphans.push(OrphanAdr {
                    path: path.clone(),
                    title: None,
                    reason: "ADR note is not connected to the knowledge graph".to_string(),
                });
            }
        }

        orphans
    }

    // ─── Internal Helpers ────────────────────────────────────────────────────

    /// Rebuild node_map from the graph (needed after node removal due to index swapping).
    fn rebuild_node_map(&mut self) {
        self.node_map.clear();
        for idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight(idx) {
                let _ = self.node_map.insert(node.path.clone(), idx);
            }
        }
    }
}

/// A step or node in a lineage traversal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineageNode {
    /// Document path.
    pub path: String,
    /// Document title if known.
    pub title: Option<String>,
    /// Hop distance from the start node (0 for start note).
    pub depth: usize,
    /// Edge type traversed to reach this note.
    pub edge_type: String,
    /// Direction traversed ("start", "outgoing", "incoming").
    pub direction: String,
}

/// Broken link detected in taxonomy validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrokenLink {
    /// Source document path containing the link.
    pub source: String,
    /// Target path or wikilink that could not be resolved.
    pub target: String,
    /// Edge type (e.g. "Wikilink", "supersedes", etc.).
    pub edge_type: String,
    /// Edge provenance.
    pub provenance: EdgeProvenance,
}

/// Circular dependency detected in a directed acyclic relation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CircularDependency {
    /// The edge type where the cycle exists (e.g. "supersedes").
    pub edge_type: String,
    /// The cycle path (e.g. ["A.md", "B.md", "A.md"]).
    pub cycle: Vec<String>,
}

/// Orphan ADR detected in taxonomy validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrphanAdr {
    /// Path to the orphan ADR note.
    pub path: String,
    /// Note title if known.
    pub title: Option<String>,
    /// Human-readable explanation.
    pub reason: String,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Community Detection (Louvain) ──────────────────────────────────────────

/// A detected community of nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    /// Community identifier.
    pub id: usize,
    /// Paths of nodes in this community.
    pub members: Vec<String>,
    /// Modularity contribution of this community to the overall partition.
    pub modularity_contribution: f64,
}

/// Result of community detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDetectionResult {
    /// Detected communities.
    pub communities: Vec<Community>,
    /// Overall modularity of the partition (Q ∈ [-0.5, 1.0]).
    pub modularity: f64,
    /// Number of iterations the algorithm ran.
    pub iterations: usize,
}

/// Per-community density statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDensity {
    /// Community id.
    pub community_id: usize,
    /// Number of nodes in the community.
    pub node_count: usize,
    /// Number of internal edges (edges within the community, treating as undirected).
    pub internal_edges: usize,
    /// Density: internal_edges / max_possible_internal_edges.
    pub density: f64,
}

impl KnowledgeGraph {
    /// Detect communities using the Louvain modularity-based algorithm.
    ///
    /// Treats the directed graph as undirected for community detection
    /// (each directed edge contributes weight in both directions).
    /// Returns communities sorted by size (largest first).
    pub fn detect_communities(&self) -> CommunityDetectionResult {
        let node_count = self.graph.node_count();
        if node_count == 0 {
            return CommunityDetectionResult {
                communities: Vec::new(),
                modularity: 0.0,
                iterations: 0,
            };
        }

        // Build undirected adjacency with weights for Louvain.
        // node_indices in petgraph may not be contiguous after removals,
        // so map them to a compact 0..N range.
        let indices: Vec<NodeIndex> = self.graph.node_indices().collect();
        let n = indices.len();
        let mut idx_to_compact: HashMap<NodeIndex, usize> = HashMap::new();
        for (i, &idx) in indices.iter().enumerate() {
            let _ = idx_to_compact.insert(idx, i);
        }

        // Build symmetric adjacency: adj[i][j] = sum of weights between i and j (undirected).
        // For the Louvain formula, we store the symmetric weight A_{ij}.
        let mut adj: HashMap<(usize, usize), f64> = HashMap::new();
        // m = sum of all edge weights (each directed edge counted once).
        let mut m: f64 = 0.0;

        for edge in self.graph.edge_references() {
            let src = *idx_to_compact.get(&edge.source()).unwrap();
            let tgt = *idx_to_compact.get(&edge.target()).unwrap();
            let w = edge.weight().weight as f64;

            if src != tgt {
                *adj.entry((src, tgt)).or_insert(0.0) += w;
                *adj.entry((tgt, src)).or_insert(0.0) += w;
                m += w; // Each directed edge adds w to total
            }
        }

        // If no edges, each node is its own community.
        if m == 0.0 {
            let communities: Vec<Community> = indices
                .iter()
                .enumerate()
                .map(|(i, &idx)| {
                    let path = self.graph.node_weight(idx).unwrap().path.clone();
                    Community { id: i, members: vec![path], modularity_contribution: 0.0 }
                })
                .collect();
            return CommunityDetectionResult { communities, modularity: 0.0, iterations: 0 };
        }

        // In the Louvain formula, we use 2m as the normalization.
        // k_i = sum of weights of edges incident to node i (in the undirected sense).
        // Since adj is symmetric, k_i = sum_j adj[i][j].
        let two_m = 2.0 * m;

        // Compute degree (sum of adj weights) per node.
        let mut k: Vec<f64> = vec![0.0; n];
        for (&(src, _tgt), &w) in &adj {
            k[src] += w;
        }

        // Build neighbor lists for efficiency.
        let mut neighbors: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for (&(src, tgt), &w) in &adj {
            neighbors[src].push((tgt, w));
        }

        // Initialize: each node in its own community.
        let mut community: Vec<usize> = (0..n).collect();

        // sigma_tot[c] = sum of k_i for all nodes i in community c.
        let mut sigma_tot: Vec<f64> = k.clone();
        // sigma_in[c] = sum of adj weights for edges within community c (both endpoints in c).
        let mut sigma_in: Vec<f64> = vec![0.0; n];

        let mut iterations = 0;
        let max_iterations = 100;

        loop {
            iterations += 1;
            let mut improved = false;

            for i in 0..n {
                let ci = community[i];
                let ki = k[i];

                // Compute weight of edges from i to nodes in each neighboring community.
                let mut community_weights: HashMap<usize, f64> = HashMap::new();
                for &(j, w) in &neighbors[i] {
                    let cj = community[j];
                    *community_weights.entry(cj).or_insert(0.0) += w;
                }

                // k_{i,in} = weight from i to nodes in its own community.
                let ki_in_own = *community_weights.get(&ci).unwrap_or(&0.0);

                // Remove i from its community.
                sigma_in[ci] -= ki_in_own;
                sigma_tot[ci] -= ki;

                // Find best community to place i.
                let mut best_community = ci;
                let mut best_delta_q = 0.0f64;

                for (&cj, &ki_in_cj) in &community_weights {
                    // Delta Q for moving i into community cj:
                    // delta_Q = [ki_in_cj / (2m)] - [sigma_tot[cj] * ki / (2m)^2]
                    // Simplified: delta_Q = (ki_in_cj - sigma_tot[cj] * ki / two_m) / two_m
                    let delta_q = ki_in_cj / two_m - (sigma_tot[cj] * ki) / (two_m * two_m);
                    if delta_q > best_delta_q {
                        best_delta_q = delta_q;
                        best_community = cj;
                    }
                }

                // Also consider putting i back in its own community (delta_q = 0 baseline).
                // The baseline gain from removing from ci is already subtracted.
                // So if no community gives positive delta, stay in ci.
                if best_delta_q <= 0.0 {
                    best_community = ci;
                }

                // Place i in best_community.
                community[i] = best_community;
                let ki_in_best = *community_weights.get(&best_community).unwrap_or(&0.0);
                sigma_in[best_community] += ki_in_best;
                sigma_tot[best_community] += ki;

                if best_community != ci {
                    improved = true;
                }
            }

            if !improved || iterations >= max_iterations {
                break;
            }
        }

        // Build community membership map.
        let mut community_members: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, &c) in community.iter().enumerate() {
            community_members.entry(c).or_default().push(i);
        }

        // Compute overall modularity and per-community contributions.
        // Q = sum_c [ (sigma_in_c / 2m) - (sigma_tot_c / 2m)^2 ]
        let mut q: f64 = 0.0;
        let mut communities_out: Vec<Community> = Vec::new();
        let mut community_id = 0;

        for (_c, members) in &community_members {
            let member_set: HashSet<usize> = members.iter().copied().collect();

            // sigma_in: sum of adj weights within community (each internal edge counted in both directions).
            let mut s_in: f64 = 0.0;
            // sigma_tot: sum of k_i for all nodes in community.
            let mut s_tot: f64 = 0.0;

            for &i in members {
                s_tot += k[i];
                for &(j, w) in &neighbors[i] {
                    if member_set.contains(&j) {
                        s_in += w;
                    }
                }
            }

            // Q_c = (s_in / 2m) - (s_tot / 2m)^2
            let modularity_contribution = (s_in / two_m) - (s_tot / two_m).powi(2);
            q += modularity_contribution;

            let member_paths: Vec<String> = members
                .iter()
                .map(|&i| {
                    let idx = indices[i];
                    self.graph.node_weight(idx).unwrap().path.clone()
                })
                .collect();

            communities_out.push(Community {
                id: community_id,
                members: member_paths,
                modularity_contribution,
            });
            community_id += 1;
        }

        // Sort by size descending.
        communities_out.sort_by(|a, b| b.members.len().cmp(&a.members.len()));
        // Reassign IDs after sorting.
        for (i, c) in communities_out.iter_mut().enumerate() {
            c.id = i;
        }

        CommunityDetectionResult { communities: communities_out, modularity: q, iterations }
    }

    /// Compute per-community density statistics.
    ///
    /// Uses the current community assignments from `detect_communities()`.
    pub fn community_densities(&self) -> Vec<CommunityDensity> {
        let result = self.detect_communities();
        let mut densities = Vec::new();

        for community in &result.communities {
            let node_count = community.members.len();
            if node_count <= 1 {
                densities.push(CommunityDensity {
                    community_id: community.id,
                    node_count,
                    internal_edges: 0,
                    density: 0.0,
                });
                continue;
            }

            let member_set: HashSet<&str> = community.members.iter().map(|s| s.as_str()).collect();

            // Count internal edges (directed edges where both endpoints are in community).
            let mut internal_edges = 0usize;
            for member in &community.members {
                if let Some(&idx) = self.node_map.get(member) {
                    for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                        let target_idx = edge.target();
                        if let Some(target_node) = self.graph.node_weight(target_idx) {
                            if member_set.contains(target_node.path.as_str()) {
                                internal_edges += 1;
                            }
                        }
                    }
                }
            }

            // Max possible directed edges in community = n * (n - 1).
            let max_edges = node_count * (node_count - 1);
            let density =
                if max_edges > 0 { internal_edges as f64 / max_edges as f64 } else { 0.0 };

            densities.push(CommunityDensity {
                community_id: community.id,
                node_count,
                internal_edges,
                density,
            });
        }

        densities
    }
}

/// Extract target paths from a frontmatter field value.
/// Handles both single string and array of strings.
fn extract_frontmatter_targets(frontmatter: &serde_json::Value, field: &str) -> Vec<String> {
    match frontmatter.get(field) {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(arr)) => {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[allow(unused_results)]
mod tests {
    use super::*;
    use ctxvault_common::config::{EdgeClass, EdgeSource, EdgeTypeConfig};
    use ctxvault_common::types::{Document, WikiLink};

    fn make_doc(path: &str, title: &str, tags: Vec<&str>, wikilinks: Vec<&str>) -> Document {
        Document {
            path: path.to_string(),
            frontmatter: None,
            title: Some(title.to_string()),
            tags: tags.into_iter().map(|t| t.to_string()).collect(),
            wikilinks: wikilinks
                .into_iter()
                .map(|t| WikiLink { target: t.to_string(), alias: None })
                .collect(),
            template: None,
            content: String::new(),
            content_hash: String::new(),
        }
    }

    fn wikilink_config() -> EdgeTypeConfig {
        EdgeTypeConfig {
            name: "Wikilink".to_string(),
            source: EdgeSource::Wikilink,
            weight: 1.0,
            bidirectional: false,
            field: None,
            direction: None,
            max_frequency: None,
            class: None,
            description: None,
            allowed_source_templates: None,
            allowed_target_templates: None,
        }
    }

    fn tag_config() -> EdgeTypeConfig {
        EdgeTypeConfig {
            name: "SharedTag".to_string(),
            source: EdgeSource::Tag,
            weight: 0.5,
            bidirectional: false,
            field: None,
            direction: None,
            max_frequency: None,
            class: None,
            description: None,
            allowed_source_templates: None,
            allowed_target_templates: None,
        }
    }

    #[test]
    fn test_add_remove_nodes() {
        let mut graph = KnowledgeGraph::new();

        graph.add_node("a.md", Some("A"));
        graph.add_node("b.md", Some("B"));
        graph.add_node("c.md", Some("C"));
        assert_eq!(graph.node_count(), 3);

        graph.remove_node("b.md").unwrap();
        assert_eq!(graph.node_count(), 2);
        assert!(graph.get_node("b.md").is_none());
        assert!(graph.get_node("a.md").is_some());
        assert!(graph.get_node("c.md").is_some());

        // Removing a non-existent node returns error.
        assert!(graph.remove_node("nonexistent.md").is_err());
    }

    #[test]
    fn test_add_edges() {
        let mut graph = KnowledgeGraph::new();

        graph.add_node("a.md", Some("A"));
        graph.add_node("b.md", Some("B"));
        graph.add_node("c.md", Some("C"));

        graph.add_edge(
            "a.md",
            "b.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );
        graph.add_edge(
            "b.md",
            "c.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );
        assert_eq!(graph.edge_count(), 2);

        // Traversal from a should reach b and c.
        let results = graph.traverse_bfs("a.md", 3, None, None);
        let paths: Vec<&str> = results.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"b.md"));
        assert!(paths.contains(&"c.md"));
    }

    #[test]
    fn test_build_edges_from_wikilinks() {
        let mut graph = KnowledgeGraph::new();

        let doc = make_doc("note.md", "Note", vec![], vec!["target1.md", "target2.md"]);
        let configs = vec![wikilink_config()];

        graph.build_edges_for_document(&doc, &configs, std::slice::from_ref(&doc));

        assert_eq!(graph.edge_count(), 2);
        assert!(graph.get_node("target1.md").is_some());
        assert!(graph.get_node("target2.md").is_some());

        // Verify forward links.
        let fwd = graph.forwardlinks("note.md", None);
        let targets = fwd.get("Wikilink").unwrap();
        assert!(targets.contains(&"target1.md".to_string()));
        assert!(targets.contains(&"target2.md".to_string()));
    }

    #[test]
    fn test_build_edges_from_shared_tags() {
        let mut graph = KnowledgeGraph::new();

        let doc_a = make_doc("a.md", "A", vec!["rust", "async"], vec![]);
        let doc_b = make_doc("b.md", "B", vec!["rust"], vec![]);
        let doc_c = make_doc("c.md", "C", vec!["python"], vec![]);

        let all_docs = vec![doc_a.clone(), doc_b.clone(), doc_c.clone()];
        let configs = vec![tag_config()];

        graph.build_edges_for_document(&doc_a, &configs, &all_docs);

        // a shares "rust" with b, but not with c.
        assert!(graph.edge_count() >= 1);
        let fwd = graph.forwardlinks("a.md", None);
        let shared = fwd.get("SharedTag").unwrap();
        assert!(shared.contains(&"b.md".to_string()));
        assert!(!shared.contains(&"c.md".to_string()));
    }

    #[test]
    fn test_bfs_traversal() {
        let mut graph = KnowledgeGraph::new();

        // Chain: A → B → C → D → E
        graph.add_edge("A", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("C", "D", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("D", "E", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        let results = graph.traverse_bfs("A", 2, None, None);
        let paths: Vec<&str> = results.iter().map(|(p, _)| p.as_str()).collect();

        assert!(paths.contains(&"B"));
        assert!(paths.contains(&"C"));
        assert!(!paths.contains(&"D"));
        assert!(!paths.contains(&"E"));

        // Verify depths.
        let b_depth = results.iter().find(|(p, _)| p == "B").unwrap().1;
        let c_depth = results.iter().find(|(p, _)| p == "C").unwrap().1;
        assert_eq!(b_depth, 1);
        assert_eq!(c_depth, 2);
    }

    #[test]
    fn test_dfs_traversal() {
        let mut graph = KnowledgeGraph::new();

        // Chain: A → B → C → D → E
        graph.add_edge("A", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("C", "D", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("D", "E", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        let results = graph.traverse_dfs("A", 3, None, None);
        let paths: Vec<&str> = results.iter().map(|(p, _)| p.as_str()).collect();

        // DFS should reach B, C, D within depth 3.
        assert!(paths.contains(&"B"));
        assert!(paths.contains(&"C"));
        assert!(paths.contains(&"D"));
        // E is at depth 4, so should not appear.
        assert!(!paths.contains(&"E"));
    }

    #[test]
    fn test_save_load() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node("a.md", Some("A"));
        graph.add_node("b.md", Some("B"));
        graph.add_edge(
            "a.md",
            "b.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.bin");

        graph.save(&path).unwrap();

        let loaded = KnowledgeGraph::load(&path).unwrap();
        assert_eq!(loaded.node_count(), graph.node_count());
        assert_eq!(loaded.edge_count(), graph.edge_count());
        assert!(loaded.get_node("a.md").is_some());
        assert!(loaded.get_node("b.md").is_some());

        // Verify edge is preserved.
        let fwd = loaded.forwardlinks("a.md", None);
        assert!(fwd.get("Link").unwrap().contains(&"b.md".to_string()));
    }

    #[test]
    fn test_backlinks_forwardlinks() {
        let mut graph = KnowledgeGraph::new();
        graph.add_edge("A", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("A", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        // backlinks(B) should contain A.
        let bl = graph.backlinks("B", None);
        let sources = bl.get("Link").unwrap();
        assert!(sources.contains(&"A".to_string()));
        assert_eq!(sources.len(), 1);

        // forwardlinks(A) should contain B and C.
        let fl = graph.forwardlinks("A", None);
        let targets = fl.get("Link").unwrap();
        assert!(targets.contains(&"B".to_string()));
        assert!(targets.contains(&"C".to_string()));
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn test_shortest_path() {
        let mut graph = KnowledgeGraph::new();
        graph.add_edge("A", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("C", "D", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        let path = graph.shortest_path("A", "D", None, None).unwrap();
        assert_eq!(path, vec!["A", "B", "C", "D"]);

        // No path from D to A (directed graph).
        assert!(graph.shortest_path("D", "A", None, None).is_none());

        // Non-existent node.
        assert!(graph.shortest_path("A", "Z", None, None).is_none());
    }

    #[test]
    fn test_graph_stats() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node("orphan.md", Some("Orphan"));
        graph.add_edge(
            "a.md",
            "b.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );
        graph.add_edge("a.md", "c.md", "Tag", 0.5, EdgeProvenance::SharedTag, EdgeClass::Semantic);
        graph.add_edge(
            "b.md",
            "c.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );

        let stats = graph.stats();
        assert_eq!(stats.node_count, 4); // orphan, a, b, c
        assert_eq!(stats.edge_count, 3);
        assert_eq!(stats.orphan_count, 1);
        assert_eq!(*stats.edge_type_distribution.get("Link").unwrap(), 2);
        assert_eq!(*stats.edge_type_distribution.get("Tag").unwrap(), 1);

        // Most connected: a.md has 2 outgoing, b.md has 1 in + 1 out = 2, c.md has 2 in.
        assert!(!stats.most_connected.is_empty());
    }

    // ─── Community Detection Tests ─────────────────────────────────────

    #[test]
    fn test_community_detection_empty_graph() {
        let graph = KnowledgeGraph::new();
        let result = graph.detect_communities();
        assert!(result.communities.is_empty());
        assert_eq!(result.modularity, 0.0);
        assert_eq!(result.iterations, 0);
    }

    #[test]
    fn test_community_detection_single_node() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node("single.md", Some("Single"));

        let result = graph.detect_communities();
        // Single node with no edges: own community.
        assert_eq!(result.communities.len(), 1);
        assert_eq!(result.communities[0].members.len(), 1);
        assert_eq!(result.communities[0].members[0], "single.md");
    }

    #[test]
    fn test_community_detection_two_cliques() {
        // Two obvious communities: A-B-C fully connected, D-E-F fully connected.
        let mut graph = KnowledgeGraph::new();

        // Clique 1: A, B, C
        graph.add_edge("A", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "A", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("A", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("C", "A", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("C", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        // Clique 2: D, E, F
        graph.add_edge("D", "E", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("E", "D", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("D", "F", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("F", "D", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("E", "F", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("F", "E", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        // Single weak link between cliques.
        graph.add_edge("C", "D", "Link", 0.1, EdgeProvenance::Wikilink, EdgeClass::Structural);

        let result = graph.detect_communities();

        // Should detect 2 communities.
        assert_eq!(
            result.communities.len(),
            2,
            "Expected 2 communities, got {}: {:?}",
            result.communities.len(),
            result.communities
        );

        // Modularity should be positive (good partition).
        assert!(
            result.modularity > 0.0,
            "Modularity should be positive for well-separated clusters: {}",
            result.modularity
        );

        // Each community should have 3 members.
        let mut sizes: Vec<usize> = result.communities.iter().map(|c| c.members.len()).collect();
        sizes.sort();
        assert_eq!(sizes, vec![3, 3]);

        // Verify that A, B, C are in the same community.
        let abc_community =
            result.communities.iter().find(|c| c.members.contains(&"A".to_string())).unwrap();
        assert!(abc_community.members.contains(&"B".to_string()));
        assert!(abc_community.members.contains(&"C".to_string()));

        // Verify that D, E, F are in the same community.
        let def_community =
            result.communities.iter().find(|c| c.members.contains(&"D".to_string())).unwrap();
        assert!(def_community.members.contains(&"E".to_string()));
        assert!(def_community.members.contains(&"F".to_string()));
    }

    #[test]
    fn test_community_detection_disconnected_components() {
        // Two disconnected components should be in separate communities.
        let mut graph = KnowledgeGraph::new();

        graph.add_edge("A", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "A", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        graph.add_edge("C", "D", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("D", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        let result = graph.detect_communities();

        // Should have at least 2 communities.
        assert!(
            result.communities.len() >= 2,
            "Disconnected components should be in separate communities"
        );

        // A and B should be together.
        let ab_community =
            result.communities.iter().find(|c| c.members.contains(&"A".to_string())).unwrap();
        assert!(ab_community.members.contains(&"B".to_string()));

        // C and D should be together.
        let cd_community =
            result.communities.iter().find(|c| c.members.contains(&"C".to_string())).unwrap();
        assert!(cd_community.members.contains(&"D".to_string()));

        // A and C should NOT be in the same community.
        assert!(!ab_community.members.contains(&"C".to_string()));
    }

    #[test]
    fn test_community_detection_no_edges() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node("A", None);
        graph.add_node("B", None);
        graph.add_node("C", None);

        let result = graph.detect_communities();

        // No edges means each node is its own community.
        assert_eq!(result.communities.len(), 3);
        assert_eq!(result.modularity, 0.0);
    }

    #[test]
    fn test_community_densities() {
        let mut graph = KnowledgeGraph::new();

        // Fully connected triangle.
        graph.add_edge("A", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "A", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("A", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("C", "A", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("B", "C", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);
        graph.add_edge("C", "B", "Link", 1.0, EdgeProvenance::Wikilink, EdgeClass::Structural);

        let densities = graph.community_densities();

        // Should have at least one community.
        assert!(!densities.is_empty());

        // The fully connected triangle community should have density 1.0.
        let triangle = densities
            .iter()
            .find(|d| d.node_count == 3)
            .expect("Should find a community with 3 nodes");
        assert!(
            (triangle.density - 1.0).abs() < 0.001,
            "Fully connected triangle should have density 1.0, got {}",
            triangle.density
        );
    }

    // ─── Structural Lineage & Taxonomy Tests ───────────────────────────────

    #[test]
    fn test_traverse_lineage_outgoing() {
        let mut graph = KnowledgeGraph::new();
        // ADR-003 supersedes ADR-002, which supersedes ADR-001
        graph.add_edge(
            "docs/adrs/003.md",
            "docs/adrs/002.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        graph.add_edge(
            "docs/adrs/002.md",
            "docs/adrs/001.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let chain = graph.traverse_lineage("docs/adrs/003.md", "supersedes", "outgoing", 5);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].path, "docs/adrs/003.md");
        assert_eq!(chain[0].depth, 0);
        assert_eq!(chain[0].direction, "start");

        assert_eq!(chain[1].path, "docs/adrs/002.md");
        assert_eq!(chain[1].depth, 1);
        assert_eq!(chain[1].direction, "outgoing");

        assert_eq!(chain[2].path, "docs/adrs/001.md");
        assert_eq!(chain[2].depth, 2);
        assert_eq!(chain[2].direction, "outgoing");
    }

    #[test]
    fn test_traverse_lineage_incoming() {
        let mut graph = KnowledgeGraph::new();
        // ADR-003 supersedes ADR-002, which supersedes ADR-001
        graph.add_edge(
            "docs/adrs/003.md",
            "docs/adrs/002.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        graph.add_edge(
            "docs/adrs/002.md",
            "docs/adrs/001.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let chain = graph.traverse_lineage("docs/adrs/001.md", "supersedes", "incoming", 5);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].path, "docs/adrs/001.md");
        assert_eq!(chain[0].depth, 0);

        assert_eq!(chain[1].path, "docs/adrs/002.md");
        assert_eq!(chain[1].depth, 1);
        assert_eq!(chain[1].direction, "incoming");

        assert_eq!(chain[2].path, "docs/adrs/003.md");
        assert_eq!(chain[2].depth, 2);
        assert_eq!(chain[2].direction, "incoming");
    }

    #[test]
    fn test_traverse_lineage_handles_cycle() {
        let mut graph = KnowledgeGraph::new();
        // Cycle: A -> B -> A
        graph.add_edge(
            "A.md",
            "B.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        graph.add_edge(
            "B.md",
            "A.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let chain = graph.traverse_lineage("A.md", "supersedes", "outgoing", 5);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].path, "A.md");
        assert_eq!(chain[1].path, "B.md");
    }

    #[test]
    fn test_extract_lineage_for_node() {
        let mut graph = KnowledgeGraph::new();
        // Note B supersedes Note A
        graph.add_edge(
            "B.md",
            "A.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        // Note A implements Spec C
        graph.add_edge(
            "A.md",
            "C.md",
            "implements",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        // Note D depends on Note A
        graph.add_edge(
            "D.md",
            "A.md",
            "depends_on",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let lineage = graph.extract_lineage_for_node("A.md").expect("Should have lineage for A.md");
        assert_eq!(lineage.superseded_by, vec!["B.md"]);
        assert_eq!(lineage.implements, vec!["C.md"]);
        assert_eq!(lineage.depended_on_by, vec!["D.md"]);
        assert!(lineage.supersedes.is_empty());
    }

    #[test]
    fn test_detect_broken_links() {
        let mut graph = KnowledgeGraph::new();
        graph.add_edge(
            "docs/valid.md",
            "docs/missing.md",
            "Wikilink",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );
        graph.add_edge(
            "docs/valid.md",
            "docs/existing.md",
            "Wikilink",
            1.0,
            EdgeProvenance::Wikilink,
            EdgeClass::Structural,
        );

        let mut existing = HashSet::new();
        existing.insert("docs/valid.md".to_string());
        existing.insert("docs/existing.md".to_string());

        let broken = graph.detect_broken_links(&existing);
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].source, "docs/valid.md");
        assert_eq!(broken[0].target, "docs/missing.md");
    }

    #[test]
    fn test_detect_circular_dependencies() {
        let mut graph = KnowledgeGraph::new();
        // A -> B -> C -> A
        graph.add_edge(
            "A.md",
            "B.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        graph.add_edge(
            "B.md",
            "C.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );
        graph.add_edge(
            "C.md",
            "A.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let cycles = graph.detect_circular_dependencies(&["supersedes"]);
        assert!(!cycles.is_empty());
        assert_eq!(cycles[0].edge_type, "supersedes");
        assert!(cycles[0].cycle.contains(&"A.md".to_string()));
        assert!(cycles[0].cycle.contains(&"B.md".to_string()));
        assert!(cycles[0].cycle.contains(&"C.md".to_string()));
    }

    #[test]
    fn test_detect_orphan_adrs() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node("docs/adrs/001.md", Some("Connected ADR"));
        graph.add_node("docs/adrs/002.md", Some("Orphan ADR"));
        graph.add_node("docs/specs/engine.md", Some("Spec"));

        graph.add_edge(
            "docs/adrs/001.md",
            "docs/specs/engine.md",
            "adr_for",
            1.0,
            EdgeProvenance::Frontmatter,
            EdgeClass::Structural,
        );

        let adr_paths = vec!["docs/adrs/001.md".to_string(), "docs/adrs/002.md".to_string()];

        let orphans = graph.detect_orphan_adrs(&adr_paths);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].path, "docs/adrs/002.md");
    }
}
