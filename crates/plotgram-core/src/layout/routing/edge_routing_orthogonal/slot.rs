//! Slot/magnet point system for orthogonal edge routing

use super::*;
use crate::layout::geometry::Point;
use crate::layout::group::{CorridorAxis, GroupRoutingContext, SiblingOrientation};
use crate::layout::{NodeLayout, Port};
use crate::layout::routing::common::edge_geometry::node_center;

const VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP: f64 = 0.4;
const VERTICAL_PREFERENCE_THRESHOLD_HORIZONTAL_SIBLINGS: f64 = 0.8;
#[allow(dead_code)]
const VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_ALIGNED: f64 = 0.4;
const VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_UNALIGNED: f64 = 0.5;
const VERTICAL_PREFERENCE_THRESHOLD_CROSS_ANCESTOR: f64 = 0.5;
const SIDE_ALIGN_MARGIN: f64 = 20.0;
const EXIT_CHECK_DISTANCE: f64 = 32.0;

// DockingStrategy + choose_docking_strategy 已迁入 crate::layout::routing::model::solution（Slice C1）。
pub use crate::layout::routing::model::solution::{choose_docking_strategy, DockingStrategy};

/// R1：Endpoint 已外提到 `routing/model`；保留转发供 ortho 内 `use super::*`。
pub use crate::layout::routing::model::Endpoint;

/// Deterministically choose connection sides based on geometric relationship
///
/// Returns `(side_a, side_b)`, the connection sides for nodes A and B in canonical order.
#[allow(dead_code)]
pub fn choose_pair_sides(a: &NodeLayout, b: &NodeLayout) -> (Port, Port) {
    choose_pair_sides_with_group(a, b, "", "", None)
}

pub fn choose_pair_sides_with_group(
    a: &NodeLayout,
    b: &NodeLayout,
    a_id: &str,
    b_id: &str,
    group_ctx: Option<&GroupRoutingContext>,
) -> (Port, Port) {
    let a_center = node_center(a);
    let b_center = node_center(b);
    let acx = a_center.x;
    let acy = a_center.y;
    let bcx = b_center.x;
    let bcy = b_center.y;
    let dx = bcx - acx;
    let dy = bcy - acy;

    let ox = range_overlap(a.x, a.x + a.width, b.x, b.x + b.width);
    let oy = range_overlap(a.y, a.y + a.height, b.y, b.y + b.height);

    // Step 1: Hard constraint elimination
    let mut candidates: Vec<(Port, Port)> = Vec::new();
    if dy > -EPS {
        candidates.push((Port::Bottom, Port::Top));
    }
    if dy < EPS {
        candidates.push((Port::Top, Port::Bottom));
    }
    if dx > -EPS {
        candidates.push((Port::Right, Port::Left));
    }
    if dx < EPS {
        candidates.push((Port::Left, Port::Right));
    }

    if ox > EPS && oy <= EPS {
        candidates.retain(|(from, _)| is_vertical_port(*from));
    } else if oy > EPS && ox <= EPS {
        candidates.retain(|(from, _)| !is_vertical_port(*from));
    }

    // Step 2: Single candidate quick return
    if candidates.len() == 1 {
        return candidates[0];
    }
    if candidates.is_empty() {
        return fallback_by_axis(dx, dy, VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP);
    }

    // Step 3: Group-aware classification
    if let Some(ctx) = group_ctx {
        if let Some(forced) = group_forced_side(a, b, a_id, b_id, acx, acy, dx, dy, ox, oy, ctx) {
            return forced;
        }
    }

    let threshold = if let Some(ctx) = group_ctx {
        classify_threshold(a, b, a_id, b_id, acx, acy, dx, dy, ox, oy, ctx)
    } else {
        VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP
    };

    let preferred = select_by_threshold(dx, dy, ox, oy, threshold);

    // Step 4: Light exit check
    if has_clear_exit(a, preferred.0, a_id, group_ctx) && has_clear_exit(b, preferred.1, b_id, group_ctx) {
        return preferred;
    }

    for cand in &candidates {
        if *cand == preferred {
            continue;
        }
        if has_clear_exit(a, cand.0, a_id, group_ctx) && has_clear_exit(b, cand.1, b_id, group_ctx) {
            return *cand;
        }
    }

    preferred
}

