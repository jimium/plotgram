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
use crate::layout::quality::refine::RefineConfig;
use crate::layout::snap::grid_snap::EdgeSnapConfig;
use std::collections::HashMap;

/// 只读节点/分组快照（doc16 §5.4 FrozenNodeProduct）。
///
/// Phase 6 typestate：字段私有，仅提供 `&` getter；禁止外部可变写入。
/// G4：同时冻结 groups 指纹——route 后组框不得再变（canvas 平移在 assert 之后）。
#[derive(Debug, Clone)]
pub struct FrozenNodeProduct {
    nodes: HashMap<String, NodeLayout>,
    groups: HashMap<String, GroupLayout>,
    /// 节点指纹（用于校验冻结契约）。
    fingerprint: String,
    /// 分组几何指纹（量化 0.01px）。
    groups_fingerprint: String,
}

impl FrozenNodeProduct {
    /// 从当前布局结果捕获只读快照。
    pub fn capture(result: &LayoutResult) -> Self {
        Self {
            fingerprint: crate::layout::quality::metrics::node_fingerprint(result),
            groups_fingerprint: groups_geometry_fingerprint(result),
            nodes: result.nodes.clone(),
            groups: result.groups.clone().into_map(),
        }
    }

    /// 只读节点表。
    pub fn nodes(&self) -> &HashMap<String, NodeLayout> {
        &self.nodes
    }

    /// 只读分组表。
    pub fn groups(&self) -> &HashMap<String, GroupLayout> {
        &self.groups
    }

    /// 校验路由后节点与分组均未被修改（所有构建模式下执行）。
    pub fn assert_unchanged(&self, result: &LayoutResult) {
        assert_eq!(
            self.fingerprint,
            crate::layout::quality::metrics::node_fingerprint(result),
            "FrozenNodeProduct: 路由修改了节点坐标（违反冻结契约）"
        );
        assert_eq!(
            self.groups_fingerprint,
            groups_geometry_fingerprint(result),
            "FrozenNodeProduct: 路由修改了 group 几何（违反冻结契约）"
        );
    }
}

/// 确定性 groups 几何指纹（按 id 排序，量化到 0.01）。
fn groups_geometry_fingerprint(result: &LayoutResult) -> String {
    use std::collections::BTreeMap;
    let mut parts: BTreeMap<&str, String> = BTreeMap::new();
    for (id, g) in result.groups.iter() {
        parts.insert(
            id.as_str(),
            format!(
                "{:.2},{:.2},{:.2},{:.2}",
                g.x, g.y, g.width, g.height
            ),
        );
    }
    parts
        .into_iter()
        .map(|(id, geom)| format!("{id}:{geom}"))
        .collect::<Vec<_>>()
        .join("|")
}

