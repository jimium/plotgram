//! Group Frame 统一 handoff：L3→L1 重算、整形、架构图后处理。

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, SugiyamaLayoutConfig};
use crate::layout::constants;
#[allow(unused_imports)]
use crate::layout::group;
use crate::layout::recipes::architecture::post_layout;
use crate::layout::pipeline::plan::LayoutPlan;
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
        algo: &str,
    ) {
        self.apply_frame_state(diagram, layout, algo, FrameMode::Recompute);
    }

    /// 边路由前：同步 Border Shell 权威 group rect（spec §7.1）。
    pub fn refresh_before_route(
        &self,
        diagram: &Diagram,
        layout: &mut LayoutResult,
        algo: &str,
    ) {
        if diagram.groups.is_empty() {
            return;
        }
        self.apply_frame_state(diagram, layout, algo, FrameMode::Recompute);
        #[cfg(debug_assertions)]
        group::debug_assert_routing_groups_contain_members(diagram, &layout.nodes, &layout.groups);
    }

    /// V2/refine 推开节点后：重算 bounds + 行对齐恢复 + L1 整形 + 架构居中。
    pub fn restore_after_node_moves(
        &self,
        diagram: &Diagram,
        layout: &mut LayoutResult,
        algo: &str,
        pre_recompute_y: &HashMap<String, f64>,
    ) {
        self.apply_frame_state(
            diagram,
            layout,
            algo,
            FrameMode::Restore { pre_recompute_y },
        );
    }

    /// 三入口统一状态机（A5）：主序列固定为
    /// `recompute_group_bounds → [Restore: realign] → apply_group_frame →
    /// [Recompute+arch+Fit: shrink] → [arch: center]`；各入口仅在此主序列上
    /// 开关差异步骤，逐一等价于旧的三份实现，行为与 node_fp 不变。
    fn apply_frame_state(
        &self,
        diagram: &Diagram,
        layout: &mut LayoutResult,
        algo: &str,
        mode: FrameMode<'_>,
    ) {
        recompute_group_bounds(diagram, layout, self.padding);
        // Restore：refine 推开节点后先按 pre_y 恢复行对齐（Recompute 路径不做）。
        if let FrameMode::Restore { pre_recompute_y } = mode {
            realign_group_rows(&mut layout.groups, pre_recompute_y);
        }
        let report = apply_group_frame(&self.spec, diagram, layout);
        layout.hints.group_frame_report = Some(report);
        // Fit：整形/量化可能留下高于 base∪egb 的空壳，再收回一次（仅 Recompute 路径）。
        if matches!(mode, FrameMode::Recompute)
            && algo == "architecture"
            && matches!(self.spec.track_sizing, super::TrackSizing::Fit)
        {
            let side_gutters = layout
                .hints
                .group_routing
                .as_ref()
                .map(|h| &h.side_gutters);
            crate::layout::group::frame::shrink_groups_to_required_padding(
                diagram,
                &mut layout.groups,
                &layout.nodes,
                self.padding,
                crate::layout::engines::common::group_bounds::container_padding_for_leaf(self.padding),
                side_gutters,
            );
        }
        if algo == "architecture" {
            post_layout::center_single_group_rows(diagram, layout);
        }
    }
}

/// Group Frame 状态机的入口模式（A5：三入口收敛为单一主序列）。
#[derive(Clone, Copy)]
enum FrameMode<'a> {
    /// L3 snap 后 / 路由前 refresh：不做行对齐恢复；arch+Fit 追加空壳收回。
    Recompute,
    /// 节点推开后 restore：先按 `pre_recompute_y` 恢复行对齐；不做空壳收回。
    Restore { pre_recompute_y: &'a HashMap<String, f64> },
}

/// 从 plan 解析 `group_padding`（按布局算法）。
pub fn group_padding_from_plan(plan: &LayoutPlan, algo: &str) -> f64 {
    match algo {
        "flowchart" | "er" | "state" => {
            SugiyamaLayoutConfig::from_options(&plan.layout_options).group_padding
        }
        "architecture" => {
            ArchitectureV2LayoutConfig::from_options(&plan.layout_options).group_padding
        }
        _ => constants::SUGIYAMA_GROUP_PADDING,
    }
}