fn group_forced_side(
    _a: &NodeLayout,
    _b: &NodeLayout,
    a_id: &str,
    b_id: &str,
    acx: f64,
    _acy: f64,
    dx: f64,
    _dy: f64,
    _ox: f64,
    _oy: f64,
    ctx: &GroupRoutingContext,
) -> Option<(Port, Port)> {
    let ga = ctx.node_leaf_group(a_id)?;
    let gb = ctx.node_leaf_group(b_id)?;
    let orient = ctx.sibling_orientation(ga, gb)?;

    if orient == SiblingOrientation::Vertical {
        let target_group = if dx < 0.0 { ctx.groups.get(gb) } else { ctx.groups.get(gb) };
        let target_gl = target_group?;
        let aligned = acx >= target_gl.x - SIDE_ALIGN_MARGIN
            && acx <= target_gl.x + target_gl.width + SIDE_ALIGN_MARGIN;
        if !aligned {
            let h_corridor = ctx.corridor_between_groups(ga, gb, CorridorAxis::Horizontal);
            if h_corridor.is_some() {
                if dx < 0.0 {
                    return Some((Port::Left, Port::Right));
                } else {
                    return Some((Port::Right, Port::Left));
                }
            }
        }
    }
    None
}

fn classify_threshold(
    _a: &NodeLayout,
    _b: &NodeLayout,
    a_id: &str,
    b_id: &str,
    _acx: f64,
    _acy: f64,
    _dx: f64,
    _dy: f64,
    _ox: f64,
    _oy: f64,
    ctx: &GroupRoutingContext,
) -> f64 {
    let ga = ctx.node_leaf_group(a_id);
    let gb = ctx.node_leaf_group(b_id);

    match (ga, gb) {
        (None, None) => VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP,
        (Some(ga), Some(gb)) if ga == gb => VERTICAL_PREFERENCE_THRESHOLD_SAME_GROUP,
        (Some(ga), Some(gb)) => {
            if let Some(orient) = ctx.sibling_orientation(ga, gb) {
                match orient {
                    SiblingOrientation::Horizontal => {
                        VERTICAL_PREFERENCE_THRESHOLD_HORIZONTAL_SIBLINGS
                    }
                    SiblingOrientation::Vertical => {
                        VERTICAL_PREFERENCE_THRESHOLD_VERTICAL_SIBLINGS_UNALIGNED
                    }
                }
            } else {
                VERTICAL_PREFERENCE_THRESHOLD_CROSS_ANCESTOR
            }
        }
        _ => VERTICAL_PREFERENCE_THRESHOLD_CROSS_ANCESTOR,
    }
}

fn select_by_threshold(dx: f64, dy: f64, ox: f64, oy: f64, threshold: f64) -> (Port, Port) {
    let prefer_vertical = if ox > EPS && oy <= EPS {
        true
    } else if oy > EPS && ox <= EPS {
        false
    } else if ox <= EPS {
        dy.abs() >= dx.abs() * threshold
    } else {
        dy.abs() >= dx.abs()
    };

    if prefer_vertical {
        if dy >= 0.0 {
            (Port::Bottom, Port::Top)
        } else {
            (Port::Top, Port::Bottom)
        }
    } else if dx >= 0.0 {
        (Port::Right, Port::Left)
    } else {
        (Port::Left, Port::Right)
    }
}

fn fallback_by_axis(dx: f64, dy: f64, threshold: f64) -> (Port, Port) {
    let prefer_vertical = dy.abs() >= dx.abs() * threshold;
    if prefer_vertical {
        if dy >= 0.0 {
            (Port::Bottom, Port::Top)
        } else {
            (Port::Top, Port::Bottom)
        }
    } else if dx >= 0.0 {
        (Port::Right, Port::Left)
    } else {
        (Port::Left, Port::Right)
    }
}

#[allow(dead_code)]
fn opposite_side(p: Port) -> Port {
    match p {
        Port::Top => Port::Bottom,
        Port::Bottom => Port::Top,
        Port::Left => Port::Right,
        Port::Right => Port::Left,
    }
}

