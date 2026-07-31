//! Preferred node sizes measured before layout (ADR-005 / engine plan).
//!
//! Graph stays structural; sizes live beside it on [`crate::contract::LayoutContract`].

use std::collections::BTreeMap;
use std::fmt;

use crate::geometry::Size;
use crate::graph::{Graph, Group};

/// Map of node id → preferred size. Uses [`BTreeMap`] for deterministic iteration.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NodeSizes {
    sizes: BTreeMap<String, Size>,
}

impl NodeSizes {
    pub fn new() -> Self {
        Self {
            sizes: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, id: impl Into<String>, size: Size) {
        self.sizes.insert(id.into(), size);
    }

    pub fn get(&self, id: &str) -> Option<Size> {
        self.sizes.get(id).copied()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.sizes.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.sizes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sizes.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Size)> {
        self.sizes.iter().map(|(k, v)| (k.as_str(), *v))
    }

    /// Every node in `graph` (recursive) must have an entry.
    pub fn require_all(&self, graph: &Graph) -> Result<(), MissingNodeSize> {
        for id in graph.all_node_ids() {
            if !self.contains(&id) {
                return Err(MissingNodeSize { node_id: id });
            }
        }
        Ok(())
    }
}

/// A required preferred size was missing for a node id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingNodeSize {
    pub node_id: String,
}

impl fmt::Display for MissingNodeSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "missing preferred size for node `{}` (measure before layout)",
            self.node_id
        )
    }
}

impl std::error::Error for MissingNodeSize {}

impl Graph {
    /// All node ids in declaration order (top-level, then groups depth-first).
    pub fn all_node_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        for n in &self.nodes {
            out.push(n.id.clone());
        }
        for g in &self.groups {
            g.collect_node_ids(&mut out);
        }
        out
    }
}

impl Group {
    fn collect_node_ids(&self, out: &mut Vec<String>) {
        for n in &self.nodes {
            out.push(n.id.clone());
        }
        for g in &self.groups {
            g.collect_node_ids(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attr::AttrMap;
    use crate::graph::{Group, Node, NodeRole};

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        }
    }

    #[test]
    fn require_all_reports_missing_ids() {
        let graph = Graph {
            nodes: vec![node("a")],
            edges: vec![],
            groups: vec![Group {
                id: "g".into(),
                label: None,
                attrs: AttrMap::new(),
                nodes: vec![node("b")],
                edges: vec![],
                groups: vec![],
            }],
            partition: None,
        };
        let mut sizes = NodeSizes::new();
        sizes.insert("a", Size::new(40.0, 20.0));
        assert!(matches!(
            sizes.require_all(&graph),
            Err(MissingNodeSize { node_id }) if node_id == "b"
        ));
        sizes.insert("b", Size::new(10.0, 10.0));
        sizes.require_all(&graph).unwrap();
    }
}
