//! 统一布局配方 trait。
//!
//! 每种图类型实现 [`LayoutRecipe`]，定义自己的 compile/solve/product 逻辑。
//! 生命周期：compile → solve → audit → product。
//!
//! ## 设计原则
//!
//! 1. **associated types**：每种配方有自己的 Problem/Solution 类型，避免 Box 开销
//! 2. **默认编排**：`execute()` 有默认实现，简单配方只需实现 4 个方法
//! 3. **可插拔 solver**：coordinate 类用 PAVA+梯度，sequence 用规则，circular 用几何
//! 4. **类型擦除**：[`LayoutRecipeDyn`] 用于 LayoutStrategy 委托，避免泛型传染

use crate::ast::Diagram;
use crate::layout::LayoutResult;

/// 通用布局配方 trait。
///
/// 每种图类型实现此 trait，定义自己的 compile/solve/product 逻辑。
/// 生命周期：compile → solve → audit → product。
pub trait LayoutRecipe {
    /// 问题 IR 类型（每种配方不同）。
    type Problem;
    /// 解类型（每种配方不同）。
    type Solution;

    /// 配方名称（用于日志）。
    fn name(&self) -> &'static str;

    /// 编译：Diagram → Problem IR。
    fn compile(&self, diagram: &Diagram) -> Self::Problem;

    /// 求解：Problem → Solution。
    fn solve(&self, problem: &Self::Problem) -> Self::Solution;

    /// 审计：验证硬约束（默认 no-op，coordinate 类配方可覆盖为 P0 审计）。
    fn audit(&self, _problem: &Self::Problem, _solution: &Self::Solution) {}

    /// 物化：Solution → LayoutResult。
    fn product(&self, solution: &Self::Solution, diagram: &Diagram) -> LayoutResult;

    /// 完整执行（默认编排，子类可覆盖以添加自定义流程）。
    fn execute(&self, diagram: &Diagram) -> LayoutResult {
        let problem = self.compile(diagram);
        let solution = self.solve(&problem);
        self.audit(&problem, &solution);
        self.product(&solution, diagram)
    }
}

/// 类型擦除的 Recipe 接口（预留：未来 LayoutStrategy 统一委托入口）。
///
/// 由于 `LayoutRecipe` 使用 associated types，无法直接做 trait object。
/// 此 trait 提供擦除后的统一调用入口。当前无调用方，待 Phase R2+ 启用。
pub trait LayoutRecipeDyn {
    /// 配方名称。
    fn recipe_name(&self) -> &'static str;

    /// 类型擦除的完整执行。
    fn execute_erased(&self, diagram: &Diagram) -> LayoutResult;
}

/// 所有 LayoutRecipe 自动实现 LayoutRecipeDyn。
impl<T: LayoutRecipe> LayoutRecipeDyn for T {
    fn recipe_name(&self) -> &'static str {
        <T as LayoutRecipe>::name(self)
    }

    fn execute_erased(&self, diagram: &Diagram) -> LayoutResult {
        <T as LayoutRecipe>::execute(self, diagram)
    }
}
