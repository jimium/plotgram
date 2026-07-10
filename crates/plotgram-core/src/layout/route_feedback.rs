//! 布局 ↔ 路由反馈：路由与 refine。

use crate::ast::Diagram;
use crate::layout::refine::{run_refine, RefineConfig};
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

    /// 预路由阶段：当前无额外调整，直接透传布局结果。
    pub fn apply_pre_route(&self, result: LayoutResult) -> PreRouteFeedback {
        PreRouteFeedback { result }
    }

    /// 路由 → refine。
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

        routed
    }
}
