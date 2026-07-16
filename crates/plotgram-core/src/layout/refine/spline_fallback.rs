//! refine 最后一轮：对仍穿障的边降级为 spline 可见性图绕障。

use crate::ast::Diagram;
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, compute_bezier_controls, label_t_for_diagram, point_at_path_t,
};
use crate::layout::edge::common::obstacle_check::curve_intersects_obstacles;
use crate::layout::edge::common::routing_skeleton::{resolve_endpoints, RoutingContext};
use crate::layout::edge::common::self_loop::{route_self_loop, self_loop_indices, SelfLoopStyle};
use crate::layout::edge::edge_routing_bezier::BezierConfig;
use crate::layout::edge::edge_routing_spline::{
    build_full_path, fit_multi_segment_spline, sample_bezier,
};
use crate::layout::edge::visibility;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, LayoutResult, PathGeometry, Port};
use std::collections::HashSet;

use super::crossing::analyze_edge_node_crossings;
use super::RefineConfig;

const SPLINE_SAMPLES_PER_SEGMENT: usize = 12;
const ORTHOGONAL_STUB: f64 = 16.0;
const ORTHOGONAL_OUTER_MARGIN: f64 = 32.0;
const MAX_LOCAL_ORTHOGONAL_FALLBACK_EDGES: usize = 10;

