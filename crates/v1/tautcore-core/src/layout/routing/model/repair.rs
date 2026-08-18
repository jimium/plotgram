//! `RouteRepairIntent` + `RouteAuditReport`（doc16 §13.2 / R8 Slice 8b）。
//!
//! doc16 §13.2：auditor / repair 阶段发现违规时**不得**直接改最终 points，而是编译
//! 结构化 [`RouteRepairIntent`] 交回 solver，由 Coordinator 固定轮次再解（留 R10）。
//!
//! ## R8 Slice 8b 范围
//!
//! 本 Slice 仅做**结构就位 + 记账**（用户选定 Q2）：4 处 post-route repair
//! （through / group interior / trunk / crossing）在**原地写几何**的同时，追加产出
//! 对应 intent 收集进 [`RouteAuditReport`]；真正「回 solver 重评分」的再解循环留 R10。
//! 因此本轮 intent 是 behavior-neutral 记账，几何逐字节不变。
//!
//! ## 设计红线（AGENTS.md / doc16）
//!
//! - §2 确定性：`affected_edges` 按 [`StableEdgeId`]（声明序）升序去重，不依赖 HashSet 序。
//! - §13.2：obstacle-model 违规（穿组 / 穿节点）记为 `degraded`，**不静默冒充成功**。

use super::audit::AuditViolation;
use super::stable_edge::StableEdgeId;
use std::collections::BTreeSet;

/// E2：穿节点 repair 的最小间隙（px；与 lint 0.5px 判定容差 + 安全边距）。
const THROUGH_NODE_CLEARANCE: f64 = 4.0;
/// E2：穿组 repair 的最小间隙（px）。
const GROUP_INTERIOR_CLEARANCE: f64 = 4.0;

/// repair 优先级（越靠前 Coordinator 越先处理；R10 消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepairPriority {
    /// 硬约束违规（穿节点 / 穿组）——correctness 轨，必须消解。
    Hard,
    /// 质量约束（trunk 重合 / 交叉过多）——quality 轨，尽力优化。
    Quality,
}

/// 被违反的路由约束标识（doc16 §13.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteConstraintId {
    /// 边穿越无关节点内部。
    EdgeThroughNode,
    /// 边穿越无关分组内部。
    EdgeCrossesGroupInterior,
    /// 平行 trunk 非语义重合。
    TrunkOverlap,
    /// 边交叉过多。
    EdgeCrossing,
    /// overshoot Z 折合并（topology-changing sanitize；留 R10 回 solver 重评分）。
    OvershootMerge,
    /// 端点未落在节点边界带内（Slice E1）。
    EndpointBoundary,
    /// 首末段方向与 port 不一致 / 受保护 stub 被缩短（Slice E1）。
    StubDirection,
    /// 正交 family 存在非轴对齐段（Slice E1）。
    NonOrthogonalSegment,
    /// trunk 共享段与 merge annotation 不一致（Slice E1）。
    MergeInconsistent,
}

impl RouteConstraintId {
    /// E2：约束对应的 repair 优先级（correctness 轨 Hard / quality 轨 Quality）。
    pub fn priority(self) -> RepairPriority {
        match self {
            RouteConstraintId::EdgeThroughNode
            | RouteConstraintId::EdgeCrossesGroupInterior
            | RouteConstraintId::EndpointBoundary
            | RouteConstraintId::StubDirection
            | RouteConstraintId::NonOrthogonalSegment
            | RouteConstraintId::MergeInconsistent => RepairPriority::Hard,
            RouteConstraintId::TrunkOverlap
            | RouteConstraintId::EdgeCrossing
            | RouteConstraintId::OvershootMerge => RepairPriority::Quality,
        }
    }

    /// E2：按约束类型给定的最小间隙；`None` 表示由 solver 决定。
    pub fn required_clearance(self) -> Option<f64> {
        match self {
            RouteConstraintId::EdgeThroughNode => Some(THROUGH_NODE_CLEARANCE),
            RouteConstraintId::EdgeCrossesGroupInterior => Some(GROUP_INTERIOR_CLEARANCE),
            _ => None,
        }
    }
}

/// 单条 repair 意图（doc16 §13.2）。
///
/// auditor / repair 发现违规时编译此结构交回 solver。R8 本轮几何仍由 post-route pass
/// **原地写**，intent 仅作结构化记账（真正回 solver 再解留 R10）。
#[derive(Debug, Clone, PartialEq)]
pub struct RouteRepairIntent {
    /// 受影响边（按 [`StableEdgeId`] 升序去重，§2 确定性）。
    pub affected_edges: Vec<StableEdgeId>,
    /// 被违反的约束。
    pub violated_constraints: Vec<RouteConstraintId>,
    /// 禁用资源（走廊 / 槽位标识；R10 solver 消费——本轮留空占位）。
    pub forbidden_resources: Vec<String>,
    /// 需要的最小间隙（px；`None` 表示由 solver 决定）。
    pub required_clearance: Option<f64>,
    /// 优先级。
    pub priority: RepairPriority,
}

