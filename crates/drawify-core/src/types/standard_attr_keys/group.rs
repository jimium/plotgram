//! Group 级 `attributes.standard` 键。

pub use super::diagram::LAYOUT;

// 解析策略：`parse_atom_attribute_value`

/// 分组边框样式（solid / dashed / dotted）
pub const BORDER_STYLE: &str = "border_style";

// 解析策略：`parse_string_attribute_value`

/// 分组颜色
pub const COLOR: &str = "color";
