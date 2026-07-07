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
    // ── Edge Bundling 专项规则（仅 Warning，供算法优化参考）──
    /// 同 bundle 内多条边指向同一节点（箭头冗余）
    BundledArrowConvergence,
    /// 同 bundle 内存在语义反向的边（出入方向不一致）
    BundledOppositeFlow,
    /// bundle 的 merge leg 分叉点过密
    BundleMergeDensity,
    /// bundle 的 fork leg 交叉
    BundleForkOverlap,
    /// bundle 主干穿过节点
    BundleTrunkThroughNode,
}

impl LintRuleId {
    pub const COUNT: usize = 13;

    pub const ALL: [LintRuleId; Self::COUNT] = [
        LintRuleId::NodeOverlap,
        LintRuleId::GroupOverlap,
        LintRuleId::NodeOutsideGroup,
        LintRuleId::ChildGroupOutsideParent,
        LintRuleId::EdgeThroughNode,
        LintRuleId::EdgeCrossing,
        LintRuleId::EdgeOnGroupBorder,
        LintRuleId::EdgeCrossesGroupInterior,
        LintRuleId::BundledArrowConvergence,
        LintRuleId::BundledOppositeFlow,
        LintRuleId::BundleMergeDensity,
        LintRuleId::BundleForkOverlap,
        LintRuleId::BundleTrunkThroughNode,
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
            LintRuleId::BundledArrowConvergence => 8,
            LintRuleId::BundledOppositeFlow => 9,
            LintRuleId::BundleMergeDensity => 10,
            LintRuleId::BundleForkOverlap => 11,
            LintRuleId::BundleTrunkThroughNode => 12,
        }
    }

    pub fn default_severity(self) -> LintSeverity {
        match self {
            LintRuleId::EdgeCrossing
            | LintRuleId::EdgeOnGroupBorder
            | LintRuleId::BundledArrowConvergence
            | LintRuleId::BundledOppositeFlow
            | LintRuleId::BundleMergeDensity
            | LintRuleId::BundleForkOverlap
            | LintRuleId::BundleTrunkThroughNode => LintSeverity::Warning,
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
            LintRuleId::BundledArrowConvergence => "bundled_arrow_convergence",
            LintRuleId::BundledOppositeFlow => "bundled_opposite_flow",
            LintRuleId::BundleMergeDensity => "bundle_merge_density",
            LintRuleId::BundleForkOverlap => "bundle_fork_overlap",
            LintRuleId::BundleTrunkThroughNode => "bundle_trunk_through_node",
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
