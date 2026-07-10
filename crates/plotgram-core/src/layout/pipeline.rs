//! 布局管线编排器：将 `compute_layout_with_plan` 的多阶段逻辑结构化。

use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::layout::constants;
use crate::layout::edge_postprocess;
use crate::layout::grid_snap;
use crate::layout::group_frame::GroupFramePass;
use crate::layout::plan::LayoutPlan;
use crate::layout::postprocess;
use crate::layout::refine;
use crate::layout::registry;
use crate::layout::route_feedback::{LayoutRouteFeedback, PreRouteFeedback};
use crate::layout::{resolve_effective_direction, EdgeRoutingStrategy, LayoutResult};
use crate::layout::perf::Instant;
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
        crate::perf_log!("[perf] layout: {:.2}ms", layout_elapsed.as_secs_f64() * 1000.0);

        self.apply_node_frame(&node_align_config, &mut result)?;

        if produces_edges {
            postprocess::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
            return Ok(result);
        }

        let t_routing = Instant::now();
        let mut result = self.run_routing_pipeline(algo, result)?;
        let routing_elapsed = t_routing.elapsed();
        crate::perf_log!("[perf] routing: {:.2}ms", routing_elapsed.as_secs_f64() * 1000.0);

        postprocess::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
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
        let feedback = LayoutRouteFeedback::new(self.diagram, self.plan, algo);
        let PreRouteFeedback {
            result: mut result_v2,
        } = feedback.apply_pre_route(result);
        crate::perf_log!("[perf]   pre-route: {:.2}ms", t0.elapsed().as_secs_f64() * 1000.0);

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
        let mut result = feedback.complete_routing(router.as_ref(), result_v2, &refine_config);
        crate::perf_log!("[perf]   route: {:.2}ms", t_route.elapsed().as_secs_f64() * 1000.0);

        let t_post = Instant::now();
        // P1: 路由后仅做几何排斥（不含量化），量化推迟到管道末尾
        edge_postprocess::repulse_edges_only(
            &mut result.edges,
            &result.groups,
            &edge_snap_config,
        );

        result = self.run_post_route_group_frame(algo, result, &gf_pass, &*router, &edge_snap_config)?;

        if algo == "architecture" {
            let t_prs = Instant::now();
            let prs_grew = crate::layout::group::post_route_shell::post_route_shell_expand(
                self.diagram,
                &mut result,
            );
            let prs_ms = t_prs.elapsed().as_secs_f64() * 1000.0;
            if prs_grew {
                let pre_positions: HashMap<String, (f64, f64)> = result
                    .nodes
                    .iter()
                    .map(|(id, n)| (id.clone(), (n.x, n.y)))
                    .collect();
                crate::layout::group_frame::resolve_all_sibling_overlaps(
                    &gf_pass.spec,
                    self.diagram,
                    &mut result,
                );
                let moved_nodes: HashSet<String> = result
                    .nodes
                    .iter()
                    .filter_map(|(id, n)| {
                        pre_positions.get(id).and_then(|(px, py)| {
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
                if !moved_nodes.is_empty() {
                    result = router.route_after_node_moves(self.diagram, result, &moved_nodes);
                }
            }
            // 无论 PRS 是否扩壳，都做一次内容包络安全网（leaf + 容器 + padding）。
            let leaf_pad = gf_pass.padding;
            let container_pad =
                crate::layout::node::common::group_bounds::container_padding_for_leaf(leaf_pad);
            crate::layout::group_frame::expand_groups_to_contain_contents(
                self.diagram,
                &mut result.groups,
                &result.nodes,
                leaf_pad,
                container_pad,
            );
            grid_snap::update_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
            if prs_grew {
                edge_postprocess::repulse_edges_only(
                    &mut result.edges,
                    &result.groups,
                    &edge_snap_config,
                );
            }
            if let Some(debug) = result.hints.gutter_budget_debug.as_mut() {
                debug.prs_ms = prs_ms;
                debug.prs_grew = prs_grew;
            } else {
                result.hints.gutter_budget_debug = Some(crate::layout::GutterBudgetDebug {
                    prs_ms,
                    prs_grew,
                    ..Default::default()
                });
            }
        }

        // P1: 像素量化在管道最末尾执行，仅运行一次
        edge_postprocess::snap_and_repulse_edges(
            &mut result.edges,
            &result.groups,
            &edge_snap_config,
        );

        crate::perf_log!("[perf]   post-process: {:.2}ms", t_post.elapsed().as_secs_f64() * 1000.0);

        Ok(result)
    }

    fn run_post_route_group_frame(
        &self,
        algo: &str,
        mut result: LayoutResult,
        gf_pass: &GroupFramePass,
        router: &dyn EdgeRoutingStrategy,
        edge_snap_config: &grid_snap::EdgeSnapConfig,
    ) -> Result<LayoutResult, DiagnosticError> {
        if !edge_snap_config.enabled || result.groups.is_empty() {
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

        if max_node_disp >= 1.0 {
            let moved_nodes: HashSet<String> = result
                .nodes
                .iter()
                .filter_map(|(id, n)| {
                    pre_gf_positions.get(id).and_then(|(px, py)| {
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
            result = router.route_after_node_moves(self.diagram, result, &moved_nodes);

            // P1: 组框修复后仅做几何排斥，量化推迟到管道末尾
            edge_postprocess::repulse_edges_only(
                &mut result.edges,
                &result.groups,
                edge_snap_config,
            );
        } else {
            // P1: 无重路由时也仅做几何排斥
            edge_postprocess::repulse_edges_only(
                &mut result.edges,
                &result.groups,
                edge_snap_config,
            );
        }

        grid_snap::update_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
        Ok(result)
    }
}
