//! 布局 ↔ 路由反馈：路由与 refine。

use crate::ast::Diagram;
use crate::layout::refine::{run_refine, RefineConfig};
use crate::layout::space_budget::{enforce_horizontal_gaps, SpaceBudget};
use crate::layout::{EdgeRoutingStrategy, LayoutResult};

/// 预路由反馈：待路由布局。
pub struct PreRouteFeedback {
    pub result: LayoutResult,
}

/// 路由后 refine。
pub struct LayoutRouteFeedback<'a> {
    diagram: &'a Diagram,
}

impl<'a> LayoutRouteFeedback<'a> {
    pub fn new(diagram: &'a Diagram) -> Self {
        Self { diagram }
    }

    /// 预路由：确保 SpaceBudget 存在并 enforce 同层缝。
    pub fn apply_pre_route(&self, mut result: LayoutResult) -> PreRouteFeedback {
        if result.hints.space_budget.is_none() {
            result.hints.space_budget = Some(SpaceBudget::from_diagram(self.diagram));
        }
        if let Some(budget) = result.hints.space_budget.clone() {
            enforce_horizontal_gaps(&mut result.nodes, &budget);
        }
        PreRouteFeedback { result }
    }

    /// 路由 → refine → 仅在契约失败时兜底消重叠并增量重路由。
    pub fn complete_routing(
        &self,
        router: &dyn EdgeRoutingStrategy,
        layout: LayoutResult,
        refine_config: &RefineConfig,
    ) -> LayoutResult {
        let t_route = crate::layout::perf::Instant::now();
        let mut routed = router.route(self.diagram, layout);
        crate::perf_log!(
            "[perf]       router.route: {:.2}ms",
            t_route.elapsed().as_secs_f64() * 1000.0
        );

        if router.supports_refine() {
            let t_refine = crate::layout::perf::Instant::now();
            routed = run_refine(self.diagram, routed, router, refine_config);
            crate::perf_log!(
                "[perf]       run_refine: {:.2}ms",
                t_refine.elapsed().as_secs_f64() * 1000.0
            );
        }

        // S3：仅当水平缝仍违反契约时才兜底推开 + 增量重路由 + repulse
        // (R-4:与 pipeline.rs S3 兜底保持一致,含 repulse_edges_only)
        let (routed, moved) = crate::layout::space_budget_guard::resolve_budget_violations(
            self.diagram, routed,
        );
        crate::layout::space_budget_guard::reroute_and_repulse(
            self.diagram, routed, router, &moved, &router.edge_snap_config(),
        )
    }
}
