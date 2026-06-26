//! 布局 ↔ 路由反馈闭环：友好性诊断/调整、路由与 refine。

use crate::ast::Diagram;
use crate::layout::friendliness;
use crate::layout::plan::LayoutPlan;
use crate::layout::refine::{run_refine, RefineConfig};
use crate::layout::{EdgeRoutingStrategy, LayoutResult};

/// 预路由反馈：待路由布局。
pub struct PreRouteFeedback {
    pub result: LayoutResult,
}

/// 统一 Friendliness V1/V2 与路由后 refine。
pub struct LayoutRouteFeedback<'a> {
    diagram: &'a Diagram,
    plan: &'a LayoutPlan,
    algo: &'a str,
}

impl<'a> LayoutRouteFeedback<'a> {
    pub fn new(diagram: &'a Diagram, plan: &'a LayoutPlan, algo: &'a str) -> Self {
        Self {
            diagram,
            plan,
            algo,
        }
    }

    /// V1 诊断 + 可选 V2 节点微调；返回待路由布局。
    pub fn apply_pre_route(&self, mut result: LayoutResult) -> PreRouteFeedback {
        let v1_enabled = self.plan.friendliness.v1_enabled();
        let v2_env_disabled = std::env::var("DRAWIFY_NO_V2_ADJUST").as_deref() == Ok("1");
        let v2_enabled = self.plan.friendliness.v2_enabled() && !v2_env_disabled;

        if v1_enabled {
            let evaluator = friendliness::RoutingFriendlinessEvaluator::for_layout(self.algo);
            result.hints.friendliness_report = Some(evaluator.evaluate(self.diagram, &result));
        }

        if v2_enabled {
            let adjuster = friendliness::adjuster::FriendlinessAdjuster::with_default();
            result = adjuster.apply(self.diagram, result);
            let evaluator = friendliness::RoutingFriendlinessEvaluator::for_layout(self.algo);
            result.hints.friendliness_report = Some(evaluator.evaluate(self.diagram, &result));
        }

        PreRouteFeedback { result }
    }

    /// 路由 → refine。
    pub fn complete_routing(
        &self,
        router: &dyn EdgeRoutingStrategy,
        layout: LayoutResult,
        refine_config: &RefineConfig,
    ) -> LayoutResult {
        let t_route = std::time::Instant::now();
        let mut routed = router.route(self.diagram, layout);
        eprintln!("[perf]       router.route: {:.2}ms", t_route.elapsed().as_secs_f64() * 1000.0);

        if router.supports_refine() {
            let t_refine = std::time::Instant::now();
            routed = run_refine(self.diagram, routed, router, refine_config);
            eprintln!("[perf]       run_refine: {:.2}ms", t_refine.elapsed().as_secs_f64() * 1000.0);
        }

        routed
    }
}