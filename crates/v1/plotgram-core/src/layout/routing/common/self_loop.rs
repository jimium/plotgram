//! 自环边统一几何：矩形折线（orthogonal）与小贝塞尔环（curved）。

use crate::ast::Relation;
use crate::layout::edge_point;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
use crate::layout::routing::common::edge_geometry::{build_edge_labels, node_center, parse_label_t, point_at_path_t};
use crate::layout::routing::model::{CubicPath, RoutePath};

/// 自环绘制风格
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfLoopStyle {
    /// 右上角矩形折线环（orthogonal / straight 默认）
    Orthogonal,
    /// 小贝塞尔环（circular / curved 默认）
    Curved,
}

/// 为每条自环边分配同节点内的序号（0, 1, 2…），用于角落轮转。
pub fn self_loop_indices(relations: &[Relation]) -> std::collections::HashMap<usize, usize> {
    let mut per_node: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut indices = std::collections::HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        if rel.from.as_str() != rel.to.as_str() {
            continue;
        }
        let node = rel.from.as_str();
        let idx = per_node.get(node).copied().unwrap_or(0);
        indices.insert(i, idx);
        per_node.insert(node.to_string(), idx + 1);
    }
    indices
}

/// 自环拓扑解（Slice D1）：只产 topology（[`RoutePath`]）+ 标签锚点事实，
/// 几何物化由 GeometryMaterializer 完成，标签由 LabelSolver 在冻结几何上放置。
#[derive(Debug, Clone)]
pub struct SelfLoopSolution {
    /// 路径拓扑：Curved → `Cubic`，Orthogonal → `Orthogonal` 折线。
    pub path: RoutePath,
    /// 标签锚点（Curved=贝塞尔 apex，Orthogonal=环外侧中点）。
    pub label_anchor: Point,
    /// 标签相对锚点的外向偏移。
    pub label_offset: Point,
    pub from_port: Port,
    pub to_port: Port,
}

/// 求解自环拓扑（Slice D1）：与 [`route_self_loop`] 同源同参，但不产 `EdgeLayout`。
pub fn solve_self_loop(
    node: &NodeLayout,
    loop_index: usize,
    style: SelfLoopStyle,
) -> SelfLoopSolution {
    let corner = corner_for_index(loop_index);
    match style {
        SelfLoopStyle::Orthogonal => {
            let loop_r = (node.width.min(node.height) * 0.28).max(16.0) + (loop_index as f64) * 6.0;
            solve_orthogonal_self_loop(node, corner, loop_r)
        }
        SelfLoopStyle::Curved => {
            let loop_r = (node.width.min(node.height) * 0.30).max(18.0) + (loop_index as f64) * 5.0;
            solve_curved_self_loop(node, corner, loop_r)
        }
    }
}

fn solve_orthogonal_self_loop(node: &NodeLayout, corner: Corner, loop_r: f64) -> SelfLoopSolution {
    let (path, label_point, from_port, to_port) = orthogonal_loop_geometry(node, corner, loop_r);
    SelfLoopSolution {
        path: RoutePath::orthogonal(path),
        label_anchor: label_point,
        label_offset: label_outward_offset(corner, loop_r),
        from_port,
        to_port,
    }
}

fn solve_curved_self_loop(node: &NodeLayout, corner: Corner, loop_r: f64) -> SelfLoopSolution {
    let (sx, sy, ex, ey, apex, from_port, to_port) = curved_loop_endpoints(node, corner, loop_r);
    let cp1 = Point::new(sx + loop_r * corner.dx, sy + loop_r * corner.dy);
    let cp2 = Point::new(
        apex.x - loop_r * corner.perp_x * 0.35,
        apex.y - loop_r * corner.perp_y * 0.35,
    );
    SelfLoopSolution {
        path: RoutePath::Cubic(CubicPath {
            start: Point::new(sx, sy),
            end: Point::new(ex, ey),
            controls: [cp1, cp2],
        }),
        label_anchor: apex,
        label_offset: label_outward_offset(corner, loop_r),
        from_port,
        to_port,
    }
}

