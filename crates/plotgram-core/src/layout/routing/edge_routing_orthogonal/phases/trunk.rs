//! 受保护 trunk 提取 + trunk 后 feedback 重路由
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构，行为不变）。

use super::super::*;
use crate::layout::routing::common::parallel_edges::build_parallel_aware_edge_labels;
use crate::layout::routing::edge_routing_orthogonal::visibility_graph::OrthogonalVisibilityGraph;
use crate::layout::routing::model::solution::EndpointAssignment;
use std::collections::HashMap;

/// 从 S3 merge_intervals 提取垂直受保护干线 `(x, y_lo, y_hi)`（去重、排序）。
pub(crate) fn extract_protected_vertical_trunks(
    merge_intervals: &std::collections::HashMap<usize, Vec<crate::layout::routing::MergeInterval>>,
) -> Vec<(f64, f64, f64)> {
    let mut trunks: Vec<(f64, f64, f64)> = Vec::new();
    let mut keys: Vec<usize> = merge_intervals.keys().copied().collect();
    keys.sort_unstable();
    for ei in keys {
        let Some(ivs) = merge_intervals.get(&ei) else {
            continue;
        };
        for iv in ivs {
            if iv.horizontal {
                continue;
            }
            let y0 = iv.t0.min(iv.t1);
            let y1 = iv.t0.max(iv.t1);
            trunks.push((iv.coord, y0, y1));
        }
    }
    trunks.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
    });
    trunks.dedup_by(|a, b| {
        (a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0 && (a.2 - b.2).abs() < 1.0
    });
    trunks
}

/// S4：在 FanIn 干线写定后，重路由 feedback/监控边并加重干线穿越惩罚。
pub(crate) fn phase_reroute_feedback_after_trunk(
    feedback_edge_set: &std::collections::HashSet<usize>,
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    endpoint_assignments: &[EndpointAssignment],
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    parallel: &crate::layout::routing::common::parallel_edges::ParallelGroups,
    protected_trunks: &[(f64, f64, f64)],
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    ovg: Option<&OrthogonalVisibilityGraph>,
) -> usize {
    let mut order: Vec<usize> = feedback_edge_set.iter().copied().collect();
    order.sort_unstable();
    if order.is_empty() {
        return 0;
    }
    grid.remove_by_edges(&order);
    let mut rerouted = 0usize;
    for &ei in &order {
        if ei >= endpoint_assignments.len() {
            continue;
        }
        let ea = &endpoint_assignments[ei];
        let (from_id, to_id) = relations
            .get(ei)
            .map(|rel| (rel.from.as_str(), rel.to.as_str()))
            .unwrap_or(("", ""));
        if from_id == to_id {
            continue;
        }
        let from_ep = ea.project_endpoint(true, from_id.to_string(), Point::zero());
        let to_ep = ea.project_endpoint(false, to_id.to_string(), Point::zero());
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };
        let strict = should_strict_group_transit(
            profile,
            group_ctx,
            from_id,
            to_id,
            corridor_plan.chains.contains_key(&ei),
            true,
        );
        let mut path_stats = PathSelectStats::default();
        let path = validated_corridor_path(
            ei,
            from_ep.anchor,
            to_ep.anchor,
            from_id,
            to_id,
            corridor_plan,
            group_ctx,
            nodes,
            obstacles,
            cfg.channel_margin,
        )
        .unwrap_or_else(|| {
            let mut ctx =
                OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                    .with_strict_group_transit(strict)
                    .with_prefer_outer_ring(true)
                    .with_protected_trunks(protected_trunks);
            if let Some(ovg_ref) = ovg {
                ctx = ctx.with_ovg(ovg_ref);
            }
            let mut first = select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut path_stats),
                false,
            );
            if path_stats.degraded {
                let mut boost_stats = PathSelectStats::default();
                let mut ctx2 =
                    OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                        .with_strict_group_transit(strict)
                        .with_corridor_boost(true)
                        .with_prefer_outer_ring(true)
                        .with_protected_trunks(protected_trunks);
                if let Some(ovg_ref) = ovg {
                    ctx2 = ctx2.with_ovg(ovg_ref);
                }
                let boosted = select_best_path_with_scorer_stats(
                    &ctx2,
                    &pair,
                    &DefaultScorer,
                    Some(&mut boost_stats),
                    false,
                );
                if !boost_stats.degraded || boost_stats.candidate_count > path_stats.candidate_count
                {
                    path_stats = boost_stats;
                    first = boosted;
                }
            }
            first
        });
        if path.len() < 2 {
            continue;
        }
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
        }
        grid.insert_path(&path, ei);
        let labels = match relations.get(ei) {
            Some(rel) => {
                build_parallel_aware_edge_labels(rel, ei, relations, &parallel.offsets, &path)
            }
            None => Vec::new(),
        };
        let mut edge = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: ea.from_port,
            to_port: ea.to_port,
        };
        edge.set_polyline_points(path);
        edges[ei] = edge;
        rerouted += 1;
    }
    ortho_stats.feedback_rerouted_after_trunk = rerouted;
    rerouted
}
