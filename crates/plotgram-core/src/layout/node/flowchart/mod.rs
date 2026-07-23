//! 流程图布局（`layout_algo: flowchart`）。
//!
//! 图类型门面算法：底层共享 [`super::sugiyama_v2`] 引擎与
//! [`preset::FLOWCHART_PRESET`](super::sugiyama_v2::preset::FLOWCHART_PRESET)，
//! 流程图专属微调在此覆写。
//!
//! # 分治布局
//!
//! 含 group 时走分治路径（[`group_divide::divide_flowchart_with_groups`]）：
//! 每个 group 独立调用 Sugiyama 布局，再按拓扑序垂直堆叠合并。
//! 无 group 时走原路径（`engine::compute_with_preset`），不受影响。
//!
//! # Recipe
//!
//! 通过 [`LayoutRecipe`](crate::layout::kernel::recipe::LayoutRecipe) 编排布局生命周期。

pub mod group_divide;

use crate::ast::Diagram;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::kernel::recipe::LayoutRecipe;
use crate::layout::node::sugiyama_v2::{engine, preset};
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy, NodeAlignConfig};
use crate::types::DiagramType;

/// 流程图布局（`layout_algo: flowchart`）。
pub struct FlowchartLayout {
    config: SugiyamaLayoutConfig,
}

impl FlowchartLayout {
    pub fn new(config: SugiyamaLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(SugiyamaLayoutConfig::from_options(options))
    }
}

impl Default for FlowchartLayout {
    fn default() -> Self {
        Self::new(SugiyamaLayoutConfig::default())
    }
}

impl LayoutStrategy for FlowchartLayout {
    fn name(&self) -> &'static str {
        "flowchart"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[DiagramType::Flowchart]
    }

    fn supports_custom(&self) -> bool {
        true
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
        let recipe = FlowchartLayoutRecipe { config: self.config };
        recipe.execute(diagram)
    }

    fn node_align_config(&self) -> NodeAlignConfig {
        NodeAlignConfig::default_flowchart()
    }
}

// ─── Recipe 实现 ────────────────────────────────────────

/// 流程图布局配方。
///
/// 双路径分发：有 group 走分治，无 group 走 Sugiyama 引擎。
struct FlowchartLayoutRecipe {
    config: SugiyamaLayoutConfig,
}

/// 流程图问题 IR。
enum FlowchartProblem {
    /// 无 group：Sugiyama 引擎路径
    Flat,
    /// 有 group：分治路径
    DivideConquer,
}

impl LayoutRecipe for FlowchartLayoutRecipe {
    type Problem = FlowchartProblem;
    type Solution = LayoutResult;

    fn name(&self) -> &'static str {
        "flowchart"
    }

    fn compile(&self, diagram: &Diagram) -> FlowchartProblem {
        if group_divide::should_divide(diagram) {
            FlowchartProblem::DivideConquer
        } else {
            FlowchartProblem::Flat
        }
    }

    fn solve(&self, _problem: &FlowchartProblem) -> LayoutResult {
        // 占位：实际逻辑在 execute 中
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
        if group_divide::should_divide(diagram) {
            group_divide::divide_flowchart_with_groups(diagram, self.config)
        } else {
            engine::compute_with_preset(diagram, &preset::FLOWCHART_PRESET, self.config)
        }
    }
}
