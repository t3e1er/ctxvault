//! Tiered Level-of-Detail (LOD) progressive disclosure layout pipelines.
//!
//! Enforces visual progressive disclosure:
//! - Tier 0: Galaxy Overview (~1,000 hubs and community centroids)
//! - Tier 1: Corpus Shell (budget-capped at 25,000–50,000 nodes)
//! - Tier 2: Local Ego Subgraph (1–3 hops around clicked entity)

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::layout::{
    assign_color_and_type, compute_force_layout, GraphLayout, LayoutConfig, NodeLayout,
};
use crate::loader::{CorpusCatalog, CorpusSnapshot};

/// Maximum nodes allowed in Tier 0 Galaxy view.
pub const TIER_0_BUDGET: usize = 1000;
/// Default node budget for Tier 1 Corpus view.
pub const TIER_1_DEFAULT_BUDGET: usize = 25000;

/// Compute Tier 0: Galaxy Overview across all loaded corpora or a specific corpus.
pub fn build_tier_0_overview(catalog: &CorpusCatalog) -> GraphLayout {
    let mut all_nodes = Vec::new();
    let mut all_edges = Vec::new();

    let corpus_names = catalog.corpus_names();
    let num_corpora = corpus_names.len().max(1);

    for (c_idx, name) in corpus_names.iter().enumerate() {
        let Some(snapshot) = catalog.get_corpus(name) else {
            continue;
        };

        // Budget per corpus in multi-corpus mode
        let per_corpus_budget = (TIER_0_BUDGET / num_corpora).max(50);
        let layout = build_tier_1_corpus(&snapshot, per_corpus_budget);

        // Position offset for galaxy separation: distribute corpora around a wide circle
        let angle = 2.0 * std::f32::consts::PI * (c_idx as f32) / (num_corpora as f32);
        let galaxy_radius =
            if num_corpora > 1 { 800.0 + (num_corpora as f32) * 150.0 } else { 0.0 };
        let offset_x = angle.cos() * galaxy_radius;
        let offset_z = angle.sin() * galaxy_radius;

        let base_id = all_nodes.len() as u32;
        let mut id_map = HashMap::new();

        for (local_i, mut node) in layout.nodes.into_iter().enumerate() {
            let global_id = base_id + (local_i as u32);
            id_map.insert(node.id, global_id);

            node.id = global_id;
            node.position[0] += offset_x;
            node.position[2] += offset_z;
            all_nodes.push(node);
        }

        for mut edge in layout.edges {
            if let (Some(&src), Some(&tgt)) = (id_map.get(&edge.source), id_map.get(&edge.target)) {
                edge.source = src;
                edge.target = tgt;
                all_edges.push(edge);
            }
        }
    }

    GraphLayout {
        corpus: "all".to_string(),
        nodes: all_nodes,
        edges: all_edges,
        communities_count: num_corpora,
    }
}

/// Compute Tier 1: Corpus Shell layout bounded by a maximum node budget.
pub fn build_tier_1_corpus(snapshot: &CorpusSnapshot, budget: usize) -> GraphLayout {
    let pet_graph = snapshot.graph.inner();
    let total_nodes = pet_graph.node_count();

    if total_nodes == 0 {
        return GraphLayout {
            corpus: snapshot.name.clone(),
            nodes: Vec::new(),
            edges: Vec::new(),
            communities_count: 0,
        };
    }

    // Collect degree per node
    let mut degree_pairs: Vec<(petgraph::graph::NodeIndex, usize)> = pet_graph
        .node_indices()
        .map(|idx| {
            let in_d = pet_graph.edges_directed(idx, Direction::Incoming).count();
            let out_d = pet_graph.edges_directed(idx, Direction::Outgoing).count();
            (idx, in_d + out_d)
        })
        .collect();

    // Sort descending by degree for hub selection
    degree_pairs.sort_by(|a, b| b.1.cmp(&a.1));

    // Select top nodes within budget
    let selected_indices: HashSet<petgraph::graph::NodeIndex> =
        degree_pairs.iter().take(budget).map(|&(idx, _)| idx).collect();

    let mut paths = Vec::new();
    let mut titles = Vec::new();
    let mut degrees = Vec::new();
    let mut communities = Vec::new();
    let mut idx_to_local: HashMap<petgraph::graph::NodeIndex, usize> = HashMap::new();

    // Run community detection on full or sampled graph
    let community_res = snapshot.graph.detect_communities();
    let mut node_to_comm: HashMap<String, u32> = HashMap::new();
    for (comm_id, comm) in community_res.communities.iter().enumerate() {
        for member in &comm.members {
            node_to_comm.insert(member.clone(), comm_id as u32);
        }
    }

    for (local_idx, &pet_idx) in selected_indices.iter().enumerate() {
        let node_data = &pet_graph[pet_idx];
        idx_to_local.insert(pet_idx, local_idx);

        paths.push(node_data.path.clone());
        titles.push(node_data.title.clone());

        let deg = pet_graph.edges_directed(pet_idx, Direction::Incoming).count()
            + pet_graph.edges_directed(pet_idx, Direction::Outgoing).count();
        degrees.push(deg);

        let comm = node_to_comm.get(&node_data.path).copied().unwrap_or(0);
        communities.push(comm);
    }

    // Extract edges between selected nodes
    let mut raw_edges = Vec::new();
    for pet_edge in pet_graph.edge_references() {
        if let (Some(&src_loc), Some(&tgt_loc)) =
            (idx_to_local.get(&pet_edge.source()), idx_to_local.get(&pet_edge.target()))
        {
            let w = pet_edge.weight();
            raw_edges.push((src_loc, tgt_loc, w.edge_type.clone(), w.weight));
        }
    }

    let config = LayoutConfig {
        iterations: if paths.len() > 10000 { 20 } else { 35 },
        ..Default::default()
    };

    let (positions, edges) =
        compute_force_layout(&paths, &titles, &degrees, &communities, &raw_edges, &config);

    let nodes = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let (entity_type, color_rgb) = assign_color_and_type(&path, communities[i]);
            let deg = degrees[i];
            let size = 2.0 + (deg as f32).sqrt().min(15.0);

            NodeLayout {
                id: i as u32,
                path,
                title: titles[i].clone(),
                position: positions[i],
                degree: deg,
                community: communities[i],
                entity_type,
                color_rgb,
                size,
            }
        })
        .collect();

    GraphLayout {
        corpus: snapshot.name.clone(),
        nodes,
        edges,
        communities_count: community_res.communities.len().max(1),
    }
}

