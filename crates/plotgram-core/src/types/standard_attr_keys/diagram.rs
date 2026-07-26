//! Diagram 级 `attributes.standard` 键。

// 解析策略：`parse_string_attribute_value`

/// 图表标题
pub const TITLE: &str = "title";

// 解析策略：`parse_algorithm_config_value`（Atom 或 Config）

/// 布局算法选择
pub const LAYOUT: &str = "layout";
/// 边路由算法选择
pub const EDGE_ROUTING: &str = "edge_routing";
/// Group Frame 统一配置块（组间宏观几何的唯一 DSL 入口）
///
/// 语法：
/// - 完整：`group_frame: stack { axis: horizontal, gap: 48, track: equal, … }`
/// - 场景短名：`group_frame: strips` / `lanes` / `fit` / `stages` / `tiles`
/// - 短名可覆盖：`group_frame: strips { gap: 60 }`
pub const GROUP_FRAME: &str = "group_frame";

// 解析策略：`parse_atom_attribute_value`

/// 布局方向（top-to-bottom / left-to-right / radial）
pub const DIRECTION: &str = "direction";
/// 渲染风格
pub const RENDER_STYLE: &str = "render_style";
/// 主题
pub const THEME: &str = "theme";
/// 布局管线选择（legacy / atlas / shadow，Atlas Stage 0 对拍开关）
pub const PIPELINE: &str = "pipeline";

// 值类型：Boolean

/// 网格吸附开关（控制边像素量化 + L1 group quantize）
pub const SNAP: &str = "snap";

/// 节点结构对齐开关（rank/layer 轴独立控制，见 `align: rank | layer | full | false`）
pub const ALIGN: &str = "align";

/// 已移除的旧语法糖键（仅用于校验报错提示，请改用 [`GROUP_FRAME`]）。
pub mod removed {
    pub const GROUP_SIZING: &str = "group_sizing";
    pub const GROUP_GAP: &str = "group_gap";
    pub const GROUP_ALIGN: &str = "group_align";
    pub const GROUP_ARRANGEMENT: &str = "group_arrangement";

    pub const ALL: &[&str] = &[GROUP_SIZING, GROUP_GAP, GROUP_ALIGN, GROUP_ARRANGEMENT];
}
