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
use crate::layout::atlas::plan::{Plan, Slot, SubstrateSketch};
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
            let topo = topology_plan(self.diagram);
            let slots_match = prev.node_slots.len() == topo.node_slots.len()
                && prev
                    .node_slots
                    .keys()
                    .zip(topo.node_slots.keys())
                    .all(|(a, b)| a == b);
            if slots_match && !prev.channels.is_empty() {
                crate::perf_log!("[atlas] prev Plan present → attempt phase I skip");
            }
        }

        let mut result = match &contract {
            AtlasContract::Hierarchical(hc) => self.run_hierarchical(hc)?,
            AtlasContract::Tree { .. }
            | AtlasContract::Sequence { .. }
            | AtlasContract::Circular { .. } => {
                // Tree / Sequence / Circular：复用现有 recipe + 路由（BuiltinEdges 自动跳过路由）
                // Wave3 记债：非 Hier Ink 内化前仍委托 LayoutPipeline
                crate::layout::pipeline::runner::LayoutPipeline::new(self.diagram, self.plan).run()?
            }
        };

        if result.hints.atlas_plan.is_none() {
            result.hints.atlas_plan = Some(Arc::new(topology_plan(self.diagram)));
        }
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
            result.edges = super::ink::materialize_edges(
                self.diagram,
                &metric.plan,
                &metric.substrate,
                &result.nodes,
                &output.track_coords,
            );
            result.hints.atlas_plan = Some(Arc::new(metric.plan.clone()));
            crate::perf_log!(
                "[atlas] ink materialize: {} edges (channels={})",
                result.edges.len(),
                metric.plan.channels.len()
            );
            // Ink 几何可能与组框 AABB 相交（channel L6 只证 track 拓扑）；
            // 落笔后对穿组边做 dogleg 硬修，闭合 D3 穿组=0 证明链。
            repair_ink_group_pierces(self.diagram, &mut result);
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
        canvas_finalize::finalize_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
        let write_thresh = match self.diagram.diagram_type {
            crate::types::DiagramType::Architecture => 1,
            _ => 0,
        };
        crate::layout::group::write_counter::warn_if_group_writes_excessive(write_thresh);

        Ok(result)
    }
}

/// Ink 后穿组硬修：对 `edge_crosses_group_interior` 边跑 dogleg（aggressive 裙边）。
fn repair_ink_group_pierces(diagram: &Diagram, result: &mut LayoutResult) {
    use std::collections::HashSet;
    if result.groups.is_empty() || result.edges.is_empty() {
        return;
    }
    let mut pierced = HashSet::new();
    for i in 0..result.edges.len() {
        if crate::layout::quality::lint::edge_index_crosses_group_interior(diagram, result, i) {
            pierced.insert(i);
        }
    }
    if pierced.is_empty() {
        return;
    }
    crate::perf_log!(
        "[atlas] ink post-repair: {} group-pierce edge(s)",
        pierced.len()
    );
    crate::layout::quality::refine::reroute_edges_for_repair(result, diagram, &pierced, true);
}

/// 拓扑级 Plan：实体序 + 边 id 集合，供 PlanDiff / 增量入口。
pub fn topology_plan(diagram: &Diagram) -> Plan {
    let mut node_slots = std::collections::BTreeMap::new();
    for (i, e) in diagram.entities.iter().enumerate() {
        node_slots.insert(
            e.id.as_str().to_string(),
            Slot {
                rank: 0,
                order: i,
            },
        );
    }
    let mut plan = Plan {
        substrate: SubstrateSketch {
            rank_count: 1,
            order_count: diagram.entities.len(),
        },
        ..Default::default()
    };
    plan.node_slots = node_slots;
    for (i, _) in diagram.relations.iter().enumerate() {
        plan.channels.insert(i, Vec::new());
    }
    plan
}
