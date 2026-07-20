//! 布局管线编排器：将 `compute_layout_with_plan` 的多阶段逻辑结构化。

use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::layout::canvas_finalize;
use crate::layout::constants;
use crate::layout::grid_snap;
use crate::layout::group_frame::GroupFramePass;
use crate::layout::perf::Instant;
use crate::layout::plan::LayoutPlan;
use crate::layout::post_route;
use crate::layout::refine;
use crate::layout::registry;
use crate::layout::route_feedback::{LayoutRouteFeedback, PreRouteFeedback};
use crate::layout::{resolve_effective_direction, EdgeRoutingStrategy, LayoutResult, Port};
use std::collections::{HashMap, HashSet};

/// 布局管线。
pub(crate) struct LayoutPipeline<'a> {
    diagram: &'a Diagram,
    plan: &'a LayoutPlan,
}

impl<'a> LayoutPipeline<'a> {
    pub fn new(diagram: &'a Diagram, plan: &'a LayoutPlan) -> Self {
        Self { diagram, plan }
    }

    pub fn run(self) -> Result<LayoutResult, DiagnosticError> {
        let algo = self.plan.layout_algo.as_str();

        let strategy = registry::build_layout_strategy(algo, self.plan).ok_or_else(|| {
            super::layout_config_error(
                self.diagram,
                crate::types::standard_attr_keys::diagram::LAYOUT,
                algo,
                &super::known_layout_algo_names(),
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

        let horizontal = effective_dir == Some("left-to-right");

        grid_snap::align_nodes(result, node_align_config, horizontal);
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
        } = feedback.apply_pre_route(result);
        crate::perf_log!(
            "[perf]   pre-route: {:.2}ms",
            t0.elapsed().as_secs_f64() * 1000.0
        );

        let edge_routing_style =
            LayoutPlan::resolve_effective_edge_routing(self.diagram, self.plan, &result_v2.hints);
        let router = registry::build_edge_routing_strategy(edge_routing_style.as_str(), self.plan)
            .ok_or_else(|| {
                super::layout_config_error(
                    self.diagram,
                    crate::types::standard_attr_keys::diagram::EDGE_ROUTING,
                    edge_routing_style.as_str(),
                    &super::known_edge_routing_names(),
                )
            })?;
        let mut edge_snap_config = router.edge_snap_config();
        if let Some(false) = grid_snap::diagram_snap_attribute(self.diagram) {
            edge_snap_config.enabled = false;
        }
        // P5: 按节点密度自适应网格步长（小图精细、大图粗放，减少密集区视觉碎片）
        edge_snap_config.grid_step = grid_snap::adaptive_grid_step(result_v2.nodes.len());
        let gf_pass = GroupFramePass::resolve(self.diagram, self.plan, algo);
        if !self.diagram.groups.is_empty() {
            gf_pass.refresh_before_route(self.diagram, &mut result_v2, algo);
        }

        let refine_config = refine::RefineConfig::default();
        let t_route = Instant::now();
        let mut result = feedback.complete_routing(
            router.as_ref(),
            result_v2,
            &refine_config,
            &edge_snap_config,
        );
        crate::perf_log!(
            "[perf]   route: {:.2}ms",
            t_route.elapsed().as_secs_f64() * 1000.0
        );

        let t_post = Instant::now();
        // P1: 路由后仅做几何排斥（不含量化），量化推迟到管道末尾
        post_route::repulse_edges_only(&mut result.edges, &result.groups, &edge_snap_config);

        result =
            self.run_post_route_group_frame(algo, result, &gf_pass, &*router, &edge_snap_config)?;

        let hook = super::post_route::AlgoProfile::from_algo(algo).post_route_hook();
        result = hook.after_route(
            self.diagram,
            result,
            &*router,
            &gf_pass.spec,
            &edge_snap_config,
            gf_pass.padding,
        );

        // S3：PRS 后仅在契约失败时兜底；margin 来自 SpaceBudget
        let (mut result, mut moved_for_overlap) =
            crate::layout::space_budget_guard::resolve_budget_violations(self.diagram, result);
        if !result.groups.is_empty() {
            let explicit_equal = crate::layout::group_frame::has_explicit_equal_track(self.diagram);
            // budget guard 之后由 GroupFramePass 恢复完整 L1 契约；不能只做
            // content-fit recompute，否则 `track: equal` 会在管线末尾被冲掉。
            let pre_recompute_y: HashMap<String, f64> = result
                .groups
                .iter()
                .map(|(id, group)| (id.clone(), group.y))
                .collect();
            let pre_frame_nodes: HashMap<String, (f64, f64)> = result
                .nodes
                .iter()
                .map(|(id, node)| (id.clone(), (node.x, node.y)))
                .collect();
            if explicit_equal {
                gf_pass.restore_after_node_moves(self.diagram, &mut result, algo, &pre_recompute_y);
            } else {
                crate::layout::group_frame::recompute_group_bounds(
                    self.diagram,
                    &mut result,
                    gf_pass.padding,
                );
            }
            if algo == "architecture" {
                // L2.2：局部刚体重申多 client→hub 质心（Skip on 碰撞/越框）
                let hub_moved = crate::layout::node::architecture_v2::post_layout::
                    reassert_multi_client_hub_centroids(self.diagram, &mut result);
                moved_for_overlap.extend(hub_moved);
                if explicit_equal {
                    moved_for_overlap.extend(
                        crate::layout::node::architecture_v2::post_layout::
                            align_cross_scope_pendant_chains(self.diagram, &mut result),
                    );
                }
            }
            for (id, node) in &result.nodes {
                if pre_frame_nodes.get(id).is_some_and(|(x, y)| {
                    (node.x - x).abs() > super::post_route::NODE_MOVE_REROUTE_EPS
                        || (node.y - y).abs() > super::post_route::NODE_MOVE_REROUTE_EPS
                }) {
                    moved_for_overlap.insert(id.clone());
                }
            }
        }
        result = crate::layout::space_budget_guard::reroute_and_repulse(
            self.diagram,
            result,
            router.as_ref(),
            &moved_for_overlap,
            &edge_snap_config,
        );

        // A3 节点冻结屏障：step 10 之后节点坐标必须不变（此后仅改边几何/label/annotation）。
        let node_freeze = crate::layout::edge_stages::NodeFreeze::capture(&result);

        // P1: 像素量化在管道最末尾执行，仅运行一次
        let sorted_node_ids: Vec<String> = {
            let mut ids: Vec<String> = result.nodes.keys().cloned().collect();
            ids.sort();
            ids
        };
        let annotations = result.hints.route_annotations.clone();
        post_route::snap_and_repulse_edges_with_guard(
            &mut result.edges,
            &result.groups,
            &edge_snap_config,
            annotations.as_ref(),
            Some(&result.nodes),
            Some(&self.diagram.relations),
            Some(&sorted_node_ids),
        );

        // 消毒 2.0：snap/repulse 可能抖回微台阶与斜段，正交路由在量化后再消一次
        if edge_routing_style == "orthogonal" {
            let from_side: Vec<_> = result.edges.iter().map(|e| e.from_port).collect();
            let to_side: Vec<_> = result.edges.iter().map(|e| e.to_port).collect();
            // 几何已冻结：启用 overshoot Z 折合并，清理「冲过端口再折回」的多余折点。
            // 保守版（router step 4g）不合并，避免改动反馈进节点重定位扰动全局布局。
            crate::layout::edge::edge_routing_orthogonal::sanitize_orthogonal_edges_with_guard(
                &mut result.edges,
                &self.diagram.relations,
                &from_side,
                &to_side,
                true,
                annotations.as_ref(),
                Some(&result.nodes),
                Some(&sorted_node_ids),
            );
            // V3a / P3.1 + 轨道 A：D 末正反向 gap 审计 + 同侧 dock 共锚（sanitize 之后；C 预修在 phase_lane 末）。
            self.enforce_d_stage_separation(&mut result, &from_side, &to_side);

            // L3：architecture 在 C 期 stub_occ 仅诊断；节点已冻结后于 D 末做 exact 跨对共柱真修。
            // 只改边几何 → node_fp 不变；须在 label 避让前完成并刷新 annotation。
            if algo == "architecture" {
                let stub_gap = crate::layout::edge::parallel_gap_for_diagram(
                    self.diagram.diagram_type.clone(),
                );
                let stub_stats = crate::layout::edge::edge_routing_orthogonal::
                    resolve_exact_stub_occupancy_post_route(
                        &mut result.edges,
                        &self.diagram.relations,
                        &from_side,
                        &to_side,
                        &result.nodes,
                        stub_gap,
                    );
                if stub_stats.stubs_shifted > 0 {
                    let prev = result.hints.route_annotations.clone();
                    result.hints.route_annotations = Some(
                        crate::layout::edge::refresh_route_annotations_preserving_semantics(
                            &result.edges,
                            &from_side,
                            &to_side,
                            prev.as_ref(),
                        ),
                    );
                    crate::perf_log!(
                        "[perf]     d_stub_exact_post_route: shifted={} unresolved_exact={} degraded={}",
                        stub_stats.stubs_shifted,
                        stub_stats.unresolved_conflicts,
                        stub_stats.degraded
                    );
                }
            }

            // L5.1：几何冻结后、label 前 —— 对仍 through 的边做保组 dogleg 试修。
            // refine 期有组图常看不到最终 through（后处理才引入）；此处与 lint 对齐。
            {
                let t_through = crate::layout::perf::Instant::now();
                crate::layout::refine::repair_through_edges_post_route(self.diagram, &mut result);
                crate::perf_log!(
                    "[perf]     d_through_repair: {:.2}ms",
                    t_through.elapsed().as_secs_f64() * 1000.0
                );
            }

            // 仅 lint 穿组边：局部裙边/两跳 dogleg（禁全局建廊剪枝）。
            {
                let t_group = crate::layout::perf::Instant::now();
                crate::layout::refine::repair_group_interior_edges_post_route(
                    self.diagram,
                    &mut result,
                );
                crate::perf_log!(
                    "[perf]     d_group_interior_repair: {:.2}ms",
                    t_group.elapsed().as_secs_f64() * 1000.0
                );
            }

            // 平行 trunk 分槽：repair 可能重建路径造成新非语义共线，在此（冻结后）按
            // lint 口径最终分离。仅 architecture，回退保 through/穿组不劣化。
            {
                let t_sep = crate::layout::perf::Instant::now();
                crate::layout::refine::separate_trunk_overlaps_post_route(
                    self.diagram,
                    &mut result,
                );
                crate::perf_log!(
                    "[perf]     d_trunk_separate: {:.2}ms",
                    t_sep.elapsed().as_secs_f64() * 1000.0
                );
            }

            // N2：repair 后 lint 同语义只读复校（记账 / degraded，不扩写几何）。
            crate::layout::refine::recheck_lint_pierce_post_freeze(self.diagram, &mut result);

            // A3 折线冻结屏障：step 16 之后仅允许改 label/annotation，不得再动折点。
            let polyline_freeze = crate::layout::edge_stages::PolylineFreeze::capture(&result);

            // 标签避让必须是几何冻结后的**最终**步骤：sanitize 会按平行边规则
            // 重建所有标签；snap/repulse 又移动了路径。router 内不再提前 resolve（P3.3）。
            let label_config =
                crate::layout::edge::common::label_candidate::LabelPlacementConfig::for_diagram(
                    self.diagram.diagram_type.clone(),
                    !result.groups.is_empty(),
                );
            crate::layout::edge::common::label_avoidance::resolve_label_overlaps_with_config(
                &mut result.edges,
                &result.nodes,
                &result.groups,
                label_config,
            );
            if let Some(annotations) = result.hints.route_annotations.as_ref() {
                crate::layout::edge::common::label_avoidance::dedupe_labels_on_declared_merges(
                    &mut result.edges,
                    annotations,
                );
            }

            // A3 折线冻结校验：label 阶段若改动了折点则打 warning（软校验，不 panic）。
            polyline_freeze.warn_if_changed(&result);
        }



        crate::perf_log!(
            "[perf]   post-process: {:.2}ms",
            t_post.elapsed().as_secs_f64() * 1000.0
        );

        // A3 节点冻结校验：后处理尾段（step 11-18）不得挪节点。
        node_freeze.assert_unchanged(&result);

        Ok(result)
    }

    /// V3a / P3.1 + 轨道 A：D 末正反向 gap 审计（min_gap）+ 同侧 dock 共锚（dock_sep）。
    /// sanitize 之后、label 之前；使用图类型对应 gap 避免 architecture 误用 flowchart 8px。
    /// 内部仍分别走 [`enforce_reverse_pair_min_gap`] + [`enforce_reverse_pair_dock_separation`]，
    /// 几何输出与两次独立调用字节级一致；仅编排入口合并以减少 pipeline.rs 重复 bookkeeping。
    fn enforce_d_stage_separation(&self, result: &mut LayoutResult, from_side: &[Port], to_side: &[Port]) {
        let parallel_gap =
            crate::layout::edge::parallel_gap_for_diagram(self.diagram.diagram_type.clone());
        crate::layout::edge::edge_routing_orthogonal::enforce_reverse_pair_min_gap(
            &mut result.edges, &self.diagram.relations, parallel_gap,
        );
        let dock_gap = parallel_gap
            .max(crate::layout::edge::edge_routing_orthogonal::COMPACT_SLOT_PITCH);
        crate::layout::edge::edge_routing_orthogonal::enforce_reverse_pair_dock_separation(
            &mut result.edges, &self.diagram.relations, &result.nodes,
            from_side, to_side, dock_gap,
        );
    }

    fn run_post_route_group_frame(
        &self,
        algo: &str,
        mut result: LayoutResult,
        gf_pass: &GroupFramePass,
        router: &dyn EdgeRoutingStrategy,
        edge_snap_config: &grid_snap::EdgeSnapConfig,
    ) -> Result<LayoutResult, DiagnosticError> {
        if result.groups.is_empty() {
            return Ok(result);
        }

        let pre_recompute_y: HashMap<String, f64> = result
            .groups
            .iter()
            .map(|(id, g)| (id.clone(), g.y))
            .collect();
        let pre_gf_positions: HashMap<String, (f64, f64)> = result
            .nodes
            .iter()
            .map(|(id, n)| (id.clone(), (n.x, n.y)))
            .collect();

        gf_pass.restore_after_node_moves(self.diagram, &mut result, algo, &pre_recompute_y);

        let max_node_disp = result
            .nodes
            .iter()
            .map(|(id, n)| {
                pre_gf_positions
                    .get(id)
                    .map(|(px, py)| {
                        let dx = n.x - px;
                        let dy = n.y - py;
                        (dx * dx + dy * dy).sqrt()
                    })
                    .unwrap_or(f64::MAX)
            })
            .fold(0.0f64, f64::max);

        if max_node_disp >= super::post_route::NODE_MOVE_REROUTE_EPS {
            let moved_nodes: HashSet<String> = result
                .nodes
                .iter()
                .filter_map(|(id, n)| {
                    pre_gf_positions.get(id).and_then(|(px, py)| {
                        let dx = n.x - px;
                        let dy = n.y - py;
                        if (dx * dx + dy * dy).sqrt() >= super::post_route::NODE_MOVE_REROUTE_EPS {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                })
                .collect();
            result = router.route_after_node_moves(self.diagram, result, &moved_nodes);

            // P1: 组框修复后仅做几何排斥，量化推迟到管道末尾
            post_route::repulse_edges_only(&mut result.edges, &result.groups, edge_snap_config);
        } else {
            // P1: 无重路由时也仅做几何排斥
            post_route::repulse_edges_only(&mut result.edges, &result.groups, edge_snap_config);
        }

        grid_snap::update_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
        Ok(result)
    }
}
