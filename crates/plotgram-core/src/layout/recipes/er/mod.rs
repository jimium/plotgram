//! ER 图布局（`layout_algo: er`）。
//!
//! 图类型门面算法：底层共享 [`super::sugiyama_v2`] 引擎与
//! [`preset::ER_PRESET`](super::sugiyama_v2::preset::ER_PRESET)，
//! ER 图专属微调（实体尺寸估算、边距对齐等）在此覆写。

use crate::ast::Diagram;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::kernel::recipe::LayoutRecipe;
use crate::layout::engines::common::group_bounds::{self, GroupPadding};
use crate::layout::engines::layered::{coordinate, layered_kernel::LayeredKernel, preset};
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
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
/// 走 LayeredKernel + CoordinateKernel，ER_PRESET。
struct ErRecipe {
    config: SugiyamaLayoutConfig,
}

/// ER 图问题 IR：LayeredKernel 产出的分层草稿。
struct ErProblem {
    draft: crate::layout::engines::layered::layered_kernel::LayeredDraft,
}

/// ER 图求解结果。
struct ErSolution {
    nodes: std::collections::HashMap<String, crate::layout::NodeLayout>,
    solved_problem: Option<crate::layout::kernel::coordinate::model::CoordinateProblem>,
    draft: crate::layout::engines::layered::layered_kernel::LayeredDraft,
}

impl LayoutRecipe for ErRecipe {
    type Problem = ErProblem;
    type Solution = ErSolution;

    fn name(&self) -> &'static str {
        "er"
    }

    fn compile(&self, diagram: &Diagram) -> ErProblem {
        let draft = LayeredKernel::compute(
            diagram,
            &preset::ER_PRESET,
            self.config,
        );
        ErProblem { draft }
    }

    fn solve(&self, problem: &ErProblem) -> ErSolution {
        let draft = &problem.draft;
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
        ErSolution { nodes, solved_problem, draft: draft.clone() }
    }

    fn product(&self, solution: &ErSolution, diagram: &Diagram) -> LayoutResult {
        let ErSolution { nodes, solved_problem, draft } = solution;
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
                edge_routing_style: EdgeRoutingStyle::Spline,
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
        // 空图快速返回
        if diagram.entities.is_empty() {
            return LayoutResult {
                nodes: std::collections::HashMap::new(),
                groups: std::collections::HashMap::new(),
                edges: vec![],
                total_width: preset::ER_PRESET.padding * 2.0,
                total_height: preset::ER_PRESET.padding * 2.0,
                hints: Default::default(),
            };
        }

        // 标准生命周期：compile → solve → product
        let problem = self.compile(diagram);
        let solution = self.solve(&problem);
        self.product(&solution, diagram)
    }
}
