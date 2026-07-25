//! 圆形布局专用弧形边路由
//!
//! 注册为 `edge_routing: circular`。读取布局产出的圆簇提示（或从节点坐标反推），
//! 同圆内边走弧形贝塞尔，跨圆边走端口贝塞尔。
//!
//! 障碍避让：路由完成后采样曲线检测穿障，穿障的边退化到 spline 绕行折线。

use crate::ast::{Diagram};
use crate::layout::geometry::Point;
use crate::layout::routing::common::circular_support::{
    CircleGroup, resolve_circle_groups,
};
use crate::layout::{
    edge_point, EdgeLayout, LayoutResult, NodeLayout, PathGeometry, Port,
};
use crate::layout::routing::common::edge_geometry::{
    node_center, undirected_pair_key, select_port, compute_bezier_controls, cubic_bezier_point,
    DEFAULT_BEZIER_TENSION, build_edge_labels,
};
use crate::layout::routing::common::self_loop::{self_loop_indices, route_self_loop, SelfLoopStyle};
use crate::layout::routing::common::label_placement::{LabelContext, LabelPlacer, RadialPlacer};
use std::collections::HashMap;

const PARALLEL_SPACING: f64 = 0.10;

/// 在圆形节点布局完成后，为所有边计算弧形路径
pub fn route_edges_circular(diagram: &Diagram, mut result: LayoutResult) -> LayoutResult {
    let circles = resolve_circle_groups(diagram, &result.nodes, &result.hints);
    if circles.is_empty() || diagram.relations.is_empty() {
        return result;
    }

    let node_placement = build_node_placement(diagram, &circles);
    let (lane_offsets, arc_sides) = compute_lane_offsets(diagram, &node_placement);

    // 构建障碍索引（用于穿障检测与退化绕行）
    // 4.2: 懒构建——快速预检无边可能穿障时跳过 O(n²) 构建
    let (node_id_to_idx, obstacle_index) = if crate::layout::routing::common::routing_skeleton::quick_check_need_obstacle_index(&result, &diagram.relations) {
        let (idx, obs) = crate::layout::routing::common::routing_skeleton::build_obstacle_context(&result);
        (idx, Some(obs))
    } else {
        (HashMap::new(), None)
    };

    let self_loop_idx = self_loop_indices(&diagram.relations);
    let mut edges = Vec::with_capacity(diagram.relations.len());

    for (i, rel) in diagram.relations.iter().enumerate() {
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        let mut edge = if from_id == to_id {
            match result.nodes.get(from_id) {
                Some(nl) => {
                    let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                    route_self_loop(rel, nl, loop_idx, SelfLoopStyle::Curved)
                }
                None => EdgeLayout::empty(),
            }
        } else {
            let (from_nl, to_nl) = match (
                result.nodes.get(from_id),
                result.nodes.get(to_id),
            ) {
                (Some(f), Some(t)) => (f, t),
                _ => {
                    edges.push(EdgeLayout::empty());
                    continue;
                }
            };

            if let (Some(from_pos), Some(to_pos)) = (
                node_placement.get(from_id),
                node_placement.get(to_id),
            ) {
                if from_pos.circle_idx == to_pos.circle_idx {
                    route_intra_circle_edge(
                        from_nl,
                        to_nl,
                        &circles[from_pos.circle_idx],
                        from_pos.pos_idx,
                        to_pos.pos_idx,
                        lane_offsets[i],
                        arc_sides[i],
                        rel,
                    )
                } else {
                    route_inter_circle_edge(from_nl, to_nl, lane_offsets[i], arc_sides[i], rel)
                }
            } else {
                route_inter_circle_edge(from_nl, to_nl, lane_offsets[i], arc_sides[i], rel)
            }
        };

        // ── 穿障检测：采样曲线，若穿过非端点节点则退化到 spline 绕行 ──
        let from_idx = node_id_to_idx.get(from_id).copied().unwrap_or(usize::MAX);
        let to_idx = node_id_to_idx.get(to_id).copied().unwrap_or(usize::MAX);
        let skip = [from_idx, to_idx];

        if let Some(ref obstacle_index) = obstacle_index {
            if crate::layout::routing::common::obstacle_check::curve_intersects_obstacles(&edge, obstacle_index, &skip) {
                if let (Some(start), Some(end)) = (edge.path_start(), edge.path_end()) {
                    let detour = obstacle_index.shortest_path(start, end, &skip);
                    if !detour.is_empty() {
                        edge.geometry = PathGeometry::Polyline { points: detour };
                        // 几何已换：按折线重建标签
                        let middle_t = crate::layout::routing::common::edge_geometry::parse_label_t(rel);
                        let sampled = edge.path_points().into_owned();
                        edge.labels = build_edge_labels(rel, middle_t, Point::new(0.0, -6.0), |t| {
                            crate::layout::routing::common::edge_geometry::point_at_path_t(&sampled, t)
                        });
                    } else {
                        // R5：空 detour 时走 outer 折线兜底，避免静默保留穿障 Bezier
                        let outer = outer_polyline_detour(start, end, &result.nodes, from_id, to_id);
                        let probe = EdgeLayout {
                            geometry: PathGeometry::Polyline {
                                points: outer.clone(),
                            },
                            labels: Vec::new(),
                            from_port: edge.from_port,
                            to_port: edge.to_port,
                        };
                        if !crate::layout::routing::common::obstacle_check::curve_intersects_obstacles(
                            &probe,
                            obstacle_index,
                            &skip,
                        ) || outer.len() >= 3
                        {
                            edge.geometry = PathGeometry::Polyline { points: outer };
                            let middle_t =
                                crate::layout::routing::common::edge_geometry::parse_label_t(rel);
                            let sampled = edge.path_points().into_owned();
                            edge.labels =
                                build_edge_labels(rel, middle_t, Point::new(0.0, -6.0), |t| {
                                    crate::layout::routing::common::edge_geometry::point_at_path_t(
                                        &sampled, t,
                                    )
                                });
                        }
                    }
                }
            }
        }

        edges.push(edge);
    }

    // 标签避让：径向推开策略（统一到 edge::common::label_placement::RadialPlacer）
    let label_ctx = LabelContext::new(
        &result.nodes,
        &result.groups,
    );
    RadialPlacer::default()
        .place(&mut edges, &label_ctx);
    result.edges = edges;
    result
}

