use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};

use groundcontrol_common::config::EdgeClass;
use groundcontrol_common::types::{EdgeKind, EdgeProvenance, ResolutionConfidence};

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
    /// Open typed edge kind (Universal semantic edge or interned grammar relation).
    pub kind: EdgeKind,
    /// Weight of this edge.
    pub weight: f32,
    /// How this edge was created.
    pub provenance: EdgeProvenance,
    /// Edge class for filtering purposes.
    pub class: EdgeClass,
    /// Name of the corpus the target lives in, for cross-corpus links.
    pub target_corpus: Option<String>,
    /// Confidence band for a resolved cross-corpus link (`None` for intra-corpus).
    pub confidence: Option<ResolutionConfidence>,
    /// Repo-relative path of the target endpoint in `target_corpus`.
    pub target_path: Option<String>,
    /// Fully qualified symbol / endpoint name at the remote target.
    pub target_symbol: Option<String>,
    /// Free-form kind of the remote endpoint (e.g. `"Symbol"`, `"Route"`, `"Channel"`, `"RpcEndpoint"`, `"Resource"`).
    pub target_kind: Option<String>,
}

impl GraphEdge {
    /// Retrieve the typed [`groundcontrol_common::types::EdgeKind`] of this edge.
    #[inline]
    pub fn kind(&self) -> &EdgeKind {
        &self.kind
    }

    /// Retrieve the canonical string representation of this edge type.
    #[inline]
    pub fn edge_type(&self) -> &str {
        self.kind.as_str()
    }
}

/// On-disk schema version stamped into `GraphData`.
pub const GRAPH_SCHEMA_VERSION: u32 = 3;

/// Serializable wrapper for persistence.
#[derive(Serialize, Deserialize)]
pub(crate) struct GraphData {
    /// Schema version stamp; see [`GRAPH_SCHEMA_VERSION`].
    pub(crate) version: u32,
    /// The serialized directed graph.
    pub(crate) graph: DiGraph<GraphNode, GraphEdge>,
}

/// Borrowed serializable wrapper for zero-clone streaming persistence.
#[derive(Serialize)]
pub(crate) struct GraphDataRef<'a> {
    pub(crate) version: u32,
    pub(crate) graph: &'a DiGraph<GraphNode, GraphEdge>,
}
