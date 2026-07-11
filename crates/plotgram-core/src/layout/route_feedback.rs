//! 布局 ↔ 路由反馈：路由与 refine。

use crate::ast::Diagram;
use crate::layout::refine::{run_refine, RefineConfig};
use crate::layout::space_budget::{
    enforce_horizontal_gaps, horizontal_gap_violations, resolve_residual_with_budget, SpaceBudget,
};
use crate::layout::{EdgeRoutingStrategy, LayoutResult};
use std::collections::{HashMap, HashSet};

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

        // S3：仅当水平缝仍违反契约时才兜底推开
        let budget = routed
            .hints
            .space_budget
            .clone()
            .unwrap_or_else(|| SpaceBudget::from_diagram(self.diagram));
        if !horizontal_gap_violations(&routed.nodes, &budget).is_empty() {
            let pre: HashMap<String, (f64, f64)> = routed
                .nodes
                .iter()
                .map(|(id, n)| (id.clone(), (n.x, n.y)))
                .collect();
            resolve_residual_with_budget(&mut routed.nodes, Some(&budget));
            routed.hints.space_budget = Some(budget);
            let moved: HashSet<String> = routed
                .nodes
                .iter()
                .filter_map(|(id, n)| {
                    pre.get(id).and_then(|(px, py)| {
                        let dx = n.x - px;
                        let dy = n.y - py;
                        if (dx * dx + dy * dy).sqrt() >= 1.0 {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                })
                .collect();
            if !moved.is_empty() {
                routed = router.route_after_node_moves(self.diagram, routed, &moved);
            }
        } else if routed.hints.space_budget.is_none() {
            routed.hints.space_budget = Some(budget);
        }

        routed
    }
}