/// 生成标准化自环几何；`loop_index` 决定角落轮转（0=右上，1=左上，2=右下，3=左下）。
pub fn route_self_loop(
    rel: &Relation,
    node: &NodeLayout,
    loop_index: usize,
    style: SelfLoopStyle,
) -> EdgeLayout {
    match style {
        SelfLoopStyle::Orthogonal => route_orthogonal_self_loop(rel, node, loop_index),
        SelfLoopStyle::Curved => route_curved_self_loop(rel, node, loop_index),
    }
}

/// Phase A: 空间感知自环路由。
///
/// 与 `route_self_loop` 的区别：
/// - 感知周围节点，选择净空最大的方向（而非固定轮转）
/// - 自适应环尺寸（根据可用空间调整）
/// - 避免与邻居节点重叠
pub fn route_self_loop_aware(
    rel: &Relation,
    node: &NodeLayout,
    node_id: &str,
    loop_index: usize,
    style: SelfLoopStyle,
    all_nodes: &std::collections::HashMap<String, NodeLayout>,
) -> EdgeLayout {
    // 计算四个方向的可用净空
    let clearances = compute_corner_clearances(node, node_id, all_nodes);
    // 选择最优方向（净空最大且未被先前的自环占用）
    let best_corner_idx = select_best_corner_index(loop_index, &clearances);
    match style {
        SelfLoopStyle::Orthogonal => {
            let loop_r = adaptive_loop_size(node, clearances[best_corner_idx].1, loop_index);
            route_orthogonal_self_loop_with_size(rel, node, best_corner_idx, loop_r)
        }
        SelfLoopStyle::Curved => {
            let loop_r = adaptive_loop_size(node, clearances[best_corner_idx].1, loop_index);
            route_curved_self_loop_with_size(rel, node, best_corner_idx, loop_r)
        }
    }
}

/// 四个角落方向的净空距离（到最近邻居节点的距离）
fn compute_corner_clearances(
    node: &NodeLayout,
    node_id: &str,
    all_nodes: &std::collections::HashMap<String, NodeLayout>,
) -> [(usize, f64); 4] {
    // 四个方向：0=右上, 1=左上, 2=右下, 3=左下
    let cx = node.x + node.width / 2.0;
    let cy = node.y + node.height / 2.0;
    let probe_dist = (node.width.max(node.height)) * 1.5 + 50.0;

    let mut clearances = [f64::MAX; 4];

    for (other_id, other) in all_nodes.iter() {
        if other_id.as_str() == node_id {
            continue;
        }
        let ocx = other.x + other.width / 2.0;
        let ocy = other.y + other.height / 2.0;

        // 判断邻居在哪个象限
        let dx = ocx - cx;
        let dy = ocy - cy;

        // 计算到邻居的最近距离（边缘到边缘）
        let gap_x = if dx > 0.0 {
            (other.x - (node.x + node.width)).max(0.0)
        } else {
            (node.x - (other.x + other.width)).max(0.0)
        };
        let gap_y = if dy > 0.0 {
            (other.y - (node.y + node.height)).max(0.0)
        } else {
            (node.y - (other.y + other.height)).max(0.0)
        };
        let dist = (gap_x * gap_x + gap_y * gap_y).sqrt();

        // 更新对应象限的净空
        if dx >= 0.0 && dy <= 0.0 {
            clearances[0] = clearances[0].min(dist); // 右上
        }
        if dx <= 0.0 && dy <= 0.0 {
            clearances[1] = clearances[1].min(dist); // 左上
        }
        if dx >= 0.0 && dy >= 0.0 {
            clearances[2] = clearances[2].min(dist); // 右下
        }
        if dx <= 0.0 && dy >= 0.0 {
            clearances[3] = clearances[3].min(dist); // 左下
        }
    }

    // 限制探测范围
    for c in clearances.iter_mut() {
        *c = c.min(probe_dist);
    }

    // 返回 (corner_index, clearance) 按净空降序
    let mut result = [
        (0, clearances[0]),
        (1, clearances[1]),
        (2, clearances[2]),
        (3, clearances[3]),
    ];
    result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    result
}

