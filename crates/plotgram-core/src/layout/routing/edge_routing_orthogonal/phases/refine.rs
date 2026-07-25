//! 重路由 / stub / lane 精修
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构，行为不变）。
//! Phase 0：已删除死代码 `phase_straighten_align`（整模块 straighten 已删）。

use super::super::*;
use std::collections::HashMap;

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

    // Phase 0（策略 B）：C 段 stub 仅诊断；真修统一迁至 D 段
    // `resolve_exact_stub_occupancy_post_route`（全正交图）。
    let records = collect_stub_occupancy(edges, relations, from_side, to_side);
    let conflicts = find_stub_occupancy_conflicts(&records, relations, parallel_gap);
    ortho_stats.stub_occupancy_conflicts = conflicts.len();
    ortho_stats.stub_cross_pair_conflicts = conflicts.iter().filter(|c| !c.reverse_pair).count();
    crate::perf_log!(
        "[perf]     x3_lane_assignment: {:.2}ms ({} groups, {} shifted, {} failed); stub_occ conflicts={} (C diagnose-only)",
        t_lane.elapsed().as_secs_f64() * 1000.0,
        lane_stats.lane_groups,
        lane_stats.segments_shifted,
        lane_stats.shifts_failed,
        conflicts.len()
    );

    // Phase 0（策略 B）：C 段 dock 分离已删；D 段 coordinator 唯一收口（接受 S2-5 节点反馈变化）。

    lane_assignments
}
