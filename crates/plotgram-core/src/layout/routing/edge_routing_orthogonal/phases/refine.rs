//! 对齐 / 重路由 / stub / lane 精修
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构，行为不变）。

use super::super::*;
use crate::layout::routing::common::edge_geometry::undirected_pair_key;
use crate::layout::routing::common::parallel_edges::build_parallel_aware_edge_labels;
use crate::layout::routing::edge_routing_orthogonal::visibility_graph::OrthogonalVisibilityGraph;
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
pub(crate) fn phase_straighten_align(
    nodes: &HashMap<String, NodeLayout>,
    n: usize,
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &mut HashMap<(usize, bool), Endpoint>,
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    relations: &[crate::ast::Relation],
    reverse_pairs: &std::collections::BTreeSet<String>,
    parallel: &crate::layout::routing::common::parallel_edges::ParallelGroups,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    space_budget: &Option<crate::layout::demand::space_budget::SpaceBudget>,
    ovg: Option<&OrthogonalVisibilityGraph>,
) {
    // ── 4c. 直连偏好对齐：正对端口边的 slot 锚点对齐修正 ──
    // 在 replan_slots 之后执行，确保anchor位置是最终的slot排序结果。
    // 修改anchor后需要重路由受影响的边，因此放在 X-1 reroute 之前。
    let t_align2 = crate::layout::perf::Instant::now();
    let old_endpoints: HashMap<(usize, bool), Endpoint> = endpoint_map.clone();
    // 仅 reverse_pairs 边携带非零 parallel offset；straighten 对齐到 center±offset
    let mut straighten_offsets = vec![0.0; n];
    for ((edge_index, _), _) in endpoint_map.iter() {
        let rel = &relations[*edge_index];
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        if reverse_pairs.contains(&key) {
            straighten_offsets[*edge_index] = parallel.offsets[*edge_index];
        }
    }
    straighten_preferred_alignments(
        nodes,
        n,
        &from_side,
        &to_side,
        endpoint_map,
        &straighten_offsets,
    );

    // 找出anchor被修改的边，需要重路由
    let mut align_reroute: Vec<usize> = Vec::new();
    for i in 0..n {
        for &is_from in &[true, false] {
            let old_ep = old_endpoints.get(&(i, is_from));
            let new_ep = endpoint_map.get(&(i, is_from));
            if let (Some(o), Some(ne)) = (old_ep, new_ep) {
                if (o.anchor.x - ne.anchor.x).abs() > EPS || (o.anchor.y - ne.anchor.y).abs() > EPS
                {
                    align_reroute.push(i);
                    break;
                }
            }
        }
    }
    if !align_reroute.is_empty() {
        align_reroute.sort_unstable();
        grid.remove_by_edges(&align_reroute);
        for &ei in &align_reroute {
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
            let mut path_stats = PathSelectStats::default();
            let has_chain = corridor_plan.chains.contains_key(&ei);
            let corridor_ok = validated_corridor_path(
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
            );
            let prefer_outer = false; // P5 保守：align 重路由不强制外环
            let candidate = corridor_ok.unwrap_or_else(|| {
                let pair = EndpointPair {
                    from: from_ep.clone(),
                    to: to_ep.clone(),
                };
                let boost = space_budget
                    .as_ref()
                    .map(|b| b.corridor_boost_requested)
                    .unwrap_or(false);
                let mut ctx = OrthoRoutingContext::new(
                    nodes, group_ctx, &grid, cfg, profile, obstacles, None,
                )
                .with_strict_group_transit(should_strict_group_transit(
                    profile,
                    group_ctx,
                    from_id,
                    to_id,
                    has_chain,
                    false,
                ))
                .with_corridor_boost(boost || has_chain || !group_ctx.is_same_leaf_group(from_id, to_id))
                .with_prefer_outer_ring(prefer_outer);
                if let Some(ovg_ref) = ovg {
                    ctx = ctx.with_ovg(ovg_ref);
                }
                select_best_path_with_scorer_stats(
                    &ctx,
                    &pair,
                    &DefaultScorer,
                    Some(&mut path_stats),
                    false,
                )
            });
            if candidate.len() >= 2 {
                grid.insert_path(&candidate, ei);
                let labels = match relations.get(ei) {
                    Some(rel) => build_parallel_aware_edge_labels(
                        rel,
                        ei,
                        relations,
                        &parallel.offsets,
                        &candidate,
                    ),
                    None => Vec::new(),
                };
                let mut edge = EdgeLayout {
                    geometry: PathGeometry::Polyline { points: Vec::new() },
                    labels,
                    from_port: from_side[ei],
                    to_port: to_side[ei],
                };
                edge.set_polyline_points(candidate);
                edges[ei] = edge;
            }
        }
    }
    crate::perf_log!(
        "[perf]     4c_straighten_align: {:.2}ms (aligned {} edges)",
        t_align2.elapsed().as_secs_f64() * 1000.0,
        align_reroute.len()
    );
}

