//! Group Frame 统一 handoff：L3→L1 重算、整形、架构图后处理。

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, SugiyamaLayoutConfig};
use crate::layout::constants;
use crate::layout::group;
use crate::layout::intent::PinSet;
use crate::layout::node::architecture_v2::post_layout;
use crate::layout::plan::LayoutPlan;
use crate::layout::LayoutResult;
use std::collections::HashMap;

use super::{
    apply_group_frame, group_padding_for_algo, realign_group_rows, recompute_group_bounds,
    resolve_group_frame_spec, GroupFrameSpec, GroupPadding,
};

/// 一次布局 pass 所需的 Group Frame 规格与 padding（从 diagram + plan 解析）。
#[derive(Debug, Clone)]
pub struct GroupFramePass {
    pub spec: GroupFrameSpec,
    pub padding: GroupPadding,
}

impl GroupFramePass {
    /// 从 diagram 与 plan 解析 L1 Group Frame 参数。
    pub fn resolve(diagram: &Diagram, plan: &LayoutPlan, algo: &str) -> Self {
        let group_padding = group_padding_from_plan(plan, algo);
        Self {
            spec: resolve_group_frame_spec(diagram, algo),
            padding: group_padding_for_algo(algo, group_padding),
        }
    }

    /// L3 snap 后：重算 group bounds + L1 整形 + 架构图单行居中。
    pub fn apply_after_node_snap(
        &self,
        diagram: &Diagram,
        layout: &mut LayoutResult,
        pinned: &PinSet,
        algo: &str,
    ) {
        recompute_group_bounds(diagram, layout, self.padding);
        apply_group_frame(&self.spec, diagram, layout, pinned);
        if algo == "architecture" {
            post_layout::center_single_group_rows(diagram, layout);
        }
    }

    /// 边路由前：同步 Border Shell 权威 group rect（spec §7.1）。
    pub fn refresh_before_route(
        &self,
        diagram: &Diagram,
        layout: &mut LayoutResult,
        pinned: &PinSet,
        algo: &str,
    ) {
        if diagram.groups.is_empty() {
            return;
        }
        self.apply_after_node_snap(diagram, layout, pinned, algo);
        #[cfg(debug_assertions)]
        group::debug_assert_routing_groups_contain_members(diagram, &layout.nodes, &layout.groups);
    }

    /// V2/refine 推开节点后：重算 bounds + 行对齐恢复 + L1 整形 + 架构居中。
    pub fn restore_after_node_moves(
        &self,
        diagram: &Diagram,
        layout: &mut LayoutResult,
        pinned: &PinSet,
        algo: &str,
        pre_recompute_y: &HashMap<String, f64>,
    ) {
        recompute_group_bounds(diagram, layout, self.padding);
        realign_group_rows(&mut layout.groups, pre_recompute_y);
        apply_group_frame(&self.spec, diagram, layout, pinned);
        if algo == "architecture" {
            post_layout::center_single_group_rows(diagram, layout);
        }
    }
}

/// 从 plan 解析 `group_padding`（按布局算法）。
pub fn group_padding_from_plan(plan: &LayoutPlan, algo: &str) -> f64 {
    match algo {
        "sugiyama" | "sugiyama-v2" | "flowchart" | "er" => {
            SugiyamaLayoutConfig::from_options(&plan.layout_options).group_padding
        }
        "architecture" => {
            ArchitectureV2LayoutConfig::from_options(&plan.layout_options).group_padding
        }
        _ => constants::SUGIYAMA_GROUP_PADDING,
    }
}
