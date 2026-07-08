//! 布局管线编排器：将 `compute_layout_with_plan_and_overlay` 的多阶段逻辑结构化。

use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::layout::constants;
use crate::layout::edge_postprocess;
use crate::layout::geometry::Point;
use crate::layout::grid_snap;
use crate::layout::group_frame::GroupFramePass;
use crate::layout::intent::{self, IntentStatus, LayoutIntentOverlay, RefinementReport};
use crate::layout::plan::LayoutPlan;
use crate::layout::postprocess;
use crate::layout::refine;
use crate::layout::registry;
use crate::layout::route_feedback::{LayoutRouteFeedback, PreRouteFeedback};
use crate::layout::{resolve_effective_direction, EdgeRoutingStrategy, LayoutResult};
use std::collections::{HashMap, HashSet};
use crate::layout::perf::Instant;

/// 带意图叠加层的布局管线。
pub(crate) struct LayoutPipeline<'a> {
    diagram: &'a Diagram,
    plan: &'a LayoutPlan,
    overlay: Option<&'a LayoutIntentOverlay>,
}

impl<'a> LayoutPipeline<'a> {
    pub fn new(
        diagram: &'a Diagram,
        plan: &'a LayoutPlan,
        overlay: Option<&'a LayoutIntentOverlay>,
    ) -> Self {
        Self {
            diagram,
            plan,
            overlay,
        }
    }

    pub fn run(self) -> Result<(LayoutResult, Option<RefinementReport>), DiagnosticError> {
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

        let mut report = RefinementReport::default();
        let valid_topology = self.validate_topology_intents(&mut report);

        let t_layout = Instant::now();
        let mut result = strategy.compute_with_overlay(self.diagram, Some(&valid_topology));
        let layout_elapsed = t_layout.elapsed();
        crate::perf_log!("[perf] layout: {:.2}ms", layout_elapsed.as_secs_f64() * 1000.0);

        self.evaluate_topology_satisfaction(&valid_topology, &mut result, &mut report);

        let mut pinned = intent::PinSet::default();
        if let Some(ov) = self.overlay {
            let geo_report =
                intent::geometric::apply_geometric_refinement(&mut result, ov, &mut pinned, self.diagram);
            report.merge(geo_report);
        }

        self.apply_node_frame(&node_align_config, &mut result, &pinned)?;

        if produces_edges {
            postprocess::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
            let report_opt = if self.overlay.is_some() {
                Some(report)
            } else {
                None
            };
            return Ok((result, report_opt));
        }

        let t_routing = Instant::now();
        let mut result = self.run_routing_pipeline(algo, result, &pinned, &mut report)?;
        let routing_elapsed = t_routing.elapsed();
        crate::perf_log!("[perf] routing: {:.2}ms", routing_elapsed.as_secs_f64() * 1000.0);

        postprocess::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);

