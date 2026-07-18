//! 冻结后贴廊试修：对 lint 穿组且有走廊链的跨 leaf 边，重试 `validated_corridor_path`。
//!
//! 只改边几何；整批后穿组/through 变差则由调用方回退。不改主链规划/打分。

use crate::ast::Diagram;
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, label_t_for_diagram, point_at_path_t,
};
use crate::layout::geometry::Point;
use crate::layout::group::{routing_algo_for_diagram, GroupRoutingContext};
use crate::layout::{EdgeLayout, LayoutResult, PathGeometry};
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
        let from_anchor = pts[0];
        let to_anchor = pts[pts.len() - 1];
        let from_port = edge.from_port;
        let to_port = edge.to_port;

        let Some(path) = validated_corridor_path(
            edge_index,
            from_anchor,
            to_anchor,
            from_id,
            to_id,
            &plan,
            &group_ctx,
            &result.nodes,
            &obstacles,
            POST_ROUTE_CORRIDOR_STUB,
        )
        .or_else(|| {
            // validated 因 path_avoids / clean 口径拒掉时，再用 lint 同语义收一次
            // try_build 结果（P1：不穿组优先；跨 leaf 可容忍 through）。
            let raw = try_build_corridor_path(
                edge_index,
                from_anchor,
                to_anchor,
                from_id,
                to_id,
                &plan,
                &group_ctx,
                POST_ROUTE_CORRIDOR_STUB,
            )?;
            if raw.len() < 2 {
                return None;
            }
            let mut probe = result.clone();
            if edge_index >= probe.edges.len() {
                return None;
            }
            probe.edges[edge_index] = EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: raw.clone(),
                },
                labels: Vec::new(),
                from_port,
                to_port,
            };
            if crate::layout::lint::edge_index_crosses_group_interior(diagram, &probe, edge_index)
            {
                return None;
            }
            Some(raw)
        }) else {
            continue;
        };

        let middle_t = label_t_for_diagram(diagram, rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&path, t)
        });
        let candidate = EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: path,
            },
            labels,
            from_port,
            to_port,
        };

        let mut probe = result.clone();
        if edge_index < probe.edges.len() {
            probe.edges[edge_index] = candidate.clone();
        }
        if crate::layout::lint::edge_index_crosses_group_interior(diagram, &probe, edge_index) {
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
