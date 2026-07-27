//! `AtlasPipeline`（23 号文 Stage 6：四 Dialect + 落笔）。
//!
//! 全部图种默认走 Atlas；Hierarchical 走通道 Ink，其余委托 recipe 布局/路由
//!（仍挂在 Atlas 管线下，不再因图种回落 Legacy）。

use std::sync::Arc;

use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::layout::atlas::dialect::{
    compile_atlas, AtlasContract, DialectKind, HierarchicalPreset,
};
use crate::layout::atlas::plan::Plan;
use crate::layout::pipeline::plan::LayoutPlan;
use crate::layout::types::LayoutResult;

use super::solve;

/// Atlas 管线入口（与 `LayoutPipeline` 同签名，供 entry 分发）。
pub struct AtlasPipeline<'a> {
    diagram: &'a Diagram,
    plan: &'a LayoutPlan,
    /// Stage 6：上次 Plan（拓扑不变时供增量入口使用）。
    prev_plan: Option<&'a Plan>,
}

impl<'a> AtlasPipeline<'a> {
    pub fn new(diagram: &'a Diagram, plan: &'a LayoutPlan) -> Self {
        Self {
            diagram,
            plan,
            prev_plan: None,
        }
    }

    pub fn with_prev_plan(mut self, prev: &'a Plan) -> Self {
        self.prev_plan = Some(prev);
        self
    }

    pub fn run(self) -> Result<LayoutResult, DiagnosticError> {
        let (scheme, contract) = compile_atlas(self.diagram);
        crate::perf_log!(
            "[atlas] dialect={} scheme={}",
            match scheme.kind {
                DialectKind::Hierarchical => "hierarchical",
                DialectKind::Tree => "tree",
                DialectKind::Sequence => "sequence",
                DialectKind::Circular => "circular",
            },
            scheme.id.0
        );

        if let Some(prev) = self.prev_plan {
            // 拓扑粗检：实体序一致则后续 metric 路径可跳过相 I 选路
            let ids = entity_ids_in_order(self.diagram);
            let slots_match = prev.node_slots.len() == ids.len()
                && prev.node_slots.keys().zip(ids.iter()).all(|(a, b)| a == b);
            if slots_match && !prev.channels.is_empty() {
                crate::perf_log!("[atlas] prev Plan present → attempt phase I skip");
            }
        }

        let result = match &contract {
            AtlasContract::Hierarchical(hc) => self.run_hierarchical(hc)?,
            AtlasContract::Tree { .. }
            | AtlasContract::Sequence { .. }
            | AtlasContract::Circular { .. } => {
                // Tree / Sequence / Circular：复用现有 recipe + 路由（BuiltinEdges 自动跳过路由）
                // Wave3 记债：非 Hier Ink 内化前仍委托 LayoutPipeline
                // R4：非 Hier 不写入假 atlas_plan（channels 空 / rank=0）
                crate::layout::pipeline::runner::LayoutPipeline::new(self.diagram, self.plan).run()?
            }
        };

        Ok(result)
    }