/// Compute Tier 2: Local Ego Subgraph around a focal node path up to `max_hops`.
pub fn build_tier_2_local(
    snapshot: &CorpusSnapshot,
    center_path: &str,
    max_hops: usize,
) -> Option<GraphLayout> {
    let pet_graph = snapshot.graph.inner();
    let center_idx = snapshot.graph.get_node(center_path)?;

    let mut visited: HashSet<petgraph::graph::NodeIndex> = HashSet::new();
    let mut queue: VecDeque<(petgraph::graph::NodeIndex, usize)> = VecDeque::new();

    visited.insert(center_idx);
    queue.push_back((center_idx, 0));

    // Breadth-First Search up to max_hops
    while let Some((curr, hops)) = queue.pop_front() {
        if hops >= max_hops {
            continue;
        }

        for edge in pet_graph.edges_directed(curr, Direction::Outgoing) {
            let target = edge.target();
            if visited.insert(target) {
                queue.push_back((target, hops + 1));
            }
        }
        for edge in pet_graph.edges_directed(curr, Direction::Incoming) {
            let source = edge.source();
            if visited.insert(source) {
                queue.push_back((source, hops + 1));
            }
        }
    }

    let mut paths = Vec::new();
    let mut titles = Vec::new();
    let mut degrees = Vec::new();
    let mut communities = Vec::new();
    let mut idx_to_local = HashMap::new();

    for (local_idx, &pet_idx) in visited.iter().enumerate() {
        let node_data = &pet_graph[pet_idx];
        idx_to_local.insert(pet_idx, local_idx);

        paths.push(node_data.path.clone());
        titles.push(node_data.title.clone());

        let deg = pet_graph.edges_directed(pet_idx, Direction::Incoming).count()
            + pet_graph.edges_directed(pet_idx, Direction::Outgoing).count();
        degrees.push(deg);
        communities.push(if pet_idx == center_idx { 1 } else { 0 });
    }

    let mut raw_edges = Vec::new();
    for &pet_idx in &visited {
        for edge in pet_graph.edges_directed(pet_idx, Direction::Outgoing) {
            if let (Some(&src_loc), Some(&tgt_loc)) =
                (idx_to_local.get(&pet_idx), idx_to_local.get(&edge.target()))
            {
                let w = edge.weight();
                raw_edges.push((src_loc, tgt_loc, w.edge_type.clone(), w.weight));
            }
        }
    }

    let config = LayoutConfig { iterations: 45, spring_length: 35.0, ..Default::default() };

    let (positions, edges) =
        compute_force_layout(&paths, &titles, &degrees, &communities, &raw_edges, &config);

    let nodes = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let is_center = path == center_path;
            let (entity_type, mut color) = assign_color_and_type(&path, communities[i]);
            if is_center {
                color = 0xf59e0b; // Bright amber for focal node
            }
            let deg = degrees[i];
            let size = if is_center { 12.0 } else { 4.0 + (deg as f32).sqrt().min(8.0) };

            NodeLayout {
                id: i as u32,
                path,
                title: titles[i].clone(),
                position: positions[i],
                degree: deg,
                community: communities[i],
                entity_type,
                color_rgb: color,
                size,
            }
        })
        .collect();

    Some(GraphLayout { corpus: snapshot.name.clone(), nodes, edges, communities_count: 2 })
}
