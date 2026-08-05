//! Layout result: geometry output from the engine.
//!
//! Minimal viable structure: node frames, edge paths, label slots.
//! The renderer also needs the original [`crate::graph::Graph`] (shape / variant /
//! arrow / style) plus [`crate::render::RenderMeta`] — see [`crate::render::RenderInput`].

use crate::diagnostics::LayoutDiagnostics;
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

/// Edge geometry as a tagged sum type.
///
/// - [`EdgePath::Polyline`]: orthogonal / polyline / octilinear / straight
/// - [`EdgePath::Cubic`]: cubic Bézier (P0=start, P3=end, controls=[P1,P2]);
///   render draws SVG `C`, verify may sample
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgePath {
    /// Ordered polyline: source anchor → bends → target anchor.
    Polyline { points: Vec<Point> },
    /// Single cubic Bézier segment.
    Cubic {
        start: Point,
        end: Point,
        /// Control points `[P1, P2]`.
        controls: [Point; 2],
    },
}

impl EdgePath {
    pub fn polyline(points: Vec<Point>) -> Self {
        Self::Polyline { points }
    }

    pub fn cubic(start: Point, end: Point, controls: [Point; 2]) -> Self {
        Self::Cubic {
            start,
            end,
            controls,
        }
    }

    /// Borrow polyline vertices; `None` for [`EdgePath::Cubic`].
    pub fn polyline_points(&self) -> Option<&[Point]> {
        match self {
            Self::Polyline { points } => Some(points.as_slice()),
            Self::Cubic { .. } => None,
        }
    }

    /// Mutable polyline vertices; `None` for [`EdgePath::Cubic`].
    pub fn polyline_points_mut(&mut self) -> Option<&mut Vec<Point>> {
        match self {
            Self::Polyline { points } => Some(points),
            Self::Cubic { .. } => None,
        }
    }

    /// Endpoints `(start, end)` when the path has a defined span.
    pub fn start_end(&self) -> Option<(Point, Point)> {
        match self {
            Self::Polyline { points } if points.len() >= 2 => {
                Some((points[0], *points.last().unwrap()))
            }
            Self::Polyline { .. } => None,
            Self::Cubic { start, end, .. } => Some((*start, *end)),
        }
    }

    /// Polyline approximation for verify / score / ascii.
    ///
    /// Cubic is sampled with a fixed count (deterministic).
    pub fn samples(&self) -> Vec<Point> {
        match self {
            Self::Polyline { points } => points.clone(),
            Self::Cubic {
                start,
                end,
                controls,
            } => sample_cubic_bezier(*start, controls[0], controls[1], *end, CUBIC_SAMPLE_COUNT),
        }
    }
}

const CUBIC_SAMPLE_COUNT: usize = 24;

fn sample_cubic_bezier(p0: Point, p1: Point, p2: Point, p3: Point, samples: usize) -> Vec<Point> {
    let n = samples.max(2);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let u = 1.0 - t;
        let uu = u * u;
        let tt = t * t;
        out.push(Point {
            x: uu * u * p0.x + 3.0 * uu * t * p1.x + 3.0 * u * tt * p2.x + tt * t * p3.x,
            y: uu * u * p0.y + 3.0 * uu * t * p1.y + 3.0 * u * tt * p2.y + tt * t * p3.y,
        });
    }
    out
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
    /// Routed geometry (polyline or cubic).
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
/// for rendering ([`crate::render::RenderInput`]).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayoutResult {
    pub nodes: Vec<NodePlacement>,
    pub edges: Vec<EdgePlacement>,
    pub groups: Vec<GroupPlacement>,
    pub labels: Vec<LabelSlot>,
    pub canvas_width: f64,
    pub canvas_height: f64,
    /// Structured observations from the layout run (roadmap phase C).
    /// Never affects geometry; `#[serde(default)]` keeps older JSON readable.
    #[serde(default)]
    pub diagnostics: LayoutDiagnostics,
}
