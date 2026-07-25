//! 跨组边走廊（corridor）路由 — Phase 2.4 stub。
//!
//! 重实现已删除；保留签名与空计划，供调用方编译通过。
//! 真实选路走 LexA* / `select_best_path_with_scorer_stats`。

use std::collections::HashMap;

use crate::ast::Relation;
use crate::layout::geometry::Point;
use crate::layout::group::{CorridorAxis, GroupRoutingContext};

use super::profile::OrthoRoutingProfile;

/// 跨组走廊路由计划（空 stub；chains/lanes/load 恒为空）。
#[derive(Debug, Clone, Default)]
pub struct CorridorRoutePlan {
    /// edge_index → 走廊索引链
    pub chains: HashMap<usize, Vec<usize>>,
    /// (edge_index, corridor_index) → 车道序号
    pub lanes: HashMap<(usize, usize), usize>,
    /// corridor_index → 该走廊上的边数
    pub corridor_load: HashMap<usize, usize>,
}

/// 为跨组边规划走廊链与车道分配（stub：恒返回空计划）。
pub fn plan_corridor_routes(
    _relations: &[Relation],
    _group_ctx: &GroupRoutingContext,
    _profile: &OrthoRoutingProfile,
) -> CorridorRoutePlan {
    CorridorRoutePlan::default()
}

/// 尝试按走廊计划构建路径（stub：恒返回 `None`）。
pub fn try_build_corridor_path(
    _edge_index: usize,
    _from_anchor: Point,
    _to_anchor: Point,
    _from_id: &str,
    _to_id: &str,
    _plan: &CorridorRoutePlan,
    _group_ctx: &GroupRoutingContext,
    _stub_len: f64,
    _outer_bypass: bool,
) -> Option<Vec<Point>> {
    None
}

/// 取边在走廊链上的 cross-axis 偏移（stub：恒返回 `None`）。
pub(crate) fn planned_cross_axis_offset_for_edge(
    _edge_index: usize,
    _plan: &CorridorRoutePlan,
    _group_ctx: &GroupRoutingContext,
) -> Option<(CorridorAxis, f64)> {
    None
}
