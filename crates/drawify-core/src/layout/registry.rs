//! 布局/边路由算法注册表（名称列表与工厂入口的唯一来源）。

use super::algorithm_config::SugiyamaLayoutConfig;
use super::plan::LayoutPlan;
use super::{EdgeRoutingStrategy, LayoutStrategy};

/// 已注册的节点布局算法名（与 `build_layout_strategy` 保持一致）。
pub const LAYOUT_ALGORITHM_NAMES: &[&str] = &[
    "mindmap",
    "circular",
    "sequence",
    "force-directed",
    "flowchart",
    "er",
    "state",
    "sugiyama-v2",
    "architecture",
    "sugiyama",
];

/// 已注册的边路由算法名（与 `build_edge_routing_strategy` 保持一致）。
pub const EDGE_ROUTING_NAMES: &[&str] = &["straight", "bezier", "spline", "circular", "orthogonal", "organic"];

pub(super) fn build_layout_strategy(
    algo: &str,
    plan: &LayoutPlan,
) -> Option<Box<dyn LayoutStrategy>> {
    use super::node::{
        architecture_v2, backup, circular, er, flowchart, force_directed, mindmap, sequence,
        sugiyama_v2,
    };

    let strategy: Box<dyn LayoutStrategy> = match algo {
        "mindmap" => Box::new(mindmap::MindmapLayout::from_options(&plan.layout_options)),
        "circular" => Box::new(circular::CircularLayout::from_options(&plan.layout_options)),
        "sequence" => Box::new(sequence::SequenceLayout::from_options(&plan.layout_options)),
        "force-directed" => Box::new(force_directed::ForceDirectedLayout::from_options(
            &plan.layout_options,
        )),
        "flowchart" => Box::new(flowchart::FlowchartLayout::from_options(&plan.layout_options)),
        "er" => Box::new(er::ErLayout::from_options(&plan.layout_options)),
        "state" => Box::new(circular::StateLayout::from_options(&plan.layout_options)),
        "sugiyama-v2" => Box::new(sugiyama_v2::SugiyamaV2Layout::new(
            SugiyamaLayoutConfig::from_options(&plan.layout_options),
        )),
        "architecture" => Box::new(architecture_v2::ArchitectureV2Layout::from_options(
            &plan.layout_options,
        )),
        "sugiyama" => Box::new(backup::sugiyama::SugiyamaLayout::new(SugiyamaLayoutConfig::from_options(
            &plan.layout_options,
        ))),
        _ => return None,
    };
    Some(strategy)
}

pub(super) fn build_edge_routing_strategy(
    algo: &str,
    plan: &LayoutPlan,
) -> Option<Box<dyn EdgeRoutingStrategy>> {
    use super::edge::{
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

pub(super) fn all_layout_strategies() -> Vec<Box<dyn LayoutStrategy>> {
    let empty_plan = LayoutPlan::default_for_catalog();
    LAYOUT_ALGORITHM_NAMES
        .iter()
        .filter_map(|name| build_layout_strategy(name, &empty_plan))
        .collect()
}

pub(super) fn all_routing_strategies() -> Vec<Box<dyn EdgeRoutingStrategy>> {
    EDGE_ROUTING_NAMES
        .iter()
        .filter_map(|name| {
            let plan = LayoutPlan::catalog_edge_plan(name);
            build_edge_routing_strategy(name, &plan)
        })
        .collect()
}
