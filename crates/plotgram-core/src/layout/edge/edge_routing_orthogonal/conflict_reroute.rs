//! X-1: multi-round conflict resolution rerouting.

use super::*;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
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
    // Iteration 3：默认 2 轮；冲突边多时升到 3
    let mut max_rounds = DEFAULT_REROUTE_ROUNDS;

    for round in 0..MAX_REROUTE_ROUNDS {
        if round >= max_rounds {
            break;
        }
        // 检测所有冲突边（path_edge_spacing_violations 内部已豁免 stub 段）
        let mut conflicts: Vec<(usize, usize)> = Vec::new(); // (ei, violation_count)
        for ei in 0..n {
            if edges[ei].path_is_empty() || failed_edges.contains(&ei) {
                continue;
            }
            let points: Vec<Point> = edges[ei].path_points().into_owned();
            let viols = path_edge_spacing_violations(&points, grid, parallel_gap);
            if !viols.is_empty() {
                conflicts.push((ei, viols.len()));
            }
        }

        if conflicts.is_empty() {
            break;
        }
        if conflicts.len() > REROUTE_ESCALATE_CONFLICT_THRESHOLD {
            max_rounds = MAX_REROUTE_ROUNDS;
        }

        // 按违规数降序排列（稳定排序保证确定性）
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
                    grid.insert_path(&corridor_path, ei);
                    let mut edge = EdgeLayout {
                        geometry: PathGeometry::Polyline { points: Vec::new() },
                        labels: edges[ei].labels.clone(),
                        from_port: from_side[ei],
                        to_port: to_side[ei],
                    };
                    edge.set_polyline_points(corridor_path);
                    edges[ei] = edge;
                    continue;
                }
            }

            let mut clean_path: Option<Vec<Point>> = None;

            for &margin in &reroute_margins {
                let r_cfg = OrthoConfig {
                    channel_margin: margin,
                    ..*cfg
                };
                let boost = margin > cfg.channel_margin + 0.5;
                let ctx = RoutingContext::new(
                    nodes,
                    group_ctx,
                    grid,
                    &r_cfg,
                    profile,
                    obstacles,
                    Some(&load_map),
                )
                .with_strict_group_transit(should_strict_group_transit(
                    profile,
                    group_ctx,
                    from_id,
                    to_id,
                    corridor_plan.chains.contains_key(&ei),
                ))
                .with_corridor_boost(boost);
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
                    clean_path = Some(candidate);
                    break;
                }
            }

            match clean_path {
                Some(path) => {
                    let labels = if path.len() >= 2 {
                        match relations.get(ei) {
                            Some(rel) => {
                                crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels_auto(
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
                    total_rerouted += 1;
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
