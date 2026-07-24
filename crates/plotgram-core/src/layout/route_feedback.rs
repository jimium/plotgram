//! 布局 ↔ 路由反馈：路由与 refine。

use crate::ast::Diagram;
use crate::layout::demand::PressureSnapshot;
use crate::layout::kernel::coordinate::model::{
    ConstraintSource, ConstraintSourceKind, CoordinateProblem, HardConstraint,
};
use crate::layout::kernel::coordinate::optimizer::solve;
use crate::layout::refine::{run_refine, RefineConfig};
use crate::layout::demand::space_budget::{
    enforce_vertical_rank_gaps, node_group_scopes,
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
            // Phase B: 所有图类型使用 solver，节点已冻结，不再执行 enforce_horizontal_gaps
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
        mut layout: LayoutResult,
        refine_config: &RefineConfig,
        edge_snap_config: &crate::layout::EdgeSnapConfig,
    ) -> LayoutResult {
        // Phase C: 路由前冻结节点快照，路由不应修改节点坐标
        layout.hints.frozen_nodes =
            Some(crate::layout::kernel::frozen::freeze_nodes(&layout.nodes));

        let t_route = crate::layout::perf::Instant::now();
        let mut routed = router.route(self.diagram, layout);
        crate::perf_log!(
            "[perf]       router.route: {:.2}ms",
            t_route.elapsed().as_secs_f64() * 1000.0
        );

        // Phase C: 运行时守卫——验证路由未修改节点坐标
        if let Some(frozen) = routed.hints.frozen_nodes.as_ref() {
            let modified = crate::layout::kernel::frozen::verify_frozen_integrity(
                frozen,
                &routed.nodes,
            );
            debug_assert!(
                modified.is_empty(),
                "[Phase C] 路由意外修改了节点坐标: {:?}",
                modified
            );
        }

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
        let (routed, moved) = crate::layout::demand::space_budget_guard::resolve_budget_violations(
            self.diagram, routed,
        );
        let mut routed = crate::layout::demand::space_budget_guard::reroute_and_repulse(
            self.diagram, routed, router, &moved, edge_snap_config,
        );

        // Phase F: route feedback re-solve——路由后压力超阈值时重新求解坐标
        if let Some(problem) = routed.hints.coordinate_problem.as_ref() {
            let post_pressure = PressureSnapshot::compute(self.diagram, &routed);
            // 从 nodes 提取当前 solver 空间坐标（cross-axis center）
            let current_coords: Vec<f64> = problem
                .vars
                .iter()
                .map(|v| {
                    routed
                        .nodes
                        .get(&v.stable_id)
                        .map(|n| n.x + n.width / 2.0)
                        .unwrap_or(v.axis_size / 2.0) // dummy/axis: 用初值近似
                })
                .collect();

            if let Some(new_coords) =
                route_feedback_resolve(problem, &post_pressure, &current_coords, 2)
            {
                crate::perf_log!(
                    "[route-feedback] applying re-solved coordinates, re-routing"
                );
                // 回写新坐标到 nodes
                for (var, &new_c) in problem.vars.iter().zip(new_coords.iter()) {
                    if let Some(node) = routed.nodes.get_mut(&var.stable_id) {
                        node.x = new_c - node.width / 2.0;
                    }
                }
                // 重新冻结 + 全量重路由
                routed.hints.frozen_nodes =
                    Some(crate::layout::kernel::frozen::freeze_nodes(&routed.nodes));
                routed = router.route(self.diagram, routed);
            }
        }

        routed
    }
}

// ─── Phase E: Route feedback re-solve ────────────────────────────────────────

/// 路由压力反馈阈值：corridor 超载数超过此值时触发 re-solve。
const PRESSURE_THRESHOLD_CORRIDORS: usize = 2;
/// 路由压力反馈阈值：最大边难度分超过此值时触发 re-solve。
const PRESSURE_THRESHOLD_SCORE: f64 = 6.0;
/// re-solve 时额外添加的最小分离增量（px）。
const ROUTE_DEMAND_EXTRA_GAP: f64 = 12.0;