impl RouteRepairIntent {
    /// 从一批违规边下标 + 单一约束 + 优先级构造。
    ///
    /// `edges` 经 [`BTreeSet`] 去重并升序，产出确定性 `affected_edges`（§2）。
    pub fn from_edges(
        edges: impl IntoIterator<Item = usize>,
        constraint: RouteConstraintId,
        priority: RepairPriority,
    ) -> Self {
        let affected: BTreeSet<usize> = edges.into_iter().collect();
        Self {
            affected_edges: affected.into_iter().map(StableEdgeId).collect(),
            violated_constraints: vec![constraint],
            forbidden_resources: Vec::new(),
            required_clearance: None,
            priority,
        }
    }

    /// Slice E2：由扩展审计违规编译真实 repair intents。
    ///
    /// 按 [`RouteConstraintId`] 分组（BTreeMap 升序，§2 确定性）；每组：
    /// - `affected_edges`：真实违规边，升序去重；
    /// - `forbidden_resources`：穿越的节点/分组 id，升序去重；
    /// - `required_clearance` / `priority`：按约束类型给定。
    ///
    /// 通用几何完整性违规（`constraint_id() == None`）不参与编译。
    pub fn compile_from_violations(violations: &[AuditViolation]) -> Vec<RouteRepairIntent> {
        use std::collections::BTreeMap;
        let mut by_constraint: BTreeMap<RouteConstraintId, (BTreeSet<usize>, BTreeSet<String>)> =
            BTreeMap::new();
        for v in violations {
            let Some(cid) = v.kind.constraint_id() else {
                continue;
            };
            let entry = by_constraint.entry(cid).or_default();
            entry.0.insert(v.edge.index());
            if let Some(resource) = &v.resource {
                entry.1.insert(resource.clone());
            }
        }
        by_constraint
            .into_iter()
            .map(|(cid, (edges, resources))| RouteRepairIntent {
                affected_edges: edges.into_iter().map(StableEdgeId).collect(),
                violated_constraints: vec![cid],
                forbidden_resources: resources.into_iter().collect(),
                required_clearance: cid.required_clearance(),
                priority: cid.priority(),
            })
            .collect()
    }
}

/// 审计 + repair 报告（doc16 §13.2 / §5.4）。
///
/// 汇总一次 finalize / snap 之后的 repair intents 与 degraded 状态。**不静默冒充成功**：
/// 残余 obstacle-model 违规（穿组 / 穿节点）置 `degraded`，交由上层（R10 Coordinator）再解。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RouteAuditReport {
    /// 本轮登记的 repair intents（按登记序：through → group → trunk → crossing → residual）。
    pub intents: Vec<RouteRepairIntent>,
    /// 是否存在**残余**未消解的 obstacle-model 违规（degraded：不冒充成功）。
    pub degraded: bool,
}

impl RouteAuditReport {
    /// 追加一条 intent。
    pub fn push(&mut self, intent: RouteRepairIntent) {
        self.intents.push(intent);
    }

    /// 标记 degraded（存在残余未消解的硬约束违规）。
    pub fn mark_degraded(&mut self) {
        self.degraded = true;
    }

    /// 并入另一份报告（intents 追加，degraded 取或）。
    pub fn merge(&mut self, other: RouteAuditReport) {
        self.intents.extend(other.intents);
        self.degraded |= other.degraded;
    }

    /// 无 degraded 即通过（correctness 轨对齐）。
    pub fn passed(&self) -> bool {
        !self.degraded
    }

    /// 人类可读摘要（诊断日志用）。
    pub fn describe(&self) -> String {
        format!("intents={} degraded={}", self.intents.len(), self.degraded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_edges_sorts_and_dedupes() {
        // 乱序 + 重复下标 → 升序去重（§2 确定性）。
        let intent = RouteRepairIntent::from_edges(
            [5usize, 1, 5, 3, 1],
            RouteConstraintId::EdgeThroughNode,
            RepairPriority::Hard,
        );
        assert_eq!(
            intent.affected_edges,
            vec![StableEdgeId(1), StableEdgeId(3), StableEdgeId(5)]
        );
        assert_eq!(intent.violated_constraints, vec![RouteConstraintId::EdgeThroughNode]);
        assert_eq!(intent.priority, RepairPriority::Hard);
        assert!(intent.forbidden_resources.is_empty());
        assert!(intent.required_clearance.is_none());
    }

    #[test]
    fn report_default_is_clean() {
        let report = RouteAuditReport::default();
        assert!(report.passed());
        assert_eq!(report.describe(), "intents=0 degraded=false");
    }

    #[test]
    fn push_and_merge_accumulate() {
        let mut a = RouteAuditReport::default();
        a.push(RouteRepairIntent::from_edges(
            [0usize],
            RouteConstraintId::EdgeCrossing,
            RepairPriority::Quality,
        ));
        let mut b = RouteAuditReport::default();
        b.push(RouteRepairIntent::from_edges(
            [2usize],
            RouteConstraintId::EdgeThroughNode,
            RepairPriority::Hard,
        ));
        b.mark_degraded();
        a.merge(b);
        assert_eq!(a.intents.len(), 2);
        assert!(a.degraded);
        assert!(!a.passed());
        assert_eq!(a.describe(), "intents=2 degraded=true");
    }
}
