//! 架构图专用布局 v2
//!
//! **有顶层分组时**：走 `two_phase`（组内 hint 或 sugiyama_v2 → 组间宏观定位 → 坐标回填）。
//! **无分组时**：走本目录全局 Sugiyama（`rank` → `order` → `coordinate`）；
//! Phase 3 起组内复杂拓扑已委托 `sugiyama_v2::ARCHITECTURE_PRESET`，本目录 rank/order/coordinate
//! **冻结扩展**（仅服务无顶层 group 路径与 hint 几何模式）。

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, ARCHITECTURE_V2_LAYOUT_OPTIONS};
use crate::layout::node::common::node_sizing;
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy, NodeAlignConfig};
use crate::types::DiagramType;
use std::collections::{HashMap, HashSet};

pub(in super::super) mod acyclic;
pub(in super::super) mod constants;
pub(in super::super) mod coordinate;
pub(in super::super) mod order;
pub(in super::super) mod postprocess;
pub(in super::super) mod rank;
pub(in super::super) mod types;

const APPLICABLE_TYPES: &[DiagramType] = &[DiagramType::Architecture];

/// 架构图专用布局 v2
pub struct ArchitectureV2Layout {
    config: ArchitectureV2LayoutConfig,
}

impl ArchitectureV2Layout {
    pub fn new(config: ArchitectureV2LayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(ArchitectureV2LayoutConfig::from_options(options))
    }
}

impl Default for ArchitectureV2Layout {
    fn default() -> Self {
        Self::new(ArchitectureV2LayoutConfig::default())
    }
}

impl LayoutStrategy for ArchitectureV2Layout {
    fn name(&self) -> &'static str {
        "architecture"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        ARCHITECTURE_V2_LAYOUT_OPTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let config = self.config;
        if diagram.entities.is_empty() {
            return LayoutResult {
                nodes: HashMap::new(),
                groups: HashMap::new(),
                edges: vec![],
                total_width: config.padding * 2.0,
                total_height: config.padding * 2.0,
                hints: Default::default(),
            };
        }

        let sizes = node_sizing::standard_node_sizes(diagram);
        let mut graph = types::GraphIndex::build(diagram);
        let group_map = types::build_group_map(diagram);

        // 先注入约束（与 sugiyama_v2::graph::build_graph 一致），使 FAS 能看到完整拓扑。
        // 约束边参与图拓扑但不作为 FAS 反转候选（见 find_edges_to_reverse 的 non_reversible 参数）。
        let constraint_edges: Vec<(&str, &str)> = diagram
            .constraints
            .iter()
            .map(|c| (c.from.as_str(), c.to.as_str()))
            .collect();
        acyclic::inject_irreversible_edges(&mut graph, &constraint_edges);

        // FAS：约束边永不被反转
        let constraint_set: HashSet<(String, String)> = constraint_edges
            .iter()
            .map(|(f, t)| (f.to_string(), t.to_string()))
            .collect();
        let reversed_edges = acyclic::find_edges_to_reverse(&graph, &constraint_set);

        if !group_map.top_groups.is_empty() {
            return super::two_phase::compute_two_phase_layout(
                diagram,
                &graph,
                &group_map,
                &sizes,
                &reversed_edges,
                config,
            );
        }

        let ranks = rank::assign_ranks_group_aware(diagram, &graph, &group_map, &reversed_edges, &constraint_set);
        let decl_index = crate::layout::decl_order::entity_sibling_decl_index(diagram);
        let layers = order::build_layers(&ranks, &decl_index);
        let ordered_layers = order::order_layers_group_aware(
            &graph,
            &group_map,
            &layers,
            &reversed_edges,
            &decl_index,
        );
        let nodes = coordinate::assign_coordinates(
            diagram,
            &graph,
            &group_map,
            &ordered_layers,
            &sizes,
            &reversed_edges,
        );

        let mut ctx = super::pipeline::LayoutContext {
            diagram,
            graph: &graph,
            group_map: &group_map,
            sizes: &sizes,
            config,
            ordered_layers: &ordered_layers,
            reversed: &reversed_edges,
            nodes,
            groups: HashMap::new(),
        };
        super::pipeline::run_pipeline(&mut ctx);

        let (total_width, total_height) = crate::layout::node::common::canvas_bounds::canvas_size(
            &ctx.nodes,
            &ctx.groups,
            constants::PADDING,
        );

        LayoutResult {
            nodes: ctx.nodes,
            groups: ctx.groups,
            edges: vec![],
            total_width,
            total_height,
            hints: crate::layout::LayoutHints {
                edge_routing_style: crate::layout::EdgeRoutingStyle::Orthogonal,
                sugiyama_ranks: Some(ranks),
                ..Default::default()
            },
        }
    }

    fn node_align_config(&self) -> NodeAlignConfig {
        NodeAlignConfig::default_architecture()
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
