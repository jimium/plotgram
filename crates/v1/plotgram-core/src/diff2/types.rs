//! Intent Diff 变更类型定义
//!
//! 本模块的类型用于表达两份 RawDiagram 之间的语义差异，
//! 并作为 patch 的输入。

use crate::ast::RawDiagram;
use serde::{Deserialize, Serialize};

/// 变更操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOp {
    Add,
    Remove,
    Modify,
}

/// 变更目标类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeTarget {
    /// diagram 级属性或 diagram_type
    Diagram,
    /// entity
    Entity,
    /// relation
    Relation,
    /// group
    Group,
    /// node_style / edge_style 声明
    StyleDecl,
}

/// 变更路径 — 精确定位 RawDiagram 中的元素
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangePath {
    pub target: ChangeTarget,
    /// 元素标识：
    /// - `Entity`: entity id（如 `"api"`）
    /// - `Relation`: `"from->to"`（无 label）或 `"from->to::label"`（有 label）
    /// - `Group`: group id
    /// - `StyleDecl`: `"node_style/target"` 或 `"edge_style/target"`
    /// - `Diagram`: `None`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 子属性键（`None` 表示整个元素增删）：
    /// - `"diagram_type"` / `"label"` / `"group_id"` / `"parent_id"` / `"arrow"`
    /// - `"standard/<key>"` / `"style/<key>"` / `"meta/<key>"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attr_key: Option<String>,
}

impl ChangePath {
    pub fn diagram_attr(key: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::Diagram,
            id: None,
            attr_key: Some(key.into()),
        }
    }

    pub fn entity(id: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::Entity,
            id: Some(id.into()),
            attr_key: None,
        }
    }

    pub fn entity_attr(id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::Entity,
            id: Some(id.into()),
            attr_key: Some(key.into()),
        }
    }

    pub fn relation(id: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::Relation,
            id: Some(id.into()),
            attr_key: None,
        }
    }

    pub fn relation_attr(id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::Relation,
            id: Some(id.into()),
            attr_key: Some(key.into()),
        }
    }

    pub fn group(id: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::Group,
            id: Some(id.into()),
            attr_key: None,
        }
    }

    pub fn group_attr(id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::Group,
            id: Some(id.into()),
            attr_key: Some(key.into()),
        }
    }

    pub fn style_decl(id: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::StyleDecl,
            id: Some(id.into()),
            attr_key: None,
        }
    }

    pub fn style_decl_attr(id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            target: ChangeTarget::StyleDecl,
            id: Some(id.into()),
            attr_key: Some(key.into()),
        }
    }
}

/// 单个变更记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub op: ChangeOp,
    pub path: ChangePath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<serde_json::Value>,
}

impl Change {
    pub fn add(path: ChangePath, value: serde_json::Value) -> Self {
        Self {
            op: ChangeOp::Add,
            path,
            old_value: None,
            new_value: Some(value),
        }
    }

    pub fn remove(path: ChangePath, old_value: serde_json::Value) -> Self {
        Self {
            op: ChangeOp::Remove,
            path,
            old_value: Some(old_value),
            new_value: None,
        }
    }

    pub fn modify(
        path: ChangePath,
        old_value: serde_json::Value,
        new_value: serde_json::Value,
    ) -> Self {
        Self {
            op: ChangeOp::Modify,
            path,
            old_value: Some(old_value),
            new_value: Some(new_value),
        }
    }
}

/// 变更集 — diff 的输出，patch 的输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSet {
    pub changes: Vec<Change>,
}

impl ChangeSet {
    pub fn new(changes: Vec<Change>) -> Self {
        Self { changes }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }
}

/// Patch 应用结果
#[derive(Debug, Clone)]
pub struct PatchResult {
    /// patch 后的 RawDiagram（即使部分变更失败也返回已应用部分的结果）
    pub diagram: RawDiagram,
    /// 成功应用的变更数
    pub applied: usize,
    /// 失败的变更错误列表
    pub errors: Vec<String>,
}

impl PatchResult {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}
