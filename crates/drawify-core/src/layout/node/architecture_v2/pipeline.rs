//! 架构图布局后处理 Phase Pipeline
//!
//! 将 `compute` 中 Phase 5~6.4 的后处理步骤拆分为独立的 Phase，
//! 每个 Phase 可单独测试、可按 Preset 启用/禁用。
//!
//! ## Phase 列表（按执行顺序）
//! 1. `OverlapRemovalPhase`：节点重叠消除 + 基础设施行重平衡
//! 2. `ClampPhase`：钳制到非负区域
//! 3. `NeighborAlignmentPhase`：邻接中心对齐（减少不必要的边拐弯）
//! 4. `HubCenteringPhase`：hub 居中 + 客户端对齐 + 基础设施行重平衡
//! 5. `GroupBoundsPhase`：计算分组边界 + 钳制分组
//! 6. `GroupOverlapPhase`：消除相邻分组边框重叠
//! 7. `GroupAlignmentPhase`：基础设施行重平衡
//!
//! **注意**：顶层分组左缘对齐已迁移至 L1 Group Frame（`apply_group_frame`），
//! 不再在 pipeline 内执行（见 `group-frame-spec.md` §4.3）。

use crate::ast::Diagram;
use crate::layout::node::common::group_bounds::{self, GroupPadding};
use crate::layout::{GroupLayout, NodeLayout};
use std::collections::HashMap;

use super::layout::coordinate::{
    align_client_nodes_to_hubs, align_nodes_to_neighbors, center_group_hub_nodes,
    rebalance_infrastructure_layers,
};
use super::layout::constants::GROUP_LABEL_HEIGHT;
use super::layout::postprocess::{
    clamp_groups_to_canvas, clamp_to_canvas, remove_node_overlaps, resolve_group_overlaps,
};
use super::layout::types::{GraphIndex, GroupMap};
use crate::layout::algorithm_config::ArchitectureV2LayoutConfig;

/// 后处理共享上下文，承载各 Phase 间的可变状态
pub(super) struct LayoutContext<'a> {
    pub diagram: &'a Diagram,
    pub graph: &'a GraphIndex,
    pub group_map: &'a GroupMap,
    pub sizes: &'a HashMap<String, (f64, f64)>,
    pub config: ArchitectureV2LayoutConfig,
    pub ordered_layers: &'a [Vec<String>],
    pub nodes: HashMap<String, NodeLayout>,
    pub groups: HashMap<String, GroupLayout>,
}

/// 后处理 Phase trait
pub(super) trait Phase: std::fmt::Debug {
    /// 就地修改 ctx
    fn apply(&self, ctx: &mut LayoutContext);
}

/// 默认后处理管线（按原始 compute 顺序）
pub(super) fn default_pipeline() -> Vec<Box<dyn Phase>> {
    vec![
        Box::new(OverlapRemovalPhase),
        Box::new(ClampPhase),
        Box::new(NeighborAlignmentPhase),
        Box::new(HubCenteringPhase),
        Box::new(GroupBoundsPhase),
        Box::new(GroupOverlapPhase),
        Box::new(GroupAlignmentPhase),
    ]
}

/// 运行后处理管线
pub(super) fn run_pipeline(ctx: &mut LayoutContext) {
    for phase in default_pipeline() {
        phase.apply(ctx);
    }
}

// ─── 具体 Phase 实现 ──────────────────────────────────────

/// Phase 5: 节点重叠消除 + 基础设施行重平衡
#[derive(Debug)]
struct OverlapRemovalPhase;

impl Phase for OverlapRemovalPhase {
    fn apply(&self, ctx: &mut LayoutContext) {
        remove_node_overlaps(&mut ctx.nodes, ctx.sizes);
        rebalance_infrastructure_layers(
            ctx.graph,
            ctx.group_map,
            ctx.ordered_layers,
            ctx.sizes,
            &mut ctx.nodes,
        );
    }
}

/// Phase 5.5: 钳制到非负区域
#[derive(Debug)]
struct ClampPhase;

impl Phase for ClampPhase {
    fn apply(&self, ctx: &mut LayoutContext) {
        clamp_to_canvas(&mut ctx.nodes, ctx.sizes);
    }
}

/// Phase 5.6: 邻接中心对齐——将每个节点向其上下游邻居的中位数中心对齐，
/// 减少不必要的正交边拐弯。跳过基础设施层和组内节点。
#[derive(Debug)]
struct NeighborAlignmentPhase;

impl Phase for NeighborAlignmentPhase {
    fn apply(&self, ctx: &mut LayoutContext) {
        align_nodes_to_neighbors(
            ctx.graph,
            ctx.group_map,
            ctx.ordered_layers,
            ctx.sizes,
            &mut ctx.nodes,
        );
    }
}

/// Phase 5.6: hub 居中 + 客户端对齐 + 基础设施行重平衡
#[derive(Debug)]
struct HubCenteringPhase;

impl Phase for HubCenteringPhase {
    fn apply(&self, ctx: &mut LayoutContext) {
        center_group_hub_nodes(
            ctx.graph,
            ctx.group_map,
            ctx.ordered_layers,
            ctx.sizes,
            &mut ctx.nodes,
        );
        align_client_nodes_to_hubs(
            ctx.graph,
            ctx.group_map,
            ctx.ordered_layers,
            ctx.sizes,
            &mut ctx.nodes,
        );
        rebalance_infrastructure_layers(
            ctx.graph,
            ctx.group_map,
            ctx.ordered_layers,
            ctx.sizes,
            &mut ctx.nodes,
        );
    }
}

/// Phase 6: 计算分组边界 + 钳制分组
#[derive(Debug)]
struct GroupBoundsPhase;

impl Phase for GroupBoundsPhase {
    fn apply(&self, ctx: &mut LayoutContext) {
        ctx.groups = group_bounds::compute_group_bounds(
            ctx.diagram,
            &ctx.nodes,
            GroupPadding::uniform(ctx.config.group_padding, GROUP_LABEL_HEIGHT),
        );
        clamp_groups_to_canvas(&mut ctx.nodes, &mut ctx.groups);
    }
}

/// Phase 6.2: 消除相邻分组边框重叠
#[derive(Debug)]
struct GroupOverlapPhase;

impl Phase for GroupOverlapPhase {
    fn apply(&self, ctx: &mut LayoutContext) {
        resolve_group_overlaps(ctx.diagram, &mut ctx.nodes, &mut ctx.groups);
    }
}

/// Phase 6.3: 基础设施行重平衡
///
/// 顶层分组左缘对齐已迁移至 L1 Group Frame（`apply_group_frame`），
/// 此 Phase 仅保留基础设施行重平衡。
#[derive(Debug)]
struct GroupAlignmentPhase;

impl Phase for GroupAlignmentPhase {
    fn apply(&self, ctx: &mut LayoutContext) {
        rebalance_infrastructure_layers(
            ctx.graph,
            ctx.group_map,
            ctx.ordered_layers,
            ctx.sizes,
            &mut ctx.nodes,
        );
    }
}
