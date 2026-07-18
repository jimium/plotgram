//! 冻结后贴廊试修：对 lint 穿组且有走廊链的跨 leaf 边，重试走廊重建。
//!
//! 只改边几何；整批后穿组/through 变差则由调用方回退。不改主链规划/打分。
//!
//! 候选顺序：原锚点 → 换侧端口；每档先 `validated` 再 `try_build`+lint
//!（`try_build` 已含廊上第三方组外绕）。

use crate::ast::Diagram;
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, label_t_for_diagram, point_at_path_t,
};
use crate::layout::geometry::Point;
use crate::layout::group::{routing_algo_for_diagram, GroupRoutingContext};
use crate::layout::{EdgeLayout, LayoutResult, NodeLayout, PathGeometry, Port};
use std::collections::HashSet;

use super::context::PreparedObstacles;
use super::corridor_route::{plan_corridor_routes, try_build_corridor_path};
use super::profile::OrthoRoutingProfile;
use super::run::validated_corridor_path;

const POST_ROUTE_CORRIDOR_STUB: f64 = 18.0;

/// 对指定边索引用计划走廊重建路径；仅当候选避组时采纳。
///
/// 返回成功贴廊的边数。
pub(crate) fn stick_edges_onto_corridor(
    diagram: &Diagram,
    result: &mut LayoutResult,
    edge_indices: &HashSet<usize>,
) -> usize {
    if edge_indices.is_empty() || result.groups.is_empty() {
        return 0;
    }

    let algo = routing_algo_for_diagram(diagram);
    let group_ctx = GroupRoutingContext::from_layout(diagram, result, algo);
    if group_ctx.corridors.is_empty() {
        return 0;
    }
    let profile = OrthoRoutingProfile::for_diagram_type(diagram.diagram_type.clone());
    let plan = plan_corridor_routes(&diagram.relations, &group_ctx, &profile);
    if plan.chains.is_empty() {
        return 0;
    }
    let obstacles = PreparedObstacles::build(&result.nodes, &group_ctx);

    let mut ordered: Vec<usize> = edge_indices.iter().copied().collect();
    ordered.sort_unstable();
    let mut accepted = 0usize;

    for &edge_index in &ordered {
        if !plan.chains.contains_key(&edge_index) {
            continue;
        }
        let Some(rel) = diagram.relations.get(edge_index) else {
            continue;
        };
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        if group_ctx.is_same_leaf_group(from_id, to_id) {
            continue;
        }
        let Some(edge) = result.edges.get(edge_index) else {
            continue;
        };
        let pts = edge.path_points();
        if pts.len() < 2 {
            continue;
        }
        let Some(from_nl) = result.nodes.get(from_id) else {
            continue;
        };
        let Some(to_nl) = result.nodes.get(to_id) else {
            continue;
        };

        let mut port_pairs: Vec<(Point, Point, Port, Port)> = vec![(
            pts[0],
            pts[pts.len() - 1],
            edge.from_port,
            edge.to_port,
        )];
        const SIDES: [Port; 4] = [Port::Top, Port::Bottom, Port::Left, Port::Right];
        for &fp in &SIDES {
            for &tp in &SIDES {
                if fp == edge.from_port && tp == edge.to_port {
                    continue;
                }
                port_pairs.push((port_anchor(from_nl, fp), port_anchor(to_nl, tp), fp, tp));
            }
        }

        let mut chosen: Option<(Vec<Point>, Port, Port)> = None;
        let mut diag_none = 0usize;
        let mut diag_pierce = 0usize;
        for (from_anchor, to_anchor, from_port, to_port) in port_pairs {
            match try_stick_path_diag(
                diagram,
                result,
                edge_index,
                from_id,
                to_id,
                from_anchor,
                to_anchor,
                from_port,
                to_port,
                &plan,
                &group_ctx,
                &obstacles,
            ) {
                StickTry::Ok(path) => {
                    chosen = Some((path, from_port, to_port));
                    break;
                }
                StickTry::BuildNone => diag_none += 1,
                StickTry::StillPierces => diag_pierce += 1,
            }
        }
        let Some((path, from_port, to_port)) = chosen else {
            crate::perf_log!(
                "[perf]     corridor_stick: edge[{}] {}→{} fail none={} pierce={} chain={:?}",
                edge_index,
                from_id,
                to_id,
                diag_none,
                diag_pierce,
                plan.chains.get(&edge_index)
            );
            continue;
        };

        let middle_t = label_t_for_diagram(diagram, rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&path, t)
        });
        let candidate = EdgeLayout {
            geometry: PathGeometry::Polyline { points: path },
            labels,
            from_port,
            to_port,
        };

        let mut probe = result.clone();
        if edge_index < probe.edges.len() {
            probe.edges[edge_index] = candidate.clone();
        }
        if crate::layout::lint::edge_index_crosses_group_interior(diagram, &probe, edge_index)
            || edge_index_through_foreign_node(diagram, &probe, edge_index)
        {
            continue;
        }

        result.edges[edge_index] = candidate;
        accepted += 1;
        crate::perf_log!(
            "[perf]     corridor_stick: edge[{}] {}→{} ok",
            edge_index,
            from_id,
            to_id
        );
    }

    if accepted > 0 {
        crate::perf_log!(
            "[perf]     corridor_stick: accepted={}/{}",
            accepted,
            ordered.len()
        );
    }
    accepted
}