/// Phase 3：分层批量边序（有 rank 时低层先占通道；feedback + 监控枢纽全局延后）
pub(crate) fn phase_layer_order(
    relations: &[crate::ast::Relation],
    sugiyama_ranks: Option<&HashMap<String, usize>>,
    feedback_assignment: &feedback_side::FeedbackSideAssignment,
    s4_monitor_corridor: bool,
    difficulty_scores: Option<&[f64]>,
) -> (Vec<usize>, std::collections::HashSet<usize>) {
    let t2 = crate::layout::perf::Instant::now();
    let node_degree = layer_order::compute_node_degrees(relations);
    let mut feedback_edge_set: std::collections::HashSet<usize> =
        feedback_assignment.hints.keys().copied().collect();
    // S4：无分组 architecture 下，监控枢纽被动入边并入延后集（不改端口）
    if s4_monitor_corridor {
        for ei in feedback_side::monitor_hub_edge_indices(relations) {
            feedback_edge_set.insert(ei);
        }
    }
    let edge_order = layer_order::compute_edge_order_with_feedback(
        relations,
        sugiyama_ranks,
        &node_degree,
        Some(&feedback_edge_set),
        difficulty_scores,
    );
    let head: Vec<usize> = edge_order.iter().copied().take(12).collect();
    crate::perf_log!(
        "[edge-order] n={} scores={} head={:?}",
        relations.len(),
        difficulty_scores.is_some(),
        head
    );
    crate::perf_log!(
        "[perf]     step2_slots+step3_order: {:.2}ms",
        t2.elapsed().as_secs_f64() * 1000.0
    );
    (edge_order, feedback_edge_set)
}

