//! Group Frame padding 选择（按算法）。
//!
//! 本文件从 `mod.rs` 拆分而来，仅做代码搬家，无行为变更。

use crate::layout::kernel::group::bounds::GroupPadding;

/// 按算法返回 Group Frame 使用的 padding（与 `grid_snap::refresh_layout_bounds` 对齐）。
///
/// - `architecture`：非对称 padding (28, 48, 56, 76)
/// - 其他：`uniform(group_padding, 16.0)`（header_height=16，与 `refresh_layout_bounds` 一致）
/// 按配方契约返回 Group Frame padding（与 `grid_snap::refresh_layout_bounds` 对齐）。
///
/// Phase 5：偏好 `GroupFrameSpec.architecture_recipe`；本函数保留给仅有 algo 名的调用点。
pub fn group_padding_for_algo(algo: &str, group_padding: f64) -> GroupPadding {
    // 单一入口：与 resolve_group_frame_spec 的 architecture 默认对齐
    match algo {
        "architecture" => GroupPadding::architecture(),
        _ => GroupPadding::uniform(group_padding, 16.0),
    }
}
