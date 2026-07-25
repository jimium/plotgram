//! 路由协调器（doc16 §5.4 / R12 Slice 12a）。
//!
//! [`RoutingCoordinator`] 是边路由的统一编排入口，生命周期：
//!
//! ```text
//! prepare → compile → solve → materialize → audit
//!   → repair loop（固定轮次）→ geometry freeze → label solve
//!   → refine → budget guard → repulse → final audit → product
//! ```
//!
//! ## R12a 范围
//!
//! coordinator 是路由的唯一编排入口：runner.rs 经 `execute` 调用，
//! 内部串起 route → frozen assert → refine → budget guard → repulse。
//! Phase F（route feedback re-solve）暂留 runner（它改节点坐标，属于 layout 层）。
//!
//! [`FrozenNodeProduct`] 是 router 的正式只读输入：
//! 只读节点/分组快照，router 只消费它，不得修改节点坐标。

use crate::ast::Diagram;
use crate::layout::types::{GroupLayout, LayoutResult, NodeLayout};
use crate::layout::RoutingRecipeDyn;
use crate::layout::refine::RefineConfig;
use crate::layout::snap::grid_snap::EdgeSnapConfig;
use std::collections::HashMap;

/// 只读节点/分组快照（doc16 §5.4 FrozenNodeProduct）。
///
/// router 只消费它（读取节点位置/尺寸/端口），不得修改节点坐标。
/// R12b：fingerprint 校验在所有构建模式下执行（release 也校验）。
#[derive(Debug, Clone)]
pub struct FrozenNodeProduct {
    /// 节点布局快照（id → 位置/尺寸/端口）。
    pub nodes: HashMap<String, NodeLayout>,
    /// 分组布局快照（id → 位置/尺寸）。
    pub groups: HashMap<String, GroupLayout>,
    /// 节点指纹（用于校验冻结契约）。
    fingerprint: String,
}

impl FrozenNodeProduct {
    /// 从当前布局结果捕获只读快照。
    pub fn capture(result: &LayoutResult) -> Self {
        Self {
            fingerprint: crate::layout::quality::metrics::node_fingerprint(result),
            nodes: result.nodes.clone(),
            groups: result.groups.clone(),
        }
    }

    /// 校验路由后节点未被修改（所有构建模式下执行）。
    pub fn assert_unchanged(&self, result: &LayoutResult) {
        assert_eq!(
            self.fingerprint,
            crate::layout::quality::metrics::node_fingerprint(result),
            "FrozenNodeProduct: 路由修改了节点坐标（违反冻结契约）"
        );
    }
}

/// 协调器配置（doc16 §5.4 CoordinatorConfig）。
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// repair loop 最大轮次（默认 2；skeleton 阶段实际跑 1 轮）。
    pub max_repair_rounds: usize,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_repair_rounds: 2,
        }
    }
}

/// 路由协调器：边路由的统一编排入口（doc16 §5.4）。
///
/// R12a：coordinator 是路由的唯一编排入口，内部串起
/// route → frozen assert → refine → budget guard → repulse。
pub struct RoutingCoordinator {
    config: CoordinatorConfig,
}

impl RoutingCoordinator {
    pub fn new(config: CoordinatorConfig) -> Self {
        Self { config }
    }