/// 选择最优角落：净空最大且未被先前的自环占用
fn select_best_corner_index(loop_index: usize, clearances: &[(usize, f64); 4]) -> usize {
    // 对于第 N 个自环，跳过前 N-1 个已占用的方向
    // clearances 已按净空降序排列
    if loop_index < 4 {
        clearances[loop_index].0
    } else {
        clearances[loop_index % 4].0
    }
}

/// 根据可用净空自适应计算环尺寸
fn adaptive_loop_size(node: &NodeLayout, clearance: f64, loop_index: usize) -> f64 {
    let base = node.width.min(node.height);
    let min_r = 14.0;
    let max_r = base * 0.45;
    // 净空的 50% 作为环尺寸，但不超过节点尺寸的 45%
    let from_clearance = clearance * 0.5;
    let from_node = base * 0.30 + loop_index as f64 * 8.0;
    from_clearance.min(from_node).clamp(min_r, max_r)
}

fn route_orthogonal_self_loop(rel: &Relation, node: &NodeLayout, loop_index: usize) -> EdgeLayout {
    let corner = corner_for_index(loop_index);
    let loop_r = (node.width.min(node.height) * 0.28).max(16.0) + (loop_index as f64) * 6.0;
    route_orthogonal_self_loop_inner(rel, node, corner, loop_r)
}

/// Phase A: 指定尺寸的自环路由（空间感知版本使用）
fn route_orthogonal_self_loop_with_size(
    rel: &Relation,
    node: &NodeLayout,
    corner_idx: usize,
    loop_r: f64,
) -> EdgeLayout {
    let corner = corner_for_index(corner_idx);
    route_orthogonal_self_loop_inner(rel, node, corner, loop_r)
}

fn route_orthogonal_self_loop_inner(
    rel: &Relation,
    node: &NodeLayout,
    corner: Corner,
    loop_r: f64,
) -> EdgeLayout {
    let (path, label_point, from_port, to_port) =
        orthogonal_loop_geometry(node, corner, loop_r);

    let label_offset = label_outward_offset(corner, loop_r);
    let middle_t = parse_label_t(rel);
    let labels = build_edge_labels(
        rel,
        middle_t,
        label_offset,
        |t| point_at_path_t(&path, t),
    );

    let mut edge = EdgeLayout {
        geometry: PathGeometry::Polyline { points: Vec::new() },
        labels,
        from_port,
        to_port,
    };
    edge.set_polyline_points(path);
    edge.labels.iter_mut().for_each(|lbl| {
        lbl.center = Point::new(
            label_point.x + label_offset.x,
            label_point.y + label_offset.y,
        );
    });
    edge
}

fn route_curved_self_loop(rel: &Relation, node: &NodeLayout, loop_index: usize) -> EdgeLayout {
    let corner = corner_for_index(loop_index);
    let loop_r = (node.width.min(node.height) * 0.30).max(18.0) + (loop_index as f64) * 5.0;
    route_curved_self_loop_inner(rel, node, corner, loop_r)
}

/// Phase A: 指定尺寸的曲线自环路由（空间感知版本使用）
fn route_curved_self_loop_with_size(
    rel: &Relation,
    node: &NodeLayout,
    corner_idx: usize,
    loop_r: f64,
) -> EdgeLayout {
    let corner = corner_for_index(corner_idx);
    route_curved_self_loop_inner(rel, node, corner, loop_r)
}

