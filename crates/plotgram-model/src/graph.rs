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
    /// Explicit rendering shape override (closed set; `None` = inferred by kind/profile).
    pub shape: Option<String>,
    /// Free-form attributes (`kind`, `status`, `style.*`, `meta.*`, …).
    pub attrs: AttrMap,
}

impl Node {
    /// Semantic kind from attrs (`kind:`), if present.
    pub fn kind(&self) -> Option<&str> {
        self.attrs.get("kind").and_then(|v| v.as_str())
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

/// A group container (dsl-spec §6). Recursive: may contain nested groups.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Group {
    /// Unique identifier.
    pub id: String,
    /// Display label (required for groups).
    pub label: String,
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
}