fn has_clear_exit(
    nl: &NodeLayout,
    side: Port,
    node_id: &str,
    group_ctx: Option<&GroupRoutingContext>,
) -> bool {
    let ctx = match group_ctx {
        Some(c) => c,
        None => return true,
    };
    let center = node_center(nl);
    let (start, end) = match side {
        Port::Top => (Point::new(center.x, nl.y - EXIT_CHECK_DISTANCE), Point::new(center.x, nl.y)),
        Port::Bottom => (Point::new(center.x, nl.y + nl.height), Point::new(center.x, nl.y + nl.height + EXIT_CHECK_DISTANCE)),
        Port::Left => (Point::new(nl.x - EXIT_CHECK_DISTANCE, center.y), Point::new(nl.x, center.y)),
        Port::Right => (Point::new(nl.x + nl.width, center.y), Point::new(nl.x + nl.width + EXIT_CHECK_DISTANCE, center.y)),
    };

    let node_groups: std::collections::HashSet<&str> = ctx
        .node_to_groups
        .get(node_id)
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    let mut group_ids: Vec<&String> = ctx.groups.keys().collect();
    group_ids.sort();
    for gid in group_ids {
        let gl = &ctx.groups[gid];
        if node_groups.contains(gid.as_str()) {
            continue;
        }
        if segment_bbox_overlaps_group(start, end, gl) {
            return false;
        }
    }
    true
}

fn segment_bbox_overlaps_group(a: Point, b: Point, gl: &crate::layout::GroupLayout) -> bool {
    let min_x = a.x.min(b.x) - EPS;
    let max_x = a.x.max(b.x) + EPS;
    let min_y = a.y.min(b.y) - EPS;
    let max_y = a.y.max(b.y) + EPS;

    if max_x < gl.x || min_x > gl.x + gl.width {
        return false;
    }
    if max_y < gl.y || min_y > gl.y + gl.height {
        return false;
    }
    true
}

/// Calculate slot fraction for the `rank`-th connection point out of `count` on an edge of length `edge_len`
///
/// Uses "fixed pitch + centered" strategy: adjacent points are spaced at `pitch`,
/// the whole group is centered symmetrically; when the edge is too short,
/// spacing is automatically compressed to fit within margins.
pub fn slot_fraction(rank: usize, count: usize, edge_len: f64, pitch: f64) -> f64 {
    if count <= 1 {
        return 0.5;
    }
    let usable = edge_len * (1.0 - 2.0 * SLOT_MARGIN_RATIO);
    let span = (pitch * (count as f64 - 1.0)).min(usable);
    let pitch = span / (count as f64 - 1.0);
    let offset = (rank as f64 - (count as f64 - 1.0) / 2.0) * pitch;
    0.5 + offset / edge_len.max(EPS)
}

/// 与 [`slot_fraction`] 相同的「固定间距 + 居中」策略，但围绕给定的 `base_frac`
/// 展开而非固定 0.5。
///
/// 用于同一节点同一侧存在多个并线子组时：先为每个子组分配一个互不重叠的
/// 锚点带中心 `base_frac`，再让子组内的连接点围绕该中心紧凑分布。
/// 当 `base_frac == 0.5` 时与 [`slot_fraction`] 完全等价。
pub fn slot_fraction_around(
    rank: usize,
    count: usize,
    edge_len: f64,
    pitch: f64,
    base_frac: f64,
) -> f64 {
    if count <= 1 {
        return base_frac;
    }
    let usable = edge_len * (1.0 - 2.0 * SLOT_MARGIN_RATIO);
    let span = (pitch * (count as f64 - 1.0)).min(usable);
    let actual_pitch = span / (count as f64 - 1.0);
    let offset = (rank as f64 - (count as f64 - 1.0) / 2.0) * actual_pitch;
    let frac = base_frac + offset / edge_len.max(EPS);
    frac.clamp(SLOT_MARGIN_RATIO, 1.0 - SLOT_MARGIN_RATIO)
}

/// Calculate slot anchor coordinates for a given fraction on a node's side
pub fn slot_anchor(nl: &NodeLayout, side: Port, frac: f64) -> Point {
    match side {
        Port::Top => Point::new(nl.x + nl.width * frac, nl.y),
        Port::Bottom => Point::new(nl.x + nl.width * frac, nl.y + nl.height),
        Port::Left => Point::new(nl.x, nl.y + nl.height * frac),
        Port::Right => Point::new(nl.x + nl.width, nl.y + nl.height * frac),
    }
}

/// Check if a port is vertical (top/bottom)
pub fn is_vertical_port(side: Port) -> bool {
    matches!(side, Port::Top | Port::Bottom)
}

fn range_overlap(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    (a_max.min(b_max) - a_min.max(b_min)).max(0.0)
}
