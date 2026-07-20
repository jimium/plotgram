//! refine 最后一轮：对仍穿障的边降级为正交 dogleg 绕障 + 显式 degraded 标注。
//!
//! Tier D（2026-07-20）：原 spline/bezier 密采样降级路径已删除（手册 §3.5 ★ 红线）。
//! 所有 fallback 候选统一走 `orthogonal_detour_try_ports`（与 lint 对齐硬门禁）；
//! 非正交原图若 dogleg 失败，保留原边并写入 `degraded` 归因。

use crate::ast::Diagram;
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, label_t_for_diagram, point_at_path_t,
};
use crate::layout::edge::common::routing_skeleton::{resolve_endpoints, RoutingContext};
use crate::layout::edge::common::self_loop::{route_self_loop, self_loop_indices, SelfLoopStyle};
use crate::layout::edge::visibility;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, LayoutResult, PathGeometry, Port};
use std::collections::HashSet;

use super::crossing::analyze_edge_node_crossings;
use super::RefineConfig;

const ORTHOGONAL_STUB: f64 = 16.0;
const ORTHOGONAL_OUTER_MARGIN: f64 = 32.0;
/// 全量共线/严重度门禁较贵；仅在问题边较少时启用。穿节点/穿组硬过滤不受此限。
const MAX_LOCAL_ORTHOGONAL_QUALITY_EDGES: usize = 10;

/// degraded 标注原因：原 spline/bezier 密采样降级已删除，dogleg 替换未生成候选。
const DEGRADED_REASON: &str = "spline_fallback_removed:bezier_or_multi_segment_spline";

/// 对指定边索引用 dogleg 重路由（混合路由兜底）。
///
/// C9：仅当替换后 crossing 不劣于原边时才采纳。
///
/// `aggressive_group_skirt`：并集裙边 + 换侧端口。仅用于节点冻结后的穿组试修，
/// 避免在 refine（冻结前）改边连坐 space-budget / group_frame 导致 node_fp 漂移。
pub(crate) fn reroute_edges_with_spline(
    result: &mut LayoutResult,
    diagram: &Diagram,
    edge_indices: &HashSet<usize>,
    config: &RefineConfig,
) {
    reroute_edges_with_spline_ex(result, diagram, edge_indices, config, false);
}

