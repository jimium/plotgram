//! `PathAssignmentSolver`：R6 单一路径求解主流程（Slice 6b）。
//!
//! 取代原本三套并行的重路由控制流——two-round（拥堵检测 + 精路由）、deferred
//! OVG（大图 degraded 边局部 OVG 重路由）、conflict reroute——收敛为一条主流程：
//!
//! 1. **initial shortest path**：由 `phase_route_edges` 逐边求得（solver 之前已执行）。
//! 2. **conflict graph**：把 congestion（[`detect_congestion`]）、obstacle crossing
//!    （穿越非自身节点障碍）、spacing violation（[`collect_spacing_conflicts`]）三类
//!    冲突统一成一套带 [`DegradedReason`] 的冲突集合。
//! 3. **bounded rip-up/reroute**：固定轮次（[`MAX_REROUTE_ROUNDS`]），每轮对冲突边
//!    rip-up 后用递增 margin 经 [`find_clean_reroute_path`] 重路由；大图在冲突邻域
//!    按需构建局部 OVG（[`build_local_ovg`]）作为**内部策略**，而非独立控制流。
//! 4. **best global score**：每轮记录全局冲突分，取轮次间最优（重路由只接受洁净
//!    路径，故过程单调不劣化，末轮即最优；仍显式择优以对齐 doc §20 判据）。
//! 5. **degraded typed reason**：无洁净路径时记录 [`DegradedReason`]（对齐 doc §4.5，
//!    先落内部枚举，不要求序列化）。
//!
//! # 边界（R6）
//! - R6：单一主流程为默认且唯一控制流，由 `run.rs` 无条件调用（无 flag）。
//! - 每条候选路径仍过硬验证（节点/分组/边间距）——穿组硬门禁由
//!   `find_clean_reroute_path` 内部的 `path_is_clean` + `path_avoids_group_interiors`
//!   + `path_is_clean_from_edges` 保证，不下放。
//! - 确定性：冲突集合以 `BTreeMap` 收敛并按 `(reason, edge_index)` 稳定排序；不依赖
//!   `HashMap` 迭代序。

use super::run::detect_congestion;
use super::visibility_graph::{build_local_ovg, OrthogonalVisibilityGraph};
use super::*;
use crate::layout::geometry::Rect;
use crate::layout::routing::common::parallel_edges::build_parallel_aware_edge_labels_auto;
use crate::layout::routing::model::solution::{DegradedReason, EndpointAssignment, RoutePath};
use std::collections::{BTreeMap, HashMap, HashSet};

/// 多轮 rip-up/reroute 上限。
const MAX_REROUTE_ROUNDS: usize = 3;
/// 拥堵桶阈值（对齐旧 two-round `detect_congestion(&edges, 3)`）。
const CONGESTION_THRESHOLD: usize = 3;
/// 局部 OVG 覆盖冲突区域的外扩边距（对齐旧 two-round / deferred OVG 的 80px）。
const LOCAL_OVG_MARGIN: f64 = 80.0;

/// 收集所有存在间距违规的边索引及其违规数。
///
/// 跳过空路径边和已标记失败的边。stub 段在 `path_edge_spacing_violations`
/// 内部已豁免。
fn collect_spacing_conflicts(
    paths: &[RoutePath],
    grid: &SegmentGrid,
    parallel_gap: f64,
    failed_edges: &HashSet<usize>,
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
/// 递增 `channel_margin` 调用 `select_best_path_with_scorer_stats`（默认 LexA*）。
/// 返回第一条通过节点/分组/边间距硬检查的路径；若全部失败返回 `None`。
#[allow(clippy::too_many_arguments)]
fn find_clean_reroute_path(
    from_ep: &Endpoint,
    to_ep: &Endpoint,
    cfg: &OrthoConfig,
    reroute_margins: &[f64],
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    grid: &SegmentGrid,
    profile: &OrthoRoutingProfile,
    obstacles: &PreparedObstacles,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    parallel_gap: f64,
    ovg: Option<&OrthogonalVisibilityGraph>,
) -> Option<Vec<Point>> {
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
        )
        .with_strict_group_transit(should_strict_group_transit(false))
        .with_corridor_boost(boost)
        .with_prefer_outer_ring(false);
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
            return Some(candidate);
        }
    }

    None
}

