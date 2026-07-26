//! 布局/边路由算法注册表（名称列表与工厂入口的唯一来源）。

use super::plan::LayoutPlan;
use crate::layout::{RoutingRecipeDyn, LayoutStrategy};

/// 已注册的节点布局算法名（与 `build_layout_strategy` 保持一致）。
///
/// 每个图类型对应一个专属配方，`auto` 为元选择器（解析为 profile 默认算法）。
/// Stage 6：`circular` 一等注册（原仅 StateRecipe 内部委托）。
pub const LAYOUT_ALGORITHM_NAMES: &[&str] = &[
    "auto",
    "mindmap",
    "sequence",
    "flowchart",
    "er",
    "state",
    "architecture",
    "circular",
];

/// 已注册的边路由算法名（与 `build_edge_routing_strategy` 保持一致）。
pub const EDGE_ROUTING_NAMES: &[&str] = &["straight", "bezier", "spline", "circular", "orthogonal", "organic"];

pub(super) fn build_layout_strategy(
    algo: &str,
    plan: &LayoutPlan,
) -> Option<Box<dyn LayoutStrategy>> {
    use crate::layout::recipes::{
        architecture, circular, er, flowchart, mindmap, sequence, state,
    };

    // "auto" 解析为 profile 默认算法（由 LayoutPlan::resolve 已处理）。
    // 此处为防御性回退：若 auto 未被上游解析，默认 flowchart。
    let effective_algo = if algo == "auto" {
        plan.resolved_auto_algo.as_deref().unwrap_or("flowchart")
    } else {
        algo
    };

    let strategy: Box<dyn LayoutStrategy> = match effective_algo {
        "mindmap" => Box::new(mindmap::MindmapLayout::from_options(&plan.layout_options)),
        "sequence" => Box::new(sequence::SequenceLayout::from_options(&plan.layout_options)),
        "flowchart" => Box::new(flowchart::FlowchartLayout::from_options(&plan.layout_options)),
        "er" => Box::new(er::ErLayout::from_options(&plan.layout_options)),
        "state" => Box::new(state::StateLayout::from_options(&plan.layout_options)),
        "architecture" => Box::new(architecture::ArchitectureV2Layout::from_options(
            &plan.layout_options,
        )),
        "circular" => Box::new(circular::CircularLayout::from_options(&plan.layout_options)),
        _ => return None,
    };
    Some(strategy)
}

pub(in crate::layout) fn build_edge_routing_strategy(
    algo: &str,
    plan: &LayoutPlan,
) -> Option<Box<dyn RoutingRecipeDyn>> {
    use crate::layout::routing::recipe::{
        BezierRecipe, CircularRecipe, OrganicRecipe, OrthogonalRecipe, RecipeRouter, SplineRecipe,
        StraightRecipe,
    };

    let strategy: Box<dyn RoutingRecipeDyn> = match algo {
        "straight" => Box::new(RecipeRouter::new(StraightRecipe)),
        "bezier" => Box::new(RecipeRouter::new(BezierRecipe::from_options(
            &plan.edge_options,
        ))),
        "spline" => Box::new(RecipeRouter::new(SplineRecipe::from_options(
            &plan.edge_options,
        ))),
        "circular" => Box::new(RecipeRouter::new(CircularRecipe)),
        "orthogonal" => Box::new(RecipeRouter::new(OrthogonalRecipe::from_options(
            &plan.edge_options,
        ))),
        "organic" => Box::new(RecipeRouter::new(OrganicRecipe::from_options(
            &plan.edge_options,
        ))),
        _ => return None,
    };
    Some(strategy)
}

pub(in crate::layout) fn all_layout_strategies() -> Vec<Box<dyn LayoutStrategy>> {
    let empty_plan = LayoutPlan::default_for_catalog();
    LAYOUT_ALGORITHM_NAMES
        .iter()
        .filter(|name| **name != "auto")
        .filter_map(|name| build_layout_strategy(name, &empty_plan))
        .collect()
}

pub(in crate::layout) fn all_routing_strategies() -> Vec<Box<dyn RoutingRecipeDyn>> {
    EDGE_ROUTING_NAMES
        .iter()
        .filter_map(|name| {
            let plan = LayoutPlan::catalog_edge_plan(name);
            build_edge_routing_strategy(name, &plan)
        })
        .collect()
}
