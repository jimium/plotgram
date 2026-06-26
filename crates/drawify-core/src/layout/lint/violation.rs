//! LayoutLint 违规类型与报告。

use serde::{Deserialize, Serialize};

/// 规则严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Error,
    Warning,
}

/// 内置 lint 规则 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintRuleId {
    /// 两个节点 AABB 重叠
    NodeOverlap,
    /// 两个无嵌套关系的分组 AABB 重叠
    GroupOverlap,
    /// 节点超出所属分组边界
    NodeOutsideGroup,
    /// 子分组超出父分组边界
    ChildGroupOutsideParent,
    /// 边路径穿过非端点节点
    EdgeThroughNode,
    /// 两条边在非共享端点处交叉
    EdgeCrossing,
    /// 边路径与分组边框重合/贴边
    EdgeOnGroupBorder,
    /// 边穿过某分组内部，但端点均不属于该分组
    EdgeCrossesGroupInterior,
}

impl LintRuleId {
    pub const COUNT: usize = 8;

    pub const ALL: [LintRuleId; Self::COUNT] = [
        LintRuleId::NodeOverlap,
        LintRuleId::GroupOverlap,
        LintRuleId::NodeOutsideGroup,
        LintRuleId::ChildGroupOutsideParent,
        LintRuleId::EdgeThroughNode,
        LintRuleId::EdgeCrossing,
        LintRuleId::EdgeOnGroupBorder,
        LintRuleId::EdgeCrossesGroupInterior,
    ];

    pub fn index(self) -> usize {
        match self {
            LintRuleId::NodeOverlap => 0,
            LintRuleId::GroupOverlap => 1,
            LintRuleId::NodeOutsideGroup => 2,
            LintRuleId::ChildGroupOutsideParent => 3,
            LintRuleId::EdgeThroughNode => 4,
            LintRuleId::EdgeCrossing => 5,
            LintRuleId::EdgeOnGroupBorder => 6,
            LintRuleId::EdgeCrossesGroupInterior => 7,
        }
    }

    pub fn default_severity(self) -> LintSeverity {
        match self {
            LintRuleId::EdgeCrossing | LintRuleId::EdgeOnGroupBorder => LintSeverity::Warning,
            _ => LintSeverity::Error,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LintRuleId::NodeOverlap => "node_overlap",
            LintRuleId::GroupOverlap => "group_overlap",
            LintRuleId::NodeOutsideGroup => "node_outside_group",
            LintRuleId::ChildGroupOutsideParent => "child_group_outside_parent",
            LintRuleId::EdgeThroughNode => "edge_through_node",
            LintRuleId::EdgeCrossing => "edge_crossing",
            LintRuleId::EdgeOnGroupBorder => "edge_on_group_border",
            LintRuleId::EdgeCrossesGroupInterior => "edge_crosses_group_interior",
        }
    }
}

/// 单条布局违规。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutViolation {
    pub rule: LintRuleId,
    pub severity: LintSeverity,
    pub message: String,
    /// 可量化度量（重叠面积 px²、超出距离 px、重合长度 px 等）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_index: Option<usize>,
}

impl LayoutViolation {
    pub fn new(rule: LintRuleId, message: impl Into<String>) -> Self {
        Self {
            rule,
            severity: rule.default_severity(),
            message: message.into(),
            metric: None,
            entity_ids: Vec::new(),
            group_ids: Vec::new(),
            edge_index: None,
        }
    }

    pub fn with_metric(mut self, metric: f64) -> Self {
        self.metric = Some(metric);
        self
    }

    pub fn with_entities(mut self, ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.entity_ids = ids.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_groups(mut self, ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.group_ids = ids.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_edge_index(mut self, index: usize) -> Self {
        self.edge_index = Some(index);
        self
    }
}

/// 完整 lint 报告。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LintReport {
    pub violations: Vec<LayoutViolation>,
}

impl LintReport {
    /// 无 error 级违规（忽略 warning）。
    pub fn is_clean(&self) -> bool {
        self.error_count() == 0
    }

    /// 根据配置判断是否可接受（`fail_on_warning` 时 warning 也算失败）。
    pub fn is_acceptable(&self, config: &super::LintConfig) -> bool {
        if config.fail_on_warning {
            self.violations.is_empty()
        } else {
            self.is_clean()
        }
    }

    pub fn error_count(&self) -> usize {
        self.violations
            .iter()
            .filter(|v| v.severity == LintSeverity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.violations
            .iter()
            .filter(|v| v.severity == LintSeverity::Warning)
            .count()
    }

    pub fn by_rule(&self, rule: LintRuleId) -> impl Iterator<Item = &LayoutViolation> {
        self.violations.iter().filter(move |v| v.rule == rule)
    }
}
