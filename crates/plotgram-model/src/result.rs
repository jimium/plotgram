//! Layout result: geometry output from the engine.
//!
//! Minimal viable structure: node frames, edge polylines, label slots.
//! The renderer also needs the original [`crate::graph::Graph`] (shape / variant /
//! arrow / style) plus [`crate::render::RenderMeta`] — see [`crate::render::RenderInput`].

use crate::geometry::{Point, Rect};
use crate::port::PortRef;

/// Placement result for a single node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodePlacement {
    /// Node id (matches `graph::Node::id`).
    pub id: String,
    /// Bounding box (position + size determined by layout).
    pub frame: Rect,
}

/// A bend point in an edge path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgePath {
    /// Ordered points: source anchor → bends → target anchor.
    pub points: Vec<Point>,
}

/// Placement result for a single edge.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgePlacement {
    /// Edge id (matches `graph::Edge::id`; required for parallel edges).
    pub id: String,
    /// Source node id (redundant with graph, kept for convenience).
    pub source: String,
    /// Target node id.
    pub target: String,
    /// The routed polyline.
    pub path: EdgePath,
    /// Resolved source port. Written by the layout composition phase
    /// (port decision, dsl-spec §7.4.1); `None` = layout did not decide
    /// ports (legacy paths). Ink must not invent or rewrite ports.
    #[serde(default)]
    pub from_port: Option<PortRef>,
    /// Resolved target port (same write-discipline as `from_port`).
    #[serde(default)]
    pub to_port: Option<PortRef>,
}

/// Who owns a label slot.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LabelOwner {
    Node(String),
    /// Edge id (not `"src>dst"` — parallel edges need a stable key).
    Edge(String),
    Group(String),
}

/// A label slot (for node labels, edge labels, group headers).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LabelSlot {
    /// Owner of this label.
    pub owner: LabelOwner,
    /// Role hint for multi-label edges (`mid` / `head` / `tail`), optional.
    pub role: Option<String>,
    /// Label text.
    pub text: String,
    /// Suggested placement rect (renderer may adjust).
    pub frame: Rect,
}

/// Placement result for a group (bounding box of its children + padding).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupPlacement {
    /// Group id.
    pub id: String,
    /// Bounding box enclosing all children.
    pub frame: Rect,
}

/// Complete layout result — geometry only.
///
/// Does **not** carry shape / kind / style / theme; pair with `Graph` + `RenderMeta`
/// via [`crate::render::RenderInput`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayoutResult {
    /// Node placements (all nodes, flat — including those inside groups).
    pub nodes: Vec<NodePlacement>,
    /// Edge placements (all edges, flat).
    pub edges: Vec<EdgePlacement>,
    /// Group bounding boxes (all groups, flat).
    pub groups: Vec<GroupPlacement>,
    /// Label slots (node labels, edge labels, group headers).
    pub labels: Vec<LabelSlot>,
    /// Total canvas size.
    pub canvas_width: f64,
    pub canvas_height: f64,
}
