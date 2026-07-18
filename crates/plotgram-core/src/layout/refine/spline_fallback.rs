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
/// 全量共线/严重度门禁较贵；仅在问题边较少时启用。穿节点/穿组硬过滤不受此限。
const MAX_LOCAL_ORTHOGONAL_QUALITY_EDGES: usize = 10;

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

        // 正交原边、以及 architecture（默认正交路由）一律走 dogleg/外廊，禁止密采样
        // spline：后者会被末尾 force_orthogonal 栅格化成假台阶，并在问题边较多时
        // 绕过穿组/共线质量门禁（federation 类密集图）。
        let original_is_orthogonal = routing_snapshot
            .edges
            .get(i)
            .is_some_and(|edge| is_orthogonal(&edge.path_points()));
        let use_orthogonal_fallback = original_is_orthogonal
            || matches!(diagram.diagram_type, crate::types::DiagramType::Architecture);
        let (geometry, sampled_for_label) = if use_orthogonal_fallback {
            let Some(points) = orthogonal_detour(
                ep.start,
                ep.end,
                ep.from_port,
                ep.to_port,
                diagram,
                i,
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
            if use_orthogonal_fallback {
                // 硬门禁：只接受不穿节点、不穿组的候选。外廊可能与其它边共享边框通道，
                // 这属于可接受的边框总线，不能用 unrelated collinear 门禁把干净外廊误杀回穿模。
                let mut hard_probe = accepted_snapshot.clone();
                if i < hard_probe.edges.len() {
                    hard_probe.edges[i] = candidate.clone();
                }
                let after_through =
                    count_single_edge_crossings(&candidate, &hard_probe, diagram, i, config);
                if after_through > 0 {
                    continue;
                }
                if crate::layout::lint::edge_index_crosses_group_interior(diagram, &hard_probe, i) {
                    continue;
                }

                if edge_indices.len() <= MAX_LOCAL_ORTHOGONAL_QUALITY_EDGES {
                    let before_quality = crate::layout::metrics::compute_collinear_sample_metrics(
                        "",
                        diagram,
                        &accepted_snapshot,
                    );
                    let after_quality = crate::layout::metrics::compute_collinear_sample_metrics(
                        "",
                        diagram,
                        &hard_probe,
                    );
                    // 干净绕障优先：允许 exact/tight 因外廊变长而上升，但不得新增
                    // unrelated_trunk / 穿节点 / 穿组（后两者上面已硬拦）。
                    if after_quality.lint.unrelated_edge_trunk_merge
                        > before_quality.lint.unrelated_edge_trunk_merge
                        || after_quality.lint.edge_through_node
                            > before_quality.lint.edge_through_node
                        || after_quality.lint.edge_crosses_group_interior
                            > before_quality.lint.edge_crosses_group_interior
                    {
                        continue;
                    }
                }
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
    diagram: &Diagram,
    edge_index: usize,
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

    if !result.nodes.is_empty() || !result.groups.is_empty() {
        let lane_offset = fallback_lane as f64 * ORTHOGONAL_STUB;
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        let mut top = f64::INFINITY;
        let mut bottom = f64::NEG_INFINITY;
        for n in result.nodes.values() {
            left = left.min(n.x);
            right = right.max(n.x + n.width);
            top = top.min(n.y);
            bottom = bottom.max(n.y + n.height);
        }
        for g in result.groups.values() {
            left = left.min(g.x);
            right = right.max(g.x + g.width);
            top = top.min(g.y);
            bottom = bottom.max(g.y + g.height);
        }
        // 先沿端口逃逸一档再折向外廊，减少同排横穿。
        let from_escape = Point::new(
            start.x + fox * (ORTHOGONAL_STUB * 3.0),
            start.y + foy * (ORTHOGONAL_STUB * 3.0),
        );
        let to_approach = Point::new(
            end.x + tox * (ORTHOGONAL_STUB * 3.0),
            end.y + toy * (ORTHOGONAL_STUB * 3.0),
        );
        for margin_extra in [0.0, ORTHOGONAL_STUB, ORTHOGONAL_STUB * 2.0] {
            let margin = ORTHOGONAL_OUTER_MARGIN + lane_offset + margin_extra;
            let xs = [left - margin, right + margin];
            let ys = [top - margin, bottom + margin];
            for (exit_pt, entry_pt) in [(from_stub, to_stub), (from_escape, to_approach)] {
                for x in xs {
                    candidates.push(vec![
                        start,
                        exit_pt,
                        Point::new(x, exit_pt.y),
                        Point::new(x, entry_pt.y),
                        entry_pt,
                        end,
                    ]);
                }
                for y in ys {
                    candidates.push(vec![
                        start,
                        exit_pt,
                        Point::new(exit_pt.x, y),
                        Point::new(entry_pt.x, y),
                        entry_pt,
                        end,
                    ]);
                }
            }
        }

        // 局部两跳/裙边：只绕「原路径已穿」的无关组 bbox，不做全局建廊剪枝。
        if let Some(rel) = diagram.relations.get(edge_index) {
            let pierced = foreign_groups_pierced_by_edge(diagram, result, edge_index, rel.from.as_str(), rel.to.as_str());
            for gl in pierced {
                for margin_extra in [ORTHOGONAL_STUB, ORTHOGONAL_STUB * 2.0, ORTHOGONAL_OUTER_MARGIN] {
                    let m = margin_extra + lane_offset;
                    let gx0 = gl.x - m;
                    let gx1 = gl.x + gl.width + m;
                    let gy0 = gl.y - m;
                    let gy1 = gl.y + gl.height + m;
                    for (exit_pt, entry_pt) in [(from_stub, to_stub), (from_escape, to_approach)] {
                        for x in [gx0, gx1] {
                            candidates.push(vec![
                                start,
                                exit_pt,
                                Point::new(x, exit_pt.y),
                                Point::new(x, entry_pt.y),
                                entry_pt,
                                end,
                            ]);
                        }
                        for y in [gy0, gy1] {
                            candidates.push(vec![
                                start,
                                exit_pt,
                                Point::new(exit_pt.x, y),
                                Point::new(entry_pt.x, y),
                                entry_pt,
                                end,
                            ]);
                        }
                        // U 形两折绕组（上→侧→下 / 左→侧→右），覆盖单侧裙边不够的跨组。
                        candidates.push(vec![
                            start,
                            exit_pt,
                            Point::new(exit_pt.x, gy0),
                            Point::new(entry_pt.x, gy0),
                            entry_pt,
                            end,
                        ]);
                        candidates.push(vec![
                            start,
                            exit_pt,
                            Point::new(exit_pt.x, gy1),
                            Point::new(entry_pt.x, gy1),
                            entry_pt,
                            end,
                        ]);
                        candidates.push(vec![
                            start,
                            exit_pt,
                            Point::new(gx0, exit_pt.y),
                            Point::new(gx0, entry_pt.y),
                            entry_pt,
                            end,
                        ]);
                        candidates.push(vec![
                            start,
                            exit_pt,
                            Point::new(gx1, exit_pt.y),
                            Point::new(gx1, entry_pt.y),
                            entry_pt,
                            end,
                        ]);
                    }
                }
            }
        }
    }

    let group_maps = crate::layout::lint::GroupInteriorMaps::new(diagram);
    let endpoint_ids = diagram.relations.get(edge_index).map(|rel| {
        (
            rel.from.as_str().to_string(),
            rel.to.as_str().to_string(),
        )
    });
    let mut clean: Vec<Vec<Point>> = candidates
        .into_iter()
        .map(simplify_orthogonal)
        .filter(|path| {
            path.len() >= 2
                && is_orthogonal(path)
                && path
                    .windows(2)
                    .all(|w| !obstacles.segment_hits_any(w[0], w[1], skip))
                && endpoint_ids.as_ref().is_some_and(|(from_id, to_id)| {
                    !path_pierces_foreign_nodes(path, result, from_id, to_id)
                })
                && !path_crosses_foreign_group_interior(
                    path,
                    diagram,
                    result,
                    edge_index,
                    &group_maps,
                )
        })
        .collect();
    clean.sort_by(|a, b| {
        a.len()
            .cmp(&b.len())
            .then_with(|| {
                path_length(a)
                    .partial_cmp(&path_length(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| path_sort_key(a).cmp(&path_sort_key(b)))
    });
    clean.into_iter().next()
}

fn path_sort_key(points: &[Point]) -> Vec<(i64, i64)> {
    points
        .iter()
        .map(|p| ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64))
        .collect()
}

fn path_pierces_foreign_nodes(
    path: &[Point],
    result: &LayoutResult,
    from_id: &str,
    to_id: &str,
) -> bool {
    // 与 lint 对齐：用完整节点框（无 shrink）；长路径仍检查中间段，短路径检查全部。
    let segment_count = path.len().saturating_sub(1);
    let skip_endpoints = segment_count > 2;
    for (seg_i, window) in path.windows(2).enumerate() {
        if skip_endpoints && (seg_i == 0 || seg_i == segment_count - 1) {
            continue;
        }
        for (node_id, nl) in &result.nodes {
            if node_id == from_id || node_id == to_id {
                continue;
            }
            if super::segment_intersects_node(window[0], window[1], nl) {
                return true;
            }
        }
    }
    false
}

fn path_crosses_foreign_group_interior(
    path: &[Point],
    diagram: &Diagram,
    result: &LayoutResult,
    edge_index: usize,
    maps: &crate::layout::lint::GroupInteriorMaps,
) -> bool {
    let mut probe = result.clone();
    if edge_index < probe.edges.len() {
        probe.edges[edge_index] = EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: path.to_vec(),
            },
            labels: Vec::new(),
            from_port: Port::Right,
            to_port: Port::Left,
        };
    }
    crate::layout::lint::edge_crosses_group_interior_with_maps(diagram, &probe, edge_index, maps)
}

/// 原路径穿入的无关组（按 id 排序，确定性）。用于局部裙边/两跳候选，禁止全图建廊。
fn foreign_groups_pierced_by_edge<'a>(
    _diagram: &Diagram,
    result: &'a LayoutResult,
    edge_index: usize,
    from_id: &str,
    to_id: &str,
) -> Vec<&'a crate::layout::GroupLayout> {
    let Some(edge) = result.edges.get(edge_index) else {
        return Vec::new();
    };
    let path = edge.path_points();
    if path.len() < 2 || result.groups.is_empty() {
        return Vec::new();
    }
    let mut gids: Vec<&String> = result.groups.keys().collect();
    gids.sort();
    let mut out = Vec::new();
    for gid in gids {
        let gl = &result.groups[gid];
        if gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        // 端点所在组不做裙边目标（出组/入组腿会合法经过）。
        let from_in = result
            .nodes
            .get(from_id)
            .is_some_and(|nl| point_in_group_loose(nl, gl));
        let to_in = result
            .nodes
            .get(to_id)
            .is_some_and(|nl| point_in_group_loose(nl, gl));
        if from_in || to_in {
            continue;
        }
        let pierces = path.windows(2).any(|w| {
            crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(
                w[0], w[1], gl,
            )
        });
        if pierces {
            out.push(gl);
        }
    }
    out
}

fn point_in_group_loose(nl: &crate::layout::NodeLayout, gl: &crate::layout::GroupLayout) -> bool {
    let cx = nl.x + nl.width * 0.5;
    let cy = nl.y + nl.height * 0.5;
    cx >= gl.x && cx <= gl.x + gl.width && cy >= gl.y && cy <= gl.y + gl.height
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
