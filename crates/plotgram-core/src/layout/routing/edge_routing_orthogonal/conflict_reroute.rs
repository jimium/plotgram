//! X-1: multi-round conflict resolution rerouting.
//!
//! R6：原多轮重路由控制流 `reroute_conflicting_edges` 已被 `path_solver` 单一主流程
//! 内化删除；本模块只保留供 solver 复用的冲突检测（`collect_spacing_conflicts`）与
//! 洁净重路由候选搜索（`find_clean_reroute_path`）两个 kernel 辅助。

use super::*;
use crate::layout::geometry::Point;
use crate::layout::NodeLayout;
use crate::layout::routing::edge_routing_orthogonal::visibility_graph::OrthogonalVisibilityGraph;
use crate::layout::routing::model::solution::RoutePath;
use std::collections::HashMap;

/// 收集所有存在间距违规的边索引及其违规数。
///
/// 跳过空路径边和已标记失败的边。stub 段在 `path_edge_spacing_violations`
/// 内部已豁免。
pub(super) fn collect_spacing_conflicts(
    paths: &[RoutePath],
    grid: &SegmentGrid,
    parallel_gap: f64,
    failed_edges: &std::collections::HashSet<usize>,
) -> Vec<(usize, usize)> {
    let mut conflicts: Vec<(usize, usize)> = Vec::new();
    for ei in 0..paths.len() {
        if paths[ei].is_empty() || failed_edges.contains(&ei) {
            continue;
        }
        let points = paths[ei].points();
        let viols = path_edge_spacing_violations(points, grid, parallel_gap);
        if !viols.is_empty() {
            conflicts.push((ei, viols.len()));
        }
    }
    conflicts
}

/// 为一条冲突边寻找干净的重路由路径。
///
/// 依次尝试：corridor 快速通道 → 递增 channel_margin 的全候选搜索。
/// 返回第一条通过节点/分组/边间距硬检查的路径；若全部失败返回 `None`。
///
/// 返回值的 `bool` 表示是否保留原标签（corridor 快速通道保留，全候选搜索重建）。
#[allow(clippy::too_many_arguments)]
pub(super) fn find_clean_reroute_path(
    ei: usize,
    from_ep: &Endpoint,
    to_ep: &Endpoint,
    from_id: &str,
    to_id: &str,
    cfg: &OrthoConfig,
    reroute_margins: &[f64],
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    grid: &SegmentGrid,
    profile: &OrthoRoutingProfile,
    obstacles: &PreparedObstacles,
    load_map: &ChannelLoadMap,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    parallel_gap: f64,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    ovg: Option<&OrthogonalVisibilityGraph>,
) -> Option<(Vec<Point>, bool)> {
    // 先试 corridor 快速通道（保留原标签）
    let has_chain = corridor_plan.chains.contains_key(&ei);
    if let Some(corridor_path) = validated_corridor_path(
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
    ) {
        if path_edge_spacing_violations(&corridor_path, grid, parallel_gap).is_empty() {
            return Some((corridor_path, true));
        }
    }
    let prefer_outer = false;

    // 递增 margin 尝试全候选搜索（重建标签）
    for &margin in reroute_margins {
        let r_cfg = OrthoConfig {
            channel_margin: margin,
            ..*cfg
        };
        let boost = margin > cfg.channel_margin + 0.5;
        let mut ctx = OrthoRoutingContext::new(
            nodes,
            group_ctx,
            grid,
            &r_cfg,
            profile,
            obstacles,
            Some(load_map),
        )
        .with_strict_group_transit(should_strict_group_transit(
            profile,
            group_ctx,
            from_id,
            to_id,
            has_chain,
            false,
        ))
        .with_corridor_boost(boost || has_chain)
        .with_prefer_outer_ring(prefer_outer);
        if let Some(ovg_ref) = ovg {
            ctx = ctx.with_ovg(ovg_ref);
        }
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };
        let mut path_stats = PathSelectStats::default();
        // 使用全候选（phase1_only=false），包含 staircases，增加找到干净路径的概率
        let candidate = select_best_path_with_scorer_stats(
            &ctx,
            &pair,
            &DefaultScorer,
            Some(&mut path_stats),
            false,
        );
        ortho_stats.total_candidates += path_stats.candidate_count;
        ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
        }

        if candidate.len() >= 2
            && path_is_clean(
                &candidate,
                pair.from_id(),
                pair.to_id(),
                nodes,
                group_ctx,
                &obstacles.sorted_node_ids,
            )
            && path_avoids_group_interiors(
                &candidate,
                pair.from_id(),
                pair.to_id(),
                group_ctx,
                &obstacles.sorted_group_ids,
            )
            && path_is_clean_from_edges(&candidate, grid, parallel_gap, STUB_GUARD_LENGTH)
        {
            return Some((candidate, false));
        }
    }

    None
}
