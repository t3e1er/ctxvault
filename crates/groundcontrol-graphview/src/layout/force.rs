//! 3D force simulation and anchor positioning computation.

use std::collections::HashMap;

use super::color::{extract_directory_key, fnv1a_hash};
use super::octree::Octree;
use super::types::{ClusterMode, EdgeLayout, LayoutConfig};

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
            let mut comm_members: HashMap<u32, Vec<usize>> = HashMap::new();
            let mut distinct_comms = Vec::new();
            for (i, &c) in communities.iter().enumerate() {
                let entry = comm_members.entry(c).or_default();
                if entry.is_empty() {
                    distinct_comms.push(c);
                }
                entry.push(i);
            }

            let num_comms = distinct_comms.len().max(1);
            let mut comm_centers: HashMap<u32, [f32; 3]> = HashMap::with_capacity(num_comms);

            // Distribute community centroids across the outer perimeter / mantle of the corpus sphere
            for (c_idx, &c) in distinct_comms.iter().enumerate() {
                let c_f = c_idx as f32;
                let n_f = num_comms as f32;
                // Golden spiral on sphere surface
                let y = 1.0 - ((c_f + 0.5) / n_f) * 2.0;
                let radius_at_y = (1.0 - y * y).max(0.0).sqrt();
                let theta = 2.399_963_2 * c_f; // Golden angle: pi * (3 - sqrt(5))
                let x = theta.cos() * radius_at_y;
                let z = theta.sin() * radius_at_y;
                // Push community clusters to the outer mantle of the corpus sphere:
                // Shell radius between 0.92 * base_radius and 1.04 * base_radius, leaving the interior hollow
                let jitter = (((c.wrapping_mul(2654435761)) & 0xFF) as f32) / 255.0;
                let r = base_radius * (0.92 + 0.12 * jitter);
                comm_centers.insert(c, [x * r, y * r, z * r]);
            }

            let mut anchors = vec![[0.0f32; 3]; n];
            for (&c, member_indices) in &comm_members {
                let center = comm_centers.get(&c).copied().unwrap_or([0.0, 0.0, 0.0]);
                let m = member_indices.len();
                let m_f = m as f32;
                // Tight volumetric radius for this community cluster based on member count
                let cluster_r = (m_f).cbrt() * 20.0 + 22.0;

                for (rank, &node_idx) in member_indices.iter().enumerate() {
                    let r_f = rank as f32;
                    // Spherical Fibonacci lattice distribution within the community cluster volume
                    let y = if m > 1 { 1.0 - ((r_f + 0.5) / m_f) * 2.0 } else { 0.0 };
                    let rad_y = (1.0 - y * y).max(0.0).sqrt();
                    let theta = 2.399_963_2 * r_f;
                    let x = theta.cos() * rad_y;
                    let z = theta.sin() * rad_y;
                    // Uniform volume distribution scaling r by cube root: (rank/M)^(1/3)
                    let vol_scale = ((r_f + 0.5) / m_f).cbrt();
                    let node_r = cluster_r * vol_scale;

                    let a_x = center[0] + x * node_r;
                    let a_y = center[1] + y * node_r;
                    let a_z = center[2] + z * node_r;

                    anchors[node_idx] = [a_x, a_y, a_z];
                }
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
    raw_edges: &[(usize, usize, String, f32, u8, u8)],
    config: &LayoutConfig,
) -> (Vec<[f32; 3]>, Vec<EdgeLayout>) {
    let n = paths.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }

    // 1. Calculate cluster anchors based on Directory or Community mode
    let num_communities = communities.iter().collect::<std::collections::HashSet<_>>().len();
    let base_radius = (n as f32).cbrt() * 45.0 + 350.0 + (num_communities as f32) * 15.0;
    let anchors = compute_cluster_anchors(paths, communities, config.cluster_mode, base_radius);

    // 2. Initial placement: 3D anchor center + 3D spherical volumetric jitter
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n);
    let mut velocities: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; n];
    let masses: Vec<f32> = degrees.iter().map(|&d| 1.0 + (d as f32).sqrt()).collect();

    for i in 0..n {
        let a = anchors[i];
        if config.cluster_mode == ClusterMode::Community {
            positions.push(a);
        } else {
            let h = fnv1a_hash(&paths[i]);
            let theta = ((h & 0xFFFF) as f32 / 65535.0) * 2.0 * std::f32::consts::PI;
            let phi = ((((h >> 16) & 0xFFFF) as f32 / 65535.0) - 0.5) * std::f32::consts::PI;
            let r_jitter = 15.0 + (((h >> 8) & 0xFF) as f32 / 255.0) * 90.0;

            let jx = r_jitter * phi.cos() * theta.cos();
            let jy = r_jitter * phi.sin();
            let jz = r_jitter * phi.cos() * theta.sin();

            positions.push([a[0] + jx, a[1] + jy, a[2] + jz]);
        }
    }

    // 3. Multi-step relaxation loop
    let dt = 0.5f32;
    for _ in 0..config.iterations {
        // Build Barnes-Hut octree
        let octree = Octree::build(&positions, &masses, config.theta);

        // A. Repulsive forces in parallel
        let mut forces = octree.compute_all_repulsions(&positions, config.repulsion, 100.0);

        // B. Attractive spring forces along edges
        for &(src, tgt, _, w, _, _) in raw_edges {
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

            let (gx, gy, gz) = if config.cluster_mode == ClusterMode::Community {
                let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2] + 1.0).sqrt();
                if r > base_radius * 1.25 {
                    let over = r - base_radius * 1.25;
                    (-p[0] / r * over * 0.015, -p[1] / r * over * 0.015, -p[2] / r * over * 0.015)
                } else if r < base_radius * 0.50 {
                    // Push out gently from hollow core to preserve floating label visibility
                    let under = base_radius * 0.50 - r;
                    (p[0] / r * under * 0.025, p[1] / r * under * 0.025, p[2] / r * under * 0.025)
                } else {
                    (0.0, 0.0, 0.0)
                }
            } else {
                (
                    -p[0] * config.center_gravity,
                    -p[1] * config.center_gravity,
                    -p[2] * config.center_gravity,
                )
            };

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

    // 4. Inter-community centroid repulsion pass (pure Rust cluster push)
    if config.cluster_mode == ClusterMode::Community && num_communities > 1 {
        let mut comm_members: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, &c) in communities.iter().enumerate() {
            comm_members.entry(c).or_default().push(i);
        }

        // Run relaxation iterations between community centroids to prevent overlapping blobs
        for _ in 0..4 {
            let mut centroids: HashMap<u32, [f32; 3]> = HashMap::new();
            for (&c, members) in &comm_members {
                let mut sum = [0.0f32; 3];
                for &idx in members {
                    sum[0] += positions[idx][0];
                    sum[1] += positions[idx][1];
                    sum[2] += positions[idx][2];
                }
                let count = members.len() as f32;
                centroids.insert(c, [sum[0] / count, sum[1] / count, sum[2] / count]);
            }

            let comm_list: Vec<u32> = centroids.keys().copied().collect();
            let mut shifts: HashMap<u32, [f32; 3]> = HashMap::new();

            for i in 0..comm_list.len() {
                for j in (i + 1)..comm_list.len() {
                    let c1 = comm_list[i];
                    let c2 = comm_list[j];
                    let p1 = centroids[&c1];
                    let p2 = centroids[&c2];
                    let dx = p1[0] - p2[0];
                    let dy = p1[1] - p2[1];
                    let dz = p1[2] - p2[2];
                    let dist = (dx * dx + dy * dy + dz * dz + 1.0).sqrt();
                    let min_dist = 220.0;
                    if dist < min_dist {
                        let push = (min_dist - dist) * 0.15;
                        let nx = dx / dist;
                        let ny = dy / dist;
                        let nz = dz / dist;

                        let s1 = shifts.entry(c1).or_insert([0.0, 0.0, 0.0]);
                        s1[0] += nx * push;
                        s1[1] += ny * push;
                        s1[2] += nz * push;

                        let s2 = shifts.entry(c2).or_insert([0.0, 0.0, 0.0]);
                        s2[0] -= nx * push;
                        s2[1] -= ny * push;
                        s2[2] -= nz * push;
                    }
                }
            }

            for (c, shift) in shifts {
                if let Some(members) = comm_members.get(&c) {
                    for &idx in members {
                        positions[idx][0] += shift[0];
                        positions[idx][1] += shift[1];
                        positions[idx][2] += shift[2];
                    }
                }
            }
        }
    }

    // Convert raw edges to EdgeLayout, preserving edge_class and confidence.
    let edges = raw_edges
        .iter()
        .map(|&(s, t, ref rel, w, ec, conf)| EdgeLayout {
            source: s as u32,
            target: t as u32,
            edge_type: rel.clone(),
            weight: w,
            edge_class: ec,
            confidence: conf,
        })
        .collect();

    (positions, edges)
}