pub(crate) fn reroute_edges_with_spline_ex(
    result: &mut LayoutResult,
    diagram: &Diagram,
    edge_indices: &HashSet<usize>,
    config: &RefineConfig,
    aggressive_group_skirt: bool,
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

        // 正交原边、以及 architecture（默认正交路由）一律走 dogleg/外廊。
        // 非正交原图（state/er/mindmap 等）原本走 spline/bezier 密采样降级（平滑曲线）；
        // Tier D 删除密采样路径后改走 dogleg 会把平滑曲线替换为正交折线，对 ER/State 等
        // 紧密成对样例引入大量共线严重度（tight_sev 退化）。保守策略：非正交原图
        // **保留原边 + 显式 degraded 标注**，由后续 `recheck_lint_pierce_post_freeze`
        // 复用同一 degraded 字段做诊断。dogleg 仅用于正交原图（含 Architecture）。
        let original_is_orthogonal = routing_snapshot
            .edges
            .get(i)
            .is_some_and(|edge| is_orthogonal(&edge.path_points()));
        let use_orthogonal_fallback = original_is_orthogonal
            || matches!(diagram.diagram_type, crate::types::DiagramType::Architecture);

        if !use_orthogonal_fallback {
            // 非正交原图：spline/bezier 已删除，dogleg 会破坏平滑几何；保留原边 + degraded。
            mark_degraded(result, i, DEGRADED_REASON);
            continue;
        }

        let Some((points, chosen_from_port, chosen_to_port)) = orthogonal_detour_try_ports(
            &ep,
            diagram,
            i,
            &routing_snapshot,
            &obstacle_index,
            &skip,
            fallback_lane,
            aggressive_group_skirt,
        ) else {
            // 正交原图 / Architecture dogleg 失败：与历史行为一致（continue 不标 degraded）。
            continue;
        };

        let sampled_for_label = points.clone();
        let geometry = PathGeometry::Polyline { points };

        let middle_t = label_t_for_diagram(diagram, rel);
        let labels =
            build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
                point_at_path_t(&sampled_for_label, t)
            });

        let candidate = EdgeLayout {
            geometry,
            labels,
            from_port: chosen_from_port,
            to_port: chosen_to_port,
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
                // 硬门禁与 lint 对齐：用 path_pierces_foreign_nodes（跳过端口 stub 段）判定，
                // 而非 analyze_edge_node_crossings（含端点段、node_shrink=1）。后者会把
                // 仅在 fan-in 端口 stub 处轻掠邻节点、但 lint 判定干净的外廊候选误杀，
                // 导致稠密图 through 边永远无法修复。C9（after ≤ before）仍守 analyze 不劣化。
                let cand_pts = candidate.path_points();
                if path_pierces_foreign_nodes(
                    &cand_pts,
                    &hard_probe,
                    rel.from.as_str(),
                    rel.to.as_str(),
                ) {
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

/// 标记指定边为 degraded（若 route_annotations 已存在且该边尚未被标 degraded）。
///
/// 与 `recheck_lint_pierce_post_freeze`（mod.rs）的 degraded 写入语义对齐：
/// 不改折点几何，仅记录归因字符串，便于诊断为何该边未被修复。
fn mark_degraded(result: &mut LayoutResult, edge_index: usize, reason: &str) {
    let Some(annotations) = result.hints.route_annotations.as_mut() else {
        return;
    };
    for ann in annotations.edges.iter_mut() {
        if ann.edge_index == edge_index && ann.degraded.is_none() {
            ann.degraded = Some(reason.to_string());
        }
    }
}

/// 先用原端口 dogleg；`aggressive` 时失败再换侧端口。
fn orthogonal_detour_try_ports(
    ep: &crate::layout::edge::common::routing_skeleton::EdgeEndpoints,
    diagram: &Diagram,
    edge_index: usize,
    result: &LayoutResult,
    obstacles: &visibility::ObstacleIndex,
    skip: &[usize],
    fallback_lane: usize,
    aggressive: bool,
) -> Option<(Vec<Point>, Port, Port)> {
    let mut port_pairs = vec![(ep.from_port, ep.to_port)];
    if aggressive {
        const SIDES: [Port; 4] = [Port::Top, Port::Bottom, Port::Left, Port::Right];
        for &fp in &SIDES {
            for &tp in &SIDES {
                if (fp, tp) != (ep.from_port, ep.to_port) {
                    port_pairs.push((fp, tp));
                }
            }
        }
    }
    let from_nl = result.nodes.get(ep.from_id.as_str())?;
    let to_nl = result.nodes.get(ep.to_id.as_str())?;
    for (fp, tp) in port_pairs {
        let start = if fp == ep.from_port && tp == ep.to_port {
            ep.start
        } else {
            port_anchor(from_nl, fp)
        };
        let end = if fp == ep.from_port && tp == ep.to_port {
            ep.end
        } else {
            port_anchor(to_nl, tp)
        };
        if let Some(points) = orthogonal_detour(
            start,
            end,
            fp,
            tp,
            diagram,
            edge_index,
            result,
            obstacles,
            skip,
            fallback_lane,
            aggressive,
        ) {
            return Some((points, fp, tp));
        }
    }
    None
}

fn port_anchor(nl: &crate::layout::NodeLayout, port: Port) -> Point {
    match port {
        Port::Top => Point::new(nl.x + nl.width * 0.5, nl.y),
        Port::Bottom => Point::new(nl.x + nl.width * 0.5, nl.y + nl.height),
        Port::Left => Point::new(nl.x, nl.y + nl.height * 0.5),
        Port::Right => Point::new(nl.x + nl.width, nl.y + nl.height * 0.5),
    }
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
    aggressive: bool,
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
        // 画布最终会按「节点+组+边」全局 bbox 平移；激进裙边若越出内容外廊，
        // 会改变 min_x/min_y → 绝对坐标平移 → node_fp 假漂移。外廊上界用于选优。
        let content_pad = ORTHOGONAL_OUTER_MARGIN + lane_offset + ORTHOGONAL_STUB * 2.0;
        let content_x0 = left - content_pad;
        let content_x1 = right + content_pad;
        let content_y0 = top - content_pad;
        let content_y1 = bottom + content_pad;
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

        // 局部两跳/裙边：只绕「原路径已穿」的无关组；多组连穿时再绕并集 bbox。
        if let Some(rel) = diagram.relations.get(edge_index) {
            let pierced = foreign_groups_pierced_by_edge(
                diagram,
                result,
                edge_index,
                rel.from.as_str(),
                rel.to.as_str(),
            );
            let stub_pairs = [(from_stub, to_stub), (from_escape, to_approach)];
            for gl in &pierced {
                push_rect_skirt_candidates(
                    &mut candidates,
                    start,
                    end,
                    &stub_pairs,
                    gl.x,
                    gl.y,
                    gl.x + gl.width,
                    gl.y + gl.height,
                    lane_offset,
                    aggressive,
                );
            }
            // 多组连穿并集裙边：仅冻结后穿组试修开启，避免 refine 连坐 node_fp。
            if aggressive && pierced.len() >= 2 {
                let mut ux0 = f64::INFINITY;
                let mut uy0 = f64::INFINITY;
                let mut ux1 = f64::NEG_INFINITY;
                let mut uy1 = f64::NEG_INFINITY;
                for gl in &pierced {
                    ux0 = ux0.min(gl.x);
                    uy0 = uy0.min(gl.y);
                    ux1 = ux1.max(gl.x + gl.width);
                    uy1 = uy1.max(gl.y + gl.height);
                }
                push_rect_skirt_candidates(
                    &mut candidates,
                    start,
                    end,
                    &stub_pairs,
                    ux0,
                    uy0,
                    ux1,
                    uy1,
                    lane_offset,
                    true,
                );
            }
        }

        let group_maps = crate::layout::lint::GroupInteriorMaps::new(diagram);
        let endpoint_ids = diagram.relations.get(edge_index).map(|rel| {
            (
                rel.from.as_str().to_string(),
                rel.to.as_str().to_string(),
            )
        });
        // 与 lint 对齐的硬门禁：不穿非端点节点内部（0.5px 全框）、不穿无关组内部。
        let lint_clean = |path: &[Point]| -> bool {
            path.len() >= 2
                && is_orthogonal(path)
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
        };
        let simplified: Vec<Vec<Point>> =
            candidates.into_iter().map(simplify_orthogonal).collect();
        // 二档接受：优先 18px 净距（segment_hits_any）；稠密图无 18px 候选时，
        // 兜底接受 lint 干净（净距 <18px 但仍不穿节点/组）候选——绝不放行 lint 违规路径。
        let mut clean: Vec<Vec<Point>> = simplified
            .iter()
            .filter(|path| {
                lint_clean(path)
                    && path
                        .windows(2)
                        .all(|w| !obstacles.segment_hits_any(w[0], w[1], skip))
            })
            .cloned()
            .collect();
        if clean.is_empty() {
            clean = simplified.into_iter().filter(|p| lint_clean(p)).collect();
        }
        clean.sort_by(|a, b| {
            // 激进模式：优先不越出内容外廊，避免 finalize_canvas 平移导致 node_fp 假漂移。
            let rank = |path: &[Point]| -> i32 {
                if !aggressive {
                    return 0;
                }
                if path_within_rect(path, content_x0, content_y0, content_x1, content_y1) {
                    0
                } else {
                    1
                }
            };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.len().cmp(&b.len()))
                .then_with(|| {
                    path_length(a)
                        .partial_cmp(&path_length(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| path_sort_key(a).cmp(&path_sort_key(b)))
        });
        return clean.into_iter().next();
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

fn path_within_rect(path: &[Point], x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    path.iter()
        .all(|p| p.x >= x0 - 0.1 && p.x <= x1 + 0.1 && p.y >= y0 - 0.1 && p.y <= y1 + 0.1)
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

/// 绕轴对齐矩形外框生成裙边 / U 形候选（局部两跳，非建廊）。
///
/// `aggressive`：额外大外扩 + 角绕行；仅冻结后穿组试修开启。
fn push_rect_skirt_candidates(
    candidates: &mut Vec<Vec<Point>>,
    start: Point,
    end: Point,
    stub_pairs: &[(Point, Point)],
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    lane_offset: f64,
    aggressive: bool,
) {
    // refine（非 aggressive）必须与历史三档外扩一致，否则改边会反馈 space-budget → node_fp。
    let margins: &[f64] = if aggressive {
        &[
            ORTHOGONAL_STUB,
            ORTHOGONAL_STUB * 2.0,
            ORTHOGONAL_OUTER_MARGIN,
            ORTHOGONAL_OUTER_MARGIN * 2.0,
            ORTHOGONAL_STUB * 6.0,
        ]
    } else {
        &[ORTHOGONAL_STUB, ORTHOGONAL_STUB * 2.0, ORTHOGONAL_OUTER_MARGIN]
    };
    for &margin_extra in margins {
        let m = margin_extra + lane_offset;
        let gx0 = x0 - m;
        let gx1 = x1 + m;
        let gy0 = y0 - m;
        let gy1 = y1 + m;
        for &(exit_pt, entry_pt) in stub_pairs {
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
            // U 形两折绕组（上→侧→下 / 左→侧→右）
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
            if !aggressive {
                continue;
            }
            // 角绕行：一侧通道被节点堵死时绕矩形一角。
            for (cx, cy) in [(gx0, gy0), (gx0, gy1), (gx1, gy0), (gx1, gy1)] {
                candidates.push(vec![
                    start,
                    exit_pt,
                    Point::new(exit_pt.x, cy),
                    Point::new(cx, cy),
                    Point::new(entry_pt.x, cy),
                    entry_pt,
                    end,
                ]);
                candidates.push(vec![
                    start,
                    exit_pt,
                    Point::new(cx, exit_pt.y),
                    Point::new(cx, cy),
                    Point::new(cx, entry_pt.y),
                    entry_pt,
                    end,
                ]);
            }
        }
    }
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
