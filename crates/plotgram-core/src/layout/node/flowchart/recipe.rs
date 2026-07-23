//! Flowchart 布局配方（Recipe）。
//!
//! Phase 9: 显式编排 flowchart 布局的完整生命周期。
//! Recipe 是普通 Rust 编排代码，不是动态插件系统。
//!
//! ## 编排流程
//!
//! ```text
//! compile (AST → IR)
//!   → solve (IR → coordinates)
//!   → product (coordinates → NodeLayout)
//!   → freeze (NodeLayout → FrozenNodeLayout)
//! ```

use crate::ast::Diagram;
use crate::layout::kernel::coordinate::model::CoordinateProblem;
use crate::layout::kernel::coordinate::optimizer::solve;
use crate::layout::kernel::coordinate::auditor::audit_p0;
use crate::layout::NodeLayout;
use std::collections::HashMap;

/// Flowchart 布局配方。
///
/// 编排 flowchart 布局的完整生命周期：compile → solve → product → freeze。
pub struct FlowchartRecipe;

impl FlowchartRecipe {
    /// 执行完整的布局流程。
    ///
    /// 1. compile: 从 diagram 构建 CoordinateProblem（由外部 builder 完成）
    /// 2. solve: 调用 kernel solver 求解坐标
    /// 3. audit: 验证 P0 约束满足
    /// 4. product: 输出坐标结果
    pub fn execute(problem: &CoordinateProblem) -> RecipeOutput {
        // Phase 2: solve
        let result = solve(problem);

        // Phase 3: audit
        let audit = audit_p0(problem, &result.coordinates);
        if !audit.passed() {
            crate::perf_log!(
                "[recipe] P0 audit FAILED: {} violations, max={:.2}px",
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
    pub status: crate::layout::kernel::coordinate::model::SolverStatus,
    /// P0 审计是否通过。
    pub audit_passed: bool,
    /// 各 priority 的最终 loss。
    pub loss_p1: f64,
    pub loss_p2: f64,
    pub loss_p3: f64,
}