/// 对指定边索引用 spline 可见性图重路由（混合路由兜底）。
///
/// C9：仅当替换后 crossing 不劣于原边时才采纳；空 detour 退 Bezier 后须复检穿障。
pub(crate) fn reroute_edges_with_spline(
    result: &mut LayoutResult,
    diagram: &Diagram,
    edge_indices: &HashSet<usize>,
    config: &RefineConfig,
) {
    if edge_indices.is_empty() {
        return;
    }

    let relations = &diagram.relations;
    let routing_snapshot = LayoutResult {
        nodes: result.nodes.clone(),
        groups: result.groups.clone(),
        edges: result.edges.clone(),
        total_width: result.total_width,
        total_height: result.total_height,
        hints: result.hints.clone(),
    };
    let ctx = RoutingContext::new(diagram, &routing_snapshot);
    let mut accepted_snapshot = routing_snapshot.clone();
    let self_loop_idx = self_loop_indices(relations);
    let tension = BezierConfig::default().tension;

    let mut sorted_node_ids: Vec<String> = routing_snapshot.nodes.keys().cloned().collect();
    sorted_node_ids.sort();
    let node_list: Vec<(usize, &crate::layout::NodeLayout)> = sorted_node_ids
        .iter()
        .filter_map(|id| routing_snapshot.nodes.get(id).map(|nl| (id.as_str(), nl)))
        .enumerate()
        .map(|(i, (_, nl))| (i, nl))
        .collect();
    let node_id_to_idx: std::collections::HashMap<&str, usize> = sorted_node_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let obstacle_index = visibility::ObstacleIndex::build(&node_list);

    let mut spline_count = 0usize;
    let mut updates: Vec<(usize, EdgeLayout)> = Vec::new();
    let mut ordered_edges: Vec<usize> = edge_indices.iter().copied().collect();
    ordered_edges.sort_unstable();
    for (fallback_lane, &i) in ordered_edges.iter().enumerate() {
        let Some(rel) = relations.get(i) else {
            continue;
        };
        if rel.from.as_str() == rel.to.as_str() {
            if let Some(nl) = routing_snapshot.nodes.get(rel.from.as_str()) {
                let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                updates.push((i, route_self_loop(rel, nl, loop_idx, SelfLoopStyle::Curved)));
            }
            continue;
        }

        let Some((ep, label_off)) = resolve_endpoints(&ctx, rel, i) else {
            continue;
        };

        let from_idx = node_id_to_idx
            .get(ep.from_id.as_str())
            .copied()
            .unwrap_or(usize::MAX);
        let to_idx = node_id_to_idx
            .get(ep.to_id.as_str())
            .copied()
            .unwrap_or(usize::MAX);
        let skip = [from_idx, to_idx];

        let detour_path = obstacle_index.shortest_path(ep.start, ep.end, &skip);

        let original_is_orthogonal = edge_indices.len() <= MAX_LOCAL_ORTHOGONAL_FALLBACK_EDGES
            && routing_snapshot
                .edges
                .get(i)
                .is_some_and(|edge| is_orthogonal(&edge.path_points()));
        let (geometry, sampled_for_label) = if original_is_orthogonal {
            let Some(points) = orthogonal_detour(
                ep.start,
                ep.end,
                ep.from_port,
                ep.to_port,
                &routing_snapshot,
                &obstacle_index,
                &skip,
                fallback_lane,
            ) else {
                continue;
            };
            (
                PathGeometry::Polyline {
                    points: points.clone(),
                },
                points,
            )
        } else if detour_path.is_empty() {
            let cp = compute_bezier_controls(
                ep.start.x,
                ep.start.y,
                ep.end.x,
                ep.end.y,
                ep.from_port,
                ep.to_port,
                tension,
            );
            let sampled = sample_bezier(ep.start, cp[0], cp[1], ep.end, SPLINE_SAMPLES_PER_SEGMENT);
            let candidate = EdgeLayout {
                geometry: PathGeometry::Bezier {
                    start: ep.start,
                    end: ep.end,
                    controls: cp,
                },
                labels: Vec::new(),
                from_port: ep.from_port,
                to_port: ep.to_port,
            };
            // C9：空 detour 退 Bezier 后必须复检；仍穿障则保留原边。
            if curve_intersects_obstacles(&candidate, &obstacle_index, &skip) {
                continue;
            }
            (candidate.geometry, sampled)
        } else {
            let full_path = build_full_path(ep.start, &detour_path, ep.end);
            let sampled = fit_multi_segment_spline(&full_path, SPLINE_SAMPLES_PER_SEGMENT);
            (
                PathGeometry::Polyline {
                    points: sampled.clone(),
                },
                sampled,
            )
        };

        let middle_t = label_t_for_diagram(diagram, rel);
        let labels =
            build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
                point_at_path_t(&sampled_for_label, t)
            });

        let candidate = EdgeLayout {
            geometry,
            labels,
            from_port: ep.from_port,
            to_port: ep.to_port,
        };

        // C9：仅 when after ≤ before 才替换（crossing 不增）。
        if let Some(original) = routing_snapshot.edges.get(i) {
            let before =
                count_single_edge_crossings(original, &routing_snapshot, diagram, i, config);
            let after =
                count_single_edge_crossings(&candidate, &routing_snapshot, diagram, i, config);
            if after > before {
                continue;
            }
            if original_is_orthogonal {
                let before_overlap =
                    unrelated_collinear_overlap(original, &accepted_snapshot, diagram, i);
                let after_overlap =
                    unrelated_collinear_overlap(&candidate, &accepted_snapshot, diagram, i);
                if after_overlap > before_overlap + 1.0 {
                    continue;
                }
            }
        }

        if original_is_orthogonal {
            let before_quality = crate::layout::metrics::compute_collinear_sample_metrics(
                "",
                diagram,
                &accepted_snapshot,
            );
            let mut quality_probe = accepted_snapshot.clone();
            if i < quality_probe.edges.len() {
                quality_probe.edges[i] = candidate.clone();
            }
            let after_quality = crate::layout::metrics::compute_collinear_sample_metrics(
                "",
                diagram,
                &quality_probe,
            );
            if after_quality.exact_sev > before_quality.exact_sev + 1.0
                || after_quality.tight_sev > before_quality.tight_sev + 1.0
                || after_quality.lint.unrelated_edge_trunk_merge
                    > before_quality.lint.unrelated_edge_trunk_merge
                || after_quality.lint.edge_through_node > before_quality.lint.edge_through_node
                || after_quality.lint.edge_crosses_group_interior
                    > before_quality.lint.edge_crosses_group_interior
            {
                continue;
            }
        }

        if i < accepted_snapshot.edges.len() {
            accepted_snapshot.edges[i] = candidate.clone();
        }
        updates.push((i, candidate));
        spline_count += 1;
    }

    for (i, edge) in updates {
        if i < result.edges.len() {
            result.edges[i] = edge;
        }
    }

    if spline_count > 0 {
        if let Some(stats) = result.hints.refine_debug.as_mut() {
            stats.spline_fallback_count = spline_count;
        }
    }
    crate::perf_log!(
        "[perf]         fallback candidates={} accepted={}",
        edge_indices.len(),
        spline_count
    );
}

