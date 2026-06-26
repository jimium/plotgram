//! Diagram 级 `attributes.standard` 键。

// 解析策略：`parse_string_attribute_value`

/// 图表标题
pub const TITLE: &str = "title";

// 解析策略：`parse_algorithm_config_value`（Atom 或 Config）

/// 布局算法选择
pub const LAYOUT: &str = "layout";
/// 边路由算法选择
pub const EDGE_ROUTING: &str = "edge_routing";
/// Group Frame 统一配置块（spec §6.2 sugar）
///
/// 语法：`group_frame: stack { axis: horizontal, gap: 48, track: equal, cross: start, snap: 8 }`
/// 旧属性（`group_sizing` / `group_arrangement` / `group_gap` / `group_align` / `snap`）保留为 sugar，
/// 解析为同一 `GroupFrameSpec`。
pub const GROUP_FRAME: &str = "group_frame";

// 解析策略：`parse_atom_attribute_value`

/// 布局方向（top-to-bottom / left-to-right / radial）
pub const DIRECTION: &str = "direction";
/// 渲染风格
pub const RENDER_STYLE: &str = "render_style";
/// 主题
pub const THEME: &str = "theme";
/// 分组尺寸策略
pub const GROUP_SIZING: &str = "group_sizing";

// 值类型：Number

/// 分治布局下 group 之间的间距（垂直堆叠时的垂直间距）
pub const GROUP_GAP: &str = "group_gap";

// 值类型：Atom

/// 分治布局下 group 之间的对齐方式（center / left）
pub const GROUP_ALIGN: &str = "group_align";

/// 分治布局下 group 之间的排列方向（vertical / horizontal）
pub const GROUP_ARRANGEMENT: &str = "group_arrangement";

// 值类型：Boolean

/// 网格吸附开关
pub const SNAP: &str = "snap";
