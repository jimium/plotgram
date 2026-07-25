//! 全局通道规划器 — Phase 2.4 stub。
//!
//! 重实现已删除；保留空 `ChannelPlan` 与 no-op API，供调用方编译通过。

use crate::ast::Relation;
use crate::layout::group::GroupRoutingContext;
use crate::layout::NodeLayout;
use std::collections::{BTreeMap, HashMap};

/// 全局通道规划（空 stub）。
#[derive(Clone, Debug, Default)]
pub struct ChannelPlan {
    /// 垂直通道 x 坐标 → 分配给哪些边
    pub vertical_channels: BTreeMap<i64, Vec<usize>>,
    /// 水平通道 y 坐标 → 分配给哪些边
    pub horizontal_channels: BTreeMap<i64, Vec<usize>>,
    /// 每条边的精确 lane 坐标（通道中心 + 偏移）
    pub lane_assignments: HashMap<usize, (f64, bool)>,
}

impl ChannelPlan {
    /// 查询指定边的规划通道坐标（stub：恒返回 `None`）。
    pub fn channel_for_edge(&self, _edge_idx: usize) -> Option<(f64, bool)> {
        None
    }
}

/// 通道规划算法（stub：恒返回空计划）。
pub fn plan_channels(
    _relations: &[Relation],
    _nodes: &HashMap<String, NodeLayout>,
    _group_ctx: &GroupRoutingContext,
    _edge_order: &[usize],
) -> ChannelPlan {
    ChannelPlan::default()
}

/// 为同通道多边分配 lane 偏移（stub：no-op）。
pub fn assign_lane_offsets(_plan: &mut ChannelPlan, _parallel_gap: f64) {}
