//! Projected Gradient Optimizer。
//!
//! 在 PAVA 硬约束可行域内，通过分层优先级梯度下降优化软目标。
//! 算法：
//! 1. 从 BK 初值出发，PAVA 投影到可行域。
//! 2. 按 P1 → P2 → P3 优先级逐层优化。
//! 3. 每步：计算梯度 → 步长 × 负梯度 → PAVA 投影 → 检查 loss。
//! 4. Barzilai-Borwein 自适应步长加速收敛。
//! 5. 未收敛时返回 best feasible snapshot，标记 Degraded。

use super::analysis::analyze_components;
use super::model::*;
use super::projection::{project_hard_constraints, max_hard_violation};

/// 求解 CoordinateProblem，返回最终坐标和诊断。
pub fn solve(problem: &CoordinateProblem) -> SolverResult {
    let n = problem.var_count();
    if n == 0 {
        return SolverResult {
            coordinates: vec![],
            status: SolverStatus::Converged,
            loss_p1: 0.0,
            loss_p2: 0.0,
            loss_p3: 0.0,
            iterations: 0,
            max_hard_violation: 0.0,
            diagnostics: Default::default(),
        };
    }

    let config = &problem.config;

    // 初值 = BK 坐标
    let mut coords = problem.initial.values.clone();

    // 初始 Dykstra 投影：确保所有硬约束满足
    project_hard_constraints(&mut coords, problem);

    // 无 objectives 时直接返回投影结果
    if problem.objectives.is_empty() {
        let violation = max_hard_violation(&coords, problem);
        return SolverResult {
            coordinates: coords,
            status: if violation < 1e-6 { SolverStatus::Converged } else { SolverStatus::Infeasible },
            loss_p1: 0.0,
            loss_p2: 0.0,
            loss_p3: 0.0,
            iterations: 0,
            max_hard_violation: violation,
            diagnostics: Default::default(),
        };
    }

    let mut total_iterations = 0;
    let mut diag_notes: Vec<String> = Vec::new();

    // 连通分量分析（填充诊断信息）
    let component_analysis = analyze_components(problem);

    // 预索引各优先级 terms（避免每步重复过滤）
    let p1_terms: Vec<&ObjectiveTerm> = problem.objectives_by_priority(ObjectivePriority::P1).collect();
    let p2_terms: Vec<&ObjectiveTerm> = problem.objectives_by_priority(ObjectivePriority::P2).collect();
    let p3_terms: Vec<&ObjectiveTerm> = problem.objectives_by_priority(ObjectivePriority::P3).collect();

    // 可复用 workspace（避免每轮分配）
    let mut gradient = vec![0.0f64; n];
    let mut candidate = vec![0.0f64; n];

    // 分层优化：P1 → P2 → P3
    let phases: [(ObjectivePriority, usize, &str, &[&ObjectiveTerm]); 3] = [
        (ObjectivePriority::P1, config.max_iter_p1, "P1", &p1_terms),
        (ObjectivePriority::P2, config.max_iter_p2, "P2", &p2_terms),
        (ObjectivePriority::P3, config.max_iter_p3, "P3", &p3_terms),
    ];

    // 记录 P1/P2 loss 用于 tolerance 约束
    let mut loss_p1_budget = f64::INFINITY;
    let mut loss_p2_budget = f64::INFINITY;

    for (priority, max_iter, phase_name, terms) in &phases {
        if terms.is_empty() {
            continue;
        }

        let mut step = config.initial_step;
        let mut best_coords = coords.clone();
        let mut best_loss = compute_loss(&coords, terms);
        let mut prev_gradient = vec![0.0f64; n];
        let mut prev_coords = coords.clone();
        let mut converged = false;

        for iter in 0..*max_iter {
            // 计算梯度（复用 buffer）
            gradient.iter_mut().for_each(|g| *g = 0.0);
            compute_gradient(&coords, terms, &mut gradient);

            // Barzilai-Borwein 自适应步长（第 2 步起）
            if iter > 0 {
                let bb_step = barzilai_borwein_step(
                    &coords, &prev_coords, &gradient, &prev_gradient,
                );
                if let Some(bb) = bb_step {
                    step = bb.clamp(0.001, 100.0);
                }
            }

            // 梯度下降步（复用 candidate buffer）
            for i in 0..n {
                candidate[i] = coords[i] - step * gradient[i];
            }

            // Dykstra 投影回可行域
            project_hard_constraints(&mut candidate, problem);

            // 检查 loss
            let candidate_loss = compute_loss(&candidate, terms);

            // 步长 backoff：如果 loss 增加，二分步长重试
            let mut accepted = candidate_loss <= best_loss + config.epsilon;
            if !accepted {
                let mut backoff_step = step;
                for _ in 0..config.max_step_backoff {
                    backoff_step *= 0.5;
                    for i in 0..n {
                        candidate[i] = coords[i] - backoff_step * gradient[i];
                    }
                    project_hard_constraints(&mut candidate, problem);
                    let bl = compute_loss(&candidate, terms);
                    if bl <= best_loss + config.epsilon {
                        step = backoff_step;
                        accepted = true;
                        break;
                    }
                }
            }

            if accepted {
                let candidate_loss = compute_loss(&candidate, terms);

                // P2/P3 阶段：检查不破坏 P1 budget
                if *priority != ObjectivePriority::P1 && loss_p1_budget.is_finite() {
                    if !p1_terms.is_empty() {
                        let p1_loss = compute_loss(&candidate, &p1_terms);
                        if p1_loss > loss_p1_budget + config.p1_tolerance {
                            // 拒绝此步：P1 退化超限
                            std::mem::swap(&mut prev_coords, &mut coords);
                            prev_gradient.copy_from_slice(&gradient);
                            continue;
                        }
                    }
                }

                // P3 阶段：检查不破坏 P2 budget
                if *priority == ObjectivePriority::P3 && loss_p2_budget.is_finite() {
                    if !p2_terms.is_empty() {
                        let p2_loss = compute_loss(&candidate, &p2_terms);
                        if p2_loss > loss_p2_budget + config.p1_tolerance {
                            std::mem::swap(&mut prev_coords, &mut coords);
                            prev_gradient.copy_from_slice(&gradient);
                            continue;
                        }
                    }
                }

                // 收敛检测
                let max_delta = coords.iter()
                    .zip(candidate.iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f64, f64::max);

                prev_gradient.copy_from_slice(&gradient);
                std::mem::swap(&mut prev_coords, &mut coords);
                std::mem::swap(&mut coords, &mut candidate);

                if candidate_loss < best_loss {
                    best_loss = candidate_loss;
                    best_coords = coords.clone();
                }

                if max_delta < config.epsilon {
                    converged = true;
                    total_iterations += iter + 1;
                    break;
                }
            } else {
                // 所有 backoff 失败，梯度方向无法改善
                prev_gradient.copy_from_slice(&gradient);
                prev_coords.copy_from_slice(&coords);
            }
        }

        if !converged {
            total_iterations += max_iter;
            diag_notes.push(format!(
                "{}: not converged after {} iters, loss={:.4}",
                phase_name, max_iter, best_loss
            ));
        }

        // P1 完成后记录 budget（复用预索引 terms）
        if *priority == ObjectivePriority::P1 {
            loss_p1_budget = compute_loss(&coords, &p1_terms);
        }

        // P2 完成后记录 budget（复用预索引 terms）
        if *priority == ObjectivePriority::P2 {
            loss_p2_budget = compute_loss(&coords, &p2_terms);
        }

        // 恢复 best snapshot（确保最终坐标是该 phase 的最优解）
        coords = best_coords;
    }

    // 最终 loss（复用预索引 terms）
    let final_p1 = compute_loss(&coords, &p1_terms);
    let final_p2 = compute_loss(&coords, &p2_terms);
    let final_p3 = compute_loss(&coords, &p3_terms);
    let violation = max_hard_violation(&coords, problem);

    let status = if violation > 1e-6 {
        SolverStatus::Infeasible
    } else if diag_notes.is_empty() {
        SolverStatus::Converged
    } else {
        SolverStatus::Degraded
    };

    SolverResult {
        coordinates: coords,
        status,
        loss_p1: final_p1,
        loss_p2: final_p2,
        loss_p3: final_p3,
        iterations: total_iterations,
        max_hard_violation: violation,
        diagnostics: SolverDiagnostics {
            component_count: component_analysis.count,
            notes: diag_notes,
            ..Default::default()
        },
    }
}