    fn run_hierarchical(
        &self,
        contract: &crate::layout::atlas::dialect::HierarchicalContract,
    ) -> Result<LayoutResult, DiagnosticError> {
        use crate::layout::constants;
        use crate::layout::perf::Instant;
        use crate::layout::resolve_effective_direction;
        use crate::layout::routing::coordinator::FrozenNodeProduct;
        use crate::layout::snap::{canvas_finalize, grid_snap};

        crate::layout::group::write_counter::reset_group_write_counters();

        let t_layout = Instant::now();
        let config = crate::layout::algorithm_config::SugiyamaLayoutConfig::from_options(
            &self.plan.layout_options,
        );
        let output = solve::solve_from_contract_with_prev(
            self.diagram,
            contract,
            &config,
            self.prev_plan,
        );
        let mut result = solve::assemble_layout_result(&output, self.diagram);
        crate::perf_log!(
            "[perf] atlas layout: {:.2}ms (groups={}, write_counter={}, scheme={})",
            t_layout.elapsed().as_secs_f64() * 1000.0,
            result.groups.len(),
            crate::layout::group::write_counter::group_write_count(),
            output.contract.scheme_id.0
        );

        let node_align_config = match output.contract.profile.preset {
            HierarchicalPreset::Architecture => grid_snap::NodeAlignConfig::default_architecture(),
            HierarchicalPreset::Flowchart | HierarchicalPreset::State => {
                grid_snap::NodeAlignConfig::default_flowchart()
            }
        };
        let mut node_align_config = node_align_config;
        if let Some(override_mode) = grid_snap::diagram_align_override(self.diagram) {
            node_align_config.apply_diagram_override(override_mode);
        }
        if node_align_config.enabled {
            let effective_dir = resolve_effective_direction(self.diagram);
            if effective_dir != Some("radial") {
                grid_snap::update_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
            }
        }

        let frozen = FrozenNodeProduct::capture(&result);

        let t_routing = Instant::now();
        if let Some(metric) = &output.channel {
            if let Some(prev) = self.prev_plan {
                if crate::layout::atlas::plan::diff(prev, &metric.plan).is_empty() {
                    crate::perf_log!(
                        "[atlas] hierarchical channel Plan unchanged vs prev (phase I skipped or identical)"
                    );
                }
            }
            if let Err(e) =
                super::provenance_check::assert_channel_provenance_coverage(&metric.plan)
            {
                crate::perf_log!("[atlas] provenance coverage failed before ink: {e:?}");
                return Err(DiagnosticError::layout_failed(
                    crate::ast::Span::dummy(),
                    format!("atlas plan provenance incomplete before ink: {e}"),
                ));
            }
            // L6：基底无穿组段（由构造保证的静态反证）
            let penetrations = metric.substrate.verify_no_group_penetration();
            if !penetrations.is_empty() {
                crate::perf_log!(
                    "[atlas] L6 group penetration before ink: {} violation(s)",
                    penetrations.len()
                );
                return Err(DiagnosticError::layout_failed(
                    crate::ast::Span::dummy(),
                    format!(
                        "atlas substrate group penetration: {} violation(s)",
                        penetrations.len()
                    ),
                ));
            }
            let ink_plan = {
                let node_rects: std::collections::BTreeMap<String, (f64, f64, f64, f64)> = result
                    .nodes
                    .iter()
                    .map(|(id, n)| (id.clone(), (n.x, n.y, n.width, n.height)))
                    .collect();
                let mut plan = metric.plan.clone();
                plan.assign_port_along_offsets(&node_rects);
                plan
            };
            result.edges = super::ink::materialize_edges(
                self.diagram,
                &ink_plan,
                &metric.substrate,
                &result.nodes,
                &result.groups,
                &output.track_coords,
            );
            result.hints.atlas_plan = Some(Arc::new(ink_plan.clone()));
            crate::perf_log!(
                "[atlas] ink materialize: {} edges (channels={})",
                result.edges.len(),
                ink_plan.channels.len()
            );
            {
                let pre = super::ink_verify::verify_ink_vs_plan(
                    &ink_plan,
                    &metric.substrate,
                    &result.nodes,
                    &result.edges,
                    &output.track_coords,
                    &std::collections::BTreeSet::new(),
                    Some(&result.groups),
                );
                if !pre.is_empty() {
                    crate::perf_log!("[atlas/ink-verify] pre-ink: {}", pre.len());
                }
            }
            // M7 后期：第三道 dogleg 已删；穿组由 Ink 守 gate/裙边 + M7-2 + lint 承担。
            result.hints.atlas_plan_distorted_edges.clear();
            {
                let post = super::ink_verify::verify_ink_vs_plan(
                    &ink_plan,
                    &metric.substrate,
                    &result.nodes,
                    &result.edges,
                    &output.track_coords,
                    &std::collections::BTreeSet::new(),
                    Some(&result.groups),
                );
                let (geom, dist_n) = super::ink_verify::partition_violations(&post);
                let hard = super::ink_verify::hard_geom_count(&post);
                if dist_n > 0 || geom > hard {
                    crate::perf_log!(
                        "[atlas/ink-verify] post-ink: soft_other={}",
                        geom.saturating_sub(hard)
                    );
                }
                // M7-2：几何硬 FAIL（含 PortSideMismatch）；PlanDistorted 仍软
                if hard > 0 {
                    let sample: Vec<String> = post
                        .iter()
                        .filter(|v| {
                            matches!(
                                v,
                                super::ink_verify::InkPlanViolation::MissingGeometry(_)
                                    | super::ink_verify::InkPlanViolation::NonOrthogonal(_)
                                    | super::ink_verify::InkPlanViolation::CorridorMiss { .. }
                                    | super::ink_verify::InkPlanViolation::GateMiss { .. }
                                    | super::ink_verify::InkPlanViolation::PortSideMismatch { .. }
                                    | super::ink_verify::InkPlanViolation::Scope(_, _)
                            )
                        })
                        .take(8)
                        .map(|v| format!("{v:?}"))
                        .collect();
                    crate::perf_log!(
                        "[atlas/ink-verify] post-ink: hard_geom={hard} → fail sample={sample:?}"
                    );
                    return Err(DiagnosticError::layout_failed(
                        crate::ast::Span::dummy(),
                        format!(
                            "atlas ink↔plan geometric mismatch: {hard} hard violation(s); sample={sample:?}"
                        ),
                    ));
                }
            }
            let non_ortho = super::ink::audit_orthogonal_segments(&result.edges);
            if non_ortho > 0 {
                crate::perf_log!("[atlas] ink ortho audit: {non_ortho} non-axis segment(s)");
            }
        } else {
            // Hierarchical 落笔依赖 channel metric；禁止静默空边（Post-S7 Wave0）
            let detail = output
                .relaxation
                .steps
                .iter()
                .rev()
                .find(|s| {
                    s.provenance.producer.contains("channel")
                        || s.provenance
                            .detail
                            .as_deref()
                            .is_some_and(|d| d.contains("channel") || d.contains("build failed"))
                })
                .and_then(|s| s.provenance.detail.clone())
                .unwrap_or_else(|| "channel metric missing".into());
            crate::perf_log!("[atlas] ink aborted: no channel metric ({detail})");
            return Err(DiagnosticError::layout_failed(
                crate::ast::Span::dummy(),
                format!("atlas hierarchical requires channel metric before ink: {detail}"),
            ));
        }

        {
            let label_config =
                crate::layout::routing::common::label_candidate::LabelPlacementConfig::for_diagram(
                    self.diagram.diagram_type.clone(),
                    !result.groups.is_empty(),
                );
            let assignment = crate::layout::routing::recipe::LabelSolver::solve(
                crate::layout::routing::recipe::LabelProblem {
                    edges: &mut result.edges,
                    nodes: &result.nodes,
                    groups: &result.groups,
                    config: label_config,
                    merge_annotations: result.hints.route_annotations.as_ref(),
                },
            );
            crate::perf_log!(
                "[perf]   label solve: conflicts_remaining={} degraded={:?} signature={:016x}",
                assignment.conflicts_remaining,
                assignment.degraded,
                assignment.signature
            );
        }

        crate::perf_log!(
            "[perf] atlas routing(ink): {:.2}ms",
            t_routing.elapsed().as_secs_f64() * 1000.0
        );

        frozen.assert_unchanged(&result);
        // M5：规范 → 画布 LTR（节点/边/组/label）；须在 frozen assert 之后
        if crate::layout::orientation::needs_axis_transpose(self.diagram) {
            crate::layout::orientation::apply_layout_orientation(&mut result);
            crate::perf_log!("[atlas] orientation: left-to-right axis transpose");
        }
        canvas_finalize::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
        let write_thresh = match self.diagram.diagram_type {
            crate::types::DiagramType::Architecture => 1,
            _ => 0,
        };
        crate::layout::group::write_counter::warn_if_group_writes_excessive(write_thresh);

        Ok(result)
    }
}

/// 实体 id 插入序（确定性粗检用；非假 Plan）。
fn entity_ids_in_order(diagram: &Diagram) -> Vec<String> {
    diagram
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect()
}
