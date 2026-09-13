//! 3D force-directed layout computation using Barnes-Hut n-body simulation
//! with directory module and community anchor springs.

pub mod octree;
pub mod tiered;

use std::collections::HashMap;

use octree::Octree;
use serde::{Deserialize, Serialize};

/// Method used to group nodes into spatial cluster anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterMode {
    /// Group by filesystem directory module hierarchy (first 2–3 path components).
    Directory,
    /// Group by topological graph communities (Leiden/Louvain modularity).
    Community,
}

impl Default for ClusterMode {
    fn default() -> Self {
        Self::Directory
    }
}

/// 3D position and visual properties for a single graph node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLayout {
    /// Internal integer identifier (0..N).
    pub id: u32,
    /// Authoritative repository-relative path or symbol signature.
    pub path: String,
    /// Document or symbol display title.
    pub title: Option<String>,
    /// Precomputed 3D Cartesian coordinates [x, y, z].
    pub position: [f32; 3],
    /// Total node degree (in-degree + out-degree).
    pub degree: usize,
    /// Community cluster index (from Leiden/Louvain).
    pub community: u32,
    /// Entity class/kind (DocNode, Function, Struct, Module, etc.).
    pub entity_type: String,
    /// Packed RGB color (0xRRGGBB).
    pub color_rgb: u32,
    /// Display radius size for WebGL shaders.
    pub size: f32,
}

/// Visual connection between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeLayout {
    /// Source node identifier.
    pub source: u32,
    /// Target node identifier.
    pub target: u32,
    /// Relationship type name (e.g. calls, defines, imports, wikilink).
    pub edge_type: String,
    /// Normalized edge weight.
    pub weight: f32,
}

/// Fully computed 3D scene payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphLayout {
    /// Corpus identifier or "all" for multi-corpus galaxy.
    pub corpus: String,
    /// List of placed nodes.
    pub nodes: Vec<NodeLayout>,
    /// List of placed edges.
    pub edges: Vec<EdgeLayout>,
    /// Total distinct communities identified.
    pub communities_count: usize,
}

/// Configuration parameters for the 3D force layout simulation.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Number of simulation relaxation steps.
    pub iterations: usize,
    /// Repulsion constant ($k_{\text{rep}}$).
    pub repulsion: f32,
    /// Spring stiffness constant.
    pub spring_stiffness: f32,
    /// Ideal spring rest length.
    pub spring_length: f32,
    /// Velocity damping factor (0.0..1.0).
    pub damping: f32,
    /// Barnes-Hut opening angle threshold $\theta$ (typically 0.8).
    pub theta: f32,
    /// Center gravity pulling towards coordinate origin [0,0,0].
    pub center_gravity: f32,
    /// Anchor spring stiffness pulling nodes to their module/community center.
    pub anchor_strength: f32,
    /// Active clustering mode (Directory or Community).
    pub cluster_mode: ClusterMode,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            iterations: 40,
            repulsion: 6000.0,
            spring_stiffness: 0.06,
            spring_length: 45.0,
            damping: 0.85,
            theta: 0.8,
            center_gravity: 0.001,
            anchor_strength: 0.22,
            cluster_mode: ClusterMode::Directory,
        }
    }
}

/// Extract a 2-3 component directory cluster key from a relative path.
pub fn extract_directory_key(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 1 {
        return "root".to_string();
    }
    let dir_parts = &parts[..parts.len() - 1];
    let take_count = dir_parts.len().min(3);
    dir_parts[..take_count].join("/")
}

