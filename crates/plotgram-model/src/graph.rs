//! Diagram IR: the graph model shared by engine and renderer.
//!
//! Profile expansion fills `LayoutContract` algorithm fields; the `Graph` itself
//! is the structural IR (nodes / edges / groups). No `diagram_type` on `Graph`.
//!
//! Aligned with dsl-spec §5–§7.

use crate::attr::AttrMap;

/// Arrow semantics (dsl-spec §7.2: exactly 3 kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Arrow {
    /// `->` forward / active flow.
    Forward,
    /// `-->` response / return.
    Response,
    /// `<->` bidirectional.
    Bidirectional,
}

/// A graph node (dsl-spec §5).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Node {
    /// Unique identifier (`[a-z][a-z0-9_]*`).
    pub id: String,
    /// Display label; `None` = unlabeled pure shape.
    pub label: Option<String>,
    /// Explicit rendering shape override (closed set; `None` = resolved via shape chain, dsl-spec §14.3.2).
    pub shape: Option<String>,
    /// Free-form attributes (`variant`, `icon`, `status`, `style.*`, `meta.*`, …).
    pub attrs: AttrMap,
}

impl Node {
    /// Visual variant from attrs (`variant:`), if present (dsl-spec §14.7).
    pub fn variant(&self) -> Option<&str> {
        self.attrs.get("variant").and_then(|v| v.as_str())
    }
}

/// An edge between two nodes (dsl-spec §7).
///
/// Parallel edges (same source/target) are distinguished by [`Edge::id`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Stable unique id within the graph (parser-assigned; not the DSL surface).
    pub id: String,
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Arrow semantics.
    pub arrow: Arrow,
    /// Mid-edge label.
    pub label: Option<String>,
    /// Label near target (e.g. cardinality "1").
    pub head_label: Option<String>,
    /// Label near source (e.g. cardinality "N").
    pub tail_label: Option<String>,
    /// Free-form attributes (`style.*`, `meta.*`, …).
    pub attrs: AttrMap,
}

impl Edge {
    /// Source-end port constraint from attrs (`from_side` / `from_slot`),
    /// validated per dsl-spec §7.4.2. `Ok(None)` = fully algorithm-decided.
    pub fn from_port_constraint(
        &self,
    ) -> Result<Option<crate::port::PortConstraint>, crate::port::PortConstraintError> {
        crate::port::port_constraint(&self.attrs, "from_side", "from_slot")
    }

    /// Target-end port constraint from attrs (`to_side` / `to_slot`).
    pub fn to_port_constraint(
        &self,
    ) -> Result<Option<crate::port::PortConstraint>, crate::port::PortConstraintError> {
        crate::port::port_constraint(&self.attrs, "to_side", "to_slot")
    }
}

/// A group container (dsl-spec §6). Recursive: may contain nested groups.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Group {
    /// Unique identifier.
    pub id: String,
    /// Display label; `None` = untitled group (dsl-spec §6.2).
    pub label: Option<String>,
    /// Group-level attributes (`layout` hint, `style.*`, …).
    pub attrs: AttrMap,
    /// Child nodes.
    pub nodes: Vec<Node>,
    /// Edges whose both endpoints are descendants of this group.
    pub edges: Vec<Edge>,
    /// Nested sub-groups.
    pub groups: Vec<Group>,
}

