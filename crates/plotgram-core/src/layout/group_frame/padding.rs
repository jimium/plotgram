//! Group Frame padding 选择（按算法）。
//!
//! 本文件从 `mod.rs` 拆分而来，仅做代码搬家，无行为变更。

use crate::layout::node::common::group_bounds::GroupPadding;

/// 按算法返回 Group Frame 使用的 padding（与 `grid_snap::refresh_layout_bounds` 对齐）。
///
/// - `architecture`：非对称 padding (28, 48, 56, 76)
/// - 其他：`uniform(group_padding, 16.0)`（header_height=16，与 `refresh_layout_bounds` 一致）
pub fn group_padding_for_algo(algo: &str, group_padding: f64) -> GroupPadding {
    if algo == "architecture" {
        GroupPadding::architecture_v2()
    } else {
        GroupPadding::uniform(group_padding, 16.0)
    }
}