        let report_opt = if self.overlay.is_some() {
            Some(report)
        } else {
            None
        };
        Ok((result, report_opt))
    }

    fn validate_topology_intents(&self, report: &mut RefinementReport) -> Vec<intent::topology::ValidTopologyIntent> {
        if let Some(ov) = self.overlay {
            let (valid, validation_results) =
                intent::topology::validate_topology_intents(self.diagram, ov);
            for r in validation_results {
                report.push(r.index, r.kind, r.status, r.message);
            }
            valid
        } else {
            Vec::new()
        }
    }

    fn evaluate_topology_satisfaction(
        &self,
        valid_topology: &[intent::topology::ValidTopologyIntent],
        result: &LayoutResult,
        report: &mut RefinementReport,
    ) {
        if valid_topology.is_empty() {
            return;
        }

        let skipped: HashSet<usize> = result.hints.skipped_topology_intents.iter().copied().collect();

        if let Some(ranks) = &result.hints.sugiyama_ranks {
            let satisfaction =
                intent::topology::evaluate_topology_satisfaction(valid_topology, ranks);
            for r in satisfaction {
                if skipped.contains(&r.index) {
                    report.push(
                        r.index,
                        r.kind,
                        IntentStatus::Partial,
                        Some("cross-group topology intent not supported in first phase".into()),
                    );
                } else {
                    report.push(r.index, r.kind, r.status, r.message);
                }
            }
        } else {
            for v in valid_topology {
                if skipped.contains(&v.index) {
                    report.push(
                        v.index,
                        v.kind,
                        IntentStatus::Partial,
                        Some("cross-group topology intent not supported in first phase".into()),
                    );
                } else {
                    report.push(
                        v.index,
                        v.kind,
                        IntentStatus::Partial,
                        Some("layout algorithm does not expose rank information".into()),
                    );
                }
            }
        }
    }

    fn apply_node_frame(
        &self,
        node_align_config: &grid_snap::NodeAlignConfig,
        result: &mut LayoutResult,
        pinned: &intent::PinSet,
    ) -> Result<(), DiagnosticError> {
        if !node_align_config.enabled {
            return Ok(());
        }

        let effective_dir = resolve_effective_direction(self.diagram);
        if effective_dir == Some("radial") {
            return Ok(());
        }

        let horizontal = effective_dir == Some("left-to-right");

        grid_snap::align_nodes(result, node_align_config, horizontal, pinned);
        let algo = self.plan.layout_algo.as_str();
        let gf_pass = GroupFramePass::resolve(self.diagram, self.plan, algo);
        gf_pass.apply_after_node_snap(self.diagram, result, pinned, algo);
        grid_snap::update_canvas_bounds(result, constants::DEFAULT_PADDING);
        Ok(())
    }

    fn run_routing_pipeline(
        &self,
        algo: &str,
        result: LayoutResult,
        pinned: &intent::PinSet,
        report: &mut RefinementReport,
    ) -> Result<LayoutResult, DiagnosticError> {
        let t0 = Instant::now();
        let feedback = LayoutRouteFeedback::new(self.diagram, self.plan, algo);
        let PreRouteFeedback {
            result: mut result_v2,
        } = feedback.apply_pre_route(result);
        crate::perf_log!("[perf]   pre-route: {:.2}ms", t0.elapsed().as_secs_f64() * 1000.0);

        let edge_routing_style =
            LayoutPlan::resolve_effective_edge_routing(self.diagram, self.plan, &result_v2.hints);
        let router = registry::build_edge_routing_strategy(edge_routing_style.as_str(), self.plan).ok_or_else(
            || {
                super::layout_config_error(
                    self.diagram,
                    crate::types::standard_attr_keys::diagram::EDGE_ROUTING,
                    edge_routing_style.as_str(),
                    &super::known_edge_routing_names(),
                )
            },
        )?;
        let mut edge_snap_config = router.edge_snap_config();
        if let Some(false) = grid_snap::diagram_snap_attribute(self.diagram) {
            edge_snap_config.enabled = false;
        }
        // P5: 按节点密度自适应网格步长（小图精细、大图粗放，减少密集区视觉碎片）
        edge_snap_config.grid_step = grid_snap::adaptive_grid_step(result_v2.nodes.len());
        let is_orthogonal = router.name() == "orthogonal";

        let gf_pass = GroupFramePass::resolve(self.diagram, self.plan, algo);
        if !self.diagram.groups.is_empty() {
            gf_pass.refresh_before_route(self.diagram, &mut result_v2, pinned, algo);
        }

        let refine_config = refine::RefineConfig::default();
        let t_route = Instant::now();
        let mut result = feedback.complete_routing(
            router.as_ref(),
            result_v2,
            &refine_config,
        );
        crate::perf_log!("[perf]   route: {:.2}ms", t_route.elapsed().as_secs_f64() * 1000.0);

        let t_post = Instant::now();
        // P1: 路由后仅做几何排斥（不含量化），量化推迟到管道末尾
        edge_postprocess::repulse_edges_only(
            &mut result.edges,
            &result.groups,
            &edge_snap_config,
        );

        if let Some(ov) = self.overlay {
            if !pinned.aligned_vertical.is_empty() || !pinned.aligned_horizontal.is_empty() {
                intent::geometric::check_alignment_after_refine(&result, pinned, ov, report);
            }
        }

        result = self.run_post_route_group_frame(algo, result, pinned, &gf_pass, &*router, &edge_snap_config)?;

        if is_orthogonal && self.plan.edge_bundling.enabled {
            result = self.apply_edge_bundling(result)?;
        }

        // P1: 像素量化在管道最末尾执行（bundling 之后），仅运行一次
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
        pinned: &intent::PinSet,
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

        gf_pass.restore_after_node_moves(self.diagram, &mut result, pinned, algo, &pre_recompute_y);

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

    fn apply_edge_bundling(&self, mut result: LayoutResult) -> Result<LayoutResult, DiagnosticError> {
        let ranks = result.hints.sugiyama_ranks.as_ref();
        let features: Vec<crate::layout::edge::edge_bundling::EdgeFeatures> =
            (0..result.edges.len())
                .map(|i| {
                    let rel = &self.diagram.relations[i];
                    let pts = result.edges[i].path_points();
                    crate::layout::edge::edge_bundling::EdgeFeatures::extract(
                        i,
                        rel,
                        &result.nodes,
                        ranks,
                        &pts,
                    )
                    .unwrap_or_else(|| crate::layout::edge::edge_bundling::EdgeFeatures {
                        edge_index: i,
                        from_id: format!("_skip_{}", i),
                        to_id: format!("_skip_{}", i),
                        from_center: Point::new(0.0, 0.0),
                        to_center: Point::new(0.0, 0.0),
                        from_rank: None,
                        to_rank: None,
                        arrow_tag: "active",
                        line_style: String::new(),
                        stroke_color: String::new(),
                        stroke_width: String::new(),
                        path_length: 0.0,
                        path_points: Vec::new(),
                        direction: (1.0, 0.0),
                        has_label: false,
                        label_text: None,
                    })
                })
                .collect();

        let (bundling_result, bundling_debug) = crate::layout::edge::edge_bundling::apply_bundling(
            &mut result.edges,
            &features,
            &result.nodes,
            &self.plan.edge_bundling,
        );

        result.hints.edge_bundling = Some(crate::layout::edge::edge_bundling::EdgeBundlingHints {
            result: bundling_result,
            debug: bundling_debug,
        });

        crate::layout::edge::edge_bundling::label_placement::relayout_edge_labels_after_bundling(
            self.diagram,
            &mut result.edges,
            &result.hints.edge_bundling.as_ref().unwrap().result,
            &self.plan.edge_bundling,
            &result.nodes,
            &result.groups,
        );

        if matches!(
            self.diagram.diagram_type,
            crate::types::DiagramType::Architecture
        ) && self.plan.edge_bundling.enabled
        {
            let sorted_node_ids: Vec<String> = {
                let mut ids: Vec<String> = result.nodes.keys().cloned().collect();
                ids.sort();
                ids
            };
            crate::layout::edge::edge_routing_orthogonal::separate_unrelated_architecture_trunks_after_bundling(
                &self.diagram.relations,
                &mut result.edges,
                &result.nodes,
                &sorted_node_ids,
            );
        }

        Ok(result)
    }
}