/// Phase 4f (X-3)：Lane Assignment 车道分配
///
/// 对 bundling 无法合并的残余平行段，通过平移 cross-axis 坐标分离重合段。
/// 不插入 Z 字弯，保持正交性。
/// Slice C3.2：solve 从 `paths` 读取，返回 `Vec<LaneAssignment>` 供 RouteSolution 携带。
#[allow(clippy::too_many_arguments)]
pub(crate) fn phase_lane(
    paths: &[crate::layout::routing::model::solution::RoutePath],
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    parallel_gap: f64,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    profile: &OrthoRoutingProfile,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
) -> Vec<crate::layout::routing::model::solution::LaneAssignment> {
    let t_lane = crate::layout::perf::Instant::now();
    let (lane_assignments, lane_stats) = assign_lanes(
        paths,
        edges,
        grid,
        nodes,
        sorted_node_ids,
        relations,
        from_side,
        to_side,
        parallel_gap,
    );
    ortho_stats.lane_groups = lane_stats.lane_groups;
    ortho_stats.lane_segments_shifted = lane_stats.segments_shifted;
    ortho_stats.lane_shifts_failed = lane_stats.shifts_failed;
    if profile.corridor_lane_offsets {
        let corridor_shifted = apply_corridor_planned_offsets(
            edges,
            grid,
            nodes,
            sorted_node_ids,
            relations,
            from_side,
            to_side,
            corridor_plan,
            group_ctx,
        );
        ortho_stats.lane_segments_shifted += corridor_shifted;
        if profile.separate_unrelated_trunks {
            for _ in 0..2 {
                let shifted = separate_unrelated_trunk_overlaps(
                    edges,
                    Some(grid),
                    relations,
                    from_side,
                    to_side,
                    nodes,
                    sorted_node_ids,
                    parallel_gap,
                    profile,
                );
                ortho_stats.lane_segments_shifted += shifted;
                if shifted == 0 {
                    break;
                }
            }
        }
    }
    // S2-5：C 期 min_gap 已移除（2 点直连正反向对不触发节点反馈）；pipeline D 末作为最终写者重做。

    // S1：同侧 stub 占用。architecture（semantic_merge）C 期仅诊断，避免改边反馈
    // space-budget 动节点；exact 跨对共柱改在 pipeline D 末端
    // `resolve_exact_stub_occupancy_post_route`（保 node_fp）。flowchart 仍在此真修。
    let records = collect_stub_occupancy(edges, relations, from_side, to_side);
    let conflicts = find_stub_occupancy_conflicts(&records, relations, parallel_gap);
    ortho_stats.stub_occupancy_conflicts = conflicts.len();
    ortho_stats.stub_cross_pair_conflicts = conflicts.iter().filter(|c| !c.reverse_pair).count();
    if !profile.semantic_merge {
        let stub_stats = resolve_stub_occupancy_conflicts(
            edges,
            relations,
            from_side,
            to_side,
            nodes,
            parallel_gap,
        );
        ortho_stats.stub_occupancy_shifted = stub_stats.stubs_shifted;
        ortho_stats.stub_occupancy_degraded = stub_stats.degraded;
        // resolve 内部会重算 before；用其 shifted 覆盖 conflicts 已写入的值
        ortho_stats.stub_occupancy_conflicts = stub_stats.conflict_pairs_before;
        ortho_stats.stub_cross_pair_conflicts = stub_stats.cross_pair_conflicts_before;
        if stub_stats.stubs_shifted > 0 {
            let touched: Vec<usize> = (0..edges.len()).collect();
            grid.remove_by_edges(&touched);
            for ei in 0..edges.len() {
                if edges[ei].path_is_empty() {
                    continue;
                }
                let pts: Vec<Point> = edges[ei].path_points().into_owned();
                grid.insert_path(&pts, ei);
            }
        }
        crate::perf_log!(
            "[perf]     x3_lane_assignment: {:.2}ms ({} groups, {} shifted, {} failed); stub_occ conflicts={} shifted={} degraded={}",
            t_lane.elapsed().as_secs_f64() * 1000.0,
            lane_stats.lane_groups,
            lane_stats.segments_shifted,
            lane_stats.shifts_failed,
            stub_stats.conflict_pairs_before,
            stub_stats.stubs_shifted,
            stub_stats.degraded
        );
    } else {
        crate::perf_log!(
            "[perf]     x3_lane_assignment: {:.2}ms ({} groups, {} shifted, {} failed); stub_occ conflicts={} (arch diagnose-only)",
            t_lane.elapsed().as_secs_f64() * 1000.0,
            lane_stats.lane_groups,
            lane_stats.segments_shifted,
            lane_stats.shifts_failed,
            conflicts.len()
        );
    }

    // 轨道 A：正反向同侧 dock 共锚分离（落点最终写者；在 stub_occ 之后）
    // 注：C 期运行（节点未冻结），其边改动会经反馈影响 node 定位（S2-5 实验证实不可删）。
    let dock_gap = parallel_gap.max(COMPACT_SLOT_PITCH);
    let dock_fixed =
        enforce_reverse_pair_dock_separation(edges, relations, nodes, from_side, to_side, dock_gap);
    if dock_fixed > 0 {
        ortho_stats.lane_segments_shifted += dock_fixed;
        let touched: Vec<usize> = (0..edges.len()).collect();
        grid.remove_by_edges(&touched);
        for ei in 0..edges.len() {
            if edges[ei].path_is_empty() {
                continue;
            }
            let pts: Vec<Point> = edges[ei].path_points().into_owned();
            grid.insert_path(&pts, ei);
        }
    }

    lane_assignments
}
