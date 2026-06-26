//! 导出场景的视觉样式类型（与图表类型、编码格式无关的纯数据模型）。

use serde::Serialize;

/// 节点视觉样式（物化后的 fill/stroke/shape 等）。
#[derive(Debug, Clone, Serialize)]
pub struct NodeStyle {
    pub fill: String,
    pub stroke: String,
    pub shape: NodeShape,
    pub stroke_width: f64,
    pub stroke_dasharray: Option<String>,
    pub stroke_linecap: Option<String>,
    pub stroke_linejoin: Option<String>,
    pub transform: Option<String>,
    pub radius: Option<f64>,
    pub label_weight: Option<String>,
    pub hand_drawn: bool,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            fill: "#FFFFFF".to_string(),
            stroke: "#333333".to_string(),
            shape: NodeShape::Rect,
            stroke_width: 1.5,
            stroke_dasharray: None,
            stroke_linecap: None,
            stroke_linejoin: None,
            transform: None,
            radius: None,
            label_weight: None,
            hand_drawn: false,
        }
    }
}

impl NodeStyle {
    /// 节点圆角：优先物化后的 `radius`，否则按形状回退默认值。
    pub fn corner_radius(&self, shape: &NodeShape, width: f64, height: f64) -> f64 {
        match shape {
            NodeShape::RoundedRect => self.radius.unwrap_or(8.0),
            NodeShape::Stadium => {
                let cap = height.min(width) / 2.0;
                self.radius.map(|r| r.min(cap)).unwrap_or(cap)
            }
            NodeShape::Rect => self.radius.unwrap_or(4.0),
            _ => self.radius.unwrap_or(0.0),
        }
    }
}

/// 节点形状枚举。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NodeShape {
    Rect,
    RoundedRect,
    Circle,
    Diamond,
    Cylinder,
    Hexagon,
    Person,
    Stadium,
    /// 平行四边形（流程图输入/输出）
    Parallelogram,
    /// 文档形（底部波浪边）
    Document,
    /// 云形（架构图云服务）
    Cloud,
    /// 子流程（双边框矩形）
    Subprocess,
}

/// 边视觉样式。
#[derive(Debug, Clone, Serialize)]
pub struct EdgeStyle {
    pub stroke: String,
    pub dashed: bool,
    pub stroke_width: f64,
    pub stroke_dasharray: Option<String>,
    pub stroke_linecap: Option<String>,
    pub stroke_linejoin: Option<String>,
    pub hand_drawn: bool,
    pub arrow: ArrowStyle,
    /// 边标签样式（独立于边的描边样式）
    pub label_style: EdgeLabelStyle,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            stroke: "#555555".to_string(),
            dashed: false,
            stroke_width: 1.5,
            stroke_dasharray: None,
            stroke_linecap: None,
            stroke_linejoin: None,
            hand_drawn: false,
            arrow: ArrowStyle::Normal,
            label_style: EdgeLabelStyle::default(),
        }
    }
}

/// 边标签视觉样式。
///
/// 独立于边的描边样式，控制标签的字体、背景、边框、padding 等。
/// 默认带半透明白底，避免边路径穿过标签文字。
#[derive(Debug, Clone, Serialize)]
pub struct EdgeLabelStyle {
    /// 字号
    pub font_size: f64,
    /// 字体族（"inherit" 表示跟随主题）
    pub font_family: String,
    /// 字重（None 表示默认）
    pub font_weight: Option<String>,
    /// 文字颜色
    pub text_color: String,
    /// 背景色（None = 透明）
    pub bg_color: Option<String>,
    /// 背景不透明度（0.0 ~ 1.0）
    pub bg_opacity: f64,
    /// 边框颜色（None = 无边框）
    pub border_color: Option<String>,
    /// 边框宽度
    pub border_width: f64,
    /// 圆角半径
    pub border_radius: f64,
    /// 内边距 (水平, 垂直)
    pub padding: (f64, f64),
    /// 旋转模式（P2：支持 AlongEdge 沿边方向旋转）
    pub rotation: LabelRotation,
    /// 标签位置锚点（控制标签沿路径的放置位置）
    pub anchor: LabelAnchor,
}

impl Default for EdgeLabelStyle {
    fn default() -> Self {
        Self {
            font_size: 11.0,
            font_family: "inherit".to_string(),
            font_weight: None,
            text_color: "#666".to_string(),
            bg_color: Some("#ffffff".to_string()),
            bg_opacity: 0.85,
            border_color: None,
            border_width: 0.0,
            border_radius: 3.0,
            padding: (6.0, 3.0),
            rotation: LabelRotation::None,
            anchor: LabelAnchor::Middle,
        }
    }
}

/// 箭头样式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ArrowStyle {
    Normal,
    Hollow,
    None,
}

/// 边标签位置锚点（P1 启用：控制标签沿路径的放置位置）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LabelAnchor {
    /// 路径中点（默认）
    Middle,
    /// 靠近起点（t=0.15）
    Start,
    /// 靠近终点（t=0.85）
    End,
    /// 沿路径参数 t∈[0,1]
    AtPath(f64),
}

/// 边标签旋转模式（P2：支持沿边方向旋转）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LabelRotation {
    /// 不旋转
    None,
    /// 固定角度（度，顺时针为正）
    Fixed(f64),
    /// 沿边路径切线方向旋转（使用 `EdgeLabelLayout.rotation` 预计算值）
    AlongEdge,
}
