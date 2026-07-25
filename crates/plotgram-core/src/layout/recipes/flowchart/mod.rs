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
//! 无 group 时走 LayeredKernel + CoordinateKernel 路径。
//!
//! # Recipe
//!
//! 通过 [`LayoutRecipe`](crate::layout::kernel::recipe::LayoutRecipe) 编排布局生命周期：
//! - compile: LayeredKernel::compute → LayeredDraft
//! - solve: coordinate::assign_coordinates_brandes_koepf → 节点坐标
//! - product: 组装 LayoutResult（groups + canvas + hints）

pub mod group_divide;

use crate::ast::Diagram;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::kernel::recipe::LayoutRecipe;
use crate::layout::engines::common::group_bounds::{self, GroupPadding};
use crate::layout::engines::layered::{coordinate, layered_kernel::LayeredKernel, preset};
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, EdgeRoutingStyle, LayoutResult, LayoutStrategy, NodeAlignConfig};
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
/// 双路径分发：有 group 走分治，无 group 走 LayeredKernel + CoordinateKernel。
struct FlowchartLayoutRecipe {
    config: SugiyamaLayoutConfig,
}

/// 流程图问题 IR。
enum FlowchartProblem {
    /// 无 group：LayeredKernel 产出的分层 IR
    Flat(crate::layout::engines::layered::layered_kernel::LayeredDraft),
    /// 有 group：分治路径
    DivideConquer,
}

/// 流程图求解结果。
struct FlowchartSolution {
    /// 坐标求解结果 + draft 元数据
    nodes: std::collections::HashMap<String, crate::layout::NodeLayout>,
    solved_problem: Option<crate::layout::kernel::coordinate::model::CoordinateProblem>,
    draft: crate::layout::engines::layered::layered_kernel::LayeredDraft,
}

impl LayoutRecipe for FlowchartLayoutRecipe {
    type Problem = FlowchartProblem;
    type Solution = FlowchartSolution;

    fn name(&self) -> &'static str {
        "flowchart"
    }

    fn compile(&self, diagram: &Diagram) -> FlowchartProblem {
        if group_divide::should_divide(diagram) {
            FlowchartProblem::DivideConquer
        } else {
            let draft = LayeredKernel::compute(
                diagram,
                &preset::FLOWCHART_PRESET,
            );
            FlowchartProblem::Flat(draft)
        }
    }

    fn solve(&self, problem: &FlowchartProblem) -> FlowchartSolution {
        match problem {
            FlowchartProblem::Flat(draft) => {
                let (nodes, solved_problem) = coordinate::assign_coordinates_brandes_koepf(
                    &draft.dag,
                    &draft.proper_graph,
                    &draft.layers,
                    &draft.sizes,
                    draft.horizontal,
                    &draft.preset,
                    &draft.per_layer_gaps,
                    draft.has_order_bias,
                    &draft.end_ids,
                );
                FlowchartSolution { nodes, solved_problem, draft: draft.clone() }
            }
            FlowchartProblem::DivideConquer => {
                // 分治路径由 execute() 直接处理，不会走到这里
                panic!("DivideConquer should be handled in execute()")
            }
        }
    }

    fn product(&self, solution: &FlowchartSolution, diagram: &Diagram) -> LayoutResult {
        let FlowchartSolution { nodes, solved_problem, draft } = solution;
        let groups = group_bounds::compute_group_bounds(
            diagram,
            nodes,
            GroupPadding::uniform(self.config.group_padding, 16.0),
        );
        let group_warnings =
            group_bounds::detect_group_layout_warnings(diagram, nodes, &groups);
        let (total_width, total_height) =
            crate::layout::engines::common::canvas_bounds::canvas_size(
                nodes,
                &groups,
                draft.padding,
            );

        let mut result = LayoutResult {
            nodes: nodes.clone(),
            groups,
            edges: vec![],
            total_width,
            total_height,
            hints: crate::layout::LayoutHints {
                edge_routing_style: EdgeRoutingStyle::Orthogonal,
                sugiyama_ranks: Some(draft.sugiyama_ranks.clone()),
                group_layout_warnings: group_warnings,
                same_layer_edges: draft.same_layer_edges.clone(),
                feedback_hubs: draft.feedback_hubs.clone(),
                coordinate_problem: solved_problem.clone().map(Box::new),
                ..Default::default()
            },
        };

        if let Some(finish) = draft.preset.finish_layout {
            finish(&mut result, &draft.preset);
        }

        result
    }

    fn execute(&self, diagram: &Diagram) -> LayoutResult {
        // 分治路径需要特殊处理（compile 时无法获取 diagram）
        if group_divide::should_divide(diagram) {
            return group_divide::divide_flowchart_with_groups(diagram, self.config);
        }

        // 空图快速返回
        if diagram.entities.is_empty() {
            return LayoutResult {
                nodes: std::collections::HashMap::new(),
                groups: std::collections::HashMap::new(),
                edges: vec![],
                total_width: preset::FLOWCHART_PRESET.padding * 2.0,
                total_height: preset::FLOWCHART_PRESET.padding * 2.0,
                hints: Default::default(),
            };
        }

        // 标准生命周期：compile → solve → product
        let problem = self.compile(diagram);
        let solution = self.solve(&problem);
        self.product(&solution, diagram)
    }
}
