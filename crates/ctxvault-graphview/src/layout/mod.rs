//! 3D force-directed layout computation using Barnes-Hut n-body simulation.

pub mod octree;
pub mod tiered;

use octree::Octree;
use serde::{Deserialize, Serialize};

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
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            iterations: 40,
            repulsion: 8000.0,
            spring_stiffness: 0.05,
            spring_length: 50.0,
            damping: 0.85,
            theta: 0.8,
            center_gravity: 0.002,
        }
    }
}

/// Assign distinct aesthetic neon colors based on entity type and community.
pub fn assign_color_and_type(path: &str, community: u32) -> (String, u32) {
    if path.ends_with(".md") || path.contains("docs/") || path.contains("adr/") {
        ("DocNode".to_string(), 0x3b82f6) // Sapphire Blue
    } else if path.contains("::fn ") || path.contains("() ") || path.ends_with(".rs#") {
        ("Function".to_string(), 0x10b981) // Emerald Neon
    } else if path.contains("struct ") || path.contains("class ") || path.contains("interface ") {
        ("Struct".to_string(), 0x8b5cf6) // Electric Purple
    } else if path.contains("mod ") || path.contains("/") && !path.contains('.') {
        ("Module".to_string(), 0x06b6d4) // Cyan
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

/// Run force simulation relaxation on given nodes and edges.
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

    // 1. Deterministic initial placement on a Fibonacci sphere
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n);
    let mut velocities: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; n];
    let masses: Vec<f32> = degrees.iter().map(|&d| 1.0 + (d as f32).sqrt()).collect();

    let radius = (n as f32).cbrt() * 40.0 + 30.0;
    let phi = (1.0 + 5.0f32.sqrt()) / 2.0;

    for i in 0..n {
        let theta = 2.0 * std::f32::consts::PI * (i as f32) / phi;
        let y = 1.0 - (i as f32 / (n as f32).max(1.0)) * 2.0;
        let r_circle = (1.0 - y * y).max(0.0).sqrt();
        let x = theta.cos() * r_circle;
        let z = theta.sin() * r_circle;

        // Bias positions slightly by community cluster
        let comm_offset = (communities[i] as f32) * 2.4;
        let scale = radius + (degrees[i] as f32).min(20.0) * 5.0;
        positions.push([
            x * scale + comm_offset.sin() * 50.0,
            y * scale,
            z * scale + comm_offset.cos() * 50.0,
        ]);
    }

    // 2. Multi-step relaxation loop
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

        // C. Center gravity & velocity integration
        for i in 0..n {
            let p = &mut positions[i];
            let v = &mut velocities[i];
            let f = forces[i];

            // Origin gravity
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