pub(crate) struct NodeCirclePos {
    pub(crate) circle_idx: usize,
    pub(crate) pos_idx: usize,
}

pub(crate) fn build_node_placement(diagram: &Diagram, circles: &[CircleGroup]) -> HashMap<String, NodeCirclePos> {
    let mut map = HashMap::new();
    for (circle_idx, circle) in circles.iter().enumerate() {
        for (pos_idx, &entity_idx) in circle.entity_indices.iter().enumerate() {
            if let Some(entity) = diagram.entities.get(entity_idx) {
                map.insert(
                    entity.id.as_str().to_string(),
                    NodeCirclePos { circle_idx, pos_idx },
                );
            }
        }
    }
    map
}

/// 计算平行边 lane 偏移，以及对向边的弧侧符号（+1 / -1 = 弦两侧）。
///
/// 对 A↔B 正反边强制分到弦的两侧（一个上弧一个下弧），避免两条短弧贴在一起。
pub(crate) fn compute_lane_offsets(
    diagram: &Diagram,
    node_placement: &HashMap<String, NodeCirclePos>,
) -> (Vec<f64>, Vec<f64>) {
    let mut pair_groups: HashMap<String, Vec<usize>> = HashMap::new();
    let mut from_groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (i, rel) in diagram.relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
        from_groups
            .entry(rel.from.as_str().to_string())
            .or_default()
            .push(i);
    }

    let mut lane_offsets = vec![0.0; diagram.relations.len()];
    let mut arc_sides = vec![1.0; diagram.relations.len()];

    // 稳定迭代：按无向键排序，避免 HashMap 顺序影响正反弧分配。
    let mut pair_keys: Vec<String> = pair_groups.keys().cloned().collect();
    pair_keys.sort();
    for key in &pair_keys {
        let indices = &pair_groups[key];
        if indices.len() <= 1 {
            continue;
        }
        let mut sorted = indices.clone();
        sorted.sort();

        // 拆正反方向：forward / backward 各走弦的一侧。
        let rel0 = &diagram.relations[sorted[0]];
        let (can_from, can_to) = {
            let a = rel0.from.as_str();
            let b = rel0.to.as_str();
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        };
        let mut forward: Vec<usize> = Vec::new();
        let mut backward: Vec<usize> = Vec::new();
        for &i in &sorted {
            let rel = &diagram.relations[i];
            if rel.from.as_str() == can_from && rel.to.as_str() == can_to {
                forward.push(i);
            } else if rel.from.as_str() == can_to && rel.to.as_str() == can_from {
                backward.push(i);
            } else {
                forward.push(i);
            }
        }

        if !forward.is_empty() && !backward.is_empty() {
            let f_spread = (forward.len() as f64 - 1.0) / 2.0;
            for (lane, &i) in forward.iter().enumerate() {
                lane_offsets[i] = (lane as f64 - f_spread) * PARALLEL_SPACING;
                arc_sides[i] = 1.0;
            }
            let b_spread = (backward.len() as f64 - 1.0) / 2.0;
            for (lane, &i) in backward.iter().enumerate() {
                lane_offsets[i] = (lane as f64 - b_spread) * PARALLEL_SPACING;
                arc_sides[i] = -1.0;
            }
        } else {
            let spread = (sorted.len() as f64 - 1.0) / 2.0;
            for (lane, &i) in sorted.iter().enumerate() {
                lane_offsets[i] = (lane as f64 - spread) * PARALLEL_SPACING;
            }
        }
    }

    for key in {
        let mut keys: Vec<String> = from_groups.keys().cloned().collect();
        keys.sort();
        keys
    } {
        let indices = &from_groups[&key];
        if indices.len() <= 1 {
            continue;
        }
        let mut sorted: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| {
                let rel = &diagram.relations[i];
                rel.from.as_str() != rel.to.as_str()
            })
            .collect();
        if sorted.len() <= 1 {
            continue;
        }
        sorted.sort_by_key(|&i| {
            let to_id = diagram.relations[i].to.as_str();
            node_placement
                .get(to_id)
                .map(|p| p.pos_idx)
                .unwrap_or(0)
        });
        let spread = (sorted.len() as f64 - 1.0) / 2.0;
        for (lane, &i) in sorted.iter().enumerate() {
            let fan = (lane as f64 - spread) * PARALLEL_SPACING * 0.85;
            if lane_offsets[i].abs() < fan.abs() {
                lane_offsets[i] = fan;
            }
        }
    }

    (lane_offsets, arc_sides)
}

