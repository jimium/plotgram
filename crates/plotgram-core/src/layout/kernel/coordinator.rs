//! 统一布局生命周期管理。
//!
//! Phase D: LayoutCoordinator 编排 compile → solve → audit → product 流程。
//! 不包含图类型业务分支，只编排通用流程。
//!
//! ## 设计原则
//!
//! 1. **无图类型语义**：Coordinator 不知道 flowchart/architecture 的区别
//! 2. **Recipe 驱动**：所有布局语义由 Recipe 实现提供
//! 3. **确定性**：相同输入必须产生相同输出

use super::coordinate::auditor::audit_p0;
use super::coordinate::model::{CoordinateProblem, SolverStatus};
use super::coordinate::optimizer::solve;

/// 布局配方 trait：定义从 IR 到坐标的完整生命周期。
///
/// 每个图类型实现此 trait，提供自己的 compile/product 逻辑。
/// Coordinator 只编排通用流程（solve + audit），不关心图类型语义。
pub trait Recipe {
    /// 配方名称（用于日志）。
    fn name(&self) -> &'static str;

    /// 求解坐标（通用流程：solve + audit）。
    ///
    /// 默认实现调用 kernel solver + P0 审计。
    /// Recipe 可覆盖此方法以添加自定义后处理。
    fn solve(&self, problem: &CoordinateProblem) -> RecipeOutput {
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

        RecipeOutput {
            coordinates: result.coordinates.clone(),
            status: result.status,
            audit_passed: audit.passed(),
            loss_p1: result.loss_p1,
            loss_p2: result.loss_p2,
            loss_p3: result.loss_p3,
        }
    }
}

/// Recipe 输出：求解结果 + 审计状态。
#[derive(Debug, Clone)]
pub struct RecipeOutput {
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

/// 统一布局生命周期管理器。
///
/// 编排 compile → solve → audit → product 流程。
/// 不包含图类型业务分支，只消费 Recipe 提供的 IR。
///
/// ## 使用示例
///
/// ```ignore
/// let recipe = FlowchartRecipeAdapter;
/// let problem = recipe.compile(&diagram);
/// let output = LayoutCoordinator::run(&recipe, &problem);
/// let nodes = recipe.product(&output, &diagram);
/// ```
pub struct LayoutCoordinator;

impl LayoutCoordinator {
    /// 执行布局求解流程。
    ///
    /// 1. 调用 Recipe::solve（内部执行 kernel solver + P0 审计）
    /// 2. 返回 RecipeOutput（坐标 + 审计状态 + loss）
    ///
    /// 调用方负责 compile（构建 CoordinateProblem）和 product（物化坐标）。
    pub fn run<R: Recipe>(recipe: &R, problem: &CoordinateProblem) -> RecipeOutput {
        crate::perf_log!(
            "[coordinator] running recipe '{}' with {} vars, {} layers",
            recipe.name(),
            problem.vars.len(),
            problem.layers.len(),
        );
        recipe.solve(problem)
    }
}

// ─── 适配器：将现有 Recipe 结构体适配为 trait ─────────────────────────────────

/// FlowchartRecipe 的 Recipe trait 适配器。
pub struct FlowchartRecipeAdapter;

impl Recipe for FlowchartRecipeAdapter {
    fn name(&self) -> &'static str {
        "flowchart"
    }
}

/// ArchitectureRecipe 的 Recipe trait 适配器。
pub struct ArchitectureRecipeAdapter;

impl Recipe for ArchitectureRecipeAdapter {
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