    /// 执行路由（R12a：唯一编排入口）。
    ///
    /// 生命周期（Slice D4）：
    /// route → frozen assert → refine → budget guard → repulse
    /// → snap/quantize → D 段 repair（E5 删除前仍在 freeze 前）
    /// → materialize → audit → FrozenRouteGeometry（唯一真冻结点）
    ///
    /// `frozen_nodes` 为只读输入：router 不得修改节点坐标。
    /// 返回最终布局与 D 段 [`RouteAuditReport`]（runner 只消费，不再自行 snap/finalize）。
    ///
    /// [`RouteAuditReport`]: crate::layout::routing::model::RouteAuditReport
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        diagram: &Diagram,
        frozen_nodes: &FrozenNodeProduct,
        router: &dyn RoutingRecipeDyn,
        mut result: LayoutResult,
        refine_config: &RefineConfig,
        edge_snap_config: &EdgeSnapConfig,
        routing_config: crate::layout::routing::config::RoutingConfig,
        algo: &str,
    ) -> (LayoutResult, crate::layout::routing::model::RouteAuditReport) {
        // 构造只读 PreparedRoutingInput（Slice B：类型上不可能修改 nodes/groups）。
        let input = crate::layout::routing::model::prepared::PreparedRoutingInput::prepare(
            frozen_nodes,
            diagram,
            &result.hints,
            "", // direction 由调用方传入（待后续完善）
            router.name(),
            routing_config,
            crate::layout::routing::model::prepared::RoutingCanvas {
                width: result.total_width,
                height: result.total_height,
            },
        );

        // 初始路由（Slice B：经新 trait 签名统一承载冻结校验）。
        let t_route = crate::layout::perf::Instant::now();
        let product = router.route(&input);
        crate::perf_log!(
            "[perf]       router.route: {:.2}ms",
            t_route.elapsed().as_secs_f64() * 1000.0
        );

        // 写回 edges + hints delta。
        result.edges = product.edges;
        if product.group_routing.is_some() {
            result.hints.group_routing = product.group_routing;
        }
        if product.route_annotations.is_some() {
            result.hints.route_annotations = product.route_annotations;
        }
        if product.orthogonal_debug.is_some() {
            result.hints.orthogonal_debug = product.orthogonal_debug;
        }

        // 冻结校验：路由不得修改节点坐标。
        frozen_nodes.assert_unchanged(&result);

        // R11a: refine 对所有 router 无条件执行（非 Polyline 路径是空跑）。
        let t_refine = crate::layout::perf::Instant::now();
        result = crate::layout::refine::run_refine(diagram, result, router, refine_config);
        crate::perf_log!(
            "[perf]       run_refine: {:.2}ms",
            t_refine.elapsed().as_secs_f64() * 1000.0
        );

        // S3：budget guard + 增量重路由 + repulse。
        let (result, moved) = crate::layout::demand::space_budget_guard::resolve_budget_violations(
            diagram, result,
        );
        let mut result = crate::layout::demand::space_budget_guard::reroute_and_repulse(
            diagram, result, router, &moved, edge_snap_config,
        );

        // Slice D4：snap/quantize 前移进 Coordinator——grid-aware solver finalize 步，
        // 位于 materialize→audit→freeze 之前；runner 不再独立 snap。
        let sorted_node_ids: Vec<String> = {
            let mut ids: Vec<String> = result.nodes.keys().cloned().collect();
            ids.sort();
            ids
        };
        let annotations = result.hints.route_annotations.clone();
        crate::layout::post_route::snap_and_repulse_edges_with_guard(
            &mut result.edges,
            &result.groups,
            edge_snap_config,
            annotations.as_ref(),
            Some(&result.nodes),
            Some(&diagram.relations),
            Some(&sorted_node_ids),
        );

        // Slice E4：overshoot Z 折合并收编进唯一 materialize 管线——snap 可能抖回
        // 微台阶/斜段/overshoot 折点，正交 family 在量化后经 materializer canonicalize
        // 一次性归一（原 finalize.rs D 段调用已删）。
        if router.name() == "orthogonal" {
            let from_side: Vec<_> = result.edges.iter().map(|e| e.from_port).collect();
            let to_side: Vec<_> = result.edges.iter().map(|e| e.to_port).collect();
            crate::layout::routing::model::GeometryMaterializer::canonicalize_orthogonal_edges(
                &mut result.edges,
                &diagram.relations,
                &from_side,
                &to_side,
                true,
                annotations.as_ref(),
                Some(&result.nodes),
                Some(&sorted_node_ids),
            );

            // Slice E4：reverse pair gap / dock 共锚收口收编为 solver finalize 步
            //（原 D 段 enforce_d_stage_separation）——C 段 lane/refine 预修后，
            // snap/canonicalize 可能重新贴靠，此处做最终收口。
            let parallel_gap = crate::layout::routing::parallel_gap_for_diagram(
                diagram.diagram_type.clone(),
            );
            crate::layout::routing::edge_routing_orthogonal::enforce_reverse_pair_min_gap(
                &mut result.edges,
                &diagram.relations,
                parallel_gap,
            );
            let dock_gap = parallel_gap
                .max(crate::layout::routing::edge_routing_orthogonal::COMPACT_SLOT_PITCH);
            crate::layout::routing::edge_routing_orthogonal::enforce_reverse_pair_dock_separation(
                &mut result.edges,
                &diagram.relations,
                &result.nodes,
                &from_side,
                &to_side,
                dock_gap,
            );

            // Slice E4：architecture exact stub 共柱收口收编为 solver finalize 步
            //（原 D 段调用；C 期 stub_occ 仅诊断）。只改边几何 → node_fp 不变；
            // 须在 repair loop / label 前完成并刷新 annotation。
            if algo == "architecture" {
                let stub_stats = crate::layout::routing::edge_routing_orthogonal::
                    resolve_exact_stub_occupancy_post_route(
                        &mut result.edges,
                        &diagram.relations,
                        &from_side,
                        &to_side,
                        &result.nodes,
                        parallel_gap,
                    );
                if stub_stats.stubs_shifted > 0 {
                    let prev = result.hints.route_annotations.clone();
                    result.hints.route_annotations = Some(
                        crate::layout::routing::refresh_route_annotations_preserving_semantics(
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
        }

        // Slice E5：旧 D 段 finalizer 入口已删除——收尾（recheck / label）由
        // E3 repair loop 的最终审计与 freeze 后统一 label solve 取代。

        // Slice E3：固定轮次 repair loop——lift → materialize → audit_extended →
        // compile intents → local re-solve（≤ max_repair_rounds）。retain best
        // hard-feasible snapshot；无 hard-feasible 解时显式 degraded（不静默）。
        // 边序按 StableEdgeId 升序（§2 确定性）。
        let route_audit = {
            use crate::layout::routing::model::{
                GeometryMaterializer, RepairPriority, RouteAuditContext, RouteAuditor,
                RouteConstraintId, RouteRepairIntent, RouteSolution,
            };
            use std::collections::{BTreeSet, HashSet};

            let annotations = result.hints.route_annotations.clone();
            let ctx = RouteAuditContext::compile(
                &frozen_nodes.nodes,
                &frozen_nodes.groups,
                diagram,
                router.name(),
                annotations.as_ref(),
            );
            let audit_once = |result: &LayoutResult| -> Vec<RouteRepairIntent> {
                let mut lifted = RouteSolution::default();
                for edge in &result.edges {
                    lifted
                        .paths
                        .push(GeometryMaterializer::lift_geometry(&edge.geometry));
                }
                let materialized = GeometryMaterializer::materialize(&lifted);
                let report = RouteAuditor::audit_extended(&materialized, &ctx);
                RouteRepairIntent::compile_from_violations(&report.violations)
            };
            let hard_edge_set = |intents: &[RouteRepairIntent]| -> BTreeSet<usize> {
                intents
                    .iter()
                    .filter(|i| i.priority == RepairPriority::Hard)
                    .flat_map(|i| i.affected_edges.iter().map(|e| e.index()))
                    .collect()
            };

            let max_rounds = self.config.max_repair_rounds;
            let mut best: Option<(usize, Vec<_>, Vec<RouteRepairIntent>)> = None;
            for round in 0..=max_rounds {
                let intents = audit_once(&result);
                let hard_edges = hard_edge_set(&intents);
                let hard_count = hard_edges.len();
                if best.as_ref().map_or(true, |(c, _, _)| hard_count < *c) {
                    best = Some((hard_count, result.edges.clone(), intents.clone()));
                }
                if hard_count == 0 || round == max_rounds {
                    break;
                }
                crate::perf_log!(
                    "[perf]     e3_repair round={} hard_edges={}",
                    round,
                    hard_count
                );
                // 穿组违规走激进裙边/换侧（原 group interior repair 语义，E4 收编）；
                // 其余 Hard 违规走保守 dogleg。
                let mut normal: HashSet<usize> = HashSet::new();
                let mut aggressive: HashSet<usize> = HashSet::new();
                for intent in &intents {
                    if intent.priority != RepairPriority::Hard {
                        continue;
                    }
                    let target = if intent
                        .violated_constraints
                        .contains(&RouteConstraintId::EdgeCrossesGroupInterior)
                    {
                        &mut aggressive
                    } else {
                        &mut normal
                    };
                    for e in &intent.affected_edges {
                        target.insert(e.index());
                    }
                }
                crate::layout::refine::reroute_edges_for_repair(
                    &mut result, diagram, &normal, false,
                );
                crate::layout::refine::reroute_edges_for_repair(
                    &mut result, diagram, &aggressive, true,
                );
            }
            // retain best hard-feasible snapshot（末轮若劣于 best 则回滚）。
            let (final_hard, best_edges, final_intents) =
                best.expect("repair loop 至少执行一轮审计");
            result.edges = best_edges;
            let mut report = crate::layout::routing::model::RouteAuditReport::default();
            for intent in final_intents {
                report.push(intent);
            }
            if final_hard > 0 {
                // 无 hard-feasible 解：保留最优快照并显式 degraded（不冒充成功）。
                report.mark_degraded();
                crate::perf_log!(
                    "[warn] e3_repair degraded: residual_hard_edges={}",
                    final_hard
                );
                // N2 语义收编（原 recheck_lint_pierce_post_freeze）：残留 through 边
                // 写入 annotation.degraded（显式可见，不改 points）。
                if let Some(annotations) = result.hints.route_annotations.as_mut() {
                    let through_edges: BTreeSet<usize> = report
                        .intents
                        .iter()
                        .filter(|i| {
                            i.violated_constraints
                                .contains(&RouteConstraintId::EdgeThroughNode)
                        })
                        .flat_map(|i| i.affected_edges.iter().map(|e| e.index()))
                        .collect();
                    for ei in through_edges {
                        if let Some(ann) =
                            annotations.edges.iter_mut().find(|a| a.edge_index == ei)
                        {
                            if ann.degraded.is_none() {
                                ann.degraded =
                                    Some("e3_repair:edge_through_node".to_string());
                            }
                        }
                    }
                }
            }
            report
        };

        // Slice D4：唯一真冻结点——lift → materialize → audit → freeze → 等价写回。
        // 此后几何不可再改（label/annotation metadata 除外）。
        {
            use crate::layout::routing::model::{
                GeometryMaterializer, RouteAuditor, RouteSolution,
            };
            let mut lifted = RouteSolution::default();
            for edge in &result.edges {
                lifted
                    .paths
                    .push(GeometryMaterializer::lift_geometry(&edge.geometry));
            }
            let materialized = GeometryMaterializer::materialize(&lifted);
            match RouteAuditor::audit_and_advance(materialized) {
                Ok(audited) => {
                    let frozen_geometry = audited.freeze();
                    frozen_geometry.write_geometry_into(&mut result.edges);
                }
                Err((report, _materialized)) => {
                    // 审计失败：debug 直接暴露；release 显式 degraded，不静默（E3 repair
                    // loop 接管重解）；几何保持现状不写回。
                    debug_assert!(
                        false,
                        "Coordinator freeze 前审计失败: {report:?}"
                    );
                    crate::perf_log!(
                        "[warn] coordinator audit degraded: {}",
                        report.describe()
                    );
                }
            }
        }

        // Slice E5：label solve 统一在 freeze 之后执行（原正交 D 段收尾尾部；
        // 非正交 family 的 label 由各自 Recipe 承载，时序不变）。label 只写
        // label/annotation，不改折点——由 FrozenRouteGeometry typestate 取代
        // 已删除的折线软校验屏障。
        if router.name() == "orthogonal" {
            let label_config =
                crate::layout::routing::common::label_candidate::LabelPlacementConfig::for_diagram(
                    diagram.diagram_type.clone(),
                    !result.groups.is_empty(),
                );
            let _assignment = crate::layout::routing::recipe::LabelSolver::solve(
                crate::layout::routing::recipe::LabelProblem {
                    edges: &mut result.edges,
                    nodes: &result.nodes,
                    groups: &result.groups,
                    config: label_config,
                    merge_annotations: result.hints.route_annotations.as_ref(),
                },
            );
        }
        let _ = algo;

        (result, route_audit)
    }
}