/// 同圆内 / 跨圆边的弧形贝塞尔几何 + 标签参数（不含 label 文本渲染）。
///
/// R3 Slice 3.4：把 legacy 逐边几何拆出，供 [`CircularRecipe`](crate::layout::routing::recipe::CircularRecipe)
/// 与 legacy [`route_edges_circular`] 共用，保证 solve 产出的 `Radial` 几何与标签计划逐字一致。
pub(crate) struct CircularBezier {
    pub start: Point,
    pub end: Point,
    pub controls: [Point; 2],
    pub from_port: Port,
    pub to_port: Port,
    pub label_t: f64,
    pub label_offset: Point,
}

/// 同圆内边：弦上鼓起的弧形贝塞尔（几何 + 标签参数）。
pub(crate) fn intra_circle_bezier(
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    circle: &CircleGroup,
    from_idx: usize,
    to_idx: usize,
    lane: f64,
    arc_side: f64,
) -> CircularBezier {
    let n = circle.entity_indices.len().max(1);
    let center_pt = Point::new(circle.center.0, circle.center.1);
    let radius = circle.radius;

    let from_center = node_center(from_nl);
    let to_center = node_center(to_nl);
    let (fcx, fcy) = (from_center.x, from_center.y);
    let (tcx, tcy) = (to_center.x, to_center.y);
    let (sx, sy) = edge_point(from_nl, tcx, tcy);
    let (ex, ey) = edge_point(to_nl, fcx, fcy);

    let forward = (to_idx + n - from_idx) % n;
    let backward = n - forward;
    let steps = forward.min(backward);

    let bulge_mag = bulge_for_steps(steps, n) + lane.abs();
    // arc_side: +1 / -1 决定弦的哪一侧鼓起（正反边一上一下）。
    let bulge_pt = bulge_point_on_chord(
        Point::new(sx, sy),
        Point::new(ex, ey),
        center_pt,
        radius,
        bulge_mag,
        arc_side.signum(),
    );

    let cp1 = Point::new(
        sx + (bulge_pt.x - sx) * 0.55,
        sy + (bulge_pt.y - sy) * 0.55,
    );
    let cp2 = Point::new(
        ex + (bulge_pt.x - ex) * 0.55,
        ey + (bulge_pt.y - ey) * 0.55,
    );

    let label_t = (0.42 + lane * 0.35).clamp(0.25, 0.75);
    let (off_x, off_y) = {
        let base = cubic_bezier_point(Point::new(sx, sy), cp1, cp2, Point::new(ex, ey), label_t);
        // 标签也跟着弧侧推开，避免正反边 label 叠在同一侧。
        let off = offset_label_by_side(base, Point::new(sx, sy), Point::new(ex, ey), lane, arc_side);
        (off.x - base.x, off.y - base.y)
    };

    CircularBezier {
        start: Point::new(sx, sy),
        end: Point::new(ex, ey),
        controls: [cp1, cp2],
        from_port: select_port(sx, sy, from_nl),
        to_port: select_port(ex, ey, to_nl),
        label_t,
        label_offset: Point::new(off_x, off_y),
    }
}

