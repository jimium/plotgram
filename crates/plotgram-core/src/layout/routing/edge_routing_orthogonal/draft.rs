//! `OrthogonalDraft`：正交路由的确定性「事实编译」阶段（Slice 4 / R4）。
//!
//! 把 `run.rs` 求解流水线（`phase_route_edges` 及其后续）**入口之前**自行准备的所有事实
//! 上移到 `OrthogonalDraft::compile`，run.rs 只消费 Draft 而不再自行编排事实准备。
//!
//! 严格行为不变：本次仅做结构重构，字段计算逻辑与抽取前逐行一致，
//! 6 家族渲染 SVG 字节级一致。算法改进留到后续 Slice。
//!
//! # 边界
//! - **compile（本文件）**：`run.rs` 原 L44–L192 的确定性事实——profile / group 上下文 /
//!   障碍物 / 端口选择 / slot 分配 / 边序 / 通道与 OVG 规划。仅有的副作用是写
//!   `result.hints.group_routing`（与抽取前同处发生）。
//! - **solve（留在 run.rs）**：`edges` / `grid` / `ortho_stats` 等求解状态与
//!   `phase_route_edges` 起的所有 mutating 阶段。

use super::*;
use crate::ast::Diagram;
use crate::layout::routing::common::self_loop;
use crate::layout::routing::model::solution::choose_docking_strategy;
use crate::layout::routing::model::{EndpointAssignment, StableEdgeId};
use crate::layout::LayoutResult;
use std::collections::HashMap;



/// 正交路由求解前的全部确定性事实。
///
/// 由 [`OrthogonalDraft::compile`] 从 `Diagram` + `LayoutResult` 确定性编译得到；
/// `run.rs` 解构本结构后进入求解阶段。字段可独立单测（roles/ports/obstacles/edge_order 确定性）。
pub(super) struct OrthogonalDraft {
    /// 图类型路由画像（parallel_gap / semantic_merge 等）。
    pub(super) profile: OrthoRoutingProfile,
    /// 平行边间距（`profile.parallel_gap` 快照）。
    pub(super) parallel_gap: f64,
    /// S4 监控走廊门控（仅无分组 architecture 等语义合并场景启用）。
    pub(super) s4_monitor_corridor: bool,
    /// 有效方向是否为 left-to-right。
    pub(super) horizontal: bool,
    /// 自环边映射 `edge_idx -> loop_slot`。
    pub(super) self_loop_idx: HashMap<usize, usize>,
    /// 分组路由上下文（groups / node_to_groups / corridors）。
    pub(super) group_ctx: crate::layout::group::GroupRoutingContext,
    /// 预排序的节点/分组障碍物。
    pub(super) obstacles: PreparedObstacles,
    /// 走廊路由规划（分组穿越链）。
    pub(super) corridor_plan: corridor_route::CorridorRoutePlan,
    /// 走廊需求模型（供难度评分与逐边路由参考）。
    pub(super) corridor_model: crate::layout::demand::CorridorModel,
    /// 每条边的起点端口（phase_port_correction 会最终修正）。
    pub(super) from_side: Vec<Port>,
    /// 每条边的终点端口（phase_port_correction 会最终修正）。
    pub(super) to_side: Vec<Port>,
    /// 端点几何映射 `(edge_idx, is_from) -> Endpoint`（phase_port_correction 最终写者）。
    pub(super) endpoint_map: HashMap<(usize, bool), Endpoint>,
    /// 平行边分组与偏移。
    pub(super) parallel: crate::layout::routing::common::parallel_edges::ParallelGroups,
    /// 反向平行边对（用于 dock 分离）。
    pub(super) reverse_pairs: std::collections::BTreeSet<String>,
    /// 逐边路由顺序（低层先占通道；feedback 全局延后）。
    pub(super) edge_order: Vec<usize>,
    /// feedback 边集合（全局延后路由）。
    pub(super) feedback_edge_set: std::collections::HashSet<usize>,
    /// 全局通道规划（默认关闭，`PLOTGRAM_CHANNEL_PLANNER=1` 启用）。
    pub(super) channel_plan: Option<channel_planner::ChannelPlan>,
    /// 是否启用粗→精两轮路由（边数超阈值）。
    pub(super) use_two_round: bool,
    /// 分组矩形（供局部 OVG / 两轮路由使用）。
    pub(super) group_rects: Vec<crate::layout::geometry::Rect>,
    /// 全局 OVG（仅当启用且非延迟/非两轮时构建）。
    pub(super) ovg: Option<visibility_graph::OrthogonalVisibilityGraph>,
    /// Slice C1：统一端口分配（side + slot + anchor + capacity + protected stub）。
    pub(super) endpoint_assignments: Vec<EndpointAssignment>,
}

