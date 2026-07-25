//! 布局管线编排器：将 `compute_layout_with_plan` 的多阶段逻辑结构化。

use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::layout::snap::canvas_finalize;
use crate::layout::constants;
use crate::layout::snap::grid_snap;
use crate::layout::group::frame::GroupFramePass;
use crate::layout::perf::Instant;
use super::plan::LayoutPlan;
use crate::layout::refine;
use super::registry;
use crate::layout::route_feedback::{LayoutRouteFeedback, PreRouteFeedback};
use crate::layout::{resolve_effective_direction, LayoutResult};

/// 布局管线。
pub(crate) struct LayoutPipeline<'a> {
    diagram: &'a Diagram,
    plan: &'a LayoutPlan,
    /// Slice F2c：上次渲染的冻结路由解（增量入口透传给 Coordinator）。
    prev: Option<&'a crate::layout::routing::model::FrozenRoutingSolution>,
}

impl<'a> LayoutPipeline<'a> {
    pub fn new(diagram: &'a Diagram, plan: &'a LayoutPlan) -> Self {
        Self {
            diagram,
            plan,
            prev: None,
        }
    }

    /// Slice F2c：挂载上次渲染的冻结路由解（增量模式）。
    pub fn with_prev(
        mut self,
        prev: &'a crate::layout::routing::model::FrozenRoutingSolution,
    ) -> Self {
        self.prev = Some(prev);
        self
    }

    pub fn run(self) -> Result<LayoutResult, DiagnosticError> {
        crate::layout::group::write_counter::reset_group_write_counters();
        let algo = self.plan.layout_algo.as_str();

        let strategy = registry::build_layout_strategy(algo, self.plan).ok_or_else(|| {
            super::entry::layout_config_error(
                self.diagram,
                crate::types::standard_attr_keys::diagram::LAYOUT,
                algo,
                &super::entry::known_layout_algo_names(),
            )
        })?;
        let produces_edges = strategy.produces_edge_geometry();
        let mut node_align_config = strategy.node_align_config();
        if let Some(override_mode) = grid_snap::diagram_align_override(self.diagram) {
            node_align_config.apply_diagram_override(override_mode);
        }

        let t_layout = Instant::now();
        let mut result = strategy.compute(self.diagram);
        let layout_elapsed = t_layout.elapsed();
        crate::perf_log!(
            "[perf] layout: {:.2}ms",
            layout_elapsed.as_secs_f64() * 1000.0
        );

        // C13：sequence 等自产边布局若再跑 node align，边端点不会随节点更新。
        // 完整修复需 align 后按节点重锚定消息端点（改动面大）；此处对 produces_edges
        // 跳过 node align，避免端点漂移。默认 sequence align 本已关闭。
        if !produces_edges {
            self.apply_node_frame(&node_align_config, &mut result)?;
        }

        if produces_edges {
            canvas_finalize::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
            crate::layout::group::write_counter::warn_if_group_writes_excessive(1);
            return Ok(result);
        }

        let t_routing = Instant::now();
        let mut result = self.run_routing_pipeline(algo, result)?;
        let routing_elapsed = t_routing.elapsed();
        crate::perf_log!(
            "[perf] routing: {:.2}ms",
            routing_elapsed.as_secs_f64() * 1000.0
        );

        canvas_finalize::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
        // 写权→1：仅 materialize；canvas 刚体平移不计。
        crate::layout::group::write_counter::warn_if_group_writes_excessive(1);
        Ok(result)
    }

    fn apply_node_frame(
        &self,
        node_align_config: &grid_snap::NodeAlignConfig,
        result: &mut LayoutResult,
    ) -> Result<(), DiagnosticError> {
        if !node_align_config.enabled {
            return Ok(());
        }

        let effective_dir = resolve_effective_direction(self.diagram);
        if effective_dir == Some("radial") {
            return Ok(());
        }

        // Phase B: 所有图类型使用 solver，solver 已处理对齐，跳过 align_nodes。
        // G3：GroupFramePass 空壳，不再改写组几何。
        let algo = self.plan.layout_algo.as_str();
        let gf_pass = GroupFramePass::resolve(self.diagram, self.plan, algo);
        gf_pass.apply_after_node_snap(self.diagram, result, algo);
        grid_snap::update_canvas_bounds(result, constants::DEFAULT_PADDING);
        Ok(())
    }