enum StickTry {
    Ok(Vec<Point>),
    BuildNone,
    StillPierces,
}

fn try_stick_path_diag(
    diagram: &Diagram,
    result: &LayoutResult,
    edge_index: usize,
    from_id: &str,
    to_id: &str,
    from_anchor: Point,
    to_anchor: Point,
    from_port: Port,
    to_port: Port,
    plan: &super::corridor_route::CorridorRoutePlan,
    group_ctx: &GroupRoutingContext,
    obstacles: &PreparedObstacles,
) -> StickTry {
    let mut saw_pierce = false;
    let mut saw_build = false;
    for &stub in &[POST_ROUTE_CORRIDOR_STUB, POST_ROUTE_CORRIDOR_STUB * 2.0] {
        if let Some(path) = validated_corridor_path(
            edge_index,
            from_anchor,
            to_anchor,
            from_id,
            to_id,
            plan,
            group_ctx,
            &result.nodes,
            obstacles,
            stub,
        ) {
            let mut probe = result.clone();
            if edge_index < probe.edges.len() {
                probe.edges[edge_index] = EdgeLayout {
                    geometry: PathGeometry::Polyline {
                        points: path.clone(),
                    },
                    labels: Vec::new(),
                    from_port,
                    to_port,
                };
                if !edge_index_through_foreign_node(diagram, &probe, edge_index) {
                    return StickTry::Ok(path);
                }
            }
            saw_pierce = true;
            continue;
        }
        let Some(raw) = try_build_corridor_path(
            edge_index,
            from_anchor,
            to_anchor,
            from_id,
            to_id,
            plan,
            group_ctx,
            stub,
            true,
        ) else {
            continue;
        };
        saw_build = true;
        if raw.len() < 2 {
            continue;
        }
        let mut probe = result.clone();
        if edge_index >= probe.edges.len() {
            continue;
        }
        probe.edges[edge_index] = EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: raw.clone(),
            },
            labels: Vec::new(),
            from_port,
            to_port,
        };
        if !crate::layout::lint::edge_index_crosses_group_interior(diagram, &probe, edge_index)
            && !edge_index_through_foreign_node(diagram, &probe, edge_index)
        {
            return StickTry::Ok(raw);
        }
        saw_pierce = true;
    }
    if saw_pierce {
        StickTry::StillPierces
    } else if saw_build {
        StickTry::StillPierces
    } else {
        StickTry::BuildNone
    }
}

fn port_anchor(nl: &NodeLayout, port: Port) -> Point {
    match port {
        Port::Top => Point::new(nl.x + nl.width * 0.5, nl.y),
        Port::Bottom => Point::new(nl.x + nl.width * 0.5, nl.y + nl.height),
        Port::Left => Point::new(nl.x, nl.y + nl.height * 0.5),
        Port::Right => Point::new(nl.x + nl.width, nl.y + nl.height * 0.5),
    }
}

/// 与 lint `edge_through_node` 同口径：贴廊候选不得引入穿节点（否则整批 repair 会因 through 回退）。
fn edge_index_through_foreign_node(
    diagram: &Diagram,
    result: &LayoutResult,
    edge_index: usize,
) -> bool {
    let Some(edge) = result.edges.get(edge_index) else {
        return false;
    };
    let Some(rel) = diagram.relations.get(edge_index) else {
        return false;
    };
    let path = edge.path_points();
    if path.len() < 2 {
        return false;
    }
    let from_id = rel.from.as_str();
    let to_id = rel.to.as_str();
    let segment_count = path.len().saturating_sub(1);
    let skip_endpoints = segment_count > 2;
    let mut node_ids: Vec<&String> = result.nodes.keys().collect();
    node_ids.sort();
    for (seg_i, window) in path.windows(2).enumerate() {
        if skip_endpoints && (seg_i == 0 || seg_i == segment_count - 1) {
            continue;
        }
        for node_id in &node_ids {
            let node_id = node_id.as_str();
            if node_id == from_id || node_id == to_id {
                continue;
            }
            let Some(nl) = result.nodes.get(node_id) else {
                continue;
            };
            if crate::layout::refine::segment_intersects_node(window[0], window[1], nl) {
                return true;
            }
        }
    }
    false
}
