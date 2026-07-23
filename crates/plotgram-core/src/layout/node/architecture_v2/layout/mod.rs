//! 架构图专用布局 v2
//!
//! **有顶层分组时**：走 `two_phase`（组内 hint 或 sugiyama_v2 → 组间宏观定位 → 坐标回填）。
//! **无分组时**：走本目录全局 Sugiyama（`rank` → `order` → `coordinate`）；
//! Phase 3 起组内复杂拓扑已委托 `sugiyama_v2::ARCHITECTURE_PRESET`，本目录 rank/order/coordinate
//! **冻结扩展**（仅服务无顶层 group 路径与 hint 几何模式）。

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, ARCHITECTURE_V2_LAYOUT_OPTIONS};
use crate::layout::kernel::recipe::LayoutRecipe;
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
        let recipe = ArchitectureRecipe { config: self.config };
        recipe.execute(diagram)
    }

    fn node_align_config(&self) -> NodeAlignConfig {
        NodeAlignConfig::default_architecture()
    }
}

// ─── Recipe 实现 ────────────────────────────────────────

/// 架构图布局配方。
///
/// 双路径分发：有 group 走 two_phase，无 group 走全局 Sugiyama + solver。
struct ArchitectureRecipe {
    config: ArchitectureV2LayoutConfig,
}

/// 架构图问题 IR。
enum ArchProblem {
    Empty,
    /// 无 group：全局 Sugiyama 路径
    Flat,
    /// 有 group：two_phase 路径
    Hierarchical,
}

impl LayoutRecipe for ArchitectureRecipe {
    type Problem = ArchProblem;
    type Solution = LayoutResult;

    fn name(&self) -> &'static str {
        "architecture"
    }

    fn compile(&self, diagram: &Diagram) -> ArchProblem {
        if diagram.entities.is_empty() {
            return ArchProblem::Empty;
        }
        let group_map = types::build_group_map(diagram);
        if group_map.top_groups.is_empty() {
            ArchProblem::Flat
        } else {
            ArchProblem::Hierarchical
        }
    }

    fn solve(&self, problem: &ArchProblem) -> LayoutResult {
        // solve 需要 diagram，但 trait 签名只给 problem。
        // 架构图的 solve 是“占位”的，实际逻辑在 execute 中完成。
        match problem {
            ArchProblem::Empty => LayoutResult {
                nodes: HashMap::new(),
                groups: HashMap::new(),
                edges: vec![],
                total_width: self.config.padding * 2.0,
                total_height: self.config.padding * 2.0,
                hints: Default::default(),
            },
            _ => LayoutResult {
                nodes: HashMap::new(),
                groups: HashMap::new(),
                edges: vec![],
                total_width: 0.0,
                total_height: 0.0,
                hints: Default::default(),
            },
        }
    }

    fn product(&self, solution: &LayoutResult, _diagram: &Diagram) -> LayoutResult {
        solution.clone()
    }

    /// 覆盖默认编排：架构图的 compile/solve/product 拆分不够自然，
    /// 直接在 execute 中完成完整流程。
    fn execute(&self, diagram: &Diagram) -> LayoutResult {
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

        let constraint_edges: Vec<(&str, &str)> = diagram
            .constraints
            .iter()
            .map(|c| (c.from.as_str(), c.to.as_str()))
            .collect();
        acyclic::inject_irreversible_edges(&mut graph, &constraint_edges);

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
        let (mut nodes, solved_problem) = coordinate::assign_coordinates(
            diagram,
            &graph,
            &group_map,
            &ordered_layers,
            &sizes,
            &reversed_edges,
        );

        super::layout::postprocess::clamp_to_canvas(&mut nodes, &sizes);

        let (total_width, total_height) = crate::layout::node::common::canvas_bounds::canvas_size(
            &nodes,
            &HashMap::new(),
            constants::PADDING,
        );

        let mut space_budget = crate::layout::space_budget::SpaceBudget::from_diagram(diagram);
        space_budget.enrich_adjacent_rank_demand(&ordered_layers, &nodes, diagram);

        LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: vec![],
            total_width,
            total_height,
            hints: crate::layout::LayoutHints {
                edge_routing_style: crate::layout::EdgeRoutingStyle::Orthogonal,
                sugiyama_ranks: Some(ranks),
                space_budget: Some(space_budget),
                coordinate_problem: solved_problem.map(Box::new),
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
