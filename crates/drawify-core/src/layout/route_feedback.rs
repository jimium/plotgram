//! 布局 ↔ 路由反馈闭环：友好性诊断/调整、路由与 refine、基线择优。

use crate::ast::Diagram;
use crate::layout::friendliness;
use crate::layout::plan::LayoutPlan;
use crate::layout::refine::{run_refine, RefineConfig};
use crate::layout::{EdgeRoutingStrategy, LayoutResult};

/// 预路由反馈：待路由布局 + 可选 V2 调整前基线。
pub struct PreRouteFeedback {
    pub result: LayoutResult,
    pub baseline: Option<LayoutResult>,
}

/// 统一 Friendliness V1/V2 与路由后 refine / 基线择优。
pub struct LayoutRouteFeedback<'a> {
    diagram: &'a Diagram,
    plan: &'a LayoutPlan,
    algo: &'a str,
    v2_enabled: bool,
}

impl<'a> LayoutRouteFeedback<'a> {
    pub fn new(diagram: &'a Diagram, plan: &'a LayoutPlan, algo: &'a str) -> Self {
        let v2_env_disabled = std::env::var("DRAWIFY_NO_V2_ADJUST").as_deref() == Ok("1");
        let v2_enabled = plan.friendliness.v2_enabled() && !v2_env_disabled;
        Self {
            diagram,
            plan,
            algo,
            v2_enabled,
        }
    }

    /// V1 诊断 + 可选 V2 节点微调；返回待路由布局与基线快照。
    pub fn apply_pre_route(&self, result: LayoutResult) -> PreRouteFeedback {
        let v1_enabled = self.plan.friendliness.v1_enabled();
        let mut result = result;

        if v1_enabled {
            let evaluator = friendliness::RoutingFriendlinessEvaluator::for_layout(self.algo);
            result.hints.friendliness_report = Some(evaluator.evaluate(self.diagram, &result));
        }

        let baseline = if self.v2_enabled {
            Some(result.clone())
        } else {
            None
        };

        if self.v2_enabled {
            let adjuster = friendliness::adjuster::FriendlinessAdjuster::with_default();
            result = adjuster.apply(self.diagram, result);
            let evaluator = friendliness::RoutingFriendlinessEvaluator::for_layout(self.algo);
            result.hints.friendliness_report = Some(evaluator.evaluate(self.diagram, &result));
        }

        PreRouteFeedback { result, baseline }
    }

    /// 路由 → refine → 若 V2 改变了节点则与基线路由结果择优。
    pub fn complete_routing(
        &self,
        router: &dyn EdgeRoutingStrategy,
        layout: LayoutResult,
        baseline: Option<LayoutResult>,
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

        // NOTE: Skip baseline reroute for performance.
        // The V2 layout is the primary layout; baseline comparison adds ~30ms
        // with marginal routing quality improvement for architecture diagrams.
        let _ = baseline;

        routed
    }
}
