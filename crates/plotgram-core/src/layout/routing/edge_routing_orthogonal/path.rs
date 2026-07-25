//! Shared orthogonal path-selection interface and helpers.

use super::*;
use crate::layout::geometry::{Point, EPS};
use crate::layout::Port;

/// A routed segment recorded for overlap detection.
#[derive(Clone, Copy)]
pub struct RoutedSegment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub edge_index: usize,
}

/// Optional diagnostics produced while selecting a path.
#[derive(Default)]
pub struct PathSelectStats {
    pub candidate_count: usize,
    pub hard_filter_reject_count: usize,
    pub degraded: bool,
}

/// Select a route with the LexAStar kernel.
pub fn select_best_path_with_scorer_stats(
    ctx: &OrthoRoutingContext,
    pair: &EndpointPair,
    _scorer: &dyn CandidateScorer,
    stats: Option<&mut PathSelectStats>,
    _phase1_only: bool,
) -> Vec<Point> {
    path_kernel::select_best_path_lex_astar(ctx, pair, stats)
}

/// Convert a port side to its outward unit vector.
pub(super) fn port_outward(side: Port) -> (f64, f64) {
    match side {
        Port::Top => (0.0, -1.0),
        Port::Bottom => (0.0, 1.0),
        Port::Left => (-1.0, 0.0),
        Port::Right => (1.0, 0.0),
    }
}

/// Pick an elbow whose departure from `stub` is never inward.
pub(super) fn port_aware_elbow(stub: Point, target: Point, side: Port) -> Point {
    let (ox, oy) = port_outward(side);
    let horizontal = Point::new(target.x, stub.y);
    let vertical = Point::new(stub.x, target.y);
    let score = |p: Point| {
        let dx = p.x - stub.x;
        let dy = p.y - stub.y;
        if dx.abs() < EPS && dy.abs() < EPS { f64::NEG_INFINITY } else { dx * ox + dy * oy }
    };
    match score(horizontal).partial_cmp(&score(vertical)) {
        Some(std::cmp::Ordering::Greater) => horizontal,
        Some(std::cmp::Ordering::Less) => vertical,
        _ if matches!(side, Port::Left | Port::Right) => vertical,
        _ => horizontal,
    }
}

const GROUP_EXIT_STUB_MARGIN: f64 = 8.0;

/// Required outward stub length for an edge leaving its source leaf group.
pub(super) fn source_group_exit_stub_len(
    ctx: &OrthoRoutingContext<'_>,
    from_id: &str,
    to_id: &str,
    from_side: Port,
    sx: f64,
    sy: f64,
) -> Option<f64> {
    let from_leaf = ctx.group_ctx.node_leaf_group.get(from_id)?;
    if ctx.group_ctx.node_leaf_group.get(to_id) == Some(from_leaf) { return None; }
    let group = ctx.group_ctx.groups.get(from_leaf)?;
    let (ox, oy) = port_outward(from_side);
    let distance = if ox > 0.0 { group.x + group.width - sx }
        else if ox < 0.0 { sx - group.x }
        else if oy > 0.0 { group.y + group.height - sy }
        else { sy - group.y };
    Some((distance + GROUP_EXIT_STUB_MARGIN).max(PORT_CLEARANCE))
}