/// 冲突原因的处理优先级（越小越先重路由）：先修硬穿障，再修间距，最后拥堵。
fn reason_rank(reason: DegradedReason) -> u8 {
    match reason {
        DegradedReason::ObstacleCrossing => 0,
        DegradedReason::NoCleanPath => 0,
        DegradedReason::SpacingViolation => 1,
        DegradedReason::Congestion => 2,
        _ => 3,
    }
}

/// 统一冲突检测：congestion ∪ obstacle crossing ∪ spacing，返回按 `edge_index`
/// 升序的 `(edge_index, reason)`；同边多因取最强信号（穿障 > 间距 > 拥堵）。
///
/// `include_congestion` 关闭时不把纯拥堵边计入冲突：已有全图 OVG 时初始路径已
/// 绕障良好，仅因拥堵桶重路由会徒增绕行/间距而无收益（对齐旧 two-round 仅在
/// 无 OVG 首轮做拥堵精路由的语义）。跳过 `failed` 边。以 `BTreeMap` 收敛保证确定性。
fn detect_conflicts(
    paths: &[RoutePath],
    grid: &SegmentGrid,
    parallel_gap: f64,
    obstacles: &PreparedObstacles,
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    failed: &HashSet<usize>,
    include_congestion: bool,
) -> Vec<(usize, DegradedReason)> {
    let mut reasons: BTreeMap<usize, DegradedReason> = BTreeMap::new();

    // 拥堵（最弱信号，优先被后续更强信号覆盖）
    if include_congestion {
        for ei in detect_congestion(paths, CONGESTION_THRESHOLD) {
            reasons.entry(ei).or_insert(DegradedReason::Congestion);
        }
    }

    // 间距违规（覆盖拥堵）
    for (ei, _) in collect_spacing_conflicts(paths, grid, parallel_gap, failed) {
        reasons.insert(ei, DegradedReason::SpacingViolation);
    }

    // 穿越非自身节点障碍（最强信号，覆盖前两者）
    let node_pad = NODE_OBSTACLE_PAD + 10.0;
    let node_obstacles: Vec<(usize, Rect)> = obstacles
        .sorted_node_ids
        .iter()
        .enumerate()
        .filter_map(|(idx, id)| {
            nodes
                .get(id)
                .map(|nl| (idx, Rect::new(nl.x, nl.y, nl.width, nl.height).expanded(node_pad)))
        })
        .collect();
    for i in 0..paths.len() {
        if paths[i].is_empty() || failed.contains(&i) {
            continue;
        }
        let Some(rel) = relations.get(i) else {
            continue;
        };
        let from_obs = obstacles.sorted_node_ids.iter().position(|id| id == rel.from.as_str());
        let to_obs = obstacles.sorted_node_ids.iter().position(|id| id == rel.to.as_str());
        let pts = paths[i].points();
        let crosses = node_obstacles.iter().any(|(oi, rect)| {
            if Some(*oi) == from_obs || Some(*oi) == to_obs {
                return false;
            }
            pts.windows(2)
                .any(|w| rect.segment_crosses_interior(w[0], w[1], 0.1))
        });
        if crosses {
            reasons.insert(i, DegradedReason::ObstacleCrossing);
        }
    }

    reasons
        .into_iter()
        .filter(|(ei, _)| !failed.contains(ei))
        .collect()
}