fn route_intra_circle_edge(
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    circle: &CircleGroup,
    from_idx: usize,
    to_idx: usize,
    lane: f64,
    arc_side: f64,
    rel: &crate::ast::Relation,
) -> EdgeLayout {
    let b = intra_circle_bezier(from_nl, to_nl, circle, from_idx, to_idx, lane, arc_side);
    let labels = build_edge_labels(rel, b.label_t, b.label_offset, |t| {
        cubic_bezier_point(b.start, b.controls[0], b.controls[1], b.end, t)
    });

    EdgeLayout {
        geometry: PathGeometry::Bezier {
            start: b.start,
            end: b.end,
            controls: b.controls,
        },
        labels,
        from_port: b.from_port,
        to_port: b.to_port,
    }
}

/// 跨圆边：端口方向贝塞尔 + 沿弦法向鼓起（几何 + 标签参数）。
pub(crate) fn inter_circle_bezier(
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    lane: f64,
    arc_side: f64,
) -> CircularBezier {
    let from_center = node_center(from_nl);
    let to_center = node_center(to_nl);
    let (fcx, fcy) = (from_center.x, from_center.y);
    let (tcx, tcy) = (to_center.x, to_center.y);
    let (sx, sy) = edge_point(from_nl, tcx, tcy);
    let (ex, ey) = edge_point(to_nl, fcx, fcy);
    let from_port = select_port(sx, sy, from_nl);
    let to_port = select_port(ex, ey, to_nl);
    let mut cp = compute_bezier_controls(
        sx, sy, ex, ey, from_port, to_port, DEFAULT_BEZIER_TENSION,
    );
    // R8：跨圆双向边沿弦法向按 arc_side 鼓起，与同圆正反弧分离一致。
    let side = if arc_side >= 0.0 { 1.0 } else { -1.0 };
    let dx = ex - sx;
    let dy = ey - sy;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let nx = -dy / len;
    let ny = dx / len;
    let lift = (12.0 + lane.abs() * 16.0) * side;
    cp[0] = Point::new(cp[0].x + nx * lift, cp[0].y + ny * lift);
    cp[1] = Point::new(cp[1].x + nx * lift, cp[1].y + ny * lift);

    CircularBezier {
        start: Point::new(sx, sy),
        end: Point::new(ex, ey),
        controls: cp,
        from_port,
        to_port,
        label_t: 0.5,
        label_offset: Point::new(nx * lift * 0.5, ny * lift * 0.5),
    }
}

fn route_inter_circle_edge(
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    lane: f64,
    arc_side: f64,
    rel: &crate::ast::Relation,
) -> EdgeLayout {
    let b = inter_circle_bezier(from_nl, to_nl, lane, arc_side);
    let labels = build_edge_labels(rel, b.label_t, b.label_offset, |t| {
        cubic_bezier_point(b.start, b.controls[0], b.controls[1], b.end, t)
    });

    EdgeLayout {
        geometry: PathGeometry::Bezier {
            start: b.start,
            end: b.end,
            controls: b.controls,
        },
        labels,
        from_port: b.from_port,
        to_port: b.to_port,
    }
}

