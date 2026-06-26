//! Slot/magnet point system for orthogonal edge routing

use super::*;
use crate::layout::geometry::Point;
use crate::layout::{NodeLayout, Port};
use crate::layout::edge::common::edge_geometry::node_center;

/// 同侧多边的汇流策略
///
/// 根据同一节点同一侧的边数自适应选择分布模式，实现"入口箭头合并"效果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockingStrategy {
    /// 1 条边：单点居中
    Single,
    /// 2-3 条边：紧凑分布（间距压缩到 16px），接近汇流但仍可区分
    Compact,
    /// 4+ 条边：汇流模式，所有边共享中心入口点，路径自然分支
    Concentrate,
}

/// 根据同侧边数选择汇流策略
pub fn choose_docking_strategy(count: usize) -> DockingStrategy {
    match count {
        0..=1 => DockingStrategy::Single,
        2..=3 => DockingStrategy::Compact,
        _ => DockingStrategy::Concentrate,
    }
}

/// Endpoint descriptor for slot assignment and path building.
///
/// Carries everything the slot-assignment pass (sorting by target) and the
/// path-building pass (anchor / side / node_id) need, so that `EndpointPair`
/// can be handed to `select_best_path` without extra parameters.
#[derive(Clone)]
pub struct Endpoint {
    pub edge_index: usize,
    pub is_from: bool,
    /// Opposite node center, used for slot sorting along the side.
    pub target_x: f64,
    pub target_y: f64,
    pub lane: usize,
    /// Node id this endpoint sits on.
    pub node_id: String,
    /// Connection side (port) on the node.
    pub side: Port,
    /// Resolved slot anchor coordinates (filled in during slot assignment).
    pub anchor: Point,
}

/// Deterministically choose connection sides based on geometric relationship
///
/// Returns `(side_a, side_b)`, the connection sides for nodes A and B in canonical order.
pub fn choose_pair_sides(a: &NodeLayout, b: &NodeLayout) -> (Port, Port) {
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

    // Only horizontal overlap (shared vertical band) → use top/bottom sides
    let prefer_vertical = if ox > EPS && oy <= EPS {
        true
    } else if oy > EPS && ox <= EPS {
        false
    } else if ox <= EPS {
        // 分列排布（无水平重叠）：垂直位移明显时优先走上下端口，避免绕到侧下方
        dy.abs() >= dx.abs() * 0.4
    } else {
        // Diagonal or mutual overlap: decide by dominant axis
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
