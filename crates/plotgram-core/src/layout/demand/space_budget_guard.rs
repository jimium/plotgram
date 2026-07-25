//! SpaceBudget hint 设置。
//!
//! R11b：推节点逻辑已删除——coordinator 的 FrozenNodeProduct 保证 route 后不推节点；
//! budget 违规由 coordinator repair round 处理。
//! Slice F2c：旧的 moved-节点 diff + 空转增量重路由路径已删除——
//! 跨渲染增量统一走 `FrozenRoutingSolution` 依赖记录。

use crate::ast::Diagram;
use crate::layout::demand::space_budget::SpaceBudget;
use crate::layout::LayoutResult;

/// 检测预算违规（R11b：推节点逻辑已删除，仅设置 budget hint）。
///
/// coordinator 的 FrozenNodeProduct 保证 route 后不推节点；
/// budget 违规由 coordinator repair round 处理。
pub fn resolve_budget_violations(diagram: &Diagram, mut result: LayoutResult) -> LayoutResult {
    let budget = result
        .hints
        .space_budget
        .clone()
        .unwrap_or_else(|| SpaceBudget::from_diagram(diagram));

    result.hints.space_budget = Some(budget);
    result
}