fn route_curved_self_loop_inner(
    rel: &Relation,
    node: &NodeLayout,
    corner: Corner,
    loop_r: f64,
) -> EdgeLayout {
    // Slice D1：与 solve_curved_self_loop 同一求解核，遗留调用方字节不变。
    let sol = solve_curved_self_loop(node, corner, loop_r);
    let apex = sol.label_anchor;
    let labels = build_edge_labels(rel, 0.5, sol.label_offset, |_| apex);
    let RoutePath::Cubic(cubic) = sol.path else {
        unreachable!("solve_curved_self_loop 必产 Cubic");
    };
    EdgeLayout {
        geometry: PathGeometry::Bezier {
            start: cubic.start,
            end: cubic.end,
            controls: cubic.controls,
        },
        labels,
        from_port: sol.from_port,
        to_port: sol.to_port,
    }
}

#[derive(Clone, Copy)]
struct Corner {
    dx: f64,
    dy: f64,
    perp_x: f64,
    perp_y: f64,
    from_port: Port,
    to_port: Port,
}

fn corner_for_index(loop_index: usize) -> Corner {
    match loop_index % 4 {
        0 => Corner {
            dx: 1.0,
            dy: -1.0,
            perp_x: 0.0,
            perp_y: -1.0,
            from_port: Port::Right,
            to_port: Port::Top,
        },
        1 => Corner {
            dx: -1.0,
            dy: -1.0,
            perp_x: 0.0,
            perp_y: -1.0,
            from_port: Port::Left,
            to_port: Port::Top,
        },
        2 => Corner {
            dx: 1.0,
            dy: 1.0,
            perp_x: 0.0,
            perp_y: 1.0,
            from_port: Port::Right,
            to_port: Port::Bottom,
        },
        _ => Corner {
            dx: -1.0,
            dy: 1.0,
            perp_x: 0.0,
            perp_y: 1.0,
            from_port: Port::Left,
            to_port: Port::Bottom,
        },
    }
}

