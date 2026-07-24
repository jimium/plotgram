//! X-1: multi-round conflict resolution rerouting.

use super::*;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
use crate::layout::routing::edge_routing_orthogonal::visibility_graph::OrthogonalVisibilityGraph;
use std::collections::HashMap;

/// X-1: 多轮重路由默认上限（违规边多时可升到此值）
const MAX_REROUTE_ROUNDS: usize = 3;
/// X-1: 默认轮次；仅当冲突边数超过阈值时升到 MAX
const DEFAULT_REROUTE_ROUNDS: usize = 2;
/// X-1: 冲突边数超过此值时启用第 3 轮
const REROUTE_ESCALATE_CONFLICT_THRESHOLD: usize = 8;
/// X-1: 重路由时额外增大 channel_margin 以生成更多绕行候选
const REROUTE_EXTRA_CHANNEL_MARGIN: f64 = 40.0;

/// X-1: 多轮冲突消解重路由。
///
/// 第一轮路由使用软惩罚（edge_overlap_penalty），可能产生边重合。
/// 本函数在 replan_slots 之后执行，通过多轮迭代：
/// 1. 检测所有边中段的间距违规
/// 2. 按违规段数降序排列冲突边
/// 3. 逐条移除冲突边，尝试用更宽的通道 margin 重新路由
/// 4. 新路径必须通过 path_is_clean_from_edges 硬检查（节点+分组+边间距）
/// 5. 若找不到干净路径，保留原路径（优雅降级）
pub fn reroute_conflicting_edges(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    edges: &mut Vec<EdgeLayout>,
    grid: &mut SegmentGrid,
    cfg: &OrthoConfig,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    profile: &OrthoRoutingProfile,
    ovg: Option<&OrthogonalVisibilityGraph>,
) {
    use std::collections::HashSet;

    let n = edges.len();
    if n < 2 {
        return;
    }

    let parallel_gap = profile.parallel_gap;

    // 重路由时使用的 margin 档位：逐步增大以生成更多绕行候选
    let reroute_margins: [f64; 3] = [
        cfg.channel_margin + 10.0,
        cfg.channel_margin + 25.0,
        cfg.channel_margin + REROUTE_EXTRA_CHANNEL_MARGIN,
    ];

    let mut total_rerouted = 0usize;
    let mut rounds_done = 0usize;
    let mut failed_edges: HashSet<usize> = HashSet::new();
    let mut max_channel_load = 0usize;
    // OVG 开启时初始路由质量更高，默认 1 轮即可；否则 2 轮
    let mut max_rounds = if ovg.is_some() { 1 } else { DEFAULT_REROUTE_ROUNDS };

    for round in 0..MAX_REROUTE_ROUNDS {
        if round >= max_rounds {
            break;
        }
        // 检测所有冲突边（path_edge_spacing_violations 内部已豁免 stub 段）
        let conflicts = collect_spacing_conflicts(edges, grid, parallel_gap, &failed_edges);

        if conflicts.is_empty() {
            break;
        }
        if conflicts.len() > REROUTE_ESCALATE_CONFLICT_THRESHOLD {
            max_rounds = MAX_REROUTE_ROUNDS;
        }

        // 按违规数降序排列（稳定排序保证确定性）
        let mut conflicts = conflicts;
        conflicts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        rounds_done = round + 1;

        // Phase 3: 每轮构建通道负载图，让 scorer 偏好低负载通道，从源头减少拥堵
        let load_map = ChannelLoadMap::build(edges, crate::layout::constants::GRID_SNAP_STEP);
        max_channel_load = max_channel_load.max(load_map.max_load());

        for &(ei, _) in &conflicts {
            if failed_edges.contains(&ei) {
                continue;
            }
            // 重新检查冲突——上一次重路由可能已解决了这条边的冲突
            let current_points: Vec<Point> = edges[ei].path_points().into_owned();
            if path_edge_spacing_violations(&current_points, grid, parallel_gap).is_empty() {
                continue;
            }

            let Some(from_ep) = endpoint_map.get(&(ei, true)) else {
                continue;
            };
            let Some(to_ep) = endpoint_map.get(&(ei, false)) else {
                continue;
            };

            let (from_id, to_id) = relations
                .get(ei)
                .map(|rel| (rel.from.as_str(), rel.to.as_str()))
                .unwrap_or(("", ""));

            // 先移除当前边的旧段，避免 corridor 校验时与自身旧路径假冲突，
            // 也避免快速路径 insert 时旧段残留为幽灵段（P05 回归）
            grid.remove_by_edges(&[ei]);
            let old_points: Vec<Point> = edges[ei].path_points().into_owned();

            let clean_path = find_clean_reroute_path(
                ei,
                from_ep,
                to_ep,
                from_id,
                to_id,
                cfg,
                &reroute_margins,
                nodes,
                group_ctx,
                grid,
                profile,
                obstacles,
                &load_map,
                ortho_stats,
                parallel_gap,
                corridor_plan,
                ovg,
            );

            match clean_path {
                Some((path, preserve_old_labels)) => {
                    let labels = if preserve_old_labels {
                        edges[ei].labels.clone()
                    } else if path.len() >= 2 {
                        match relations.get(ei) {
                            Some(rel) => {
                                crate::layout::routing::common::parallel_edges::build_parallel_aware_edge_labels_auto(
                                    rel, ei, relations, &path,
                                )
                            }
                            None => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    };
                    grid.insert_path(&path, ei);
                    let mut edge = EdgeLayout {
                        geometry: PathGeometry::Polyline { points: Vec::new() },
                        labels,
                        from_port: from_side[ei],
                        to_port: to_side[ei],
                    };
                    edge.set_polyline_points(path);
                    edges[ei] = edge;
                    // corridor 快速通道不计入 rerouted 统计（对齐原 continue 分支）
                    if !preserve_old_labels {
                        total_rerouted += 1;
                    }
                }
                None => {
                    // 找不到干净路径，恢复原路径并标记为失败，后续轮次跳过
                    grid.insert_path(&old_points, ei);
                    failed_edges.insert(ei);
                }
            }
        }
    }

    ortho_stats.reroute_iterations = rounds_done;
    ortho_stats.rerouted_edges = total_rerouted;
    ortho_stats.max_channel_load = max_channel_load;
}

/// 收集所有存在间距违规的边索引及其违规数。
///
/// 跳过空路径边和已标记失败的边。stub 段在 `path_edge_spacing_violations`
/// 内部已豁免。
fn collect_spacing_conflicts(
    edges: &[EdgeLayout],
    grid: &SegmentGrid,
    parallel_gap: f64,
    failed_edges: &std::collections::HashSet<usize>,
) -> Vec<(usize, usize)> {
    let mut conflicts: Vec<(usize, usize)> = Vec::new();
    for ei in 0..edges.len() {
        if edges[ei].path_is_empty() || failed_edges.contains(&ei) {
            continue;
        }
        let points: Vec<Point> = edges[ei].path_points().into_owned();
        let viols = path_edge_spacing_violations(&points, grid, parallel_gap);
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
fn find_clean_reroute_path(
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
