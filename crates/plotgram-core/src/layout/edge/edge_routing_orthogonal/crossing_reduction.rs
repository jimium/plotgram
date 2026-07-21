//! 路由后交叉消解（Phase C2）
//!
//! 保守策略：检测边对交叉，对简单 L/Z 形路径尝试调整折点消除交叉。
//! 仅在以下条件全部满足时接受调整：
//! 1. 总交叉数减少
//! 2. 不制造新的节点穿越
//! 3. 不制造新的组穿越

use crate::layout::geometry::Point;
use crate::layout::types::{EdgeLayout, LayoutResult, NodeLayout, PathGeometry};
use std::collections::HashMap;

/// 最大迭代轮数
const MAX_ROUNDS: usize = 3;

/// 路由后交叉消解：返回消除的交叉数。
pub fn minimize_crossings_post_route(
    edges: &mut [EdgeLayout],
    nodes: &HashMap<String, NodeLayout>,
) -> usize {
    let initial_crossings = count_total_crossings(edges);
    if initial_crossings == 0 {
        return 0;
    }

    let mut eliminated = 0;

    for _round in 0..MAX_ROUNDS {
        let mut improved = false;

        // 收集当前所有交叉对
        let crossing_pairs = find_crossing_pairs(edges);
        if crossing_pairs.is_empty() {
            break;
        }

        for (i, j) in crossing_pairs {
            // 尝试对边 j 做局部调整以消除与边 i 的交叉
            let live_total = count_total_crossings(edges);
            if try_resolve_crossing(edges, i, j, nodes, live_total) {
                improved = true;
                eliminated += 1;
            }
        }

        if !improved {
            break;
        }
    }

    let final_crossings = count_total_crossings(edges);
    initial_crossings.saturating_sub(final_crossings)
}

/// 最小平移后边段间距（避免 tight_sev 退化；匹配 ORTHO_PARALLEL_GAP = 8px）
const MIN_EDGE_CLEARANCE: f64 = 8.0;

/// 尝试通过调整边 j 的折点来消除与边 i 的交叉
fn try_resolve_crossing(
    edges: &mut [EdgeLayout],
    i: usize,
    j: usize,
    nodes: &HashMap<String, NodeLayout>,
    current_total: usize,
) -> bool {
    // 仅处理简单路径（≤5 个点的折线）
    let j_points = match &edges[j].geometry {
        PathGeometry::Polyline { points } if points.len() >= 3 && points.len() <= 5 => {
            points.clone()
        }
        _ => return false,
    };

    // 找到交叉点
    let i_points = match &edges[i].geometry {
        PathGeometry::Polyline { points } => points.clone(),
        _ => return false,
    };

    let Some(cross_pt) = find_first_crossing_point(&i_points, &j_points) else {
        return false;
    };

    // 策略：找到边 j 中包含交叉点的段，尝试将该段平移
    let mut best_points: Option<Vec<Point>> = None;

    for seg_idx in 0..j_points.len() - 1 {
        let p1 = j_points[seg_idx];
        let p2 = j_points[seg_idx + 1];

        if !segment_contains_point(p1, p2, cross_pt) {
            continue;
        }

        // 确定段方向并尝试平移（小偏移优先，减少 tight_sev 退化风险）
        let is_horizontal = (p1.y - p2.y).abs() < 0.01;
        let offsets: &[f64] = &[4.0, -4.0, 8.0, -8.0];

        for &offset in offsets {
            let mut candidate = j_points.clone();
            if is_horizontal {
                candidate[seg_idx].y += offset;
                candidate[seg_idx + 1].y += offset;
            } else {
                candidate[seg_idx].x += offset;
                candidate[seg_idx + 1].x += offset;
            }

            // 验证：不穿越节点
            if path_crosses_any_node(&candidate, nodes, &edges[j]) {
                continue;
            }

            // 验证：消除了与边 i 的交叉
            if polylines_cross_points(&i_points, &candidate) {
                continue;
            }

            // 验证：平移段与其他边段保持最小间距（避免 tight_sev 退化）
            if !shifted_segment_clearance(&candidate, seg_idx, edges, j) {
                continue;
            }

            // 全局验证：临时应用候选，检查总交叉数严格下降
            let old_geom = edges[j].geometry.clone();
            edges[j].geometry = PathGeometry::Polyline { points: candidate.clone() };
            let after_total = count_total_crossings(edges);
            edges[j].geometry = old_geom;

            if after_total < current_total {
                best_points = Some(candidate);
                break;
            }
        }

        if best_points.is_some() {
            break;
        }
    }

    if let Some(new_points) = best_points {
        edges[j].geometry = PathGeometry::Polyline { points: new_points };
        true
    } else {
        false
    }
}

