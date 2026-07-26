//! Plan IR（23 号文 Stage 1 交付 1.1/1.2）：整图离散决策的唯一中间表示。
//!
//! **本阶段不接线生产**（同 [`super::channel`]）：生产路径仍走 `layout::routing`，
//! 本模块只提供旁路可测的 IR——可序列化（serde）、可稳定哈希（[`Plan::fingerprint`]）、
//! 可 diff（[`diff::diff`]），供 Stage 1 门 B「表达力验证」与后续对拍/增量使用。
//!
//! ## 与 `channel::Substrate` 的边界
//!
//! Plan **不嵌入** [`super::channel::Substrate`]：后者是 `derive_substrate` 的运行态
//! 重产物（段 links、端口容量、gate crossings），由 blueprint 确定性重建即可，
//! 序列化它只会引入冗余与漂移面。Plan 只存 rank×order 网格摘要
//! [`SubstrateSketch`] + 组覆盖区间 [`GroupScopeSpec`]（Adapter 分段所需），
//! 保证 IR 轻量、可序列化、可逐字段 diff。
//!
//! ## 相等口径
//!
//! - `==`（derive）：逐字段结构相等（含 `slot_id`、`provenance`、bundles 顺序），
//!   供 serde 往返、「拒收后不变」类断言使用。
//! - **决策口径**（[`Plan::fingerprint`] / [`Plan::semantic_eq`] / [`diff::diff`] 一致）：
//!   `provenance`、`PortRef::slot_id`、bundles 的 Vec 顺序均不参与。
//!   增量缓存用指纹判「决策是否变」与 diff 判空不会互相误判。
//!
//! ## 留债（本阶段显式不做）
//!
//! - TODO(Stage1-1.3)：Legacy Adapter——从旧管线中间产物（`LayeredDraft`、
//!   `endpoint_map`、`LaneAssignment`、`MergeInterval`）反向构造 Plan。
//! - TODO(Stage1-1.4)：Ink 原型——`(Plan, 旧坐标) → 边几何` 与旧几何对拍。
//! - TODO(Stage1-I.5)：端口策略——`ports` 字段本阶段由调用方填入。

pub mod diff;
pub mod fingerprint;

pub use diff::{Change, EdgeChange, PlanDiff, diff};

use super::channel::{Bundle, EdgeId, GateId, NodeKey, PortSide, PortSlotId, RouteOutcome,
    TrackId, detect_bundles};
use crate::layout::kernel::cost::SolverStatus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// 组键：与 [`super::channel::ChannelBlueprint`] 的组名键一致（稳定字符串标识）。
pub type GroupKey = String;

/// 边溯源映射：每条边的决策来源。
pub type ProvenanceMap = BTreeMap<EdgeId, Provenance>;

/// 决策来源（最小 stub；Stage 1.3 接 Legacy Adapter 时扩展携带信息）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    /// 通道图选路（`channel::route` 系产出）。
    ChannelRoute,
    /// 旧管线反向构造（TODO(Stage1-1.3)）。
    LegacyAdapter,
    /// 手工/测试构造。
    Manual,
}

/// rank×order 网格骨架摘要（非 `channel::Substrate` 本体，见模块级边界说明）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubstrateSketch {
    /// rank 层数。
    pub rank_count: usize,
    /// 单层最大 order 槽位数。
    pub order_count: usize,
}

/// 节点槽位：`(rank, order)` 网格坐标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub rank: usize,
    pub order: usize,
}

/// 组作用域：层次 + rank/order 覆盖区间（闭区间，对齐 `channel::GroupScope`；
/// 覆盖区间是 Adapter 分段的输入，23 号文 1.3 注意事项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupScopeSpec {
    /// 父组（None = 顶层）。
    pub parent: Option<GroupKey>,
    /// rank 跨度 `[r0, r1]`（闭区间）。
    pub ranks: (usize, usize),
    /// order 跨度 `[o0, o1]`（闭区间）。
    pub orders: (usize, usize),
}

/// 端口引用：`(node, side, slot_index)` 三元组是语义身份（24 号文 R1）；
/// `slot_id` 仅当来自已 derive 的基底时携带（重建后可能重编号，不参与指纹）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRef {
    pub node: NodeKey,
    pub side: PortSide,
    pub slot_index: u32,
    pub slot_id: Option<PortSlotId>,
}

/// 一条边的两端端口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgePorts {
    pub from: PortRef,
    pub to: PortRef,
}

/// Plan 操作错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlanError {
    /// 选路结果非 Converged：Plan 只收录成功决策，不伪造记录。
    #[error("edge {0} 的选路结果非 Converged（{1:?}），不予收录")]
    RouteNotConverged(EdgeId, SolverStatus),
    /// `channels` 已有该边但缺配套记录（gates / provenance）。
    #[error("edge {0} 已有 channels 但缺 {1} 记录")]
    MissingEdgeRecord(EdgeId, &'static str),
    /// 组 parent 指向不存在的组。
    #[error("组 {0} 的 parent {1} 不存在")]
    DanglingGroupParent(GroupKey, GroupKey),
    /// 组覆盖区间倒置（ranks / orders 起点大于终点）。
    #[error("组 {0} 的覆盖区间倒置")]
    InvertedGroupSpan(GroupKey),
    /// 端口引用了 `node_slots` 之外的节点。
    #[error("edge {0} 的端口引用未知节点 {1}")]
    UnknownPortNode(EdgeId, NodeKey),
}