fn is_orthogonal(points: &[Point]) -> bool {
    points
        .windows(2)
        .all(|w| (w[0].x - w[1].x).abs() < 0.1 || (w[0].y - w[1].y).abs() < 0.1)
}

/// 正交图的 refine 降级：只生成单次 dogleg / 外廊候选。
///
/// 可见性样条的密采样点不能交给末尾 `force_orthogonal`，否则斜线会被栅格化成
/// 多级台阶。这里使用同一障碍索引做硬过滤，并按折点数、长度、坐标稳定选优。
fn orthogonal_detour(
    start: Point,
    end: Point,
    from_port: Port,
    to_port: Port,
    result: &LayoutResult,
    obstacles: &visibility::ObstacleIndex,
    skip: &[usize],
    fallback_lane: usize,
) -> Option<Vec<Point>> {
    let (fox, foy) = port_outward(from_port);
    let (tox, toy) = port_outward(to_port);
    let from_stub = Point::new(
        start.x + fox * ORTHOGONAL_STUB,
        start.y + foy * ORTHOGONAL_STUB,
    );
    let to_stub = Point::new(end.x + tox * ORTHOGONAL_STUB, end.y + toy * ORTHOGONAL_STUB);

    let mut candidates = vec![
        vec![
            start,
            from_stub,
            Point::new(from_stub.x, to_stub.y),
            to_stub,
            end,
        ],
        vec![
            start,
            from_stub,
            Point::new(to_stub.x, from_stub.y),
            to_stub,
            end,
        ],
    ];

    if !result.nodes.is_empty() {
        let lane_offset = fallback_lane as f64 * ORTHOGONAL_STUB;
        let left = result
            .nodes
            .values()
            .map(|n| n.x)
            .fold(f64::INFINITY, f64::min)
            - ORTHOGONAL_OUTER_MARGIN
            - lane_offset;
        let right = result
            .nodes
            .values()
            .map(|n| n.x + n.width)
            .fold(f64::NEG_INFINITY, f64::max)
            + ORTHOGONAL_OUTER_MARGIN
            + lane_offset;
        let top = result
            .nodes
            .values()
            .map(|n| n.y)
            .fold(f64::INFINITY, f64::min)
            - ORTHOGONAL_OUTER_MARGIN
            - lane_offset;
        let bottom = result
            .nodes
            .values()
            .map(|n| n.y + n.height)
            .fold(f64::NEG_INFINITY, f64::max)
            + ORTHOGONAL_OUTER_MARGIN
            + lane_offset;

        for x in [left, right] {
            candidates.push(vec![
                start,
                from_stub,
                Point::new(x, from_stub.y),
                Point::new(x, to_stub.y),
                to_stub,
                end,
            ]);
        }
        for y in [top, bottom] {
            candidates.push(vec![
                start,
                from_stub,
                Point::new(from_stub.x, y),
                Point::new(to_stub.x, y),
                to_stub,
                end,
            ]);
        }
    }

    let mut clean: Vec<Vec<Point>> = candidates
        .into_iter()
        .map(simplify_orthogonal)
        .filter(|path| {
            path.len() >= 2
                && is_orthogonal(path)
                && path
                    .windows(2)
                    .all(|w| !obstacles.segment_hits_any(w[0], w[1], skip))
        })
        .collect();
    clean.sort_by(|a, b| {
        a.len().cmp(&b.len()).then_with(|| {
            path_length(a)
                .partial_cmp(&path_length(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    clean.into_iter().next()
}

fn port_outward(port: Port) -> (f64, f64) {
    match port {
        Port::Top => (0.0, -1.0),
        Port::Bottom => (0.0, 1.0),
        Port::Left => (-1.0, 0.0),
        Port::Right => (1.0, 0.0),
    }
}

fn simplify_orthogonal(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        if out
            .last()
            .is_some_and(|last| (last.x - point.x).abs() < 0.1 && (last.y - point.y).abs() < 0.1)
        {
            continue;
        }
        while out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let same_x = (a.x - b.x).abs() < 0.1 && (b.x - point.x).abs() < 0.1;
            let same_y = (a.y - b.y).abs() < 0.1 && (b.y - point.y).abs() < 0.1;
            if same_x || same_y {
                out.pop();
            } else {
                break;
            }
        }
        out.push(point);
    }
    out
}

fn path_length(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|w| (w[1].x - w[0].x).abs() + (w[1].y - w[0].y).abs())
        .sum()
}

/// 候选不能用新的长共线重叠换取穿障下降；否则多个 outer fallback 会叠成假 trunk。
fn unrelated_collinear_overlap(
    edge: &EdgeLayout,
    result: &LayoutResult,
    diagram: &Diagram,
    edge_index: usize,
) -> f64 {
    let Some(relation) = diagram.relations.get(edge_index) else {
        return 0.0;
    };
    let points = edge.path_points();
    let mut total = 0.0;
    for (other_index, other) in result.edges.iter().enumerate() {
        if other_index == edge_index {
            continue;
        }
        let Some(other_relation) = diagram.relations.get(other_index) else {
            continue;
        };
        let related = relation.from == other_relation.from
            || relation.from == other_relation.to
            || relation.to == other_relation.from
            || relation.to == other_relation.to;
        if related {
            continue;
        }
        let other_points = other.path_points();
        for segment in points.windows(2) {
            for other_segment in other_points.windows(2) {
                total += axis_aligned_overlap(
                    segment[0],
                    segment[1],
                    other_segment[0],
                    other_segment[1],
                );
            }
        }
    }
    total
}

fn axis_aligned_overlap(a: Point, b: Point, c: Point, d: Point) -> f64 {
    const TOL: f64 = 0.5;
    let ab_horizontal = (a.y - b.y).abs() <= TOL;
    let cd_horizontal = (c.y - d.y).abs() <= TOL;
    if ab_horizontal && cd_horizontal && (a.y - c.y).abs() <= TOL {
        return (a.x.max(b.x).min(c.x.max(d.x)) - a.x.min(b.x).max(c.x.min(d.x))).max(0.0);
    }
    let ab_vertical = (a.x - b.x).abs() <= TOL;
    let cd_vertical = (c.x - d.x).abs() <= TOL;
    if ab_vertical && cd_vertical && (a.x - c.x).abs() <= TOL {
        return (a.y.max(b.y).min(c.y.max(d.y)) - a.y.min(b.y).max(c.y.min(d.y))).max(0.0);
    }
    0.0
}

/// 统计单条边相对当前布局的穿障次数（用于 C9 质量门控）。
fn count_single_edge_crossings(
    edge: &EdgeLayout,
    result: &LayoutResult,
    diagram: &Diagram,
    edge_idx: usize,
    config: &RefineConfig,
) -> usize {
    let mut probe = result.clone();
    if edge_idx < probe.edges.len() {
        probe.edges[edge_idx] = edge.clone();
    }
    let metrics = analyze_edge_node_crossings(&probe, diagram, config);
    // 只计本边相关的穿障：problem_nodes 中含本 edge_idx 的 crossing 累加不精确，
    // 直接用全量 metrics 中属于本边的 edge_indices 计数。
    metrics
        .problem_nodes
        .values()
        .flat_map(|info| info.edge_indices.iter())
        .filter(|&&ei| ei == edge_idx)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::layout::{LayoutResult, NodeLayout, PathGeometry};
    use crate::types::DiagramType;

    #[test]
    fn spline_fallback_reroutes_selected_edge() {
        let span = Span::dummy();
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "a".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "b".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
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
        };

        let mut result = LayoutResult {
            nodes: [
                (
                    "a".to_string(),
                    NodeLayout {
                        x: 0.0,
                        y: 0.0,
                        width: 80.0,
                        height: 40.0,
                        ..Default::default()
                    },
                ),
                (
                    "b".to_string(),
                    NodeLayout {
                        x: 200.0,
                        y: 0.0,
                        width: 80.0,
                        height: 40.0,
                        ..Default::default()
                    },
                ),
            ]
            .into_iter()
            .collect(),
            groups: Default::default(),
            edges: vec![EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(40.0, 20.0), Point::new(240.0, 20.0)],
                },
                labels: Vec::new(),
                from_port: crate::layout::Port::Right,
                to_port: crate::layout::Port::Left,
            }],
            total_width: 300.0,
            total_height: 100.0,
            hints: Default::default(),
        };

        let mut set = HashSet::new();
        set.insert(0);
        reroute_edges_with_spline(&mut result, &diagram, &set, &RefineConfig::default());
        assert!(result.edges[0].path_len() >= 2);
    }
}