/// 路由压力反馈到 solver（最多 1-2 轮）。
///
/// 检测路由后压力，如果超过阈值，将压力转为 RouteDemand 硬约束，
/// 重新求解坐标。只在新结果更优时返回。
///
/// 返回 `Some(new_coordinates)` 表示有改善，`None` 表示无需调整或无改善。
pub fn route_feedback_resolve(
    problem: &CoordinateProblem,
    pressure: &PressureSnapshot,
    current_coords: &[f64],
    max_rounds: usize,
) -> Option<Vec<f64>> {
    // 检查是否需要 re-solve
    let corridors_over = pressure.corridors_over();
    let max_score = pressure.max_edge_score();

    if corridors_over < PRESSURE_THRESHOLD_CORRIDORS && max_score < PRESSURE_THRESHOLD_SCORE {
        return None; // 压力未超阈值，无需 re-solve
    }

    crate::perf_log!(
        "[route-feedback] pressure detected: corridors_over={}, max_score={:.1}, triggering re-solve",
        corridors_over,
        max_score
    );

    let rounds = max_rounds.min(2); // 最多 2 轮
    let mut best_coords = current_coords.to_vec();
    let mut best_score = max_score;
    let mut improved = false;

    for round in 0..rounds {
        // 构建增强问题：添加 RouteDemand 硬约束
        let mut enhanced = problem.clone();
        let extra_gap = ROUTE_DEMAND_EXTRA_GAP * (round + 1) as f64;

        // 对每个超载 corridor，增加相关层的分离距离
        for demand in &pressure.corridor.demands {
            if demand.is_over() {
                // 找到相关层的变量对，增加分离
                for layer in &mut enhanced.layers {
                    let n = layer.separations.len().max(1);
                    let per_sep = extra_gap / n as f64;
                    for sep in &mut layer.separations {
                        *sep += per_sep;
                    }
                }
                break; // 只需一次全局增强
            }
        }

        // 添加 RouteDemand 硬约束（对高难度边的端点节点增加分离）
        for (edge_idx, score) in &pressure.scores {
            if *score > PRESSURE_THRESHOLD_SCORE {
                if let Some(feat) = pressure.features.iter().find(|f| f.edge_index == *edge_idx) {
                    // 找到 from/to 对应的 var_id
                    let from_var = enhanced.vars.iter().find(|v| v.stable_id == feat.from);
                    let to_var = enhanced.vars.iter().find(|v| v.stable_id == feat.to);
                    if let (Some(fv), Some(tv)) = (from_var, to_var) {
                        let (left, right) = if fv.var_id < tv.var_id {
                            (fv.var_id, tv.var_id)
                        } else {
                            (tv.var_id, fv.var_id)
                        };
                        enhanced.hard.push(HardConstraint::MinSeparation {
                            left,
                            right,
                            distance: extra_gap,
                            source: ConstraintSource {
                                kind: ConstraintSourceKind::RouteDemand,
                                nodes: vec![feat.from.clone(), feat.to.clone()],
                                note: "route feedback re-solve",
                            },
                        });
                    }
                }
            }
        }

        // 使用当前最佳坐标作为初值
        enhanced.initial.values = best_coords.clone();

        // re-solve
        let result = solve(&enhanced);
        let new_coords = result.coordinates;

        // 比较：用 P0 审计 + 坐标变化量评估
        let audit = crate::layout::kernel::coordinate::auditor::audit_p0(&enhanced, &new_coords);
        if !audit.passed() {
            crate::perf_log!(
                "[route-feedback] round {} re-solve failed P0 audit, skipping",
                round
            );
            break;
        }

        // 简单启发式：如果坐标变化合理（不超过总跨度 20%），接受
        let max_delta = new_coords
            .iter()
            .zip(best_coords.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max);
        let span = best_coords.iter().fold(0.0_f64, |acc, &x| acc.max(x))
            - best_coords.iter().fold(f64::MAX, |acc, &x| acc.min(x));
        let max_allowed_delta = span * 0.2;

        if max_delta <= max_allowed_delta {
            best_coords = new_coords;
            improved = true;
            crate::perf_log!(
                "[route-feedback] round {} accepted: max_delta={:.1}px",
                round,
                max_delta
            );
        } else {
            crate::perf_log!(
                "[route-feedback] round {} rejected: max_delta={:.1}px > limit={:.1}px",
                round,
                max_delta,
                max_allowed_delta
            );
            break;
        }

        // 如果压力已降低，提前结束
        let _ = best_score; // 简化：首期不重新计算压力，只做 1 轮
        break;
    }

    if improved {
        Some(best_coords)
    } else {
        None
    }
}