/// FNV-1a 32-bit hash for cluster strings.
pub fn fnv1a_hash(s: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for byte in s.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

/// Map entity type name to a distinct cyber-aesthetic neon color.
pub fn color_for_entity_type(entity_type: &str, community: u32) -> u32 {
    match entity_type.to_lowercase().as_str() {
        "docnode" | "doc" | "document" | "markdown" => 0x3b82f6, // Sapphire Blue
        "function" | "method" => 0x10b981,                       // Emerald Neon
        "struct" | "class" => 0x8b5cf6,                          // Electric Purple
        "trait" | "interface" => 0xec4899,                       // Hot Pink
        "enum" | "typealias" | "type" => 0xf59e0b,               // Amber
        "module" | "file" | "package" => 0x06b6d4,               // Cyan
        "constant" | "macro" => 0x14b8a6,                        // Teal
        _ => {
            let palette = [
                0x3b82f6, 0x10b981, 0x8b5cf6, 0xf59e0b, 0xec4899, 0x06b6d4, 0x14b8a6, 0x6366f1,
                0xe11d48, 0x84cc16,
            ];
            palette[(community as usize) % palette.len()]
        }
    }
}

/// Assign distinct aesthetic neon colors based on AST entity type, path heuristics, and community.
pub fn assign_color_and_type(
    path: &str,
    community: u32,
    ast_types: Option<&HashMap<String, String>>,
) -> (String, u32) {
    // 1. Authoritative AST metadata lookup from meta.db
    if let Some(types) = ast_types {
        if let Some(sym_type) = types.get(path) {
            let color = color_for_entity_type(sym_type, community);
            return (sym_type.clone(), color);
        }
        let clean_path = path.replace('\\', "/");
        if let Some(sub) = clean_path.split('#').nth(1) {
            if let Some(sym_type) = types.get(sub) {
                let color = color_for_entity_type(sym_type, community);
                return (sym_type.clone(), color);
            }
        }
        if let Some(sub) = clean_path.split("::").last() {
            if let Some(sym_type) = types.get(sub) {
                let color = color_for_entity_type(sym_type, community);
                return (sym_type.clone(), color);
            }
        }
        let base_name = clean_path.split('/').next_back().unwrap_or(path);
        if let Some(sym_type) = types.get(base_name) {
            let color = color_for_entity_type(sym_type, community);
            return (sym_type.clone(), color);
        }
    }

    // 2. Structural file heuristics
    if path.ends_with(".md") || path.contains("docs/") || path.contains("adr/") {
        ("DocNode".to_string(), 0x3b82f6) // Sapphire Blue
    } else if path.ends_with(".rs") || path.ends_with(".ts") || path.ends_with(".js") || path.ends_with(".py") || path.ends_with(".go") {
        ("Module".to_string(), 0x06b6d4) // Cyan
    } else if path.contains("::fn ") || path.contains("() ") || path.ends_with(".rs#") {
        ("Function".to_string(), 0x10b981) // Emerald Neon
    } else if path.contains("struct ") || path.contains("class ") || path.contains("interface ") {
        ("Struct".to_string(), 0x8b5cf6) // Electric Purple
    } else {
        // Community-based gradient fallback
        let palette = [
            0x3b82f6, 0x10b981, 0x8b5cf6, 0xf59e0b, 0xec4899, 0x06b6d4, 0x14b8a6, 0x6366f1,
            0xe11d48, 0x84cc16,
        ];
        let color = palette[(community as usize) % palette.len()];
        ("CodeSymbol".to_string(), color)
    }
}

/// Compute 3D anchor positions for each node based on the selected cluster mode.
pub fn compute_cluster_anchors(
    paths: &[String],
    communities: &[u32],
    mode: ClusterMode,
    base_radius: f32,
) -> Vec<[f32; 3]> {
    let n = paths.len();
    if n == 0 {
        return Vec::new();
    }

    match mode {
        ClusterMode::Directory => {
            let mut key_to_idx: HashMap<String, usize> = HashMap::new();
            let mut keys = Vec::new();
            let mut node_keys = Vec::with_capacity(n);

            for p in paths {
                let key = extract_directory_key(p);
                let next_idx = keys.len();
                let idx = *key_to_idx.entry(key.clone()).or_insert_with(|| {
                    keys.push(key);
                    next_idx
                });
                node_keys.push(idx);
            }

            let num_clusters = keys.len().max(1);
            let mut cluster_centers: Vec<[f32; 3]> = Vec::with_capacity(num_clusters);
            for k in 0..num_clusters {
                let k_f = k as f32;
                let n_f = num_clusters as f32;
                // Fibonacci 3D sphere distribution
                let y = 1.0 - (k_f / (n_f - 1.0).max(1.0)) * 2.0;
                let radius_at_y = (1.0 - y * y).max(0.0).sqrt();
                let theta = 2.399_963_2 * k_f; // Golden ratio spiral
                let x = theta.cos() * radius_at_y;
                let z = theta.sin() * radius_at_y;
                let h = fnv1a_hash(&keys[k]);
                let r = base_radius * (0.80 + (((h >> 16) & 0xFF) as f32 / 255.0) * 0.45);
                cluster_centers.push([x * r, y * r, z * r]);
            }

            let mut anchors = Vec::with_capacity(n);
            for &k_idx in &node_keys {
                anchors.push(cluster_centers[k_idx]);
            }
            anchors
        }
        ClusterMode::Community => {
            let mut comm_to_idx: HashMap<u32, usize> = HashMap::new();
            let mut distinct_comms = Vec::new();
            for &c in communities {
                let next_idx = distinct_comms.len();
                comm_to_idx.entry(c).or_insert_with(|| {
                    distinct_comms.push(c);
                    next_idx
                });
            }

            let num_comms = distinct_comms.len().max(1);
            let mut comm_centers: Vec<[f32; 3]> = Vec::with_capacity(num_comms);
            for c in 0..num_comms {
                let c_f = c as f32;
                let n_f = num_comms as f32;
                // Fibonacci 3D sphere distribution
                let y = 1.0 - (c_f / (n_f - 1.0).max(1.0)) * 2.0;
                let radius_at_y = (1.0 - y * y).max(0.0).sqrt();
                let theta = 2.399_963_2 * c_f;
                let x = theta.cos() * radius_at_y;
                let z = theta.sin() * radius_at_y;
                let r = base_radius * 0.90;
                comm_centers.push([x * r, y * r, z * r]);
            }

            let mut anchors = Vec::with_capacity(n);
            for &c in communities {
                let c_order = *comm_to_idx.get(&c).unwrap_or(&0);
                anchors.push(comm_centers[c_order]);
            }
            anchors
        }
    }
}

/// Run force simulation relaxation on given nodes and edges with anchor springs.
pub fn compute_force_layout(
    paths: &[String],
    _titles: &[Option<String>],
    degrees: &[usize],
    communities: &[u32],
    raw_edges: &[(usize, usize, String, f32)],
    config: &LayoutConfig,
) -> (Vec<[f32; 3]>, Vec<EdgeLayout>) {
    let n = paths.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }

    // 1. Calculate cluster anchors based on Directory or Community mode
    let base_radius = (n as f32).cbrt() * 45.0 + 350.0;
    let anchors = compute_cluster_anchors(paths, communities, config.cluster_mode, base_radius);

    // 2. Initial placement: 3D anchor center + 3D spherical volumetric jitter
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n);
    let mut velocities: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; n];
    let masses: Vec<f32> = degrees.iter().map(|&d| 1.0 + (d as f32).sqrt()).collect();

    for i in 0..n {
        let a = anchors[i];
        let h = fnv1a_hash(&paths[i]);
        let theta = ((h & 0xFFFF) as f32 / 65535.0) * 2.0 * std::f32::consts::PI;
        let phi = ((((h >> 16) & 0xFFFF) as f32 / 65535.0) - 0.5) * std::f32::consts::PI;
        let r_jitter = 15.0 + (((h >> 8) & 0xFF) as f32 / 255.0) * 90.0;

        let jx = r_jitter * phi.cos() * theta.cos();
        let jy = r_jitter * phi.sin();
        let jz = r_jitter * phi.cos() * theta.sin();

        positions.push([a[0] + jx, a[1] + jy, a[2] + jz]);
    }

    // 3. Multi-step relaxation loop
    let dt = 0.5f32;
    for _ in 0..config.iterations {
        // Build Barnes-Hut octree
        let octree = Octree::build(&positions, &masses, config.theta);

        // A. Repulsive forces in parallel
        let mut forces = octree.compute_all_repulsions(&positions, config.repulsion, 100.0);

        // B. Attractive spring forces along edges
        for &(src, tgt, _, w) in raw_edges {
            if src >= n || tgt >= n || src == tgt {
                continue;
            }
            let p1 = positions[src];
            let p2 = positions[tgt];

            let dx = p2[0] - p1[0];
            let dy = p2[1] - p1[1];
            let dz = p2[2] - p1[2];
            let dist = (dx * dx + dy * dy + dz * dz + 1.0).sqrt();

            let displacement = dist - config.spring_length;
            let f_mag = config.spring_stiffness * displacement * w;

            let fx = (dx / dist) * f_mag;
            let fy = (dy / dist) * f_mag;
            let fz = (dz / dist) * f_mag;

            forces[src][0] += fx;
            forces[src][1] += fy;
            forces[src][2] += fz;

            forces[tgt][0] -= fx;
            forces[tgt][1] -= fy;
            forces[tgt][2] -= fz;
        }

        // C. Anchor spring forces pulling back toward cluster anchor
        let k_anchor = config.anchor_strength;
        for i in 0..n {
            let p = positions[i];
            let a = anchors[i];
            forces[i][0] += (a[0] - p[0]) * k_anchor * masses[i];
            forces[i][1] += (a[1] - p[1]) * k_anchor * masses[i];
            forces[i][2] += (a[2] - p[2]) * k_anchor * masses[i];
        }

        // D. Center gravity & velocity integration
        for i in 0..n {
            let p = &mut positions[i];
            let v = &mut velocities[i];
            let f = forces[i];

            let gx = -p[0] * config.center_gravity;
            let gy = -p[1] * config.center_gravity;
            let gz = -p[2] * config.center_gravity;

            v[0] = (v[0] + (f[0] + gx) * dt) * config.damping;
            v[1] = (v[1] + (f[1] + gy) * dt) * config.damping;
            v[2] = (v[2] + (f[2] + gz) * dt) * config.damping;

            // Cap max displacement per step
            let max_speed = 35.0;
            let speed = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if speed > max_speed {
                let s = max_speed / speed;
                v[0] *= s;
                v[1] *= s;
                v[2] *= s;
            }

            p[0] += v[0] * dt;
            p[1] += v[1] * dt;
            p[2] += v[2] * dt;
        }
    }

    // Convert raw edges to EdgeLayout
    let edges = raw_edges
        .iter()
        .map(|&(s, t, ref rel, w)| EdgeLayout {
            source: s as u32,
            target: t as u32,
            edge_type: rel.clone(),
            weight: w,
        })
        .collect();

    (positions, edges)
}
