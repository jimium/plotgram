//! 逐边构建路径
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构，行为不变）。

use super::super::*;
use crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels;
use crate::layout::edge::common::self_loop;
use crate::layout::edge::edge_routing_orthogonal::channel_planner::ChannelPlan;
use crate::layout::edge::edge_routing_orthogonal::visibility_graph::OrthogonalVisibilityGraph;
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
pub(crate) fn phase_route_edges(
    edge_order: &[usize],
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    parallel: &crate::layout::edge::common::parallel_edges::ParallelGroups,
    preserve_edges: &Option<std::collections::HashSet<usize>>,
    self_loop_idx: &HashMap<usize, usize>,
    space_budget: &mut Option<crate::layout::space_budget::SpaceBudget>,
    feedback_edge_set: &std::collections::HashSet<usize>,
    s4_monitor_corridor: bool,
    corridor_model: Option<&crate::layout::demand::CorridorModel>,
    ovg: Option<&OrthogonalVisibilityGraph>,
    channel_plan: Option<&ChannelPlan>,
) {

    for &i in edge_order {
        let t_edge = crate::layout::perf::Instant::now();
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        if from_id == to_id {
            if let Some(nl) = nodes.get(from_id) {
                let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                // Phase A: 使用空间感知自环路由，感知周围节点选择最优方向
                edges[i] = self_loop::route_self_loop_aware(
                    rel,
                    nl,
                    from_id,
                    loop_idx,
                    self_loop::SelfLoopStyle::Orthogonal,
                    nodes,
                );
            }
            continue;
        }

        if let Some(ref preserve) = preserve_edges {
            if preserve.contains(&i) && edges[i].path_len() >= 2 {
                let path: Vec<Point> = edges[i].path_points().into_owned();
                grid.insert_path(&path, i);
                continue;
            }
        }

        let (Some(_from_nl), Some(_to_nl)) = (nodes.get(from_id), nodes.get(to_id)) else {
            continue;
        };

        let Some(from_ep) = endpoint_map.get(&(i, true)) else {
            continue;
        };
        let Some(to_ep) = endpoint_map.get(&(i, false)) else {
            continue;
        };

        let mut corridor_boost = space_budget
            .as_ref()
            .map(|b| b.corridor_boost_requested)
            .unwrap_or(false);
        let is_feedback = feedback_edge_set.contains(&i);
        let has_chain = corridor_plan.chains.contains_key(&i);
        let same_leaf = group_ctx.is_same_leaf_group(from_id, to_id);
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };
        let strict = should_strict_group_transit(
            profile,
            group_ctx,
            from_id,
            to_id,
            has_chain,
            is_feedback,
        );

        let mut path_stats = PathSelectStats::default();
        let corridor_ok = validated_corridor_path(
            i,
            from_ep.anchor,
            to_ep.anchor,
            from_id,
            to_id,
            corridor_plan,
            group_ctx,
            nodes,
            obstacles,
            cfg.channel_margin,
        );
        // P5（保守落地）：有组图跨 leaf 外廊与 S4 monitor 解耦的「无链 prefer_outer」
        // 会在 ecommerce 等图引入新穿组；此处仍仅 S4 monitor 开外环，
        // 跨 leaf 无链靠 strict + corridor_boost 收口（完整 P5 留给后续几何）。
        // Phase A: 回环边始终偏好外环路由，避免与正向边抢内部通道。
        let prefer_outer = is_feedback || (s4_monitor_corridor && is_feedback);
        // P2：有 chain 但 validated 失败 → 显式 degraded（禁止静默 free-route 冒充成功）。
        let corridor_contract_failed = has_chain && corridor_ok.is_none();
        // Phase B3: 查询全局通道规划分配的通道坐标
        let planned_ch = channel_plan.and_then(|cp| {
            let (coord, is_vert) = cp.channel_for_edge(i)?;
            // 只注入与当前轴匹配的通道
            let from_vertical = is_vertical_port(from_ep.side);
            if from_vertical == is_vert { Some(coord) } else { None }
        });
        let mut path = corridor_ok.unwrap_or_else(|| {
            let mut ctx =
                OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                    .with_strict_group_transit(strict)
                    .with_corridor_boost(
                        corridor_boost || corridor_contract_failed || (!same_leaf && !has_chain),
                    )
                    .with_prefer_outer_ring(prefer_outer);
            if let Some(m) = corridor_model {
                ctx = ctx.with_corridor_demands(m);
            }
            if let Some(ovg_ref) = ovg {
                ctx = ctx.with_ovg(ovg_ref);
            }
            if planned_ch.is_some() {
                ctx = ctx.with_planned_channel(planned_ch);
            }
            select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut path_stats),
                false,
            )
        });
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
                    .with_prefer_outer_ring(prefer_outer);
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
        if path_stats.degraded || corridor_contract_failed {
            ortho_stats.degraded_count += 1;
            if let Some(budget) = space_budget.as_mut() {
                budget.request_corridor_boost();
            }
        }

        // 标签位置：平行/反向边错开 t + 法向偏移，避免双向边标签重叠
        let labels = if path.len() >= 2 {
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

        let mut edge = EdgeLayout {
            // 临时占位，下面用 set_polyline_points 根据 path 点数自动选择 Straight/Polyline
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: from_side[i],
            to_port: to_side[i],
        };
        edge.set_polyline_points(path);

        edges[i] = edge;
        crate::perf_log!(
            "[perf]     edge[{}] {}->{}: {} candidates, {:.2}ms",
            i,
            from_id,
            to_id,
            path_stats.candidate_count,
            t_edge.elapsed().as_secs_f64() * 1000.0
        );
    }
}
