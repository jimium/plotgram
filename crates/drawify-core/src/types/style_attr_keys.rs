//! `attributes.style` 命名空间下的样式属性键常量。
//!
//! 与 [`super::standard_attr_keys`] 对称：
//! - `standard_attr_keys`：结构/语义属性（layout、type、theme 等）
//! - `style_attr_keys`：视觉样式属性（fill、stroke、shape 等）

// ── 实体样式键 ──────────────────────────────────────────────

/// 节点填充色
pub const FILL: &str = "fill";
/// 描边颜色
pub const STROKE: &str = "stroke";
/// 描边宽度
pub const STROKE_WIDTH: &str = "stroke_width";
/// 描边虚线模式
pub const STROKE_DASHARRAY: &str = "stroke_dasharray";
/// 形状
pub const SHAPE: &str = "shape";
/// 标签字重
pub const LABEL_WEIGHT: &str = "label_weight";
/// 宽度
pub const WIDTH: &str = "width";
/// 高度
pub const HEIGHT: &str = "height";
/// 文本填充色
pub const TEXT_FILL: &str = "text_fill";
/// 字体大小
pub const FONT_SIZE: &str = "font_size";
/// 字体粗细
pub const FONT_WEIGHT: &str = "font_weight";
/// 圆角半径
pub const RADIUS: &str = "radius";
/// 变换
pub const TRANSFORM: &str = "transform";

// ── 关系样式键 ──────────────────────────────────────────────

/// 虚线
pub const DASHED: &str = "dashed";
/// 标签颜色
pub const LABEL_COLOR: &str = "label_color";

// ── 边标签样式键（EdgeLabelStyle）────────────────────────────

/// 标签背景色（"none" = 透明）
pub const LABEL_BG: &str = "label_bg";
/// 标签背景不透明度（0.0 ~ 1.0）
pub const LABEL_BG_OPACITY: &str = "label_bg_opacity";
/// 标签边框色（"none" = 无边框）
pub const LABEL_BORDER: &str = "label_border";
/// 标签边框宽度
pub const LABEL_BORDER_WIDTH: &str = "label_border_width";
/// 标签圆角半径
pub const LABEL_BORDER_RADIUS: &str = "label_border_radius";
/// 标签内边距
pub const LABEL_PADDING: &str = "label_padding";
/// 标签字号
pub const LABEL_FONT_SIZE: &str = "label_font_size";
/// 标签字重
pub const LABEL_FONT_WEIGHT: &str = "label_font_weight";
/// 标签位置锚点（middle|start|end|t:0.25）
pub const LABEL_POSITION: &str = "label_position";
/// 标签旋转（none|along_edge|<角度数值>）
pub const LABEL_ROTATION: &str = "label_rotation";

// ── 其他样式键 ──────────────────────────────────────────────

/// 背景色
pub const BACKGROUND: &str = "background";
/// 通用颜色（回退）
pub const COLOR: &str = "color";