/// The complete diagram graph model.
///
/// Wrapped in [`crate::contract::LayoutContract`] for the engine.
/// No `diagram_type` here — that stays in the profile/parser layer (ADR-001).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    /// Top-level nodes (not inside any group).
    pub nodes: Vec<Node>,
    /// Top-level edges (cross-group or between top-level nodes).
    pub edges: Vec<Edge>,
    /// Top-level groups.
    pub groups: Vec<Group>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// Total node count (recursive into groups).
    pub fn node_count(&self) -> usize {
        self.nodes.len() + self.groups.iter().map(|g| g.node_count()).sum::<usize>()
    }

    /// Total edge count (recursive into groups).
    pub fn edge_count(&self) -> usize {
        self.edges.len() + self.groups.iter().map(|g| g.edge_count()).sum::<usize>()
    }

    /// Look up a node by id (top-level first, then groups in declaration order).
    pub fn find_node(&self, id: &str) -> Option<&Node> {
        if let Some(n) = self.nodes.iter().find(|n| n.id == id) {
            return Some(n);
        }
        for g in &self.groups {
            if let Some(n) = g.find_node(id) {
                return Some(n);
            }
        }
        None
    }

    /// Look up an edge by id (top-level first, then groups in declaration order).
    pub fn find_edge(&self, id: &str) -> Option<&Edge> {
        if let Some(e) = self.edges.iter().find(|e| e.id == id) {
            return Some(e);
        }
        for g in &self.groups {
            if let Some(e) = g.find_edge(id) {
                return Some(e);
            }
        }
        None
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Group {
    /// Recursive node count within this group.
    pub fn node_count(&self) -> usize {
        self.nodes.len() + self.groups.iter().map(|g| g.node_count()).sum::<usize>()
    }

    /// Recursive edge count within this group.
    pub fn edge_count(&self) -> usize {
        self.edges.len() + self.groups.iter().map(|g| g.edge_count()).sum::<usize>()
    }

    /// Look up a node by id within this group subtree.
    pub fn find_node(&self, id: &str) -> Option<&Node> {
        if let Some(n) = self.nodes.iter().find(|n| n.id == id) {
            return Some(n);
        }
        for g in &self.groups {
            if let Some(n) = g.find_node(id) {
                return Some(n);
            }
        }
        None
    }

    /// Look up an edge by id within this group subtree.
    pub fn find_edge(&self, id: &str) -> Option<&Edge> {
        if let Some(e) = self.edges.iter().find(|e| e.id == id) {
            return Some(e);
        }
        for g in &self.groups {
            if let Some(e) = g.find_edge(id) {
                return Some(e);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            label: Some(id.to_string()),
            shape: None,
            attrs: AttrMap::new(),
        }
    }

    fn edge(id: &str, arrow: Arrow) -> Edge {
        Edge {
            id: id.to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            arrow,
            label: None,
            head_label: None,
            tail_label: None,
            attrs: AttrMap::new(),
        }
    }

    #[test]
    fn find_node_and_edge_recurse_into_nested_groups() {
        let cases = [
            ("top_node", true, false),
            ("inner_node", true, false),
            ("deep_node", true, false),
            ("missing", false, false),
            ("top_edge", false, true),
            ("inner_edge", false, true),
            ("deep_edge", false, true),
        ];

        let graph = Graph {
            nodes: vec![node("top_node")],
            edges: vec![edge("top_edge", Arrow::Forward)],
            groups: vec![Group {
                id: "g1".to_string(),
                label: Some("G1".to_string()),
                attrs: AttrMap::new(),
                nodes: vec![node("inner_node")],
                edges: vec![edge("inner_edge", Arrow::Response)],
                groups: vec![Group {
                    id: "g2".to_string(),
                    label: Some("G2".to_string()),
                    attrs: AttrMap::new(),
                    nodes: vec![node("deep_node")],
                    edges: vec![edge("deep_edge", Arrow::Bidirectional)],
                    groups: vec![],
                }],
            }],
        };

        for (id, want_node, want_edge) in cases {
            assert_eq!(graph.find_node(id).is_some(), want_node, "find_node({id})");
            assert_eq!(graph.find_edge(id).is_some(), want_edge, "find_edge({id})");
        }
        assert_eq!(
            graph.find_edge("inner_edge").map(|e| e.arrow),
            Some(Arrow::Response)
        );
        assert_eq!(
            graph.find_edge("deep_edge").map(|e| e.arrow),
            Some(Arrow::Bidirectional)
        );
    }
}