/// 整图离散决策 IR（23 号文 §3「Plan 建议骨架」）。
///
/// 全字段确定性容器（`BTreeMap` + 有序键），序列化 / 指纹 / diff 均与
/// 插入顺序无关。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// rank×order 骨架摘要。
    pub substrate: SubstrateSketch,
    /// 节点槽位 `(rank, order)`。
    pub node_slots: BTreeMap<NodeKey, Slot>,
    /// 组层次 + 网格覆盖区间。
    pub group_scopes: BTreeMap<GroupKey, GroupScopeSpec>,
    /// 边端口（本阶段由调用方填入，TODO(Stage1-I.5)）。
    pub ports: BTreeMap<EdgeId, EdgePorts>,
    /// 跨组闸口序列（与 [`RouteOutcome::gates`] 对齐）。
    pub gates: BTreeMap<EdgeId, Vec<GateId>>,
    /// track/段序列 = 路径拓扑（与 [`RouteOutcome::tracks`] 对齐）。
    pub channels: BTreeMap<EdgeId, Vec<TrackId>>,
    /// 共享 track 后缀 = 合流（[`detect_bundles`] 产物）。
    pub bundles: Vec<Bundle>,
    /// 每边决策来源。
    pub provenance: ProvenanceMap,
}

impl Plan {
    /// 收录一条边的通道选路结果。
    ///
    /// `status != Converged` 时返回 [`PlanError::RouteNotConverged`] 且 Plan
    /// 逐字段不变（不伪造记录——与 channel「显式 Infeasible」同一原则，L7-T5）。
    /// 成功则写入 `channels` / `gates`，溯源标记 [`Provenance::ChannelRoute`]，
    /// 并清空 `bundles`（channels 变更后旧合流失效，须重跑
    /// [`Plan::detect_and_set_bundles`]）。
    pub fn record_route(&mut self, edge: EdgeId, out: &RouteOutcome) -> Result<(), PlanError> {
        if out.status != SolverStatus::Converged {
            return Err(PlanError::RouteNotConverged(edge, out.status));
        }
        self.channels.insert(edge, out.tracks.clone());
        self.gates.insert(edge, out.gates.clone());
        self.provenance.insert(edge, Provenance::ChannelRoute);
        self.bundles.clear();
        Ok(())
    }

    /// 从已收录的 `channels` 检测合流并写入 `bundles`。
    ///
    /// `BTreeMap` 迭代序即 `EdgeId` 升序，与 [`detect_bundles`] 的确定性
    /// 前提一致；重复调用幂等（整体重算覆盖）。
    pub fn detect_and_set_bundles(&mut self, min_suffix: usize) {
        let paths: Vec<(EdgeId, &[TrackId])> = self
            .channels
            .iter()
            .map(|(&e, tracks)| (e, tracks.as_slice()))
            .collect();
        self.bundles = detect_bundles(&paths, min_suffix);
    }

    /// 决策语义相等：与 [`Plan::fingerprint`] 同口径——忽略 `PortRef::slot_id`、
    /// `provenance` 与 bundles 顺序（见模块头「相等口径」）。
    /// `a.semantic_eq(&b)` → 指纹必相等；`==` 则是结构相等。
    pub fn semantic_eq(&self, other: &Plan) -> bool {
        fn port_eq(a: &PortRef, b: &PortRef) -> bool {
            a.node == b.node && a.side == b.side && a.slot_index == b.slot_index
        }
        self.substrate == other.substrate
            && self.node_slots == other.node_slots
            && self.group_scopes == other.group_scopes
            && self.gates == other.gates
            && self.channels == other.channels
            && self.ports.len() == other.ports.len()
            && self.ports.iter().zip(&other.ports).all(|((ea, pa), (eb, pb))| {
                ea == eb && port_eq(&pa.from, &pb.from) && port_eq(&pa.to, &pb.to)
            })
            && canonical_bundles(&self.bundles) == canonical_bundles(&other.bundles)
    }

    /// 轻量不变量检查（Stage 1.3 Adapter 的入口门槛）：
    /// channels 有边必有 gates/provenance、组 parent 不悬空且区间不倒置、
    /// 端口只引用 `node_slots` 内节点。返回首个违例（键升序，确定性）。
    pub fn validate(&self) -> Result<(), PlanError> {
        for &edge in self.channels.keys() {
            if !self.gates.contains_key(&edge) {
                return Err(PlanError::MissingEdgeRecord(edge, "gates"));
            }
            if !self.provenance.contains_key(&edge) {
                return Err(PlanError::MissingEdgeRecord(edge, "provenance"));
            }
        }
        for (group, scope) in &self.group_scopes {
            if let Some(parent) = &scope.parent {
                if !self.group_scopes.contains_key(parent) {
                    return Err(PlanError::DanglingGroupParent(group.clone(), parent.clone()));
                }
            }
            if scope.ranks.0 > scope.ranks.1 || scope.orders.0 > scope.orders.1 {
                return Err(PlanError::InvertedGroupSpan(group.clone()));
            }
        }
        for (&edge, ep) in &self.ports {
            for p in [&ep.from, &ep.to] {
                if !self.node_slots.contains_key(&p.node) {
                    return Err(PlanError::UnknownPortNode(edge, p.node.clone()));
                }
            }
        }
        Ok(())
    }
}

/// bundles 的规范序视图（按 `(suffix, edges)` 升序）：指纹与 [`Plan::semantic_eq`]
/// 共用，使 Vec 顺序不携带语义（与 diff 的集合差口径一致）。
pub(crate) fn canonical_bundles(bundles: &[Bundle]) -> Vec<&Bundle> {
    let mut v: Vec<&Bundle> = bundles.iter().collect();
    v.sort_by(|a, b| (&a.suffix, &a.edges).cmp(&(&b.suffix, &b.edges)));
    v
}

#[cfg(test)]
mod tests;
