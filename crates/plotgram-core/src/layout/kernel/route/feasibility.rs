//! H0–H6 可行性判定器：薄封装既有几何检查。
//!
//! Phase 2.0：H0 与 [`RouteAuditor`] E1（`EndpointBoundary` / `StubDirection`）对齐同一容差与规则。

use crate::layout::geometry::Point;
use crate::layout::group::GroupRoutingContext;
use crate::layout::routing::common::path_clean::{
    path_avoids_group_interiors, path_is_clean,
};
use crate::layout::types::{EdgeLayout, GroupLayout, NodeLayout, Port};
use std::collections::HashMap;

/// 与 `routing/model/audit.rs` E1 一致。
const ENDPOINT_BOUNDARY_BAND: f64 = 8.0;
const AXIS_EPS: f64 = 0.01;
/// 正交性检查容差（略宽于 AXIS_EPS，对齐既有 sanitize/评分）。
const ORTHO_EPS: f64 = 0.5;

/// 硬约束种类（对应方案 H0–H6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HardConstraintKind {
    /// H0：端点落在节点边界带（E1 EndpointBoundary）。
    H0EndpointBoundary,
    /// H0b：stub 方向与 port 外向一致（E1 StubDirection）；计入 H0 汇总。
    H0StubDirection,
    /// H1：不穿节点实体。
    H1ThroughNode,
    /// H2：不穿非成员 group 内部。
    H2GroupInterior,
    /// H3：正交族全段轴对齐。
    H3NonOrthogonal,
    /// H4：端口/通道容量（占位）。
    H4ResourceCapacity,
    /// H5：并行/反向边最小间距（占位）。
    H5MinSeparation,
    /// H6：确定性由构造保证。
    H6Determinism,
}

/// 单条边的硬约束违反。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeHardViolation {
    pub edge_index: usize,
    pub kind: HardConstraintKind,
    pub detail: String,
}

/// 对单条边的路径跑 H0–H3。
pub fn check_edge_hard_constraints(
    edge_index: usize,
    edge: &EdgeLayout,
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    group_ctx: &GroupRoutingContext,
    sorted_node_ids: &[String],
    sorted_group_ids: &[String],
) -> Vec<EdgeHardViolation> {
    let mut out = Vec::new();
    if edge.path_is_empty() {
        return out;
    }
    let pts: Vec<Point> = edge.path_points().into_owned();
    if pts.len() < 2 {
        out.push(EdgeHardViolation {
            edge_index,
            kind: HardConstraintKind::H0EndpointBoundary,
            detail: "path has fewer than 2 points".into(),
        });
        return out;
    }

    // H3：正交性
    if !path_is_orthogonal(&pts) {
        out.push(EdgeHardViolation {
            edge_index,
            kind: HardConstraintKind::H3NonOrthogonal,
            detail: "non-axis-aligned segment".into(),
        });
    }

    // H0 EndpointBoundary：点到矩形边界距离（与 RouteAuditor::rect_boundary_distance 同语义）
    for (node_id, pt, which) in [
        (from_id, pts[0], "from"),
        (to_id, pts[pts.len() - 1], "to"),
    ] {
        let Some(nl) = nodes.get(node_id) else {
            continue;
        };
        if rect_boundary_distance(pt, nl.x, nl.y, nl.width, nl.height) > ENDPOINT_BOUNDARY_BAND {
            out.push(EdgeHardViolation {
                edge_index,
                kind: HardConstraintKind::H0EndpointBoundary,
                detail: format!("{which} endpoint not on boundary of {node_id}"),
            });
        }
    }

    // H0 StubDirection：首末段须轴对齐且与 port 外向一致（与 RouteAuditor::check_stub 同语义）
    let first = (pts[0], pts[1]);
    let last = (pts[pts.len() - 1], pts[pts.len() - 2]);
    for (port, (a, b), which) in [
        (edge.from_port, first, "from"),
        (edge.to_port, last, "to"),
    ] {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        if dx.abs() <= AXIS_EPS && dy.abs() <= AXIS_EPS {
            continue; // 零长首段：交给完整性审计
        }
        let dir_ok = match port {
            Port::Top => dy < -AXIS_EPS && dx.abs() <= AXIS_EPS,
            Port::Bottom => dy > AXIS_EPS && dx.abs() <= AXIS_EPS,
            Port::Left => dx < -AXIS_EPS && dy.abs() <= AXIS_EPS,
            Port::Right => dx > AXIS_EPS && dy.abs() <= AXIS_EPS,
        };
        if !dir_ok {
            out.push(EdgeHardViolation {
                edge_index,
                kind: HardConstraintKind::H0StubDirection,
                detail: format!("{which} stub direction != {port:?}"),
            });
        }
    }

    // H1：穿节点
    if !path_is_clean(
        &pts,
        from_id,
        to_id,
        nodes,
        group_ctx,
        sorted_node_ids,
    ) {
        out.push(EdgeHardViolation {
            edge_index,
            kind: HardConstraintKind::H1ThroughNode,
            detail: "path crosses node interior".into(),
        });
    }

    // H2：穿组
    if !groups.is_empty()
        && !path_avoids_group_interiors(
            &pts,
            from_id,
            to_id,
            group_ctx,
            sorted_group_ids,
        )
    {
        out.push(EdgeHardViolation {
            edge_index,
            kind: HardConstraintKind::H2GroupInterior,
            detail: "path crosses unrelated group interior".into(),
        });
    }

    out
}

fn path_is_orthogonal(pts: &[Point]) -> bool {
    for w in pts.windows(2) {
        let dx = (w[0].x - w[1].x).abs();
        let dy = (w[0].y - w[1].y).abs();
        if dx > ORTHO_EPS && dy > ORTHO_EPS {
            return false;
        }
    }
    true
}

/// 点到矩形边界的距离（内外均为非负）——与 RouteAuditor 一致。
fn rect_boundary_distance(p: Point, x: f64, y: f64, w: f64, h: f64) -> f64 {
    let dx_out = (x - p.x).max(p.x - (x + w)).max(0.0);
    let dy_out = (y - p.y).max(p.y - (y + h)).max(0.0);
    if dx_out > 0.0 || dy_out > 0.0 {
        (dx_out * dx_out + dy_out * dy_out).sqrt()
    } else {
        (p.x - x)
            .min(x + w - p.x)
            .min(p.y - y)
            .min(y + h - p.y)
    }
}
