//! 布局/边路由算法注册表（名称列表与工厂入口的唯一来源）。

use super::plan::LayoutPlan;
use crate::layout::{EdgeRoutingStrategy, LayoutStrategy};

/// 已注册的节点布局算法名（与 `build_layout_strategy` 保持一致）。
///
/// 每个图类型对应一个专属配方，`auto` 为元选择器（解析为 profile 默认算法）。
/// `circular` 为内部实现（StateRecipe 备选路径），不暴露于注册表。
pub const LAYOUT_ALGORITHM_NAMES: &[&str] = &[
    "auto",
    "mindmap",
    "sequence",
    "flowchart",
    "er",
    "state",
    "architecture",
];

/// 已注册的边路由算法名（与 `build_edge_routing_strategy` 保持一致）。
pub const EDGE_ROUTING_NAMES: &[&str] = &["straight", "bezier", "spline", "circular", "orthogonal", "organic"];

pub(super) fn build_layout_strategy(
    algo: &str,
    plan: &LayoutPlan,
) -> Option<Box<dyn LayoutStrategy>> {
    use crate::layout::recipes::{
        architecture, er, flowchart, mindmap, sequence, state,
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
        _ => return None,
    };
    Some(strategy)
}

pub(super) fn build_edge_routing_strategy(
    algo: &str,
    plan: &LayoutPlan,
) -> Option<Box<dyn EdgeRoutingStrategy>> {
    use crate::layout::routing::{
        edge_routing, edge_routing_bezier, edge_routing_circular, edge_routing_organic,
        edge_routing_orthogonal, edge_routing_spline,
    };

    let strategy: Box<dyn EdgeRoutingStrategy> = match algo {
        "straight" => Box::new(edge_routing::StraightRouting),
        "bezier" => Box::new(edge_routing_bezier::BezierRouting::from_options(
            &plan.edge_options,
        )),
        "spline" => Box::new(edge_routing_spline::SplineRouting::from_options(
            &plan.edge_options,
        )),
        "circular" => Box::new(edge_routing_circular::CircularRouting),
        "orthogonal" => Box::new(edge_routing_orthogonal::OrthogonalRouting::from_options(
            &plan.edge_options,
        )),
        "organic" => Box::new(edge_routing_organic::OrganicRouting::from_options(
            &plan.edge_options,
        )),
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

pub(in crate::layout) fn all_routing_strategies() -> Vec<Box<dyn EdgeRoutingStrategy>> {
    EDGE_ROUTING_NAMES
        .iter()
        .filter_map(|name| {
            let plan = LayoutPlan::catalog_edge_plan(name);
            build_edge_routing_strategy(name, &plan)
        })
        .collect()
}
