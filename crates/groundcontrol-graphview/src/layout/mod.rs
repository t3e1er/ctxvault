//! 3D force-directed layout computation using Barnes-Hut n-body simulation
//! with directory module and community anchor springs.

pub mod color;
pub mod force;
pub mod octree;
pub mod tiered;
pub mod types;

pub use color::{
    assign_color_and_type, color_for_edge_type, color_for_entity_type, extract_directory_key,
    fnv1a_hash, normalize_type_label, stellar_color_for_degree,
};
pub use force::{compute_cluster_anchors, compute_force_layout};
pub use types::{ClusterMode, EdgeLayout, GraphLayout, LayoutConfig, NodeLayout};
