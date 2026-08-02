//! Route scene: the algorithm-facing input for independent edge routers.
//!
//! A `RouteScene` is a **self-contained** description of obstacles, terminals,
//! and permissions. Routers consume only this — never `Graph` or full
//! `NodePlacement` slices. The engine facade (or test fixtures) projects
//! upstream data into a scene before invoking [`crate::EdgeRouter::route`].
//!
//! See `docs/design/routing/orthogonal/architecture.md` §3.1.

use std::collections::BTreeMap;

use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::Side;

// ─── Core scene ─────────────────────────────────────────────

/// Self-contained routing problem: obstacles + terminals + permissions.
///
/// Constructed by the facade projection or by test fixtures.
/// Routers must not look beyond this struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouteScene {
    /// Obstacle rectangles (inflated by caller if desired).
    /// Usually node frames; the router must not route through these
    /// (except the terminal's own node — see [`TerminalPair`]).
    pub obstacles: Vec<Obstacle>,

    /// Per-edge terminal pairs (source anchor + target anchor).
    /// Keyed by edge id.
    pub terminals: BTreeMap<String, TerminalPair>,

    /// Stable edge processing order. Determines routing priority and
    /// tie-breaking. Must contain exactly the same keys as `terminals`.
    pub edge_order: Vec<String>,

    /// Group boundaries (M2). Empty = no group scene.
    pub group_boundaries: Vec<GroupBoundary>,

    /// Per-edge group crossing permissions (M2). Empty = no permissions.
    pub boundary_permissions: BTreeMap<String, Vec<BoundaryCrossing>>,

    /// Router algorithm parameters.
    pub params: OrthogonalRouteParams,
}

/// A rectangular obstacle with a stable identity.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Obstacle {
    /// The node (or group) id this obstacle belongs to.
    /// Used for "own-node exemption": a terminal's own node is passable.
    pub id: String,
    /// The (optionally pre-inflated) bounding rectangle.
    pub rect: Rect,
}

// ─── Terminals ──────────────────────────────────────────────

/// Source and target anchor points for one edge.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TerminalPair {
    pub source: PortAnchor,
    pub target: PortAnchor,
}

/// A resolved port anchor: the geometric point where an edge enters/exits
/// a node, plus the side direction for perpendicular stub departure.
///
/// The **facade projection** computes `point` from node frame + port;
/// the router only reads it. This keeps the router frame-agnostic.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PortAnchor {
    /// Absolute anchor coordinate on the node boundary.
    pub point: Point,
    /// Which side of the node the port sits on (determines stub direction).
    pub side: Side,
    /// Node id this port belongs to (for own-obstacle exemption).
    pub node_id: String,
}

// ─── Groups (M2) ────────────────────────────────────────────

/// A group boundary rectangle. Acts as an obstacle for edges that do NOT
/// have permission to cross it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GroupBoundary {
    pub group_id: String,
    pub rect: Rect,
}

/// A permitted group-boundary crossing for one edge (M2).
/// Listed in source→target traversal order.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoundaryCrossing {
    pub group_id: String,
    pub direction: CrossingDirection,
    /// Optional gate region where the crossing should occur.
    pub gate_region: Option<Rect>,
}

/// Whether the edge enters or leaves a group at this crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrossingDirection {
    Enter,
    Leave,
}

// ─── Parameters ─────────────────────────────────────────────

/// Typed parameters for the orthogonal edge router.
///
/// Distinct from `profile:` / theme — these are algorithm-level knobs.
/// See architecture.md §6.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OrthogonalRouteParams {
    /// Obstacle inflation / edge-to-edge minimum clearance.
    pub spacing: f64,
    /// Port stub length (perpendicular departure from node boundary).
    pub port_stub: f64,
    /// Cost penalty per 90° bend.
    pub bend_penalty: f64,
    /// Second-round shared-segment penalty. `0` = single-round routing.
    pub shared_penalty: f64,
    /// Minimum segment length (rounded-corner budget).
    pub min_segment: f64,
    /// A* / OVG node budget gate. `0` = unlimited.
    pub max_search_nodes: u32,
    /// Number of routing rounds (1 or 2).
    pub route_rounds: u8,
}

