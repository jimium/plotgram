//! Architecture 路由前标志（原 `GroupFramePass` 空壳收口）。
//!
//! 只解析 `arch_post_layout` 与 padding；不再提供改写组几何的 apply/refresh/restore。

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, SugiyamaLayoutConfig};
use crate::layout::constants;
use crate::layout::kernel::group::bounds::GroupPadding;
use crate::layout::recipes::frame_spec::{resolve_group_frame_spec, GroupFrameSpec};
use crate::layout::pipeline::plan::LayoutPlan;

/// runner 判断是否跑 architecture orthosketch / shell 物化。
#[derive(Debug, Clone)]
pub struct ArchRouteFlags {
    pub spec: GroupFrameSpec,
    pub padding: GroupPadding,
    pub arch_post_layout: bool,
}

impl ArchRouteFlags {
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
}

fn group_padding_from_plan(plan: &LayoutPlan, algo: &str) -> f64 {
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