fn orthogonal_loop_geometry(
    node: &NodeLayout,
    corner: Corner,
    loop_r: f64,
) -> (Vec<Point>, Point, Port, Port) {
    let cx = node.x + node.width / 2.0;
    let cy = node.y + node.height / 2.0;
    let hw = node.width / 2.0;
    let hh = node.height / 2.0;
    // Iteration 3：最小段长，避免小节点上 p4→end 退化成近零长度
    const MIN_SEG: f64 = 8.0;
    let inset_start = hh.max(MIN_SEG) * 0.15;
    let inset_near = (hw * 0.15).max(MIN_SEG * 0.5).min(hw.max(MIN_SEG));
    let inset_far = (hw * 0.35).max(MIN_SEG).min(hw.max(MIN_SEG));
    let loop_out = loop_r.max(MIN_SEG);

    let (start, p2, p3, p4, end, apex) = if corner.dx > 0.0 && corner.dy < 0.0 {
        let start = Point::new(node.x + node.width, cy - inset_start);
        let p2 = Point::new(start.x + loop_out, start.y);
        let p3 = Point::new(p2.x, node.y - loop_out);
        let p4 = Point::new(cx + inset_near, p3.y);
        let end = Point::new(cx + inset_far, node.y);
        let apex = Point::new(p3.x, p3.y - loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    } else if corner.dx < 0.0 && corner.dy < 0.0 {
        let start = Point::new(node.x, cy - inset_start);
        let p2 = Point::new(start.x - loop_out, start.y);
        let p3 = Point::new(p2.x, node.y - loop_out);
        let p4 = Point::new(cx - inset_near, p3.y);
        let end = Point::new(cx - inset_far, node.y);
        let apex = Point::new(p3.x, p3.y - loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    } else if corner.dx > 0.0 && corner.dy > 0.0 {
        let start = Point::new(node.x + node.width, cy + inset_start);
        let p2 = Point::new(start.x + loop_out, start.y);
        let p3 = Point::new(p2.x, node.y + node.height + loop_out);
        let p4 = Point::new(cx + inset_near, p3.y);
        let end = Point::new(cx + inset_far, node.y + node.height);
        let apex = Point::new(p3.x, p3.y + loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    } else {
        let start = Point::new(node.x, cy + inset_start);
        let p2 = Point::new(start.x - loop_out, start.y);
        let p3 = Point::new(p2.x, node.y + node.height + loop_out);
        let p4 = Point::new(cx - inset_near, p3.y);
        let end = Point::new(cx - inset_far, node.y + node.height);
        let apex = Point::new(p3.x, p3.y + loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    };

    (
        vec![start, p2, p3, p4, end],
        apex,
        corner.from_port,
        corner.to_port,
    )
}

fn curved_loop_endpoints(
    node: &NodeLayout,
    corner: Corner,
    loop_r: f64,
) -> (f64, f64, f64, f64, Point, Port, Port) {
    let center = node_center(node);
    let ncx = center.x;
    let ncy = center.y;

    // 端点落在节点边界（与正交自环契约一致），外凸由控制点负责
    let start_toward = match corner.from_port {
        Port::Right => (node.x + node.width + 10.0, ncy + corner.dy * node.height * 0.15),
        Port::Left => (node.x - 10.0, ncy + corner.dy * node.height * 0.15),
        Port::Top => (ncx + corner.dx * node.width * 0.15, node.y - 10.0),
        Port::Bottom => (ncx + corner.dx * node.width * 0.15, node.y + node.height + 10.0),
    };
    let end_toward = match corner.to_port {
        Port::Top => (
            ncx + corner.dx * node.width * 0.15 - corner.perp_x * loop_r * 0.2,
            node.y - 10.0,
        ),
        Port::Bottom => (
            ncx + corner.dx * node.width * 0.15 - corner.perp_x * loop_r * 0.2,
            node.y + node.height + 10.0,
        ),
        Port::Right => (node.x + node.width + 10.0, ncy + corner.dy * node.height * 0.15),
        Port::Left => (node.x - 10.0, ncy + corner.dy * node.height * 0.15),
    };
    let (sx, sy) = edge_point(node, start_toward.0, start_toward.1);
    let (ex, ey) = edge_point(node, end_toward.0, end_toward.1);

    let apex = Point::new(
        sx + corner.dx * loop_r * 1.5,
        sy + corner.dy * loop_r * 1.5,
    );

    (sx, sy, ex, ey, apex, corner.from_port, corner.to_port)
}

fn label_outward_offset(corner: Corner, loop_r: f64) -> Point {
    Point::new(
        corner.dx * (loop_r * 0.25),
        corner.dy * (loop_r * 0.25),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, SourceInfo, Span,
    };

    fn node_at(x: f64, y: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: 120.0,
            height: 50.0,
            ..Default::default()
        }
    }

    fn self_relation() -> Relation {
        Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("a"),
            arrow: ArrowType::Active,
            label: Some("retry".to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn self_loop_indices_counts_per_node() {
        let span = Span::dummy();
        let mk = |from: &str, to: &str| Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let relations = vec![mk("a", "a"), mk("a", "b"), mk("a", "a"), mk("b", "b")];
        let idx = self_loop_indices(&relations);
        assert_eq!(idx.get(&0), Some(&0));
        assert_eq!(idx.get(&2), Some(&1));
        assert_eq!(idx.get(&3), Some(&0));
        assert!(!idx.contains_key(&1));
    }

    #[test]
    fn orthogonal_self_loop_has_polyline() {
        let edge = route_self_loop(&self_relation(), &node_at(10.0, 20.0), 0, SelfLoopStyle::Orthogonal);
        assert!(edge.path_len() >= 4);
        assert!(edge.has_label());
    }

    #[test]
    fn curved_self_loop_is_bezier() {
        let edge = route_self_loop(&self_relation(), &node_at(10.0, 20.0), 1, SelfLoopStyle::Curved);
        assert!(matches!(edge.geometry, PathGeometry::Bezier { .. }));
    }

    #[test]
    fn multiple_loops_use_different_corners() {
        let rel = self_relation();
        let node = node_at(0.0, 0.0);
        let e0 = route_self_loop(&rel, &node, 0, SelfLoopStyle::Orthogonal);
        let e1 = route_self_loop(&rel, &node, 1, SelfLoopStyle::Orthogonal);
        assert_ne!(e0.from_port, e1.from_port);
    }
}
