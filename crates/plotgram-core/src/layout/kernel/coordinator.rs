//! Coordinate Kernel 调用封装。
//!
//! 提供 `CoordinateProblem → validate + solve + P0 audit` 的统一入口。
//! 由分层类 LayoutRecipe（flowchart/architecture）在 solve 阶段调用。
//!
//! ## 与 LayoutRecipe 的关系
//!
//! - [`LayoutRecipe`](super::recipe::LayoutRecipe)：整图节点布局的生命周期（compile → solve → product）
//! - [`CoordinateKernel`]（本模块）：仅 CoordinateProblem → 坐标，是 LayoutRecipe::solve 内部的一个步骤
//!
//! ## 设计原则
//!
//! 1. **无图类型语义**：不知道 flowchart/architecture 的区别
//! 2. **确定性**：相同输入必须产生相同输出

use super::coordinate::analysis::problem_signature;
use super::coordinate::auditor::{audit_p0, validate_problem};
use super::coordinate::model::{CoordinateProblem, SolverStatus};
use super::coordinate::optimizer::solve;

/// Coordinate Kernel 求解输出：坐标 + 审计状态。
#[derive(Debug, Clone)]
pub struct CoordinateSolveOutput {
    /// 最终坐标（下标 = var_id）。
    pub coordinates: Vec<f64>,
    /// 求解状态。
    pub status: SolverStatus,
    /// P0 审计是否通过。
    pub audit_passed: bool,
    /// 各 priority 的最终 loss。
    pub loss_p1: f64,
    pub loss_p2: f64,
    pub loss_p3: f64,
    /// 问题确定性签名（用于调试和回归检测）。
    pub problem_signature: u64,
}

/// Coordinate Kernel 统一入口。
///
/// 封装 validate + solve + P0 audit 流程，由 LayoutRecipe 在 solve 阶段调用。
///
/// ## 使用示例
///
/// ```ignore
/// let output = CoordinateKernel::solve("architecture", &problem);
/// let centers = output.coordinates;
/// ```
pub struct CoordinateKernel;

impl CoordinateKernel {
    /// 执行 Coordinate Kernel 求解流程。
    ///
    /// 1. validate：IR 结构合法性检查
    /// 2. solve：投影梯度优化器
    /// 3. audit：P0 硬约束审计
    pub fn solve(name: &str, problem: &CoordinateProblem) -> CoordinateSolveOutput {
        let signature = problem_signature(problem);

        crate::perf_log!(
            "[coordinate-kernel] solving '{}' with {} vars, {} layers, sig={:016x}",
            name,
            problem.vars.len(),
            problem.layers.len(),
            signature,
        );

        // 前置验证：IR 结构合法性
        if let Err(errors) = validate_problem(problem) {
            crate::perf_log!(
                "[{}] problem validation FAILED: {} errors",
                name,
                errors.len()
            );
            for e in &errors {
                crate::perf_log!("[{}]   {}", name, e);
            }
            return CoordinateSolveOutput {
                coordinates: vec![0.0; problem.var_count()],
                status: SolverStatus::Infeasible,
                audit_passed: false,
                loss_p1: 0.0,
                loss_p2: 0.0,
                loss_p3: 0.0,
                problem_signature: signature,
            };
        }

        let result = solve(problem);

        let audit = audit_p0(problem, &result.coordinates);
        if !audit.passed() {
            crate::perf_log!(
                "[{}] P0 audit FAILED: {} separation + {} bound violations, max={:.2}px",
                name,
                audit.separation_violations,
                audit.bound_violations,
                audit.max_violation
            );
        }

        CoordinateSolveOutput {
            coordinates: result.coordinates.clone(),
            status: result.status,
            audit_passed: audit.passed(),
            loss_p1: result.loss_p1,
            loss_p2: result.loss_p2,
            loss_p3: result.loss_p3,
            problem_signature: signature,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::kernel::coordinate::model::*;

    fn make_simple_problem() -> CoordinateProblem {
        CoordinateProblem {
            vars: vec![
                NodeVariable {
                    var_id: 0, stable_id: "a".into(), kind: VarKind::Real,
                    rank: 0, order: 0, axis_size: 40.0, movable: true,
                },
                NodeVariable {
                    var_id: 1, stable_id: "b".into(), kind: VarKind::Real,
                    rank: 0, order: 1, axis_size: 40.0, movable: true,
                },
            ],
            layers: vec![LayerConstraintSet {
                rank: 0,
                vars: vec![0, 1],
                separations: vec![60.0],
            }],
            hard: vec![],
            objectives: vec![],
            initial: InitialCoordinates { values: vec![0.0, 100.0] },
            config: CoordinateSolverConfig::default(),
            axis: Default::default(),
        }
    }

    #[test]
    fn test_coordinate_kernel_solve() {
        let problem = make_simple_problem();
        let output = CoordinateKernel::solve("flowchart", &problem);
        assert!(output.audit_passed);
        assert_eq!(output.coordinates.len(), 2);
        // 最小分离 60.0 应满足
        assert!(output.coordinates[1] - output.coordinates[0] >= 60.0 - 0.01);
    }

    #[test]
    fn test_coordinate_kernel_deterministic() {
        let problem = make_simple_problem();
        let out1 = CoordinateKernel::solve("test", &problem);
        let out2 = CoordinateKernel::solve("test", &problem);
        assert_eq!(out1.coordinates, out2.coordinates);
    }
}
