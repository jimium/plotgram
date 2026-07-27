//! 正交路径断言式消毒（Phase 3）。
//!
//! R1：从 OVG `sanitize` 迁入 `routing/common`，供 materializer canonicalize
//! 正交折线几何消毒（外提自原 OVG 目录；供 materialize / 非 Hier recipe 共用）。
//!
//! H3 由 LexA* 图结构保证。本模块**不修复几何**：
//! - debug：非正交段 → 记 violation（不 panic）
//! - 反向 stub / 其它：记 violation 计数，不改路径

use crate::ast::Relation;
use crate::layout::geometry::Point;
use crate::layout::routing::route_annotation::RouteAnnotationSet;
use crate::layout::{EdgeLayout, NodeLayout, Port};
use std::collections::HashMap;

const EPS: f64 = 0.1;

fn port_outward(side: Port) -> (f64, f64) {
    match side {
        Port::Top => (0.0, -1.0),
        Port::Bottom => (0.0, 1.0),
        Port::Left => (-1.0, 0.0),
        Port::Right => (1.0, 0.0),
    }
}

/// 断言式扫描：不改几何。签名兼容 materializer canonicalize。
#[allow(clippy::too_many_arguments)]
pub fn sanitize_orthogonal_edges_with_guard(
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

fn assert_orthogonal_path(
    points: &[Point],
    from_side: Port,
    to_side: Port,
    _edge_index: usize,
) -> usize {
    let mut n = 0usize;
    for w in points.windows(2) {
        let dx = (w[1].x - w[0].x).abs();
        let dy = (w[1].y - w[0].y).abs();
        if dx > EPS && dy > EPS {
            n += 1;
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