    fn run_routing_pipeline(
        &self,
        algo: &str,
        result: LayoutResult,
    ) -> Result<LayoutResult, DiagnosticError> {
        let t0 = Instant::now();
        let feedback = LayoutRouteFeedback::new(self.diagram);
        let PreRouteFeedback {
            result: mut result_v2,
        } = feedback.apply_pre_route(result, true);
        crate::perf_log!(
            "[perf]   pre-route: {:.2}ms",
            t0.elapsed().as_secs_f64() * 1000.0
        );

        let edge_routing_style =
            LayoutPlan::resolve_effective_edge_routing(self.diagram, self.plan, &result_v2.hints);
        let router = registry::build_edge_routing_strategy(edge_routing_style.as_str(), self.plan)
            .ok_or_else(|| {
                super::entry::layout_config_error(
                    self.diagram,
                    crate::types::standard_attr_keys::diagram::EDGE_ROUTING,
                    edge_routing_style.as_str(),
                    &super::entry::known_edge_routing_names(),
                )
            })?;
        let mut edge_snap_config = router.edge_snap_config();
        if let Some(false) = grid_snap::diagram_snap_attribute(self.diagram) {
            edge_snap_config.enabled = false;
        }
        // P5: 按节点密度自适应网格步长（小图精细、大图粗放，减少密集区视觉碎片）
        edge_snap_config.grid_step = grid_snap::adaptive_grid_step(result_v2.nodes.len());
        let gf_pass = GroupFramePass::resolve(self.diagram, self.plan, algo);
        // G3：Frame apply/refresh/restore 空壳；不再 sibling/expand/recompute。
        if !self.diagram.groups.is_empty() {
            gf_pass.refresh_before_route(self.diagram, &mut result_v2, algo);
        }

        // Phase 5 / D4-6：删除 spacing_demand_probe 二次求解——间距走 layout builder / SpaceBudget。

        // orthosketch 抬 side_gutters；随后把 gutters 物化进 groups（仍属同一 materialize 令牌）。
        if gf_pass.arch_post_layout && !self.diagram.groups.is_empty() {
            let prs_grew = {
                let mut shell = crate::layout::group::GroupShellMut::new(self.diagram, &mut result_v2);
                shell.feedforward_orthosketch()
            };
            crate::layout::post_route::shell_expand::commit_side_gutters_into_groups(
                self.diagram,
                &mut result_v2,
            );
            crate::layout::recipes::architecture::post_layout::reassert_multi_client_hub_centroids(
                self.diagram,
                &mut result_v2,
            );
            if let Some(debug) = result_v2.hints.gutter_budget_debug.as_mut() {
                debug.prs_grew = prs_grew;
            } else {
                result_v2.hints.gutter_budget_debug = Some(crate::layout::GutterBudgetDebug {
                    prs_grew,
                    ..Default::default()
                });
            }
        }
        // Budget guard hint（不推节点，仅设置 hints）。
        let result_v2 =
            crate::layout::demand::space_budget_guard::resolve_budget_violations(
                self.diagram, result_v2,
            );

        let refine_config = refine::RefineConfig::default();
        crate::layout::group::write_counter::warn_if_group_writes_excessive(1);
        // Slice A: FrozenNodeProduct 唯一冻结点——layout finalize 后捕获，
        // 覆盖 route + 全部后处理直到 canvas transform。
        let frozen =
            crate::layout::routing::coordinator::FrozenNodeProduct::capture(&result_v2);
        // Slice B：构造只读 PreparedRoutingInput 并记录 problem signature。
        let _sig = {
            let direction = resolve_effective_direction(self.diagram).unwrap_or("");
            let prepared = crate::layout::routing::model::PreparedRoutingInput::prepare(
                &frozen,
                self.diagram,
                &result_v2.hints,
                direction,
                edge_routing_style.as_str(),
                Default::default(),
                crate::layout::routing::model::prepared::RoutingCanvas {
                    width: result_v2.total_width,
                    height: result_v2.total_height,
                },
            );
            let sig = prepared.problem_signature();
            crate::perf_log!("[perf]   route-model: {}", prepared.signature_describe());
            sig
        };
        // RoutingCoordinator 是路由的唯一编排入口。
        let coordinator =
            crate::layout::routing::coordinator::RoutingCoordinator::new(Default::default());
        let _ = _sig;
        let t_route = Instant::now();
        // Slice D4：snap/quantize 与 D 段 finalize 已前移进 Coordinator（唯一真冻结点
        // 在 execute 内 materialize→audit→freeze）；runner 只消费返回的审计报告。
        let (mut result, route_audit) = coordinator.execute(
            self.diagram,
            &frozen,
            router.as_ref(),
            result_v2,
            &refine_config,
            &edge_snap_config,
            Default::default(), // RoutingConfig：待后续从 pipeline 传入
            self.prev,
        );
        crate::perf_log!(
            "[perf]   route: {:.2}ms",
            t_route.elapsed().as_secs_f64() * 1000.0
        );

        // G4：删除 post_route_shell_expand；残留 shell 溢出 → Degraded（不改 groups）。
        let shell_overflow = gf_pass.arch_post_layout
            && !self.diagram.groups.is_empty()
            && crate::layout::post_route::shell_expand::route_shell_overflow_remaining(
                self.diagram,
                &result,
            );

        // Slice A: 冻结校验——route 后 nodes + groups 均不可变（canvas 平移在 assert 之后）。
        frozen.assert_unchanged(&result);

        // Slice E5：R8 shadow audit（lift→materialize→round-trip debug_assert）已删除，
        // 由 Coordinator 内正式 audit（E3 repair loop + 唯一真冻结点）取代；
        // runner 只消费 Coordinator 返回的最终审计报告。
        if route_audit.degraded || shell_overflow {
            if shell_overflow {
                crate::perf_log!("[warn] route shell overflow remaining → degraded (no post_route expand)");
            }
            if route_audit.degraded {
                crate::perf_log!("[warn] route audit degraded: {}", route_audit.describe());
            }
            // Phase 6：降级显式进 hints，禁止静默（bench 经 orthogonal_debug.degraded_count 可见）
            match result.hints.orthogonal_debug.as_mut() {
                Some(stats) => {
                    stats.degraded_count = stats.degraded_count.max(1);
                }
                None => {
                    result.hints.orthogonal_debug = Some(crate::layout::OrthoDebugStats {
                        degraded_count: 1,
                        ..Default::default()
                    });
                }
            }
        }
        crate::perf_log!(
            "[solver-status] route_audit_degraded={} ortho_degraded_count={}",
            route_audit.degraded,
            result
                .hints
                .orthogonal_debug
                .as_ref()
                .map(|s| s.degraded_count)
                .unwrap_or(0)
        );

        Ok(result)
    }
}