impl Default for OrthogonalRouteParams {
    fn default() -> Self {
        Self {
            spacing: 20.0,
            port_stub: 10.0,
            bend_penalty: 100.0,
            shared_penalty: 0.0,
            min_segment: 4.0,
            max_search_nodes: 0,
            route_rounds: 1,
        }
    }
}

// ─── Scene validation ───────────────────────────────────────

impl RouteScene {
    /// Validate structural invariants before routing.
    ///
    /// Group rules follow `docs/design/routing/orthogonal/phases/group-crossing.md` §2.3:
    /// single-layer groups, required `gate_region`, no silent invent. Nested /
    /// overlapping groups → error (caller maps to `UnsupportedRouteScene`).
    pub fn validate(&self) -> Result<(), String> {
        // edge_order and terminals must have the same keys.
        if self.edge_order.len() != self.terminals.len() {
            return Err(format!(
                "edge_order length {} != terminals length {}",
                self.edge_order.len(),
                self.terminals.len()
            ));
        }
        for id in &self.edge_order {
            if !self.terminals.contains_key(id) {
                return Err(format!("edge_order contains `{id}` not in terminals"));
            }
        }

        let group_ids: BTreeMap<&str, &GroupBoundary> = self
            .group_boundaries
            .iter()
            .map(|g| (g.group_id.as_str(), g))
            .collect();
        if group_ids.len() != self.group_boundaries.len() {
            return Err("duplicate group_id in group_boundaries".to_string());
        }

        // M2-group: no nested / overlapping group rects (positive-area overlap).
        for (i, a) in self.group_boundaries.iter().enumerate() {
            for b in &self.group_boundaries[i + 1..] {
                if rects_overlap_area(a.rect, b.rect) {
                    return Err(format!(
                        "group `{}` overlaps or nests with `{}` (M2 single-layer only)",
                        a.group_id, b.group_id
                    ));
                }
            }
        }

        for (edge_id, crossings) in &self.boundary_permissions {
            if !self.terminals.contains_key(edge_id) {
                return Err(format!(
                    "boundary_permissions key `{edge_id}` not in terminals"
                ));
            }
            for (ci, c) in crossings.iter().enumerate() {
                let Some(gb) = group_ids.get(c.group_id.as_str()) else {
                    return Err(format!(
                        "edge `{edge_id}` crossing[{ci}]: unknown group_id `{}`",
                        c.group_id
                    ));
                };
                let Some(gate) = c.gate_region else {
                    return Err(format!(
                        "edge `{edge_id}` crossing[{ci}] group `{}`: gate_region required (M2)",
                        c.group_id
                    ));
                };
                if !gate_touches_group_boundary(gate, gb.rect) {
                    return Err(format!(
                        "edge `{edge_id}` crossing[{ci}] group `{}`: gate_region must touch the group boundary",
                        c.group_id
                    ));
                }
            }
        }
        Ok(())
    }
}

const VALIDATE_EPS: f64 = 1e-9;

/// Positive-area overlap (edge-touching is OK).
fn rects_overlap_area(a: Rect, b: Rect) -> bool {
    a.x < b.right() - VALIDATE_EPS
        && b.x < a.right() - VALIDATE_EPS
        && a.y < b.bottom() - VALIDATE_EPS
        && b.y < a.bottom() - VALIDATE_EPS
}

/// G2: gate intersects/touches the group and is not strictly inside the interior.
fn gate_touches_group_boundary(gate: Rect, group: Rect) -> bool {
    if !rects_touch_or_overlap(gate, group) {
        return false;
    }
    let strictly_inside = gate.x > group.x + VALIDATE_EPS
        && gate.right() < group.right() - VALIDATE_EPS
        && gate.y > group.y + VALIDATE_EPS
        && gate.bottom() < group.bottom() - VALIDATE_EPS;
    !strictly_inside
}

fn rects_touch_or_overlap(a: Rect, b: Rect) -> bool {
    a.x <= b.right() + VALIDATE_EPS
        && b.x <= a.right() + VALIDATE_EPS
        && a.y <= b.bottom() + VALIDATE_EPS
        && b.y <= a.bottom() + VALIDATE_EPS
}