impl OrthogonalDraft {
    /// 从 `Diagram` + `LayoutResult` 确定性编译正交路由事实。
    ///
    /// 唯一副作用：写 `result.hints.group_routing`（与抽取前同处发生，保持字节不变）。
    pub(super) fn compile(
        diagram: &Diagram,
        result: &mut LayoutResult,
        cfg: &OrthoConfig,
    ) -> OrthogonalDraft {
        let relations = &diagram.relations;
        let n = relations.len();
        let self_loop_idx = self_loop::self_loop_indices(relations);
        let profile = OrthoRoutingProfile::for_diagram_type(diagram.diagram_type.clone());
        let parallel_gap = profile.parallel_gap;
        // S4：与 S3 同门控——仅无分组 architecture；有组大图强制外环/监控延后会抬 high tight/穿模
        let s4_monitor_corridor = profile.semantic_merge && diagram.groups.is_empty();

        let routing_algo = crate::layout::group::routing_algo_for_diagram(diagram);
        let group_ctx =
            crate::layout::group::GroupRoutingContext::from_layout(diagram, &*result, routing_algo);
        let mut group_routing = group_ctx.routing_hints();
        if let Some(existing) = &result.hints.group_routing {
            if !existing.side_gutters.is_empty() {
                group_routing.side_gutters = existing.side_gutters.clone();
            }
        }
        result.hints.group_routing = Some(group_routing);

        // 预排序节点/分组 ID，避免路由循环内重复排序（方案 2）
        let obstacles = PreparedObstacles::build(&result.nodes, &group_ctx);

        // B 族（ISS-003）：构建非矩形形状的轮廓多边形映射，用于锚点吸附
        let shape_polygons = shape_boundary::build_shape_polygons(&diagram.entities, &result.nodes);

        let horizontal =
            crate::layout::resolve_effective_direction(diagram) == Some("left-to-right");
        let feedback_assignment = feedback_side::assign_feedback_sides(
            diagram,
            relations,
            &result.nodes,
            result.hints.sugiyama_ranks.as_ref(),
            horizontal,
        );

        let corridor_plan = corridor_route::plan_corridor_routes(relations, &group_ctx, &profile);
        let corridor_model = crate::layout::demand::compute_corridor_model(diagram, &*result);

        // Phase 2：入口算一次边难度，注入边序（高分同层略提前）
        let difficulty_scores = if cfg.routing.edge_order_score {
            let features = crate::layout::demand::collect_edge_features(
                diagram,
                &*result,
                Some(&corridor_model),
            );
            let ranked = crate::layout::demand::score_edges(
                &features,
                &crate::layout::demand::DifficultyProfile::default(),
            );
            let mut dense = vec![0.0_f64; n];
            for (idx, score) in ranked {
                if let Some(slot) = dense.get_mut(idx) {
                    *slot = score;
                }
            }
            Some(dense)
        } else {
            None
        };

        // ── 1+2. 端口选择 + slot 分配 + 平行边偏移 ──
        let (from_side, to_side, _lane, endpoint_map, parallel, reverse_pairs) = phase_port_slot(
            relations,
            &result.nodes,
            &group_ctx,
            &feedback_assignment,
            cfg,
            n,
            s4_monitor_corridor,
            horizontal,
            &shape_polygons,
        );

        // ── 3. 分层批量边序（有 rank 时低层先占通道；feedback 全局延后） ──
        let (edge_order, feedback_edge_set) = phase_layer_order(
            relations,
            result.hints.sugiyama_ranks.as_ref(),
            &feedback_assignment,
            s4_monitor_corridor,
            difficulty_scores.as_deref(),
        );

        // Phase B3：全局通道规划（默认关闭）
        let channel_plan = if cfg.routing.channel_planner {
            let mut plan = channel_planner::plan_channels(
                relations,
                &result.nodes,
                &group_ctx,
                &edge_order,
            );
            // P2-1: 为同通道多边分配精确 lane 偏移
            channel_planner::assign_lane_offsets(&mut plan, profile.parallel_gap);
            Some(plan)
        } else {
            None
        };

        // Phase B1：构建 OVG（仅当环境变量启用时）——节点障碍物阻断 + 组软惩罚
        // 节点膨胀使用 NODE_OBSTACLE_PAD + 10，给路径更多 clearance，减少 tight 违规
        // R6：大图（>50 节点且非两轮）跳过全局 OVG 构建——由 PathAssignmentSolver 在
        // 冲突邻域按需构建局部 OVG（内部策略），避免大图全局 OVG 的构建开销。
        // P3-1: 两轮路由阈值——边数超过此值时启用粗→精两轮
        const OVG_DEFERRED_THRESHOLD: usize = 50;
        const TWO_ROUND_THRESHOLD: usize = 40;
        let use_two_round = n > TWO_ROUND_THRESHOLD;
        let group_rects: Vec<crate::layout::geometry::Rect> = obstacles
            .sorted_group_ids
            .iter()
            .filter_map(|gid| group_ctx.groups.get(gid))
            .map(|gl| crate::layout::geometry::Rect::from(gl))
            .collect();
        let ovg = if cfg.routing.ovg_enabled
            && (result.nodes.len() <= OVG_DEFERRED_THRESHOLD || use_two_round)
        {
            let ovg_graph = visibility_graph::build_ovg_with_groups(
                &result.nodes,
                &obstacles.sorted_node_ids,
                NODE_OBSTACLE_PAD + 10.0,
                &group_rects,
            );
            if ovg_graph.is_empty() {
                None
            } else {
                Some(ovg_graph)
            }
        } else {
            None
        };

        // ── Slice C1：构建统一 EndpointAssignment ──
        let endpoint_assignments = build_endpoint_assignments(
            n,
            &from_side,
            &to_side,
            &endpoint_map,
            &feedback_edge_set,
        );

        OrthogonalDraft {
            profile,
            parallel_gap,
            s4_monitor_corridor,
            horizontal,
            self_loop_idx,
            group_ctx,
            obstacles,
            corridor_plan,
            corridor_model,
            from_side,
            to_side,
            endpoint_map,
            parallel,
            reverse_pairs,
            edge_order,
            feedback_edge_set,
            channel_plan,
            use_two_round,
            group_rects,
            ovg,
            endpoint_assignments,
        }
    }
}