/// 计算给定 terms 的总 loss：Σ weight * residual²。
fn compute_loss(coords: &[f64], terms: &[&ObjectiveTerm]) -> f64 {
    let mut loss = 0.0f64;
    for term in terms {
        let residual = term.constant
            + term.coefficients.iter()
                .map(|&(var, coeff)| coeff * coords[var])
                .sum::<f64>();
        loss += term.weight * residual * residual;
    }
    loss
}

/// 计算梯度：grad[var] += 2 * weight * residual * coeff。
fn compute_gradient(coords: &[f64], terms: &[&ObjectiveTerm], grad: &mut [f64]) {
    grad.iter_mut().for_each(|g| *g = 0.0);
    for term in terms {
        let residual = term.constant
            + term.coefficients.iter()
                .map(|&(var, coeff)| coeff * coords[var])
                .sum::<f64>();
        let factor = 2.0 * term.weight * residual;
        for &(var, coeff) in &term.coefficients {
            grad[var] += factor * coeff;
        }
    }
}

/// Barzilai-Borwein 步长估计。
///
/// BB step = (Δx · Δx) / (Δx · Δg)，其中 Δx = x_k - x_{k-1}，Δg = g_k - g_{k-1}。
fn barzilai_borwein_step(
    coords: &[f64],
    prev_coords: &[f64],
    gradient: &[f64],
    prev_gradient: &[f64],
) -> Option<f64> {
    let n = coords.len();
    let mut dx_dot_dx = 0.0f64;
    let mut dx_dot_dg = 0.0f64;
    for i in 0..n {
        let dx = coords[i] - prev_coords[i];
        let dg = gradient[i] - prev_gradient[i];
        dx_dot_dx += dx * dx;
        dx_dot_dg += dx * dg;
    }
    if dx_dot_dg.abs() < 1e-12 || dx_dot_dx < 1e-12 {
        None
    } else {
        Some(dx_dot_dx / dx_dot_dg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::kernel::coordinate::model::*;

    /// 无 objectives 时，结果 = PAVA 投影初值。
    #[test]
    fn no_objectives_returns_pava_projection() {
        let problem = CoordinateProblem {
            vars: (0..3).map(|i| NodeVariable {
                var_id: i,
                stable_id: format!("n{}", i),
                kind: VarKind::Real,
                rank: 0,
                order: i,
                axis_size: 40.0,
                movable: true,
            }).collect(),
            layers: vec![LayerConstraintSet {
                rank: 0,
                vars: vec![0, 1, 2],
                separations: vec![50.0, 50.0],
            }],
            hard: vec![],
            objectives: vec![],
            initial: InitialCoordinates { values: vec![0.0, 30.0, 60.0] },
            config: CoordinateSolverConfig::default(),
            axis: Default::default(),
        };

        let result = solve(&problem);
        assert_eq!(result.status, SolverStatus::Converged);
        // 初值 [0, 30, 60] 违反 sep=50，PAVA 投影后应满足
        assert!(result.coordinates[1] - result.coordinates[0] >= 50.0 - 1e-9);
        assert!(result.coordinates[2] - result.coordinates[1] >= 50.0 - 1e-9);
    }

    /// PreferBKPosition：初值已满足约束时，不应移动。
    #[test]
    fn prefer_bk_no_move_when_feasible() {
        let problem = CoordinateProblem {
            vars: (0..3).map(|i| NodeVariable {
                var_id: i,
                stable_id: format!("n{}", i),
                kind: VarKind::Real,
                rank: 0,
                order: i,
                axis_size: 40.0,
                movable: true,
            }).collect(),
            layers: vec![LayerConstraintSet {
                rank: 0,
                vars: vec![0, 1, 2],
                separations: vec![50.0, 50.0],
            }],
            hard: vec![],
            objectives: vec![
                ObjectiveTerm {
                    priority: ObjectivePriority::P3,
                    coefficients: vec![(0, 1.0)],
                    constant: -100.0,
                    weight: 1.0,
                    source: ConstraintSource { kind: ConstraintSourceKind::LayerOrder, nodes: vec![], note: "bk" },
                },
                ObjectiveTerm {
                    priority: ObjectivePriority::P3,
                    coefficients: vec![(1, 1.0)],
                    constant: -200.0,
                    weight: 1.0,
                    source: ConstraintSource { kind: ConstraintSourceKind::LayerOrder, nodes: vec![], note: "bk" },
                },
                ObjectiveTerm {
                    priority: ObjectivePriority::P3,
                    coefficients: vec![(2, 1.0)],
                    constant: -300.0,
                    weight: 1.0,
                    source: ConstraintSource { kind: ConstraintSourceKind::LayerOrder, nodes: vec![], note: "bk" },
                },
            ],
            initial: InitialCoordinates { values: vec![100.0, 200.0, 300.0] },
            config: CoordinateSolverConfig::default(),
            axis: Default::default(),
        };

        let result = solve(&problem);
        assert_eq!(result.status, SolverStatus::Converged);
        // 初值已满足 sep=50，PreferBK 应保持不变
        assert!((result.coordinates[0] - 100.0).abs() < 0.1);
        assert!((result.coordinates[1] - 200.0).abs() < 0.1);
        assert!((result.coordinates[2] - 300.0).abs() < 0.1);
    }

    /// 边拉直目标：让相连节点靠近。
    #[test]
    fn edge_alignment_pulls_nodes_together() {
        // 2 层各 1 个节点，边目标让它们 x 对齐
        let problem = CoordinateProblem {
            vars: vec![
                NodeVariable { var_id: 0, stable_id: "a".into(), kind: VarKind::Real, rank: 0, order: 0, axis_size: 40.0, movable: true },
                NodeVariable { var_id: 1, stable_id: "b".into(), kind: VarKind::Real, rank: 1, order: 0, axis_size: 40.0, movable: true },
            ],
            layers: vec![
                LayerConstraintSet { rank: 0, vars: vec![0], separations: vec![] },
                LayerConstraintSet { rank: 1, vars: vec![1], separations: vec![] },
            ],
            hard: vec![],
            objectives: vec![
                // (x[0] - x[1])² → coefficients: [(0, 1.0), (1, -1.0)], constant: 0
                ObjectiveTerm {
                    priority: ObjectivePriority::P2,
                    coefficients: vec![(0, 1.0), (1, -1.0)],
                    constant: 0.0,
                    weight: 1.0,
                    source: ConstraintSource { kind: ConstraintSourceKind::NodeSeparation, nodes: vec![], note: "edge" },
                },
            ],
            initial: InitialCoordinates { values: vec![0.0, 80.0] },
            config: CoordinateSolverConfig::default(),
            axis: Default::default(),
        };

        let result = solve(&problem);
        // 两个节点应该靠近（不一定完全相等，因为只有一个目标）
        let diff = (result.coordinates[0] - result.coordinates[1]).abs();
        assert!(diff < 40.0, "nodes should be pulled closer, diff={}", diff);
    }

    /// 确定性：同输入多次求解结果相同。
    #[test]
    fn solver_deterministic() {
        let problem = CoordinateProblem {
            vars: (0..4).map(|i| NodeVariable {
                var_id: i,
                stable_id: format!("n{}", i),
                kind: VarKind::Real,
                rank: 0,
                order: i,
                axis_size: 30.0,
                movable: true,
            }).collect(),
            layers: vec![LayerConstraintSet {
                rank: 0,
                vars: vec![0, 1, 2, 3],
                separations: vec![40.0, 40.0, 40.0],
            }],
            hard: vec![],
            objectives: vec![
                ObjectiveTerm {
                    priority: ObjectivePriority::P2,
                    coefficients: vec![(0, 1.0), (2, -1.0)],
                    constant: 0.0,
                    weight: 2.0,
                    source: ConstraintSource { kind: ConstraintSourceKind::NodeSeparation, nodes: vec![], note: "align" },
                },
                ObjectiveTerm {
                    priority: ObjectivePriority::P3,
                    coefficients: vec![(1, 1.0)],
                    constant: -50.0,
                    weight: 1.0,
                    source: ConstraintSource { kind: ConstraintSourceKind::LayerOrder, nodes: vec![], note: "bk" },
                },
            ],
            initial: InitialCoordinates { values: vec![0.0, 50.0, 100.0, 150.0] },
            config: CoordinateSolverConfig::default(),
            axis: Default::default(),
        };

        let r1 = solve(&problem);
        let r2 = solve(&problem);
        assert_eq!(r1.coordinates, r2.coordinates);
        assert_eq!(r1.status, r2.status);
    }
}
