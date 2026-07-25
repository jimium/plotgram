//! Group Frame 统一 handoff（G3：生产路径不再改写组几何）。
//!
//! 保留 resolve / padding / `arch_post_layout` 标志供 runner 判断是否跑 PRS 壳；
//! `apply` / `refresh` / `restore` 为空壳，几何由 [`LayoutSession`] 物化。

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, SugiyamaLayoutConfig};
use crate::layout::constants;
use crate::layout::pipeline::plan::LayoutPlan;
use crate::layout::LayoutResult;
use std::collections::HashMap;

use super::{resolve_group_frame_spec, GroupFrameSpec, GroupPadding};

/// 一次布局 pass 所需的 Group Frame 规格与 padding（从 diagram + plan 解析）。
#[derive(Debug, Clone)]
pub struct GroupFramePass {
    pub spec: GroupFrameSpec,
    pub padding: GroupPadding,
    /// 在 resolve 时固化，避免 runner 再写 `algo == "architecture"`。
    pub arch_post_layout: bool,
}

impl GroupFramePass {
    /// 从 diagram 与 plan 解析 L1 Group Frame 参数。
    pub fn resolve(diagram: &Diagram, plan: &LayoutPlan, algo: &str) -> Self {
        let group_padding = group_padding_from_plan(plan, algo);
        let spec = resolve_group_frame_spec(diagram, algo);
        Self {
            padding: if spec.architecture_recipe {
                GroupPadding::architecture()
            } else {
                GroupPadding::uniform(group_padding, 16.0)
            },
            arch_post_layout: spec.architecture_recipe,
            spec,
        }
    }

    /// G3：空壳——不再 `recompute` / `apply_group_frame` / shrink。
    pub fn apply_after_node_snap(
        &self,
        _diagram: &Diagram,
        _layout: &mut LayoutResult,
        _algo: &str,
    ) {
    }

    /// G3：空壳——不再 containment expand / frame 重跑。
    pub fn refresh_before_route(
        &self,
        _diagram: &Diagram,
        _layout: &mut LayoutResult,
        _algo: &str,
    ) {
    }

    /// G3：空壳——不再 realign / apply_group_frame。
    pub fn restore_after_node_moves(
        &self,
        _diagram: &Diagram,
        _layout: &mut LayoutResult,
        _algo: &str,
        _pre_recompute_y: &HashMap<String, f64>,
    ) {
    }
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