/// 协调器配置（doc16 §5.4 CoordinatorConfig）。
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// repair loop 最大轮次（默认 2；skeleton 阶段实际跑 1 轮）。
    pub max_repair_rounds: usize,
    /// Stage 3：Atlas 度量相已预留通道净空时跳过 border repulse（仍做 grid snap）。
    pub skip_border_repulse: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_repair_rounds: 2,
            skip_border_repulse: false,
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
        prev: Option<&crate::layout::routing::model::FrozenRoutingSolution>,
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

        // Stage 6：废除 FrozenRoutingSolution 指纹增量（改 Plan diff / AtlasPipeline）。
        let incremental = None::<IncrementalReusePlan>;
        let _ = prev;
        if let (Some(plan), Some(p)) = (&incremental, prev) {
            if plan.zero_diff {
                // zero-diff 快路径：全 preserve → 原样写回冻结 edges/annotations，
                // 跳过 route/refine/snap/E3/label solve，保证输出字节一致。
                result.edges = plan.seeded_edges.clone();
                result.hints.route_annotations = p.route_annotations.clone();
                let frozen = crate::layout::routing::model::FrozenRoutingSolution::capture(
                    diagram, &result,
                );
                result.hints.frozen_routing = Some(std::sync::Arc::new(frozen));
                crate::perf_log!(
                    "[perf]   incremental: zero-diff full preserve ({} edges)",
                    result.edges.len()
                );
                return (
                    result,
                    crate::layout::routing::model::RouteAuditReport::default(),
                );
            }
        }

        // 初始路由（Slice B：经新 trait 签名统一承载冻结校验）。
        let t_route = crate::layout::perf::Instant::now();
        let product = match &incremental {
            Some(plan) => {
                crate::perf_log!(
                    "[perf]   incremental: dirty={} preserve={}",
                    plan.dirty.len(),
                    plan.preserve.len()
                );
                // 支持 preserve 的 family 逐边复用；否则全量重解。
                router
                    .route_preserving(&input, plan.seeded_edges.clone(), &plan.preserve)
                    .unwrap_or_else(|| router.route(&input))
            }
            None => router.route(&input),
        };
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
        result = crate::layout::quality::refine::run_refine(diagram, result, router, refine_config);
        crate::perf_log!(
            "[perf]       run_refine: {:.2}ms",
            t_refine.elapsed().as_secs_f64() * 1000.0
        );

        // S3：budget hint（R11b 后只设置 hints；Slice F2c 删除旧的空转增量路径——跨渲染增量统一走 FrozenRoutingSolution）。
        let mut result = crate::layout::demand::space_budget_guard::resolve_budget_violations(
            diagram, result,
        );

        // Slice D4：snap/quantize 前移进 Coordinator——grid-aware solver finalize 步，
        // 位于 materialize→audit→freeze 之前；runner 不再独立 snap。
        let sorted_node_ids: Vec<String> = {
            let mut ids: Vec<String> = result.nodes.keys().cloned().collect();
            ids.sort();
            ids
        };
        let annotations = result.hints.route_annotations.clone();
        if self.config.skip_border_repulse {
            // Atlas Stage 3：只 snap，不 repulse（净空已由度量相撑开）。
            if edge_snap_config.enabled {
                crate::layout::snap::grid_snap::snap_edge_waypoints_with_guard(
                    &mut result.edges,
                    &result.groups,
                    edge_snap_config,
                    annotations.as_ref(),
                    Some(&result.nodes),
                    Some(&diagram.relations),
                    Some(&sorted_node_ids),
                );
            }
        } else {
            crate::layout::routing::post_route::snap_and_repulse_edges_with_guard(
                &mut result.edges,
                &result.groups,
                edge_snap_config,
                annotations.as_ref(),
                Some(&result.nodes),
                Some(&diagram.relations),
                Some(&sorted_node_ids),
            );
        }

        // Slice E5：旧 D 段 finalizer 入口已删除——收尾（recheck / label）由
        // E3 repair loop 的最终审计与 freeze 后统一 label solve 取代。
        // R1：OVG / OrthogonalRecipe 已删；canonicalize_orthogonal_edges 不再经 Coordinator。

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
            let mut prev_hard: Option<usize> = None;
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
                // Slice F2b：hard 数未下降 → 下一轮 affected set 沿依赖图
                //（conflicts + bundle）做 1 跳固定序扩张后再局部重解。
                let stalled = prev_hard.map_or(false, |p| hard_count >= p);
                prev_hard = Some(hard_count);
                crate::perf_log!(
                    "[perf]     e3_repair round={} hard_edges={}{}",
                    round,
                    hard_count,
                    if stalled { " (stalled: 1-hop expand)" } else { "" }
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
                if stalled {
                    let adjacency = crate::layout::routing::model::edge_conflict_adjacency(
                        &result.edges,
                        result.hints.route_annotations.as_ref(),
                    );
                    expand_one_hop(&mut normal, &adjacency);
                    expand_one_hop(&mut aggressive, &adjacency);
                }
                crate::layout::quality::refine::reroute_edges_for_repair(
                    &mut result, diagram, &normal, false,
                );
                crate::layout::quality::refine::reroute_edges_for_repair(
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

        // Slice F1：label solve 全 family 统一在唯一 freeze 之后执行，Coordinator
        // 是标签的最后写者（Recipe 内只保留 plan-based 初始放置）。label 只写
        // label/annotation，不改折点——由 FrozenRouteGeometry typestate 取代
        // 已删除的折线软校验屏障。
        {
            let mut label_config =
                crate::layout::routing::common::label_candidate::LabelPlacementConfig::for_diagram(
                    diagram.diagram_type.clone(),
                    !result.groups.is_empty(),
                );
            if router.name() == "circular" {
                // circular family：以节点包围盒中心为径向候选中心，标签沿放射
                // 方向阶梯外推（min/max 对迭代序不敏感，确定性）。
                label_config.radial_center = node_bbox_center(&result.nodes);
            }
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
        // Slice F2b：在唯一 freeze + label solve 之后 capture 冻结路由解
        //（逐边依赖记录 + 指纹），经 hints Arc 供跨渲染增量入口消费。
        {
            let frozen = crate::layout::routing::model::FrozenRoutingSolution::capture(
                diagram, &result,
            );
            result.hints.frozen_routing = Some(std::sync::Arc::new(frozen));
        }

        (result, route_audit)
    }
}

/// Slice F2b：E3 affected set 沿冲突邻接表做 1 跳扩张（固定序：先排序再扩）。
fn expand_one_hop(
    set: &mut std::collections::HashSet<usize>,
    adjacency: &[std::collections::BTreeSet<usize>],
) {
    let mut seeds: Vec<usize> = set.iter().copied().collect();
    seeds.sort_unstable();
    for i in seeds {
        if let Some(partners) = adjacency.get(i) {
            for &p in partners {
                set.insert(p);
            }
        }
    }
}

/// Slice F2c：增量复用计划——clean 边（preserve）+ seeded edges + dirty 边。
struct IncrementalReusePlan {
    /// 可复用边（新版本下标）。
    preserve: std::collections::HashSet<usize>,
    /// 按声明序 seeded：preserve 边为 prev 冻结 geometry/labels/ports，dirty 边空占位。
    seeded_edges: Vec<crate::layout::types::EdgeLayout>,
    /// 必须重路由的边（新版本下标，升序）。
    dirty: std::collections::BTreeSet<usize>,
    /// 全图 zero-diff（无增删、无 dirty）→ 快路径原样写回，保字节一致。
    zero_diff: bool,
}

/// 由 prev 冻结解规划增量复用：dirty_set → 声明标签文案守卫 → preserved
/// hard audit（fail → 固定序 1 跳扩张，最多 2 跳）→ MIN_PRESERVE_RATIO 门槛。
/// 返回 `None` 表示不可增量（回退全图路由）。
fn plan_incremental_reuse(
    prev: &crate::layout::routing::model::FrozenRoutingSolution,
    diagram: &Diagram,
    result: &LayoutResult,
    frozen_nodes: &FrozenNodeProduct,
    family: &str,
) -> Option<IncrementalReusePlan> {
    use crate::layout::routing::model::{
        edge_conflict_adjacency, group_fingerprints_of, node_fingerprints_of,
        EmptyRouteReason, GeometryMaterializer, RepairPriority, RouteAuditContext,
        RouteAuditor, RoutePath, RouteRepairIntent, RouteSolution, StableEdgeStore,
    };
    use crate::layout::types::EdgeLayout;
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    let n = diagram.relations.len();
    if n == 0 {
        return None;
    }
    let new_store = StableEdgeStore::from_diagram(diagram);
    let (mut dirty, diff) = prev.dirty_set(
        &new_store,
        &node_fingerprints_of(result),
        &group_fingerprints_of(result),
    );

    // retained 映射：new_idx → prev_idx。
    let prev_of: BTreeMap<usize, usize> =
        diff.retained.iter().map(|&(p, nw)| (nw, p)).collect();

    // 声明标签文案守卫：identity/flags 不含文案，文案变化 → 该边 dirty。
    for (&new_idx, &prev_idx) in &prev_of {
        if dirty.contains(&new_idx) {
            continue;
        }
        let Some(rec) = prev.records.get(prev_idx) else {
            dirty.insert(new_idx);
            continue;
        };
        let declared = diagram
            .relations
            .get(new_idx)
            .map(|r| [r.label.clone(), r.head_label.clone(), r.tail_label.clone()])
            .unwrap_or_default();
        if declared != rec.declared_labels {
            dirty.insert(new_idx);
        }
    }
    // retained 之外的边（added 已在 dirty）防御性补齐。
    for i in 0..n {
        if !prev_of.contains_key(&i) {
            dirty.insert(i);
        }
    }

    // seeded 构造器：preserve 边携带 prev 冻结 geometry/labels/ports。
    let seeded_for = |i: usize, dirty: &BTreeSet<usize>| -> EdgeLayout {
        if dirty.contains(&i) {
            return EdgeLayout::empty();
        }
        match prev_of.get(&i).and_then(|&p| prev.records.get(p)) {
            Some(rec) => EdgeLayout {
                geometry: rec.geometry.clone(),
                labels: rec.labels.clone(),
                from_port: rec.from_port,
                to_port: rec.to_port,
            },
            None => EdgeLayout::empty(),
        }
    };

    // preserved hard audit：复用前逐边重过 audit_extended；任一 preserved 边
    // hard fail → dirty 按固定跳数扩张（最多 2 跳）重试；仍 fail → 回退全图。
    // annotations 仅在下标对齐（无增删、retained 恒等）时传入 prev 旁路注解。
    let aligned = diff.added.is_empty()
        && diff.removed.is_empty()
        && diff.retained.iter().all(|&(p, nw)| p == nw);
    let ctx = RouteAuditContext::compile(
        &frozen_nodes.nodes,
        &frozen_nodes.groups,
        diagram,
        family,
        if aligned {
            prev.route_annotations.as_ref()
        } else {
            None
        },
    );
    // 扩张邻接：prev 几何的 bbox 冲突邻接（固定序，与 dirty 演化无关）。
    let adjacency = {
        let all_seeded: Vec<EdgeLayout> =
            (0..n).map(|i| seeded_for(i, &BTreeSet::new())).collect();
        edge_conflict_adjacency(&all_seeded, None)
    };
    let mut attempts = 0;
    loop {
        let mut lifted = RouteSolution::default();
        for i in 0..n {
            if dirty.contains(&i) {
                // dirty 占位必用声明性 Empty reason（Unresolved 会审计失败）。
                lifted.paths.push(RoutePath::Empty(EmptyRouteReason::Suppressed));
            } else {
                let rec = &prev.records[prev_of[&i]];
                lifted
                    .paths
                    .push(GeometryMaterializer::lift_geometry(&rec.geometry));
            }
        }
        let materialized = GeometryMaterializer::materialize(&lifted);
        let report = RouteAuditor::audit_extended(&materialized, &ctx);
        let intents = RouteRepairIntent::compile_from_violations(&report.violations);
        let mut failed: BTreeSet<usize> = BTreeSet::new();
        for intent in &intents {
            if intent.priority != RepairPriority::Hard {
                continue;
            }
            for e in &intent.affected_edges {
                if !dirty.contains(&e.index()) {
                    failed.insert(e.index());
                }
            }
        }
        if failed.is_empty() {
            break;
        }
        if attempts >= 2 {
            // 2 跳扩张后仍 hard fail → 回退全图路由。
            return None;
        }
        attempts += 1;
        // fail 边 + 其 1 跳 bbox 邻接 → dirty（固定序）。
        for &f in &failed {
            dirty.insert(f);
            if let Some(partners) = adjacency.get(f) {
                for &p in partners {
                    dirty.insert(p);
                }
            }
        }
    }

    let preserve: HashSet<usize> = (0..n).filter(|i| !dirty.contains(i)).collect();
    if !dirty.is_empty() {
        // preserve 比例过低 → 回退全图路由（现有路径）。
        if preserve.is_empty()
            || (preserve.len() as f64 / n as f64)
                < crate::layout::routing::post_route::MIN_PRESERVE_RATIO
        {
            return None;
        }
    }
    let zero_diff = dirty.is_empty()
        && diff.added.is_empty()
        && diff.removed.is_empty()
        && prev.records.len() == n;
    let seeded_edges: Vec<EdgeLayout> = (0..n).map(|i| seeded_for(i, &dirty)).collect();
    Some(IncrementalReusePlan {
        preserve,
        seeded_edges,
        dirty,
        zero_diff,
    })
}

/// Slice F1：节点包围盒中心（circular 径向标签候选中心）。
/// min/max 聚合对 HashMap 迭代序不敏感，确定性成立。
fn node_bbox_center(
    nodes: &HashMap<String, NodeLayout>,
) -> Option<crate::layout::geometry::Point> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for nl in nodes.values() {
        min_x = min_x.min(nl.x);
        min_y = min_y.min(nl.y);
        max_x = max_x.max(nl.x + nl.width);
        max_y = max_y.max(nl.y + nl.height);
    }
    if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
        Some(crate::layout::geometry::Point::new(
            (min_x + max_x) * 0.5,
            (min_y + max_y) * 0.5,
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Diagram, Identifier, Relation, SourceInfo, Span};
    use crate::layout::geometry::Point;
    use crate::layout::routing::model::FrozenRoutingSolution;
    use crate::layout::types::{EdgeLayout, LayoutHints, PathGeometry, Port};
    use crate::types::DiagramType;

    fn rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn diagram_with(relations: Vec<Relation>) -> Diagram {
        let mut d = Diagram::new(DiagramType::Flowchart, SourceInfo::default());
        d.relations = relations;
        d
    }

    fn node(x: f64, y: f64) -> NodeLayout {
        NodeLayout { x, y, width: 10.0, height: 10.0 }
    }

    fn straight_edge(x0: f64, y0: f64, x1: f64, y1: f64) -> EdgeLayout {
        EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(x0, y0),
                end: Point::new(x1, y1),
            },
            labels: Vec::new(),
            from_port: Port::Right,
            to_port: Port::Left,
        }
    }

    /// 两条相互远离的干净边：a→b（上方）与 c→d（下方 500px）。
    fn clean_fixture() -> (Diagram, LayoutResult) {
        let diagram = diagram_with(vec![rel("a", "b"), rel("c", "d")]);
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0));
        nodes.insert("b".to_string(), node(100.0, 0.0));
        nodes.insert("c".to_string(), node(0.0, 500.0));
        nodes.insert("d".to_string(), node(100.0, 500.0));
        let result = LayoutResult {
            nodes,
            groups: crate::layout::GroupTable::new(),
            edges: vec![
                straight_edge(10.0, 5.0, 100.0, 5.0),
                straight_edge(10.0, 505.0, 100.0, 505.0),
            ],
            total_width: 110.0,
            total_height: 510.0,
            hints: LayoutHints::default(),
        };
        (diagram, result)
    }

    #[test]
    fn plan_zero_diff_preserves_all_edges() {
        let (diagram, result) = clean_fixture();
        let prev = FrozenRoutingSolution::capture(&diagram, &result);
        let frozen_nodes = FrozenNodeProduct::capture(&result);
        let plan = plan_incremental_reuse(&prev, &diagram, &result, &frozen_nodes, "spline")
            .expect("zero-diff 必须可增量");
        assert!(plan.zero_diff);
        assert!(plan.dirty.is_empty());
        assert_eq!(plan.preserve.len(), 2);
        // seeded 与冻结几何字节一致。
        assert_eq!(
            format!("{:?}", plan.seeded_edges),
            format!("{:?}", result.edges)
        );
    }

    #[test]
    fn preserved_hard_audit_fail_expands_dirty() {
        // F2 退出判据：preserved route 复用前必过 hard audit——构造 edge0
        // 穿非端点节点 mid 触发扩张：edge0 dirty，远离的 edge1 仍 preserve。
        let diagram = diagram_with(vec![rel("a", "b"), rel("c", "d")]);
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0));
        nodes.insert("b".to_string(), node(200.0, 0.0));
        nodes.insert("mid".to_string(), node(95.0, 0.0));
        nodes.insert("c".to_string(), node(0.0, 500.0));
        nodes.insert("d".to_string(), node(100.0, 500.0));
        let result = LayoutResult {
            nodes,
            groups: crate::layout::GroupTable::new(),
            edges: vec![
                straight_edge(10.0, 5.0, 200.0, 5.0),
                straight_edge(10.0, 505.0, 100.0, 505.0),
            ],
            total_width: 210.0,
            total_height: 510.0,
            hints: LayoutHints::default(),
        };
        let prev = FrozenRoutingSolution::capture(&diagram, &result);
        let frozen_nodes = FrozenNodeProduct::capture(&result);
        let plan = plan_incremental_reuse(&prev, &diagram, &result, &frozen_nodes, "spline")
            .expect("扩张后仍有可 preserve 边 → Some");
        assert!(!plan.zero_diff);
        assert!(plan.dirty.contains(&0), "穿节点边必须 dirty: {:?}", plan.dirty);
        assert!(plan.preserve.contains(&1), "干净边仍 preserve");
    }

    #[test]
    fn all_preserved_fail_falls_back_to_full_route() {
        // 单条穿节点边：扩张后 preserve 为空 → 回退全图路由（None）。
        let diagram = diagram_with(vec![rel("a", "b")]);
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0));
        nodes.insert("b".to_string(), node(200.0, 0.0));
        nodes.insert("mid".to_string(), node(95.0, 0.0));
        let result = LayoutResult {
            nodes,
            groups: crate::layout::GroupTable::new(),
            edges: vec![straight_edge(10.0, 5.0, 200.0, 5.0)],
            total_width: 210.0,
            total_height: 10.0,
            hints: LayoutHints::default(),
        };
        let prev = FrozenRoutingSolution::capture(&diagram, &result);
        let frozen_nodes = FrozenNodeProduct::capture(&result);
        assert!(
            plan_incremental_reuse(&prev, &diagram, &result, &frozen_nodes, "spline")
                .is_none()
        );
    }

    #[test]
    fn label_text_change_dirties_only_that_edge() {
        // identity/flags 不含文案：declared_labels 守卫把改文案的边标 dirty。
        let (diagram, result) = clean_fixture();
        let prev = FrozenRoutingSolution::capture(&diagram, &result);
        let frozen_nodes = FrozenNodeProduct::capture(&result);

        let mut relabeled = rel("a", "b");
        relabeled.label = Some("changed".to_string());
        let new_diagram = diagram_with(vec![relabeled, rel("c", "d")]);
        let plan =
            plan_incremental_reuse(&prev, &new_diagram, &result, &frozen_nodes, "spline")
                .expect("另一条边仍可 preserve");
        assert!(!plan.zero_diff);
        assert_eq!(plan.dirty.iter().copied().collect::<Vec<_>>(), vec![0]);
        assert!(plan.preserve.contains(&1));
    }

    #[test]
    fn incremental_zero_diff_render_is_byte_identical() {
        // Stage 6：Plan diff 增量——同一 diagram 两次渲染，拓扑 Plan 相等。
        let source = r#"diagram flowchart {
            entity a "Alpha"
            entity b "Beta"
            entity c "Gamma"
            a -> b "first"
            b -> c "second"
        }"#;
        let output = crate::pipeline::parse_prepare_validate(
            source,
            &crate::prepare::StyleRequest::default(),
        );
        let prepared = output.diagram.expect("valid diagram");
        let first = crate::layout::compute_layout_with_plan(
            prepared.inner(),
            prepared.layout_plan(),
        )
        .expect("layout");
        let prev = first
            .hints
            .atlas_plan
            .clone()
            .expect("atlas plan captured");
        let second =
            crate::layout::compute_layout_incremental(prepared.inner(), &prev)
                .expect("incremental layout");
        let prev2 = second.hints.atlas_plan.as_ref().expect("second plan");
        assert!(
            crate::layout::atlas::plan::diff(&prev, prev2).is_empty(),
            "zero-diff 重渲染 PlanDiff 应为空"
        );
        assert_eq!(
            format!("{:?}", first.edges),
            format!("{:?}", second.edges),
            "拓扑不变时边输出应稳定"
        );
    }
}