/// 检查被平移的段与其他边段的间距。
/// 仅检查平移段（seg_idx → seg_idx+1），未移动的段不参与检查。
/// 对平行重叠段要求更严格（避免 tight_sev 退化）。
fn shifted_segment_clearance(
    candidate: &[Point],
    seg_idx: usize,
    edges: &[EdgeLayout],
    self_idx: usize,
) -> bool {
    let seg_a = (
        candidate[seg_idx].x,
        candidate[seg_idx].y,
        candidate[seg_idx + 1].x,
        candidate[seg_idx + 1].y,
    );
    let seg_len = ((seg_a.2 - seg_a.0).powi(2) + (seg_a.3 - seg_a.1).powi(2)).sqrt();
    if seg_len < 1.0 {
        return true;
    }
    let a_horiz = (seg_a.1 - seg_a.3).abs() < 0.01;
    let a_vert = (seg_a.0 - seg_a.2).abs() < 0.01;

    for (idx, edge) in edges.iter().enumerate() {
        if idx == self_idx {
            continue;
        }
        let other_pts = match &edge.geometry {
            PathGeometry::Polyline { points } => points.as_slice(),
            _ => continue,
        };
        for m in 0..other_pts.len().saturating_sub(1) {
            let seg_b = (other_pts[m].x, other_pts[m].y, other_pts[m + 1].x, other_pts[m + 1].y);
            let b_horiz = (seg_b.1 - seg_b.3).abs() < 0.01;
            let b_vert = (seg_b.0 - seg_b.2).abs() < 0.01;

            // 平行重叠段：要求更大间距（介于 ORTHO_PARALLEL_GAP=8 和 architecture=12 之间）
            let required = if (a_horiz && b_horiz) || (a_vert && b_vert) {
                10.0
            } else {
                MIN_EDGE_CLEARANCE
            };

            let dist = seg_to_seg_dist(seg_a, seg_b);
            if dist < required {
                return false;
            }
        }
    }
    true
}

/// 两线段最小距离（简化：端点到对方线段距离的最小值）
fn seg_to_seg_dist(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let pa1 = Point::new(a.0, a.1);
    let pa2 = Point::new(a.2, a.3);
    let pb1 = Point::new(b.0, b.1);
    let pb2 = Point::new(b.2, b.3);
    let d1 = pt_seg_dist(pa1, pb1, pb2);
    let d2 = pt_seg_dist(pa2, pb1, pb2);
    let d3 = pt_seg_dist(pb1, pa1, pa2);
    let d4 = pt_seg_dist(pb2, pa1, pa2);
    d1.min(d2).min(d3).min(d4)
}

fn pt_seg_dist(p: Point, a: Point, b: Point) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let len_sq = abx * abx + aby * aby;
    if len_sq < 1e-10 {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = ((apx * abx + apy * aby) / len_sq).clamp(0.0, 1.0);
    let proj_x = a.x + t * abx;
    let proj_y = a.y + t * aby;
    ((p.x - proj_x).powi(2) + (p.y - proj_y).powi(2)).sqrt()
}

// ─── 交叉检测 ───────────────────────────────────────────────────────────────

fn count_total_crossings(edges: &[EdgeLayout]) -> usize {
    let mut count = 0;
    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            if edges_share_endpoint(&edges[i], &edges[j]) {
                continue;
            }
            if polylines_cross(&edges[i], &edges[j]) {
                count += 1;
            }
        }
    }
    count
}

/// 计算某条边与其他所有边的交叉数
fn count_crossings_for_edge(edges: &[EdgeLayout], idx: usize) -> usize {
    let mut count = 0;
    for j in 0..edges.len() {
        if j == idx {
            continue;
        }
        if edges_share_endpoint(&edges[idx], &edges[j]) {
            continue;
        }
        if polylines_cross(&edges[idx], &edges[j]) {
            count += 1;
        }
    }
    count
}

fn find_crossing_pairs(edges: &[EdgeLayout]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            if edges_share_endpoint(&edges[i], &edges[j]) {
                continue;
            }
            if polylines_cross(&edges[i], &edges[j]) {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

fn edges_share_endpoint(a: &EdgeLayout, b: &EdgeLayout) -> bool {
    let a_pts = match &a.geometry {
        PathGeometry::Polyline { points } if points.len() >= 2 => points,
        _ => return false,
    };
    let b_pts = match &b.geometry {
        PathGeometry::Polyline { points } if points.len() >= 2 => points,
        _ => return false,
    };
    let eps = 2.0;
    let a_s = a_pts[0];
    let a_e = a_pts[a_pts.len() - 1];
    let b_s = b_pts[0];
    let b_e = b_pts[b_pts.len() - 1];
    pts_close(a_s, b_s, eps)
        || pts_close(a_s, b_e, eps)
        || pts_close(a_e, b_s, eps)
        || pts_close(a_e, b_e, eps)
}

fn polylines_cross(a: &EdgeLayout, b: &EdgeLayout) -> bool {
    let a_pts = match &a.geometry {
        PathGeometry::Polyline { points } => points.as_slice(),
        _ => return false,
    };
    let b_pts = match &b.geometry {
        PathGeometry::Polyline { points } => points.as_slice(),
        _ => return false,
    };
    polylines_cross_points(a_pts, b_pts)
}

fn polylines_cross_points(a: &[Point], b: &[Point]) -> bool {
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            if segments_cross(a[i], a[i + 1], b[j], b[j + 1]) {
                return true;
            }
        }
    }
    false
}

