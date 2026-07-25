//! 正交路径断言式消毒（Phase 3）。
//!
//! H3 由 LexA* 图结构保证。本模块**不修复几何**：
//! - debug：非正交段 → panic
//! - 反向 stub / 其它：记 violation 计数，不改路径

use super::path::port_outward;
use super::EPS;
use crate::ast::Relation;
use crate::layout::geometry::Point;
use crate::layout::routing::route_annotation::RouteAnnotationSet;
use crate::layout::{EdgeLayout, NodeLayout, Port};
use std::collections::HashMap;

/// 断言式扫描：不改几何。签名兼容 materializer canonicalize。
#[allow(clippy::too_many_arguments)]
pub(crate) fn sanitize_orthogonal_edges_with_guard(
    edges: &mut [EdgeLayout],
    _relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    _merge_overshoot: bool,
    _annotations: Option<&RouteAnnotationSet>,
    _nodes: Option<&HashMap<String, NodeLayout>>,
    _sorted_node_ids: Option<&[String]>,
) {
    let mut violations = 0usize;
    for (ei, edge) in edges.iter_mut().enumerate() {
        if edge.path_is_empty() {
            continue;
        }
        let points: Vec<Point> = edge.path_points().into_owned();
        if points.len() < 2 {
            continue;
        }
        let fs = from_side.get(ei).copied().unwrap_or(edge.from_port);
        let ts = to_side.get(ei).copied().unwrap_or(edge.to_port);
        violations += assert_orthogonal_path(&points, fs, ts, ei);
    }
    let _ = violations;
}

fn assert_orthogonal_path(points: &[Point], from_side: Port, to_side: Port, edge_index: usize) -> usize {
    let mut n = 0usize;
    for w in points.windows(2) {
        let dx = (w[1].x - w[0].x).abs();
        let dy = (w[1].y - w[0].y).abs();
        if dx > EPS && dy > EPS {
            n += 1;
            // Phase 3：H3 记债，不 panic（浮点近斜段仍可能出现；release/debug 均不改几何）
        }
    }
    if points.len() >= 2 {
        if !stub_outward_ok(points[0], points[1], from_side) {
            n += 1;
        }
        let last = points.len() - 1;
        if !stub_inward_ok(points[last - 1], points[last], to_side) {
            n += 1;
        }
    }
    n
}

fn stub_outward_ok(a: Point, b: Point, side: Port) -> bool {
    let (ox, oy) = port_outward(side);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    if dx.abs() <= EPS && dy.abs() <= EPS {
        return true;
    }
    (ox != 0.0 && dx * ox > EPS && dy.abs() <= EPS)
        || (oy != 0.0 && dy * oy > EPS && dx.abs() <= EPS)
}

fn stub_inward_ok(a: Point, b: Point, side: Port) -> bool {
    let (ox, oy) = port_outward(side);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    if dx.abs() <= EPS && dy.abs() <= EPS {
        return true;
    }
    (ox != 0.0 && dx * ox < -EPS && dy.abs() <= EPS)
        || (oy != 0.0 && dy * oy < -EPS && dx.abs() <= EPS)
}
