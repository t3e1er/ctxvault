//! High-density packed binary wire protocol serializer for 1M+ nodes.
//!
//! Layout (WIRE_VERSION 2):
//! - Header:       16 bytes (Magic u32, Version u32, NodeCount u32, EdgeCount u32)
//! - Nodes:        32 bytes per node (unchanged from v1)
//! - Edges:        16 bytes per edge (extended from 12B)
//! - String Table: Length-prefixed UTF-8 strings
//!
//! ```text
//! Edge record layout (16 bytes):
//!   [0..4]   source        u32 LE
//!   [4..8]   target        u32 LE
//!   [8..10]  edge_type_idx u16 LE  (index into string table)
//!   [10..12] weight_q      u16 LE  (weight * 1000, clamped 0..10)
//!   [12]     edge_class    u8      (0=Structural,1=Semantic,2=Code,3=CrossModal,4=Hybrid)
//!   [13]     confidence    u8      (0=None,1=High,2=Medium,3=Speculative)
//!   [14..16] padding               (reserved, zero)
//! ```

use std::collections::HashMap;

use crate::layout::GraphLayout;

/// Magic 4-byte header ("CTXV").
pub const WIRE_MAGIC: u32 = 0x43545856;
/// Protocol format version.
pub const WIRE_VERSION: u32 = 2;

/// Packed binary serializer.
pub struct BinaryWireEncoder;

impl BinaryWireEncoder {
    /// Encode a `GraphLayout` into a packed binary byte array.
    pub fn encode(layout: &GraphLayout) -> Vec<u8> {
        let node_count = layout.nodes.len() as u32;
        let edge_count = layout.edges.len() as u32;

        let mut string_table: Vec<String> = Vec::new();
        let mut string_to_idx: HashMap<String, u16> = HashMap::new();

        let mut intern_string = |s: &str| -> u16 {
            if let Some(&idx) = string_to_idx.get(s) {
                return idx;
            }
            let idx = string_table.len() as u16;
            string_table.push(s.to_string());
            string_to_idx.insert(s.to_string(), idx);
            idx
        };

        let mut node_type_indices = Vec::with_capacity(layout.nodes.len());
        let mut node_path_indices = Vec::with_capacity(layout.nodes.len());
        for node in &layout.nodes {
            node_type_indices.push(intern_string(&node.entity_type));
            node_path_indices.push(intern_string(&node.path));
        }

        let mut edge_type_indices = Vec::with_capacity(layout.edges.len());
        for edge in &layout.edges {
            edge_type_indices.push(intern_string(&edge.edge_type));
        }

        // 16 header + 32 * nodes + 16 * edges + string table overhead
        let estimated_size = 16 + (node_count as usize) * 32 + (edge_count as usize) * 16 + 4096;
        let mut buf = Vec::with_capacity(estimated_size);

        // 1. Header (16 bytes)
        buf.extend_from_slice(&WIRE_MAGIC.to_le_bytes());
        buf.extend_from_slice(&WIRE_VERSION.to_le_bytes());
        buf.extend_from_slice(&node_count.to_le_bytes());
        buf.extend_from_slice(&edge_count.to_le_bytes());

        // 2. Nodes (32 bytes each — unchanged from v1)
        for (i, node) in layout.nodes.iter().enumerate() {
            buf.extend_from_slice(&node.id.to_le_bytes()); // 4B
            buf.extend_from_slice(&node.position[0].to_le_bytes()); // 4B
            buf.extend_from_slice(&node.position[1].to_le_bytes()); // 4B
            buf.extend_from_slice(&node.position[2].to_le_bytes()); // 4B
            buf.extend_from_slice(&(node.community as u16).to_le_bytes()); // 2B
            buf.extend_from_slice(&(node.degree as u16).to_le_bytes()); // 2B
            buf.extend_from_slice(&node.color_rgb.to_le_bytes()); // 4B
            buf.extend_from_slice(&node.size.to_le_bytes()); // 4B
            buf.extend_from_slice(&node_type_indices[i].to_le_bytes()); // 2B
            buf.extend_from_slice(&node_path_indices[i].to_le_bytes()); // 2B
        }

        // 3. Edges (16 bytes each — extended in v2)
        for (i, edge) in layout.edges.iter().enumerate() {
            buf.extend_from_slice(&edge.source.to_le_bytes()); // 4B [0..4]
            buf.extend_from_slice(&edge.target.to_le_bytes()); // 4B [4..8]
            buf.extend_from_slice(&edge_type_indices[i].to_le_bytes()); // 2B [8..10]
            let qw = ((edge.weight.clamp(0.0, 10.0)) * 1000.0) as u16;
            buf.extend_from_slice(&qw.to_le_bytes()); // 2B [10..12]
            buf.push(edge.edge_class); // 1B [12]
            buf.push(edge.confidence); // 1B [13]
            buf.extend_from_slice(&[0u8; 2]); // 2B [14..16] reserved
        }

        // 4. String Table
        let strings_count = string_table.len() as u32;
        buf.extend_from_slice(&strings_count.to_le_bytes());
        for s in string_table {
            let bytes = s.as_bytes();
            buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(bytes);
        }

        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{EdgeLayout, GraphLayout, NodeLayout};

    fn make_layout(edge_class: u8, confidence: u8) -> GraphLayout {
        GraphLayout {
            corpus: "test".to_string(),
            nodes: vec![NodeLayout {
                id: 0,
                path: "test.md".to_string(),
                title: Some("Test".to_string()),
                position: [10.0, 20.0, 30.0],
                degree: 2,
                community: 1,
                entity_type: "DocNode".to_string(),
                color_rgb: 0x3b82f6,
                size: 5.0,
            }],
            edges: vec![EdgeLayout {
                source: 0,
                target: 0,
                edge_type: "wikilink".to_string(),
                weight: 1.0,
                edge_class,
                confidence,
            }],
            communities_count: 1,
        }
    }

    #[test]
    fn test_wire_version_2_header() {
        let encoded = BinaryWireEncoder::encode(&make_layout(0, 0));
        assert_eq!(u32::from_le_bytes(encoded[0..4].try_into().unwrap()), WIRE_MAGIC);
        assert_eq!(
            u32::from_le_bytes(encoded[4..8].try_into().unwrap()),
            2,
            "WIRE_VERSION must be 2"
        );
        assert_eq!(u32::from_le_bytes(encoded[8..12].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(encoded[12..16].try_into().unwrap()), 1);
        // 16B header + 32B node + 16B edge = 64 bytes minimum before string table
        assert!(encoded.len() >= 64);
    }

    #[test]
    fn test_edge_class_and_confidence_byte_positions() {
        // Code (2), High confidence (1)
        let encoded = BinaryWireEncoder::encode(&make_layout(2, 1));
        let edge_start = 16 + 32; // after header + one node
        assert_eq!(encoded[edge_start + 12], 2, "edge_class should be Code (2)");
        assert_eq!(encoded[edge_start + 13], 1, "confidence should be High (1)");
        assert_eq!(encoded[edge_start + 14], 0, "padding[0] must be zero");
        assert_eq!(encoded[edge_start + 15], 0, "padding[1] must be zero");
    }

    #[test]
    fn test_structural_intra_corpus_defaults() {
        let encoded = BinaryWireEncoder::encode(&make_layout(0, 0));
        let edge_start = 16 + 32;
        assert_eq!(encoded[edge_start + 12], 0, "Structural = 0");
        assert_eq!(encoded[edge_start + 13], 0, "None/intra-corpus = 0");
    }
}