/// Slice C1：从既有 port/slot 结果构建统一 `Vec<EndpointAssignment>`。
///
/// 按 (node_id, side) 分组统计容量和 slot 下标，确定性排序（BTreeMap）。
fn build_endpoint_assignments(
    n: usize,
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    feedback_edge_set: &std::collections::HashSet<usize>,
) -> Vec<EndpointAssignment> {
    use std::collections::BTreeMap;

    // 按 (node_id, side) 分组统计容量
    let mut side_counts: BTreeMap<(String, Port), usize> = BTreeMap::new();
    for i in 0..n {
        if let Some(ep) = endpoint_map.get(&(i, true)) {
            *side_counts.entry((ep.node_id.clone(), ep.side)).or_default() += 1;
        }
        if let Some(ep) = endpoint_map.get(&(i, false)) {
            *side_counts.entry((ep.node_id.clone(), ep.side)).or_default() += 1;
        }
    }

    // 按 (node_id, side) 分组统计 slot 下标（按 anchor 切线坐标排序）
    let mut side_members: BTreeMap<(String, Port), Vec<(usize, bool)>> = BTreeMap::new();
    for i in 0..n {
        if let Some(ep) = endpoint_map.get(&(i, true)) {
            side_members.entry((ep.node_id.clone(), ep.side)).or_default().push((i, true));
        }
        if let Some(ep) = endpoint_map.get(&(i, false)) {
            side_members.entry((ep.node_id.clone(), ep.side)).or_default().push((i, false));
        }
    }
    // 排序：垂直端口按 anchor.x，水平端口按 anchor.y
    let mut slot_index_map: HashMap<(usize, bool), u16> = HashMap::new();
    for ((_, side), members) in side_members.iter_mut() {
        let vertical = is_vertical_port(*side);
        members.sort_by(|&(ai, af), &(bi, bf)| {
            let a = endpoint_map.get(&(ai, af)).map(|e| if vertical { e.anchor.x } else { e.anchor.y }).unwrap_or(0.0);
            let b = endpoint_map.get(&(bi, bf)).map(|e| if vertical { e.anchor.x } else { e.anchor.y }).unwrap_or(0.0);
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
                .then(ai.cmp(&bi))
                .then(af.cmp(&bf))
        });
        for (rank, &(ei, is_from)) in members.iter().enumerate() {
            slot_index_map.insert((ei, is_from), rank as u16);
        }
    }

    (0..n)
        .map(|i| {
            let from_anchor = endpoint_map.get(&(i, true)).map(|e| e.anchor).unwrap_or_else(crate::layout::geometry::Point::zero);
            let to_anchor = endpoint_map.get(&(i, false)).map(|e| e.anchor).unwrap_or_else(crate::layout::geometry::Point::zero);
            let from_slot_index = slot_index_map.get(&(i, true)).copied().unwrap_or(0);
            let to_slot_index = slot_index_map.get(&(i, false)).copied().unwrap_or(0);

            let from_capacity = endpoint_map
                .get(&(i, true))
                .and_then(|ep| side_counts.get(&(ep.node_id.clone(), ep.side)))
                .copied()
                .unwrap_or(1) as u8;
            let to_capacity = endpoint_map
                .get(&(i, false))
                .and_then(|ep| side_counts.get(&(ep.node_id.clone(), ep.side)))
                .copied()
                .unwrap_or(1) as u8;

            EndpointAssignment {
                edge: StableEdgeId(i),
                from_port: from_side[i],
                to_port: to_side[i],
                from_anchor,
                to_anchor,
                from_slot_index,
                to_slot_index,
                from_side_capacity: from_capacity,
                to_side_capacity: to_capacity,
                protected_stub: feedback_edge_set.contains(&i),
                docking: choose_docking_strategy(from_capacity.max(to_capacity) as usize),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::routing::common::test_fixtures::make_diagram_grid;

    /// 把一个 Draft 归约为可比较的确定性指纹（覆盖 R4 判据：roles/ports/obstacles/edge_order）。
    fn fingerprint(
        d: &OrthogonalDraft,
    ) -> (
        Vec<Port>,
        Vec<Port>,
        Vec<usize>,
        Vec<String>,
        Vec<String>,
        Vec<usize>,
        Vec<((usize, bool), Port, u64, u64)>,
    ) {
        // endpoint_map: 显式排序 key，anchor 用 to_bits 避免 f64 直接比较。
        let mut ep: Vec<((usize, bool), Port, u64, u64)> = d
            .endpoint_map
            .iter()
            .map(|(k, e)| (*k, e.side, e.anchor.x.to_bits(), e.anchor.y.to_bits()))
            .collect();
        ep.sort_by_key(|(k, _, _, _)| *k);

        let mut self_loops: Vec<usize> = d.self_loop_idx.keys().copied().collect();
        self_loops.sort_unstable();

        (
            d.from_side.clone(),
            d.to_side.clone(),
            d.edge_order.clone(),
            d.obstacles.sorted_node_ids.clone(),
            d.obstacles.sorted_group_ids.clone(),
            self_loops,
            ep,
        )
    }

    /// compile 对同一输入两次运行产出逐位一致的事实（确定性；禁 HashMap 迭代序泄漏）。
    #[test]
    fn compile_is_deterministic() {
        let cfg = OrthoConfig::from_spec_defaults();
        let (diagram, base) = make_diagram_grid(3, 3);

        let mut r1 = base.clone();
        let d1 = OrthogonalDraft::compile(&diagram, &mut r1, &cfg);
        let mut r2 = base.clone();
        let d2 = OrthogonalDraft::compile(&diagram, &mut r2, &cfg);

        assert_eq!(fingerprint(&d1), fingerprint(&d2));
    }

    /// compile 编译出的核心事实结构自洽（roles/ports/obstacles/edge_order 覆盖全边）。
    #[test]
    fn compile_produces_consistent_facts() {
        let cfg = OrthoConfig::from_spec_defaults();
        let (diagram, mut result) = make_diagram_grid(3, 3);
        let n = diagram.relations.len();

        let d = OrthogonalDraft::compile(&diagram, &mut result, &cfg);

        // 每条边都有起/终端口选择。
        assert_eq!(d.from_side.len(), n);
        assert_eq!(d.to_side.len(), n);
        // edge_order 是 0..n 的一个排列。
        assert_eq!(d.edge_order.len(), n);
        let mut sorted = d.edge_order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..n).collect::<Vec<_>>());
        // 障碍物节点 ID 已确定性排序。
        let mut sorted_nodes = d.obstacles.sorted_node_ids.clone();
        sorted_nodes.sort();
        assert_eq!(sorted_nodes, d.obstacles.sorted_node_ids);
        // compile 写入了 group_routing hint（唯一副作用）。
        assert!(result.hints.group_routing.is_some());
    }
}
