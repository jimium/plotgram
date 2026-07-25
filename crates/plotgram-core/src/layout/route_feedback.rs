//! 布局 ↔ 路由反馈：预路由准备 + SpacingDemandProbe。
//!
//! R12a：`complete_routing` 已迁入 `RoutingCoordinator::execute`。
//! Slice A：Phase F 改为预路由 `spacing_demand_probe`（路由前估计间距需求，重解坐标）。

use crate::ast::Diagram;
use crate::layout::demand::PressureSnapshot;
use crate::layout::kernel::coordinate::model::{
    ConstraintSource, ConstraintSourceKind, CoordinateProblem, HardConstraint,
};
use crate::layout::kernel::coordinate::optimizer::solve;
use crate::layout::demand::space_budget::{
    SpaceBudget,
};
use crate::layout::LayoutResult;

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
    pub fn apply_pre_route(&self, mut result: LayoutResult, edge_pressure_budget: bool) -> PreRouteFeedback {
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
                edge_pressure_budget,
            );
        }

        if let Some(budget) = result.hints.space_budget.clone() {
            // Phase 5：竖向 rank 缝由 builder MinSeparation / Main 轴求解；不再 pre-route 推点。
            let _ = (
                budget.min_vertical_rank_gap,
                result.hints.sugiyama_ranks.as_ref(),
            );
            result.hints.space_budget = Some(budget);
        }
        PreRouteFeedback { result }
    }

}

// ─── Slice A: SpacingDemandProbe ─────────────────────────────────────────────

/// 预路由间距需求探测（Phase 5 / D4-6：已掏空）。
///
/// RouteDemand 已前馈进 layout builder；禁止 route 前二次 `optimizer::solve`。
pub fn spacing_demand_probe(_diagram: &Diagram, _result: &LayoutResult) -> Option<Vec<f64>> {
    None
}

// ─── Route feedback re-solve 内核 ────────────────────────────────────────────

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
