//! 图表类型（语言判别式）。
//!
//! 与 AST 文档结构解耦：[`crate::ast::Diagram`] 引用本类型，
//! 但 layout、spec、theme 等模块可在无完整 AST 时单独使用。

use serde::{Deserialize, Serialize};

/// 图表类型
///
/// **扩展性设计**：
/// - MVP 阶段仅实现 Flowchart
/// - 其他类型预留，未来可扩展
/// - 每种类型可能有不同的：
///   - 布局算法偏好
///   - 节点形状约定
///   - 关系语义
///   - 验证规则
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum DiagramType {
    /// 流程图
    #[default]
    Flowchart,
    /// 时序图
    Sequence,
    /// 架构图
    Architecture,
    /// 状态图
    State,
    /// ER 图
    Er,
    /// 思维导图
    Mindmap,
    /// 自定义图表类型（扩展预留）
    Custom(String),
}

impl DiagramType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "flowchart" => Some(Self::Flowchart),
            "sequence" => Some(Self::Sequence),
            "architecture" => Some(Self::Architecture),
            "state" => Some(Self::State),
            "er" => Some(Self::Er),
            "mindmap" => Some(Self::Mindmap),
            other => Some(Self::Custom(other.to_string())),
        }
    }

    /// 获取图表类型的显示名称
    pub fn display_name(&self) -> &str {
        match self {
            Self::Flowchart => "流程图",
            Self::Sequence => "时序图",
            Self::Architecture => "架构图",
            Self::State => "状态图",
            Self::Er => "ER 图",
            Self::Mindmap => "思维导图",
            Self::Custom(name) => name.as_str(),
        }
    }

    /// 返回 theme StyleSheet 中对应的 diagram key。
    ///
    /// 与 `KNOWN_DIAGRAM_TYPES` 对齐：小写枚举变体名。
    pub fn style_key(&self) -> &str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Sequence => "sequence",
            Self::Architecture => "architecture",
            Self::State => "state",
            Self::Er => "er",
            Self::Mindmap => "mindmap",
            Self::Custom(name) => name.as_str(),
        }
    }
}

impl Serialize for DiagramType {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Flowchart => serializer.serialize_str("flowchart"),
            Self::Sequence => serializer.serialize_str("sequence"),
            Self::Architecture => serializer.serialize_str("architecture"),
            Self::State => serializer.serialize_str("state"),
            Self::Er => serializer.serialize_str("er"),
            Self::Mindmap => serializer.serialize_str("mindmap"),
            Self::Custom(name) => serializer.serialize_str(name.as_str()),
        }
    }
}

impl<'de> Deserialize<'de> for DiagramType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str(&s).unwrap_or(Self::Custom(s)))
    }
}