/// R5：shortest_path 为空时，绕节点外框走 start→mid→end 折线。
pub(crate) fn outer_polyline_detour(
    start: Point,
    end: Point,
    nodes: &HashMap<String, NodeLayout>,
    from_id: &str,
    to_id: &str,
) -> Vec<Point> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut ids: Vec<&String> = nodes.keys().collect();
    ids.sort();
    for id in ids {
        if id.as_str() == from_id || id.as_str() == to_id {
            continue;
        }
        let nl = &nodes[id];
        min_x = min_x.min(nl.x);
        min_y = min_y.min(nl.y);
        max_x = max_x.max(nl.x + nl.width);
        max_y = max_y.max(nl.y + nl.height);
    }
    let pad = 28.0;
    if !min_x.is_finite() {
        // 无第三方障碍：简单水平-垂直折线
        return vec![start, Point::new(end.x, start.y), end];
    }
    let top = min_y - pad;
    let bottom = max_y + pad;
    let left = min_x - pad;
    let right = max_x + pad;
    // 选绕上/下/左/右中较短的一条
    let candidates = [
        vec![start, Point::new(start.x, top), Point::new(end.x, top), end],
        vec![
            start,
            Point::new(start.x, bottom),
            Point::new(end.x, bottom),
            end,
        ],
        vec![
            start,
            Point::new(left, start.y),
            Point::new(left, end.y),
            end,
        ],
        vec![
            start,
            Point::new(right, start.y),
            Point::new(right, end.y),
            end,
        ],
    ];
    candidates
        .into_iter()
        .min_by(|a, b| {
            let la: f64 = a.windows(2).map(|w| w[0].distance_to(w[1])).sum();
            let lb: f64 = b.windows(2).map(|w| w[0].distance_to(w[1])).sum();
            la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or_else(|| vec![start, Point::new(end.x, start.y), end])
}

fn bulge_for_steps(steps: usize, n: usize) -> f64 {
    if n <= 1 {
        return 1.05;
    }
    match steps {
        0 => 1.05,
        1 => 1.10,
        2 => 1.16,
        s if s <= n / 4 => 1.22,
        s if s <= n / 2 => 1.32,
        _ => 1.48,
    }
}

/// 在弦中点沿弦法向鼓起；`arc_side` 符号决定上下（或左右）哪一侧。
///
/// 法向选取：优先与「圆心 → 弦中点」同向的一侧作为 +1，这样默认弧仍朝圆外凸，
/// 对向边取 -1 则朝弦另一侧（常见效果：一上一下）。
fn bulge_point_on_chord(
    start: Point,
    end: Point,
    center: Point,
    radius: f64,
    bulge_mag: f64,
    arc_side: f64,
) -> Point {
    let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let chord_len = (dx * dx + dy * dy).sqrt().max(1.0);
    let mut nx = -dy / chord_len;
    let mut ny = dx / chord_len;

    // 让 +side 与圆心→弦中点方向一致（圆外凸）。
    let to_mid = mid.sub(center);
    if to_mid.x * nx + to_mid.y * ny < 0.0 {
        nx = -nx;
        ny = -ny;
    }

    let side = if arc_side >= 0.0 { 1.0 } else { -1.0 };
    // 鼓起幅度：相对半径，保证短弦也有明显弧度分离。
    let lift = (radius * (bulge_mag - 1.0).max(0.08) + chord_len * 0.18).max(18.0);
    Point::new(mid.x + nx * lift * side, mid.y + ny * lift * side)
}

fn offset_label_by_side(pos: Point, start: Point, end: Point, lane: f64, arc_side: f64) -> Point {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let mut nx = -dy / len;
    let mut ny = dx / len;
    let side = if arc_side >= 0.0 { 1.0 } else { -1.0 };
    // 保证法向与弧侧一致（与 bulge 同一半球）。
    let mid = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
    let to_pos = Point::new(pos.x - mid.x, pos.y - mid.y);
    if (to_pos.x * nx + to_pos.y * ny) * side < 0.0 {
        nx = -nx;
        ny = -ny;
    }
    let push = 12.0 + lane.abs() * 16.0;
    Point::new(pos.x + nx * push * side, pos.y + ny * push * side)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::types::DiagramType;
    use crate::layout::recipes::circular::CircularLayoutHints;
    use crate::layout::NodeLayout;
    use std::collections::HashMap;

    fn sample_state_diagram() -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::State,
            attributes: Vec::new(),
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: Some("go".into()),
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: Vec::new(),
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    /// 构建三节点（a/b/c）状态图，关系为 a→c（b 作为障碍节点）
    fn sample_state_diagram_with_obstacle() -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::State,
            attributes: Vec::new(),
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("c"),
                    label: "C".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("c"),
                arrow: ArrowType::Active,
                label: Some("go".into()),
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: Vec::new(),
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn circular_edge_routing_produces_bezier_edges() {
        let diagram = sample_state_diagram();
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 100.0,
                y: 100.0,
                width: 80.0,
                height: 44.0,
                ..Default::default()
            },
        );
        nodes.insert(
            "b".to_string(),
            NodeLayout {
                x: 200.0,
                y: 100.0,
                width: 80.0,
                height: 44.0,
                ..Default::default()
            },
        );
        let result = LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: vec![],
            total_width: 400.0,
            total_height: 300.0,
            hints: CircularLayoutHints {
                circles: vec![CircleGroup {
                    center: (150.0, 150.0),
                    radius: 100.0,
                    entity_indices: vec![0, 1],
                }],
            }
            .into(),
        };
        let routed = route_edges_circular(&diagram, result);
        assert_eq!(routed.edges.len(), 1);
        assert!(routed.edges[0].is_bezier());
        assert!(routed.edges[0].bezier_controls().is_some());
    }

    #[test]
    fn circular_detours_around_obstacle() {
        // 两个圆：圆1含 a、b，圆2含 c。a→c 的 inter-circle bezier 会穿过 b
        let diagram = sample_state_diagram_with_obstacle();
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 50.0,
                y: 100.0,
                width: 80.0,
                height: 44.0,
                ..Default::default()
            },
        );
        nodes.insert(
            "b".to_string(),
            NodeLayout {
                x: 200.0,
                y: 100.0,
                width: 80.0,
                height: 44.0,
                ..Default::default()
            },
        );
        nodes.insert(
            "c".to_string(),
            NodeLayout {
                x: 350.0,
                y: 100.0,
                width: 80.0,
                height: 44.0,
                ..Default::default()
            },
        );
        let result = LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: vec![],
            total_width: 500.0,
            total_height: 300.0,
            hints: CircularLayoutHints {
                circles: vec![
                    CircleGroup {
                        center: (170.0, 122.0),
                        radius: 130.0,
                        entity_indices: vec![0, 1],
                    },
                    CircleGroup {
                        center: (390.0, 122.0),
                        radius: 50.0,
                        entity_indices: vec![2],
                    },
                ],
            }
            .into(),
        };
        let routed = route_edges_circular(&diagram, result);
        assert_eq!(routed.edges.len(), 1);
        // 穿障后应退化为 Polyline（绕行折线）
        assert!(
            routed.edges[0].is_polyline(),
            "circular edge through obstacle should degrade to polyline, got {:?}",
            routed.edges[0].geometry
        );
    }

    #[test]
    fn circular_keeps_bezier_when_no_obstacle() {
        // 两个节点无中间障碍，应保持 bezier
        let diagram = sample_state_diagram();
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 100.0,
                y: 100.0,
                width: 80.0,
                height: 44.0,
                ..Default::default()
            },
        );
        nodes.insert(
            "b".to_string(),
            NodeLayout {
                x: 300.0,
                y: 100.0,
                width: 80.0,
                height: 44.0,
                ..Default::default()
            },
        );
        let result = LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: vec![],
            total_width: 500.0,
            total_height: 300.0,
            hints: CircularLayoutHints {
                circles: vec![CircleGroup {
                    center: (240.0, 122.0),
                    radius: 140.0,
                    entity_indices: vec![0, 1],
                }],
            }
            .into(),
        };
        let routed = route_edges_circular(&diagram, result);
        assert_eq!(routed.edges.len(), 1);
        assert!(
            routed.edges[0].is_bezier(),
            "circular edge without obstacle should stay bezier, got {:?}",
            routed.edges[0].geometry
        );
    }
}
