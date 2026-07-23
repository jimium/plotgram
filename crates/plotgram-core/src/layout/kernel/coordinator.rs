//! Coordinate Kernel 调用封装。
//!
//! 提供 `CoordinateProblem → solve + P0 audit` 的统一入口。
//! 由分层类 LayoutRecipe（flowchart/architecture）在 solve 阶段调用。
//!
//! ## 与 LayoutRecipe 的关系
//!
//! - [`LayoutRecipe`](super::recipe::LayoutRecipe)：整图节点布局的生命周期（compile → solve → audit → product）
//! - [`CoordinateSolveStep`]（本模块）：仅 CoordinateProblem → 坐标，是 LayoutRecipe::solve 内部的一个步骤
//!
//! ## 设计原则
//!
//! 1. **无图类型语义**：不知道 flowchart/architecture 的区别
//! 2. **确定性**：相同输入必须产生相同输出

use super::coordinate::auditor::audit_p0;
use super::coordinate::model::{CoordinateProblem, SolverStatus};
use super::coordinate::optimizer::solve;

/// Coordinate Kernel 调用步骤：定义 solve + audit 的封装。
///
/// 由分层类 LayoutRecipe 在 solve 阶段调用。
/// 默认实现调用 kernel solver + P0 审计。
pub trait CoordinateSolveStep {
    /// 步骤名称（用于日志）。
    fn name(&self) -> &'static str;

    /// 求解坐标（通用流程：solve + audit）。
    fn solve(&self, problem: &CoordinateProblem) -> CoordinateSolveOutput {
        let result = solve(problem);

        let audit = audit_p0(problem, &result.coordinates);
        if !audit.passed() {
            crate::perf_log!(
                "[{}] P0 audit FAILED: {} violations, max={:.2}px",
                self.name(),
                audit.separation_violations,
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
        }
    }
}

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
}

/// Coordinate Kernel 统一入口。
///
/// 封装 solve + P0 audit 流程，由 LayoutRecipe 在 solve 阶段调用。
///
/// ## 使用示例
///
/// ```ignore
/// let output = LayoutCoordinator::run(&ArchitectureRecipeAdapter, &problem);
/// let centers = output.coordinates;
/// ```
pub struct LayoutCoordinator;

impl LayoutCoordinator {
    /// 执行 Coordinate Kernel 求解流程。
    ///
    /// 1. 调用 CoordinateSolveStep::solve（内部执行 kernel solver + P0 审计）
    /// 2. 返回 CoordinateSolveOutput（坐标 + 审计状态 + loss）
    pub fn run<R: CoordinateSolveStep>(step: &R, problem: &CoordinateProblem) -> CoordinateSolveOutput {
        crate::perf_log!(
            "[coordinator] running step '{}' with {} vars, {} layers",
            step.name(),
            problem.vars.len(),
            problem.layers.len(),
        );
        step.solve(problem)
    }
}

// ─── 适配器：为不同图种提供命名 ─────────────────────────────────

/// Flowchart 坐标求解步骤。
pub struct FlowchartRecipeAdapter;

impl CoordinateSolveStep for FlowchartRecipeAdapter {
    fn name(&self) -> &'static str {
        "flowchart"
    }
}

/// Architecture 坐标求解步骤。
pub struct ArchitectureRecipeAdapter;

impl CoordinateSolveStep for ArchitectureRecipeAdapter {
    fn name(&self) -> &'static str {
        "architecture"
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
        }
    }

    #[test]
    fn test_coordinator_runs_recipe() {
        let recipe = FlowchartRecipeAdapter;
        let problem = make_simple_problem();
        let output = LayoutCoordinator::run(&recipe, &problem);
        assert!(output.audit_passed);
        assert_eq!(output.coordinates.len(), 2);
        // 最小分离 60.0 应满足
        assert!(output.coordinates[1] - output.coordinates[0] >= 60.0 - 0.01);
    }

    #[test]
    fn test_architecture_recipe_adapter() {
        let recipe = ArchitectureRecipeAdapter;
        let problem = make_simple_problem();
        let output = LayoutCoordinator::run(&recipe, &problem);
        assert!(output.audit_passed);
    }

    #[test]
    fn test_recipe_output_deterministic() {
        let recipe = FlowchartRecipeAdapter;
        let problem = make_simple_problem();
        let out1 = LayoutCoordinator::run(&recipe, &problem);
        let out2 = LayoutCoordinator::run(&recipe, &problem);
        assert_eq!(out1.coordinates, out2.coordinates);
    }
}