/// 全局冲突分：所有边的间距违规段计数之和（越小越优）。
///
/// 仅用于调试/统计观测。因 [`find_clean_reroute_path`] 只接受洁净路径，重路由过程
/// 单调不劣化，末轮解即全局最优（best-global == 末轮），故主流程无需快照回退。
#[cfg(test)]
fn global_score(edges: &[EdgeLayout], grid: &SegmentGrid, parallel_gap: f64) -> usize {
    let mut score = 0usize;
    for edge in edges.iter() {
        if edge.path_is_empty() {
            continue;
        }
        let pts: Vec<Point> = edge.path_points().into_owned();
        score += path_edge_spacing_violations(&pts, grid, parallel_gap).len();
    }
    score
}

/// R6 单一主流程：对已初始路由的 `paths` 做统一冲突检测 + bounded rip-up/reroute
/// （单调不劣化，末轮即最优）。
///
/// 返回仍降级的边及其类型化原因（`(edge_index, DegradedReason)`，按边序）。
#[allow(clippy::too_many_arguments)]
pub(super) fn solve_paths(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    endpoint_assignments: &mut [EndpointAssignment],
    paths: &mut Vec<RoutePath>,
    labels: &mut Vec<Vec<crate::layout::EdgeLabelLayout>>,
    grid: &mut SegmentGrid,
    cfg: &OrthoConfig,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    ovg: Option<&OrthogonalVisibilityGraph>,
    group_rects: &[Rect],
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    profile: &OrthoRoutingProfile,
    parallel_gap: f64,
) -> Vec<(usize, DegradedReason)> {
    let n = paths.len();
    if n < 2 {
        return Vec::new();
    }

    // 重路由 margin 档位：逐步增大生成更多绕行候选。
    let reroute_margins: [f64; 3] = [
        cfg.channel_margin + 10.0,
        cfg.channel_margin + 25.0,
        cfg.channel_margin + 40.0,
    ];

    // ── 2. 冲突邻域局部 OVG（内部策略）──
    // 有全图 OVG 直接复用；否则在初始冲突邻域按需构建局部 OVG。
    let no_fail: HashSet<usize> = HashSet::new();
    let local_ovg: Option<OrthogonalVisibilityGraph> = if ovg.is_some() {
        None
    } else {
        let initial = detect_conflicts(paths, grid, parallel_gap, obstacles, relations, nodes, &no_fail, true);
        let mut regions: Vec<(Point, Point)> = Vec::new();
        for &(ei, _) in &initial {
            if ei < endpoint_assignments.len() {
                let ea = &endpoint_assignments[ei];
                regions.push((ea.from_anchor, ea.to_anchor));
            }
        }
        if regions.is_empty() {
            None
        } else {
            let node_pad = NODE_OBSTACLE_PAD + 10.0;
            let local = build_local_ovg(
                &regions,
                nodes,
                &obstacles.sorted_node_ids,
                node_pad,
                group_rects,
                LOCAL_OVG_MARGIN,
            );
            if local.is_empty() {
                None
            } else {
                Some(local)
            }
        }
    };
    let ovg_ref: Option<&OrthogonalVisibilityGraph> = ovg.or(local_ovg.as_ref());

    // ── 3. bounded rip-up/reroute（单调不劣化：只接受洁净路径）──
    // 拥堵仅在无全图 OVG（走局部 OVG）时作为冲突源，避免在已绕障良好的 OVG 图上
    // 因拥堵徒增绕行/间距（对齐旧 two-round 语义）。
    let include_congestion = ovg.is_none();
    let mut failed: HashSet<usize> = HashSet::new();
    let mut degraded: BTreeMap<usize, DegradedReason> = BTreeMap::new();
    let mut rounds_done = 0usize;
    let mut total_rerouted = 0usize;
    let mut max_channel_load = 0usize;

    for round in 0..MAX_REROUTE_ROUNDS {
        let mut conflicts =
            detect_conflicts(paths, grid, parallel_gap, obstacles, relations, nodes, &failed, include_congestion);
        if conflicts.is_empty() {
            break;
        }
        rounds_done = round + 1;
        // 稳定序：穿障优先，其次间距、拥堵，再按 edge_index。
        conflicts.sort_by(|a, b| {
            reason_rank(a.1)
                .cmp(&reason_rank(b.1))
                .then(a.0.cmp(&b.0))
        });

        let load_map = ChannelLoadMap::build(paths, crate::layout::constants::GRID_SNAP_STEP);
        max_channel_load = max_channel_load.max(load_map.max_load());

        for &(ei, reason) in &conflicts {
            if failed.contains(&ei) {
                continue;
            }
            if ei >= endpoint_assignments.len() {
                continue;
            }
            let ea = &endpoint_assignments[ei];
            let (from_id, to_id) = relations
                .get(ei)
                .map(|rel| (rel.from.as_str(), rel.to.as_str()))
                .unwrap_or(("", ""));
            let from_ep = ea.project_endpoint(true, from_id.to_string(), Point::zero());
            let to_ep = ea.project_endpoint(false, to_id.to_string(), Point::zero());

            // 先移除旧段，避免自身旧路径造成假冲突 / 幽灵段。
            grid.remove_by_edges(&[ei]);
            let old_points: Vec<Point> = paths[ei].points().to_vec();

            let mut clean_path = find_clean_reroute_path(
                &from_ep,
                &to_ep,
                cfg,
                &reroute_margins,
                nodes,
                group_ctx,
                grid,
                profile,
                obstacles,
                ortho_stats,
                parallel_gap,
                ovg_ref,
            );

            // Phase 3 H4：同环内尝试端口候选（四向），再 LexA*。
            let mut chosen_ports: Option<(Port, Port, Point, Point)> = None;
            if clean_path.is_none() {
                if let (Some(from_nl), Some(to_nl)) = (nodes.get(from_id), nodes.get(to_id)) {
                    let ports = [
                        Port::Top,
                        Port::Right,
                        Port::Bottom,
                        Port::Left,
                    ];
                    let cur_fp = ea.from_port;
                    let cur_tp = ea.to_port;
                    // 确定性：端口序固定；当前端口已试过，跳过 (cur_fp, cur_tp)
                    for &fp in &ports {
                        for &tp in &ports {
                            if fp == cur_fp && tp == cur_tp {
                                continue;
                            }
                            let from_anchor = super::slot::slot_anchor(from_nl, fp, 0.5);
                            let to_anchor = super::slot::slot_anchor(to_nl, tp, 0.5);
                            let mut from_try = from_ep.clone();
                            from_try.side = fp;
                            from_try.anchor = from_anchor;
                            let mut to_try = to_ep.clone();
                            to_try.side = tp;
                            to_try.anchor = to_anchor;
                            if let Some(path) = find_clean_reroute_path(
                                &from_try,
                                &to_try,
                                cfg,
                                &reroute_margins,
                                nodes,
                                group_ctx,
                                grid,
                                profile,
                                obstacles,
                                ortho_stats,
                                parallel_gap,
                                ovg_ref,
                            ) {
                                clean_path = Some(path);
                                chosen_ports = Some((fp, tp, from_anchor, to_anchor));
                                break;
                            }
                        }
                        if clean_path.is_some() {
                            break;
                        }
                    }
                }
            }

            match clean_path {
                Some(path) => {
                    if let Some((fp, tp, fa, ta)) = chosen_ports {
                        endpoint_assignments[ei].from_port = fp;
                        endpoint_assignments[ei].to_port = tp;
                        endpoint_assignments[ei].from_anchor = fa;
                        endpoint_assignments[ei].to_anchor = ta;
                    }
                    labels[ei] = if path.len() >= 2 {
                        match relations.get(ei) {
                            Some(rel) => {
                                build_parallel_aware_edge_labels_auto(rel, ei, relations, &path)
                            }
                            None => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    };
                    grid.insert_path(&path, ei);
                    paths[ei] = RoutePath::orthogonal(path);
                    total_rerouted += 1;
                    degraded.remove(&ei);
                }
                None => {
                    // 无洁净路径：恢复原路径，标记失败，记录类型化降级原因。
                    grid.insert_path(&old_points, ei);
                    failed.insert(ei);
                    let typed = match reason {
                        DegradedReason::ObstacleCrossing => DegradedReason::ObstacleCrossing,
                        _ => DegradedReason::NoCleanPath,
                    };
                    degraded.insert(ei, typed);
                }
            }
        }
    }

    ortho_stats.reroute_iterations = rounds_done;
    ortho_stats.rerouted_edges = total_rerouted;
    ortho_stats.max_channel_load = ortho_stats.max_channel_load.max(max_channel_load);

    degraded.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vpath(x: f64, y0: f64, y1: f64) -> RoutePath {
        RoutePath::orthogonal(vec![Point::new(x, y0), Point::new(x, y1)])
    }

    fn empty_obstacles() -> PreparedObstacles {
        PreparedObstacles {
            sorted_node_ids: Vec::new(),
            sorted_group_ids: Vec::new(),
        }
    }

    /// 原因优先级：穿障/无洁净 < 间距 < 拥堵（越小越先处理）。
    #[test]
    fn reason_rank_orders_hard_conflicts_first() {
        assert!(reason_rank(DegradedReason::ObstacleCrossing) < reason_rank(DegradedReason::SpacingViolation));
        assert!(reason_rank(DegradedReason::SpacingViolation) < reason_rank(DegradedReason::Congestion));
        assert_eq!(
            reason_rank(DegradedReason::NoCleanPath),
            reason_rank(DegradedReason::ObstacleCrossing)
        );
    }

    /// 冲突检测对同一输入两次调用逐位一致（禁 HashMap 迭代序泄漏）。
    #[test]
    fn detect_conflicts_is_deterministic() {
        // 3 条重合垂直边（同 x 桶）→ 触发拥堵；关系为空 → 跳过穿障检测。
        let paths = vec![
            vpath(10.0, 0.0, 100.0),
            vpath(10.0, 0.0, 100.0),
            vpath(10.0, 0.0, 100.0),
        ];
        let mut grid = SegmentGrid::new();
        for (ei, p) in paths.iter().enumerate() {
            let pts = p.points().to_vec();
            grid.insert_path(&pts, ei);
        }
        let obs = empty_obstacles();
        let relations: Vec<crate::ast::Relation> = Vec::new();
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let failed = HashSet::new();

        let r1 = detect_conflicts(&paths, &grid, 6.0, &obs, &relations, &nodes, &failed, true);
        let r2 = detect_conflicts(&paths, &grid, 6.0, &obs, &relations, &nodes, &failed, true);
        assert_eq!(r1, r2);
        assert!(!r1.is_empty(), "3 条重合垂直边应被检出冲突");
        // 输出按 edge_index 升序。
        let idx: Vec<usize> = r1.iter().map(|(ei, _)| *ei).collect();
        let mut sorted = idx.clone();
        sorted.sort_unstable();
        assert_eq!(idx, sorted);
    }

    /// failed 边不出现在冲突集合中。
    #[test]
    fn detect_conflicts_skips_failed() {
        let paths = vec![
            vpath(10.0, 0.0, 100.0),
            vpath(10.0, 0.0, 100.0),
            vpath(10.0, 0.0, 100.0),
        ];
        let mut grid = SegmentGrid::new();
        for (ei, p) in paths.iter().enumerate() {
            let pts = p.points().to_vec();
            grid.insert_path(&pts, ei);
        }
        let obs = empty_obstacles();
        let relations: Vec<crate::ast::Relation> = Vec::new();
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let mut failed = HashSet::new();
        failed.insert(1usize);

        let r = detect_conflicts(&paths, &grid, 6.0, &obs, &relations, &nodes, &failed, true);
        assert!(r.iter().all(|(ei, _)| *ei != 1), "failed 边不应出现在冲突集合");
    }

    /// 空边集合全局分为 0。
    #[test]
    fn global_score_empty_is_zero() {
        let edges: Vec<EdgeLayout> = Vec::new();
        let grid = SegmentGrid::new();
        assert_eq!(global_score(&edges, &grid, 6.0), 0);
    }
}
