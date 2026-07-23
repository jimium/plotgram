//! ER 图布局（`layout_algo: er`）。
//!
//! 图类型门面算法：底层共享 [`super::sugiyama_v2`] 引擎与
//! [`preset::ER_PRESET`](super::sugiyama_v2::preset::ER_PRESET)，
//! ER 图专属微调（实体尺寸估算、边距对齐等）在此覆写。

use crate::ast::Diagram;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::kernel::recipe::LayoutRecipe;
use crate::layout::node::sugiyama_v2::{engine, preset};
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, EdgeRoutingStyle, LayoutResult, LayoutStrategy, NodeAlignConfig};
use crate::types::DiagramType;

/// ER 图布局（`layout_algo: er`）。
pub struct ErLayout {
    config: SugiyamaLayoutConfig,
}

impl ErLayout {
    pub fn new(config: SugiyamaLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(SugiyamaLayoutConfig::from_options(options))
    }
}

impl Default for ErLayout {
    fn default() -> Self {
        Self::new(SugiyamaLayoutConfig::default())
    }
}

impl LayoutStrategy for ErLayout {
    fn name(&self) -> &'static str {
        "er"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[DiagramType::Er]
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        crate::layout::algorithm_config::SUGIYAMA_LAYOUT_OPTIONS
    }

    fn supported_directions(&self) -> &'static [&'static str] {
        const SUPPORTED_DIRECTIONS: &[&str] = &[
            crate::types::attr_constants::direction::TOP_TO_BOTTOM,
            crate::types::attr_constants::direction::LEFT_TO_RIGHT,
        ];
        SUPPORTED_DIRECTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let recipe = ErRecipe { config: self.config };
        recipe.execute(diagram)
    }

    fn node_align_config(&self) -> NodeAlignConfig {
        NodeAlignConfig::default_er()
    }
}

// ─── Recipe 实现 ────────────────────────────────────────

/// ER 图布局配方。
///
/// 委托 Sugiyama 引擎 + ER_PRESET。
struct ErRecipe {
    config: SugiyamaLayoutConfig,
}

/// ER 图问题 IR。
struct ErProblem;

impl LayoutRecipe for ErRecipe {
    type Problem = ErProblem;
    type Solution = LayoutResult;

    fn name(&self) -> &'static str {
        "er"
    }

    fn compile(&self, _diagram: &Diagram) -> ErProblem {
        ErProblem
    }

    fn solve(&self, _problem: &ErProblem) -> LayoutResult {
        LayoutResult {
            nodes: std::collections::HashMap::new(),
            groups: std::collections::HashMap::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: Default::default(),
        }
    }

    fn product(&self, solution: &LayoutResult, _diagram: &Diagram) -> LayoutResult {
        solution.clone()
    }

    fn execute(&self, diagram: &Diagram) -> LayoutResult {
        let mut result =
            engine::compute_with_preset(diagram, &preset::ER_PRESET, self.config);
        result.hints.edge_routing_style = recommended_er_edge_routing(diagram);
        result
    }
}

/// 稠密 ER（边数 > 节点数 × 1.5）与默认路径均推荐 spline 路由。
fn recommended_er_edge_routing(diagram: &Diagram) -> EdgeRoutingStyle {
    let _ = diagram;
    EdgeRoutingStyle::Spline
}
