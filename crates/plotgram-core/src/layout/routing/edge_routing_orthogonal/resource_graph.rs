//! `ResourceGraph`：R6 务实内部资源统一层（Slice 6a）。
//!
//! 把原本分散的四套路由资源——障碍物（[`PreparedObstacles`]）、正交可见性图
//! （[`OrthogonalVisibilityGraph`]）、全局通道规划（[`ChannelPlan`]）、跨组走廊
//! （[`CorridorRoutePlan`] + [`CorridorModel`]）——收敛为一个**只读**视图，供后续
//! `PathAssignmentSolver`（Slice 6b）统一消费，取代四个独立参数。
//!
//! # 边界（Slice 6a）
//! - **纯只读适配层**：只持有对既有资源的不可变引用，不复制、不改写、不改变任何
//!   选路行为；关闭 flag 时行为与现状逐字节一致。
//! - 确定性：所有对外迭代 API 以稳定序返回（复用底层已排序 `Vec` / `BTreeMap`，
//!   或按 `edge_index` 显式排序）。内部对 `HashMap` 仅做点查（`get`/`contains_key`），
//!   不驱动迭代顺序。

use super::channel_planner::ChannelPlan;
use super::context::PreparedObstacles;
use super::corridor_route::CorridorRoutePlan;
use super::visibility_graph::OrthogonalVisibilityGraph;
use crate::layout::demand::CorridorModel;
use crate::layout::geometry::Rect;

/// R6 只读资源统一层。
///
/// 聚合四套路由资源的不可变引用，对上层提供确定性访问；本身不拥有数据，
/// 生命周期 `'a` 绑定到 `run.rs` 中解构自 `OrthogonalDraft` 的资源本地变量。
#[allow(dead_code)] // Slice 6a 脚手架：部分访问器供 Slice 6b PathAssignmentSolver 消费
pub struct ResourceGraph<'a> {
    obstacles: &'a PreparedObstacles,
    visibility: Option<&'a OrthogonalVisibilityGraph>,
    channels: Option<&'a ChannelPlan>,
    corridor_plan: &'a CorridorRoutePlan,
    corridor_model: &'a CorridorModel,
    /// 按 id 排序的分组矩形（供局部 OVG 构建 / 穿组软惩罚）。
    group_rects: &'a [Rect],
}

#[allow(dead_code)] // Slice 6a 脚手架：部分访问器供 Slice 6b PathAssignmentSolver 消费
impl<'a> ResourceGraph<'a> {
    /// 从四套资源的不可变引用组装统一视图。
    pub fn assemble(
        obstacles: &'a PreparedObstacles,
        visibility: Option<&'a OrthogonalVisibilityGraph>,
        channels: Option<&'a ChannelPlan>,
        corridor_plan: &'a CorridorRoutePlan,
        corridor_model: &'a CorridorModel,
        group_rects: &'a [Rect],
    ) -> Self {
        Self {
            obstacles,
            visibility,
            channels,
            corridor_plan,
            corridor_model,
            group_rects,
        }
    }

    // ── 障碍物视图（已排序，确定性）──

