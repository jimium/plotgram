//! 顶层分组宽度策略：fit（内容贴合）与 uniform（等宽阶段条带）
//!
//! G-pre（doc 31）：不再消费 `group_frame { track }`；生产恒为 Fit。

use crate::ast::Diagram;

/// 图级分组宽度策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSizingPolicy {
    /// 组宽 = 组内内容 + padding
    Fit,
    /// 所有顶层 group 拉齐到最宽者（G-pre 后生产不再选用）
    Uniform,
}

/// G-pre：恒返回 Fit；不再读 DSL `track`。
pub fn parse_group_sizing(_diagram: &Diagram) -> GroupSizingPolicy {
    GroupSizingPolicy::Fit
}

/// 组块 trait：供宏观间距等按块 id / 是否 group 过滤
pub trait GroupWidthBlock {
    fn block_id(&self) -> &str;
    fn is_group_block(&self) -> bool;
}
