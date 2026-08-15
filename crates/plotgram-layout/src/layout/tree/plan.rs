//! Tree IR written by Compose.

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct TreePlan {
    /// Forest roots in declaration order.
    pub roots: Vec<String>,
    /// Entity nodes included in the layout, declaration order.
    pub nodes: Vec<String>,
    pub children: BTreeMap<String, Vec<String>>,
    pub parent: BTreeMap<String, String>,
    pub depth: BTreeMap<String, u32>,
    /// Directed edges used as parent→child.
    pub tree_edge_ids: Vec<String>,
    /// Remaining edges (cycles, extra parents, undirected, self-loops).
    /// Consumed as a diagnostic; ink still iterates the graph so extra edges
    /// stay visible as straight/deferred paths.
    pub extra_edge_ids: Vec<String>,
}

impl TreePlan {
    pub fn children_of(&self, id: &str) -> &[String] {
        self.children.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}