    /// 按 id 排序的节点障碍物 ID 列表。
    #[inline]
    pub fn node_obstacle_ids(&self) -> &'a [String] {
        &self.obstacles.sorted_node_ids
    }

    /// 按 id 排序的分组障碍物 ID 列表。
    #[inline]
    pub fn group_obstacle_ids(&self) -> &'a [String] {
        &self.obstacles.sorted_group_ids
    }

    /// 原始 [`PreparedObstacles`] 引用（供既有 API 直接消费）。
    #[inline]
    pub fn obstacles(&self) -> &'a PreparedObstacles {
        self.obstacles
    }

    /// 按 id 排序的分组矩形（供局部 OVG 构建）。
    #[inline]
    pub fn group_rects(&self) -> &'a [Rect] {
        self.group_rects
    }

    // ── 可见性视图 ──

    /// 正交可见性图（全图或延迟局部）；未启用 OVG 时为 `None`。
    #[inline]
    pub fn visibility(&self) -> Option<&'a OrthogonalVisibilityGraph> {
        self.visibility
    }

    // ── 通道视图 ──

    /// 指定边的规划通道 `(coord, is_vertical)`：优先取精确 lane 分配，
    /// 其次回退到通道中心。语义与 `run.rs` 原 `planned_ch` 计算一致。
    pub fn channel_for_edge(&self, edge_idx: usize) -> Option<(f64, bool)> {
        let cp = self.channels?;
        cp.lane_assignments
            .get(&edge_idx)
            .copied()
            .or_else(|| cp.channel_for_edge(edge_idx))
    }

    /// 指定边的规划通道坐标（cross-axis），供 `with_planned_channel` 注入。
    #[inline]
    pub fn planned_channel_coord(&self, edge_idx: usize) -> Option<f64> {
        self.channel_for_edge(edge_idx).map(|(coord, _)| coord)
    }

    /// 是否启用了全局通道规划。
    #[inline]
    pub fn has_channels(&self) -> bool {
        self.channels.is_some()
    }

    // ── 走廊视图 ──

    /// 指定边是否存在跨组走廊链。
    #[inline]
    pub fn has_corridor_chain(&self, edge_idx: usize) -> bool {
        self.corridor_plan.chains.contains_key(&edge_idx)
    }

    /// 指定边的走廊链（走廊索引序列）。
    #[inline]
    pub fn corridor_chain(&self, edge_idx: usize) -> Option<&'a [usize]> {
        self.corridor_plan
            .chains
            .get(&edge_idx)
            .map(|v| v.as_slice())
    }

    /// 原始走廊路由计划引用。
    #[inline]
    pub fn corridor_plan(&self) -> &'a CorridorRoutePlan {
        self.corridor_plan
    }

    /// 走廊需求模型引用（供难度评分 / 廊 over 软惩罚参考）。
    #[inline]
    pub fn corridor_model(&self) -> &'a CorridorModel {
        self.corridor_model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_obstacles() -> PreparedObstacles {
        PreparedObstacles {
            sorted_node_ids: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            sorted_group_ids: vec!["g1".to_string(), "g2".to_string()],
        }
    }

    fn make_channels() -> ChannelPlan {
        let mut cp = ChannelPlan::default();
        // 通道中心（key = coord * 100）：垂直通道 1000 → x=10.0，覆盖边 0/2。
        cp.vertical_channels.insert(1000, vec![0, 2]);
        cp.horizontal_channels.insert(500, vec![1]);
        // 边 0 有精确 lane（应优先于通道中心）。
        cp.lane_assignments.insert(0, (10.5, true));
        cp
    }

    fn make_corridors() -> CorridorRoutePlan {
        let mut plan = CorridorRoutePlan::default();
        plan.chains.insert(3, vec![0, 1]);
        plan
    }

    fn make_model() -> CorridorModel {
        CorridorModel {
            corridors: Vec::new(),
            demands: Vec::new(),
            edge_chains: HashMap::new(),
            cross_scope_edges: 0,
            with_chain: 0,
            no_chain_edges: Vec::new(),
        }
    }

    /// 障碍物视图与底层 `PreparedObstacles` 逐位一致。
    #[test]
    fn obstacle_view_matches_prepared_obstacles() {
        let (obs, cp, cor, model) =
            (make_obstacles(), make_channels(), make_corridors(), make_model());
        let rg = ResourceGraph::assemble(&obs, None, Some(&cp), &cor, &model, &[]);
        assert_eq!(rg.node_obstacle_ids(), obs.sorted_node_ids.as_slice());
        assert_eq!(rg.group_obstacle_ids(), obs.sorted_group_ids.as_slice());
    }

    /// 通道 / 走廊视图与底层资源的直接查询一致（lane 优先、通道中心回退、链点查）。
    #[test]
    fn channel_and_corridor_views_match_underlying() {
        let (obs, cp, cor, model) =
            (make_obstacles(), make_channels(), make_corridors(), make_model());
        let rg = ResourceGraph::assemble(&obs, None, Some(&cp), &cor, &model, &[]);
        // 边 0：有精确 lane，返回 lane 坐标。
        assert_eq!(rg.channel_for_edge(0), cp.lane_assignments.get(&0).copied());
        assert_eq!(rg.channel_for_edge(0), Some((10.5, true)));
        // 边 2：无 lane，回退到通道中心 x=10.0。
        assert_eq!(rg.channel_for_edge(2), cp.channel_for_edge(2));
        assert_eq!(rg.channel_for_edge(2), Some((10.0, true)));
        // planned_channel_coord 取坐标分量。
        assert_eq!(rg.planned_channel_coord(0), Some(10.5));
        // 走廊链点查。
        assert!(rg.has_corridor_chain(3));
        assert_eq!(rg.corridor_chain(3), cor.chains.get(&3).map(|v| v.as_slice()));
        assert!(!rg.has_corridor_chain(99));
        assert!(rg.has_channels());
    }

    /// 访问器对同一实例多次调用逐位一致（禁 HashMap 迭代序泄漏）。
    #[test]
    fn accessors_are_deterministic_across_calls() {
        let (obs, cp, cor, model) =
            (make_obstacles(), make_channels(), make_corridors(), make_model());
        let rg = ResourceGraph::assemble(&obs, None, Some(&cp), &cor, &model, &[]);
        let first: Vec<_> = (0..4).map(|i| rg.channel_for_edge(i)).collect();
        let second: Vec<_> = (0..4).map(|i| rg.channel_for_edge(i)).collect();
        assert_eq!(first, second);
        let chains1: Vec<_> = (0..4).map(|i| rg.has_corridor_chain(i)).collect();
        let chains2: Vec<_> = (0..4).map(|i| rg.has_corridor_chain(i)).collect();
        assert_eq!(chains1, chains2);
    }
}
