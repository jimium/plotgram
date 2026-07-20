//! 布局 ↔ 路由反馈：路由与 refine。

use crate::ast::Diagram;
use crate::layout::demand::PressureSnapshot;
use crate::layout::refine::{run_refine, RefineConfig};
use crate::layout::space_budget::{
    enforce_horizontal_gaps, enforce_vertical_rank_gaps, node_group_scopes,
    reverse_relation_pairs, SpaceBudget,
};
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

    /// 预路由：确保 SpaceBudget 存在、压力 enrich、enforce 同层缝 / 可选竖缝。
    pub fn apply_pre_route(&self, mut result: LayoutResult) -> PreRouteFeedback {
        if result.hints.space_budget.is_none() {
            result.hints.space_budget = Some(SpaceBudget::from_diagram(self.diagram));
        }

        // D4 P0 enrich（只读模型 → SpaceBudget）；快照只算一次
        let snap = PressureSnapshot::compute(self.diagram, &result);
        if let Some(budget) = result.hints.space_budget.as_mut() {
            budget.enrich_from_pressure(
                self.diagram,
                &result.nodes,
                &snap.corridor,
                &snap.bands,
                &snap.features,
            );
        }

        if let Some(budget) = result.hints.space_budget.clone() {
            enforce_horizontal_gaps(&mut result.nodes, &budget);
            if let Some(ranks) = result.hints.sugiyama_ranks.as_ref() {
                if budget.min_vertical_rank_gap.is_some() {
                    let scopes = node_group_scopes(self.diagram);
                    let reverse_pairs = reverse_relation_pairs(self.diagram);
                    enforce_vertical_rank_gaps(
                        &mut result.nodes,
                        &budget,
                        ranks,
                        &scopes,
                        &reverse_pairs,
                    );
                }
            }
            result.hints.space_budget = Some(budget);
        }
        PreRouteFeedback { result }
    }

    /// 路由 → refine → 仅在契约失败时兜底消重叠并增量重路由。
    ///
    /// `edge_snap_config` 须与 pipeline 使用同一份（含 `snap:false` / 自适应 grid_step），
    /// 避免 S3 `reroute_and_repulse` 与后续 post_route 排斥配置不一致。
    pub fn complete_routing(
        &self,
        router: &dyn EdgeRoutingStrategy,
        layout: LayoutResult,
        refine_config: &RefineConfig,
        edge_snap_config: &crate::layout::EdgeSnapConfig,
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
        // (R-4:与 pipeline.rs S3 兜底保持一致,含 repulse_edges_only；snap 配置与 pipeline 对齐)
        let (routed, moved) = crate::layout::space_budget_guard::resolve_budget_violations(
            self.diagram, routed,
        );
        crate::layout::space_budget_guard::reroute_and_repulse(
            self.diagram, routed, router, &moved, edge_snap_config,
        )
    }
}
