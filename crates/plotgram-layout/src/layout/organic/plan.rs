//! Organic IR written by Compose / Metric.

use std::collections::BTreeMap;

use plotgram_model::geometry::Rect;

/// One weakly-connected component over the undirected simple graph.
#[derive(Debug, Clone)]
pub struct OrgComponent {
    pub id: u32,
    /// Member node ids in declaration order.
    pub nodes: Vec<String>,
    /// Undirected simple adjacency, local indices into `nodes`, sorted.
    pub adjacency: Vec<Vec<usize>>,
}

impl OrgComponent {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeRole {
    /// Plain straight edge.
    Plain,
    /// Self-loop.
    Loop,
    /// Second+ edge between the same node pair.
    Parallel,
}

#[derive(Debug, Clone)]
pub struct OrganicPlan {
    pub nodes: Vec<String>,
    pub components: Vec<OrgComponent>,
    /// node id → (component id, local index).
    pub node_of: BTreeMap<String, (u32, usize)>,
    pub edge_role: BTreeMap<String, EdgeRole>,
}

#[derive(Debug, Clone)]
pub struct OrganicMetric {
    pub frames: BTreeMap<String, Rect>,
}