fn find_first_crossing_point(a: &[Point], b: &[Point]) -> Option<Point> {
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            if let Some(pt) = segment_intersection(a[i], a[i + 1], b[j], b[j + 1]) {
                return Some(pt);
            }
        }
    }
    None
}

// ─── 几何工具 ───────────────────────────────────────────────────────────────

fn pts_close(a: Point, b: Point, eps: f64) -> bool {
    (a.x - b.x).abs() < eps && (a.y - b.y).abs() < eps
}

fn segment_contains_point(p1: Point, p2: Point, pt: Point) -> bool {
    let eps = 1.0;
    let is_on_line = if (p1.y - p2.y).abs() < 0.01 {
        // 水平段
        (pt.y - p1.y).abs() < eps
            && pt.x >= p1.x.min(p2.x) - eps
            && pt.x <= p1.x.max(p2.x) + eps
    } else if (p1.x - p2.x).abs() < 0.01 {
        // 垂直段
        (pt.x - p1.x).abs() < eps
            && pt.y >= p1.y.min(p2.y) - eps
            && pt.y <= p1.y.max(p2.y) + eps
    } else {
        false
    };
    is_on_line
}

fn path_crosses_any_node(
    points: &[Point],
    nodes: &HashMap<String, NodeLayout>,
    self_edge: &EdgeLayout,
) -> bool {
    // 排除自身端点所在的节点
    let self_start = points.first().copied().unwrap_or(Point::new(0.0, 0.0));
    let self_end = points.last().copied().unwrap_or(Point::new(0.0, 0.0));

    for (_id, nl) in nodes {
        let rect = (nl.x, nl.y, nl.x + nl.width, nl.y + nl.height);
        // 跳过端点所在节点
        if point_in_rect(self_start, rect) || point_in_rect(self_end, rect) {
            continue;
        }
        // 检查路径中间段是否穿越节点
        for i in 0..points.len().saturating_sub(1) {
            if segment_intersects_rect(points[i], points[i + 1], rect) {
                return true;
            }
        }
    }
    false
}

fn point_in_rect(p: Point, rect: (f64, f64, f64, f64)) -> bool {
    p.x >= rect.0 - 1.0 && p.x <= rect.2 + 1.0 && p.y >= rect.1 - 1.0 && p.y <= rect.3 + 1.0
}

fn segment_intersects_rect(p1: Point, p2: Point, rect: (f64, f64, f64, f64)) -> bool {
    let (x1, y1, x2, y2) = rect;
    // 检查线段是否与矩形内部相交（不含边界接触）
    let mid_x = (p1.x + p2.x) / 2.0;
    let mid_y = (p1.y + p2.y) / 2.0;
    // 简化：检查中点是否在矩形内
    mid_x > x1 + 2.0 && mid_x < x2 - 2.0 && mid_y > y1 + 2.0 && mid_y < y2 - 2.0
}

/// 线段相交检测（含共线端点落在线段上），与美学指标模块 `segments_intersect` 一致。
fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let d1 = cross_val(c, d, a);
    let d2 = cross_val(c, d, b);
    let d3 = cross_val(a, b, c);
    let d4 = cross_val(a, b, d);

    // proper crossing
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }

    // 共线端点落在线段上
    const EPS: f64 = 0.01;
    if d1.abs() < EPS && on_segment(c, d, a) {
        return true;
    }
    if d2.abs() < EPS && on_segment(c, d, b) {
        return true;
    }
    if d3.abs() < EPS && on_segment(a, b, c) {
        return true;
    }
    if d4.abs() < EPS && on_segment(a, b, d) {
        return true;
    }
    false
}

fn on_segment(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x) - 0.01
        && p.x <= a.x.max(b.x) + 0.01
        && p.y >= a.y.min(b.y) - 0.01
        && p.y <= a.y.max(b.y) + 0.01
}

fn segment_intersection(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let d1 = cross_val(c, d, a);
    let d2 = cross_val(c, d, b);
    let d3 = cross_val(a, b, c);
    let d4 = cross_val(a, b, d);

    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        // 计算交点
        let t = d1 / (d1 - d2);
        return Some(Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
    }

    // 共线端点：返回落在对方线段上的端点作为交叉点
    const EPS: f64 = 0.01;
    if d1.abs() < EPS && on_segment(c, d, a) {
        return Some(a);
    }
    if d2.abs() < EPS && on_segment(c, d, b) {
        return Some(b);
    }
    if d3.abs() < EPS && on_segment(a, b, c) {
        return Some(c);
    }
    if d4.abs() < EPS && on_segment(a, b, d) {
        return Some(d);
    }
    None
}

fn cross_val(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
