//! 逐边构建路径（Slice C2a：输出 RoutePath）
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构）；Slice C2a 将输出从 EdgeLayout 切换为 RoutePath。

use super::super::*;
use crate::layout::routing::common::parallel_edges::build_parallel_aware_edge_labels;
use crate::layout::routing::common::self_loop;
use crate::layout::routing::edge_routing_orthogonal::visibility_graph::OrthogonalVisibilityGraph;
use crate::layout::routing::model::solution::{EndpointAssignment, RoutePath};
use std::collections::HashMap;

/// `phase_route_edges` 的返回值：逐边路径骨架 + 标签（Slice C2a）。
pub(crate) struct InitialPaths {
    /// 逐边路径（按边序，未求解的为 Empty）。
    pub paths: Vec<RoutePath>,
    /// 逐边标签（与 paths 对齐）。
    pub labels: Vec<Vec<crate::layout::EdgeLabelLayout>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn phase_route_edges(
    edge_order: &[usize],
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    endpoint_assignments: &[EndpointAssignment],
    existing_edges: &[EdgeLayout],
    grid: &mut SegmentGrid,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    parallel: &crate::layout::routing::common::parallel_edges::ParallelGroups,
    preserve_edges: &Option<std::collections::HashSet<usize>>,
    self_loop_idx: &HashMap<usize, usize>,
    space_budget: &mut Option<crate::layout::demand::space_budget::SpaceBudget>,
    feedback_edge_set: &std::collections::HashSet<usize>,
    _s4_monitor_corridor: bool,
    corridor_model: Option<&crate::layout::demand::CorridorModel>,
    ovg: Option<&OrthogonalVisibilityGraph>,
    first_pass: bool,
    routing_contract: &crate::layout::routing::model::RoutingContract,
) -> InitialPaths {
    let n = relations.len();
    let mut paths: Vec<RoutePath> = (0..n)
        .map(|_| RoutePath::Empty(crate::layout::routing::model::solution::EmptyRouteReason::Unresolved))
        .collect();
    let mut labels: Vec<Vec<crate::layout::EdgeLabelLayout>> = (0..n).map(|_| Vec::new()).collect();

    for &i in edge_order {
        let t_edge = crate::layout::perf::Instant::now();
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        if from_id == to_id {
            if let Some(nl) = nodes.get(from_id) {
                let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                // Phase A: 使用空间感知自环路由，感知周围节点选择最优方向
                let sl_edge = self_loop::route_self_loop_aware(
                    rel,
                    nl,
                    from_id,
                    loop_idx,
                    self_loop::SelfLoopStyle::Orthogonal,
                    nodes,
                );
                let pts: Vec<Point> = sl_edge.path_points().into_owned();
                paths[i] = RoutePath::orthogonal(pts);
                labels[i] = sl_edge.labels;
            }
            continue;
        }

        if let Some(ref preserve) = preserve_edges {
            if preserve.contains(&i) && existing_edges[i].path_len() >= 2 {
                let path: Vec<Point> = existing_edges[i].path_points().into_owned();
                grid.insert_path(&path, i);
                paths[i] = RoutePath::orthogonal(path);
                labels[i] = existing_edges[i].labels.clone();
                continue;
            }
        }

        let (Some(_from_nl), Some(_to_nl)) = (nodes.get(from_id), nodes.get(to_id)) else {
            continue;
        };

        let ea = &endpoint_assignments[i];
        let from_ep = ea.project_endpoint(true, from_id.to_string(), Point::zero());
        let to_ep = ea.project_endpoint(false, to_id.to_string(), Point::zero());

        let mut corridor_boost = space_budget
            .as_ref()
            .map(|b| b.corridor_boost_requested)
            .unwrap_or(false);
        let is_feedback = feedback_edge_set.contains(&i);
        let pair = EndpointPair {
            from: from_ep,
            to: to_ep,
        };
        let strict = should_strict_group_transit(is_feedback);

        let mut path_stats = PathSelectStats::default();
        // Phase 3.x：prefer_periphery 优先读 RoutingContract TransitIntent。
        let prefer_outer = routing_contract
            .prefer_periphery(crate::layout::routing::model::StableEdgeId(i))
            || is_feedback;
        let mut ctx =
            OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                .with_strict_group_transit(strict)
                .with_corridor_boost(corridor_boost || !group_ctx.is_same_leaf_group(from_id, to_id))
                .with_prefer_outer_ring(prefer_outer)
                .with_prefer_periphery(prefer_outer)
                .with_first_pass(first_pass);
        if let Some(m) = corridor_model {
            ctx = ctx.with_corridor_demands(m);
        }
        if let Some(ovg_ref) = ovg {
            ctx = ctx.with_ovg(ovg_ref);
        }
        let mut path = select_best_path_with_scorer_stats(
            &ctx,
            &pair,
            &DefaultScorer,
            Some(&mut path_stats),
            false,
        );
        // S2：0 候选/退化 → 升走廊预算再路由一次（加大外框垫），禁止静默脏折线
        if path_stats.degraded && !corridor_boost {
            corridor_boost = true;
            if let Some(budget) = space_budget.as_mut() {
                budget.request_corridor_boost();
            }
            let mut boost_stats = PathSelectStats::default();
            let mut ctx =
                OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                    .with_strict_group_transit(strict)
                    .with_corridor_boost(true)
                    .with_prefer_outer_ring(prefer_outer)
                    .with_first_pass(first_pass);
            if let Some(m) = corridor_model {
                ctx = ctx.with_corridor_demands(m);
            }
            if let Some(ovg_ref) = ovg {
                ctx = ctx.with_ovg(ovg_ref);
            }
            let boosted = select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut boost_stats),
                false,
            );
            if !boost_stats.degraded || boost_stats.candidate_count > path_stats.candidate_count {
                path = boosted;
                path_stats = boost_stats;
            }
        }
        ortho_stats.total_candidates += path_stats.candidate_count;
        ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
            if let Some(budget) = space_budget.as_mut() {
                budget.request_corridor_boost();
            }
        }

        // 标签位置：平行/反向边错开 t + 法向偏移，避免双向边标签重叠
        labels[i] = if path.len() >= 2 {
            match relations.get(i) {
                Some(rel) => {
                    build_parallel_aware_edge_labels(rel, i, relations, &parallel.offsets, &path)
                }
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        grid.insert_path(&path, i);
        paths[i] = RoutePath::orthogonal(path);

        crate::perf_log!(
            "[perf]     edge[{}] {}->{}: {} candidates, {:.2}ms",
            i,
            from_id,
            to_id,
            path_stats.candidate_count,
            t_edge.elapsed().as_secs_f64() * 1000.0
        );
    }

    InitialPaths { paths, labels }
}
