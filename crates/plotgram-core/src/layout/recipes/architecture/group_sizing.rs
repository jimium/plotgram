//! 顶层分组宽度策略：fit（内容贴合）与 uniform（等宽阶段条带）
//!
//! Stage 5：默认仍 Fit；Dialect Profile `group_sizing=Equal` 经
//! [`set_override_for_solve`] 在 StrongMacro 收缩期间切换为 Uniform。

use crate::ast::Diagram;
use std::cell::Cell;

thread_local! {
    static SIZING_OVERRIDE: Cell<Option<GroupSizingPolicy>> = const { Cell::new(None) };
}

/// 图级分组宽度策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSizingPolicy {
    /// 组宽 = 组内内容 + padding
    Fit,
    /// 所有顶层 group 拉齐到最宽者
    Uniform,
}

/// 生产默认 Fit；若 Dialect 设置了 override 则用之。
pub fn parse_group_sizing(_diagram: &Diagram) -> GroupSizingPolicy {
    SIZING_OVERRIDE
        .with(|c| c.get())
        .unwrap_or(GroupSizingPolicy::Fit)
}

/// StrongMacro 求解前由 Atlas 注入 Profile 的 sizing。
pub fn set_override_for_solve(policy: GroupSizingPolicy) {
    SIZING_OVERRIDE.with(|c| c.set(Some(policy)));
}

pub fn clear_override_for_solve() {
    SIZING_OVERRIDE.with(|c| c.set(None));
}

/// 组块 trait：供宏观间距等按块 id / 是否 group 过滤
pub trait GroupWidthBlock {
    fn block_id(&self) -> &str;
    fn is_group_block(&self) -> bool;
}
