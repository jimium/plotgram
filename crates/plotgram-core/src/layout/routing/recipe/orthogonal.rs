//! 正交路由 Recipe（doc16 §5.1 / R10 Slice 10b / R12c）。
//!
//! 把 `OrthogonalRouting` 的 `route()` 逻辑包装为 [`RoutingRecipe`] 接口：
//! - `compile` = 借用 diagram + 克隆 result（供 solve 消费）
//! - `solve` = `route_edges_orthogonal_inner` 核心逻辑，产出 geometry + ports
//!
//! Slice D1：自环边不再走 override 旁路——其几何已由 C 末 lift 进
//! `RouteSolution.paths`，ports 来自 `endpoint_assignments`，标签由 D-stage
//! finalizer（sanitize）统一重建，故全部边走标准 materialize 管线。
//!
//! Phase 0：`compile_with_config` 从 [`PreparedRoutingInput`] 注入真实 [`RoutingConfig`]。

use super::{RecipeSolution, RoutingRecipe};
use crate::ast::Diagram;
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::routing::config::RoutingConfig;
use crate::layout::routing::edge_routing_orthogonal::{
    OrthoConfig, ORTHOGONAL_OPTIONS,
};
use crate::layout::snap::grid_snap::EdgeSnapConfig;
use crate::layout::types::{EdgeLayout, LayoutResult};
use crate::types::DiagramType;

const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
];

/// 正交路由 Recipe：包装既有 `route_edges_orthogonal_inner` 为 compile/solve 接口。
#[derive(Clone, Copy)]
pub struct OrthogonalRecipe {
    /// DSL option 派生的 slot/channel 参数；`routing` 字段在 `compile_with_config` 时覆盖。
    config: OrthoConfig,
}

impl OrthogonalRecipe {
    pub fn from_options(options: &crate::layout::pipeline::plan::ResolvedAlgoOptions) -> Self {
        Self {
            config: OrthoConfig {
                slot_pitch: options.get_or_default(&ORTHOGONAL_OPTIONS[0]),
                channel_margin: options.get_or_default(&ORTHOGONAL_OPTIONS[1]),
                // 占位；生产路径经 `compile_with_config` 注入 PreparedRoutingInput.config。
                routing: Default::default(),
            },
        }
    }
}

/// compile 产出的 Draft：借用 diagram + 克隆 result + 已注入的 OrthoConfig。
pub struct OrthogonalSolveDraft<'a> {
    diagram: &'a Diagram,
    result: LayoutResult,
    config: OrthoConfig,
}

impl RoutingRecipe for OrthogonalRecipe {
    type Draft<'a> = OrthogonalSolveDraft<'a>;

    fn name(&self) -> &'static str {
        "orthogonal"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        ORTHOGONAL_OPTIONS
    }

    fn edge_snap_config(&self) -> EdgeSnapConfig {
        EdgeSnapConfig::default_orthogonal()
    }

    fn compile<'a>(&self, diagram: &'a Diagram, result: &'a LayoutResult) -> Self::Draft<'a> {
        self.compile_with_config(diagram, result, Default::default())
    }

    fn compile_with_config<'a>(
        &self,
        diagram: &'a Diagram,
        result: &'a LayoutResult,
        routing_config: RoutingConfig,
    ) -> Self::Draft<'a> {
        let mut config = self.config;
        config.routing = routing_config;
        OrthogonalSolveDraft {
            diagram,
            result: result.clone(),
            config,
        }
    }

    fn solve(&self, draft: &Self::Draft<'_>) -> RecipeSolution {
        // 原地包装：调用既有正交路由内核（同算法、同顺序）。
        let routed = crate::layout::routing::edge_routing_orthogonal::route_orthogonal_inner(
            draft.diagram,
            draft.result.clone(),
            draft.config,
            None,
        );

        let n = routed.edges.len();

        // Slice C2c：直接消费 C 末组装的 RouteSolution，不再逐边 lift geometry。
        // Slice D1：自环边同样走标准管线——几何已在 paths，ports 来自 endpoint_assignments，
        // 标签由 D-stage sanitize 重建，无需 override。
        let solution = routed
            .hints
            .route_solution
            .clone()
            .unwrap_or_default();

        // R12c：标签由 D-stage finalizer 统一构建（C-stage 不预置）。
        let label_plans: Vec<Option<super::EdgeLabelPlan>> = (0..n).map(|_| None).collect();

        RecipeSolution {
            solution,
            label_plans,
        }
    }

    /// Slice F2c：逐边 preserve 增量重解——draft.result.edges 已按声明序 seeded
    ///（preserve 边携带 prev 冻结几何），`route_edges_orthogonal_inner` 的
    /// incremental 分支据此仅重解 dirty 边。
    fn solve_preserving(
        &self,
        draft: &Self::Draft<'_>,
        preserve: &std::collections::HashSet<usize>,
    ) -> Option<RecipeSolution> {
        let routed = crate::layout::routing::edge_routing_orthogonal::route_orthogonal_inner(
            draft.diagram,
            draft.result.clone(),
            draft.config,
            Some(preserve.clone()),
        );

        let n = routed.edges.len();
        let solution = routed
            .hints
            .route_solution
            .clone()
            .unwrap_or_default();
        // R12c：标签由 D-stage finalizer 统一构建（与 solve 一致）。
        let label_plans: Vec<Option<super::EdgeLabelPlan>> = (0..n).map(|_| None).collect();

        Some(RecipeSolution {
            solution,
            label_plans,
        })
    }

    fn finalize(
        &self,
        mut result: LayoutResult,
        edges: Vec<EdgeLayout>,
        diagram: &Diagram,
    ) -> LayoutResult {
        // R12c：非自环边标签由 D-stage finalizer 统一构建；此处仅做 mindmap 清标签防御。
        if matches!(diagram.diagram_type, DiagramType::Mindmap) {
            let mut edges = edges;
            for edge in &mut edges {
                edge.labels.clear();
            }
            result.edges = edges;
        } else {
            result.edges = edges;
        }
        result
    }
}
