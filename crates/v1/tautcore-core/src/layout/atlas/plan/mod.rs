//! Plan IR：整图离散决策的唯一中间表示（Stage 1 交付 → Stage 4+ 生产接线）。
//!
//! Hierarchical Ink 路径消费本 IR；可序列化（serde）、可稳定哈希（[`Plan::fingerprint`]）、
//! 可 diff（[`diff::diff`]），供对拍与增量使用。
//!
//! ## 与 `channel::Substrate` 的边界
//!
//! Plan **不嵌入** [`super::channel::Substrate`]：后者是运行态重产物，由 blueprint
//! 确定性重建。Plan 只存 [`SubstrateSketch`] + [`GroupScopeSpec`]。
//!
//! ## 相等口径
//!
//! - `==`：结构相等（含 provenance / slot_id）。
//! - 决策口径：[`Plan::fingerprint`] / [`Plan::semantic_eq`] / [`diff::diff`] 一致。

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
/// `side_order` / `along_offset` 是同 `(node, side)` 上的侧内决策（M1），参与指纹/diff。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortRef {
    pub node: NodeKey,
    pub side: PortSide,
    pub slot_index: u32,
    pub slot_id: Option<PortSlotId>,
    /// 同 `(node, side)` 上的侧内序（0..n-1）；由 [`Plan::assign_port_side_orders`] 写入。
    #[serde(default)]
    pub side_order: u32,
    /// 相对侧中点、沿侧切向的有符号偏移（像素）；由 [`Plan::assign_port_along_offsets`] 写入。
    #[serde(default)]
    pub along_offset: f64,
}

/// 一条边的两端端口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 每条边在各 track 上的 lane 下标（与 `channels[edge]` 等长；Stage 4）。
    pub lane_indices: BTreeMap<EdgeId, Vec<u32>>,
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
        self.lane_indices.remove(&edge);
        self.bundles.clear();
        Ok(())
    }

    /// 从选路结果写入 `ports`（需 Substrate 解析 PortSlot → PortRef）。
    pub fn record_ports_from_outcome(
        &mut self,
        edge: EdgeId,
        out: &RouteOutcome,
        substrate: &super::channel::Substrate,
    ) {
        let (Some(fid), Some(tid)) = (out.from_port, out.to_port) else {
            return;
        };
        let (Some(fp), Some(tp)) = (substrate.port(fid), substrate.port(tid)) else {
            return;
        };
        self.ports.insert(
            edge,
            EdgePorts {
                from: PortRef {
                    node: fp.node.clone(),
                    side: fp.side,
                    slot_index: fp.slot_index,
                    slot_id: Some(fid),
                    side_order: 0,
                    along_offset: 0.0,
                },
                to: PortRef {
                    node: tp.node.clone(),
                    side: tp.side,
                    slot_index: tp.slot_index,
                    slot_id: Some(tid),
                    side_order: 0,
                    along_offset: 0.0,
                },
            },
        );
    }

    /// M1：为每条边端点写入同 `(node, side)` 上的 `side_order`（0..n-1）。
    ///
    /// 排序键用对端 `node_slots` 沿散布轴的离散坐标（Main 侧 → order，Cross 侧 → rank），
    /// 平局用 EdgeId；不依赖像素坐标。幂等覆盖。
    pub fn assign_port_side_orders(&mut self) {
        // (node, side) → [(sort_key, eid, is_from)]
        let mut groups: BTreeMap<(NodeKey, PortSide), Vec<(usize, EdgeId, bool)>> = BTreeMap::new();
        for (&eid, ep) in &self.ports {
            if ep.from.node == ep.to.node {
                continue;
            }
            for (pr, other, is_from) in [
                (&ep.from, &ep.to.node, true),
                (&ep.to, &ep.from.node, false),
            ] {
                let peer = self.node_slots.get(other.as_str());
                let key = match pr.side {
                    PortSide::MainLow | PortSide::MainHigh => peer.map(|s| s.order).unwrap_or(0),
                    PortSide::CrossLow | PortSide::CrossHigh => peer.map(|s| s.rank).unwrap_or(0),
                };
                groups
                    .entry((pr.node.clone(), pr.side))
                    .or_default()
                    .push((key, eid, is_from));
            }
        }
        for members in groups.values_mut() {
            members.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
            for (order, &(_, eid, is_from)) in members.iter().enumerate() {
                let Some(ep) = self.ports.get_mut(&eid) else {
                    continue;
                };
                let pr = if is_from { &mut ep.from } else { &mut ep.to };
                pr.side_order = order as u32;
            }
        }
    }

    /// M1：按已定 `side_order` 与节点 bbox 写入各端 `along_offset`（相对侧中点切向像素）。
    ///
    /// 须在节点几何冻结后、Ink 落笔前调用。间距策略与历史 Ink `expand_port_points` 一致。
    pub fn assign_port_along_offsets(
        &mut self,
        node_rects: &BTreeMap<String, (f64, f64, f64, f64)>,
    ) {
        use crate::layout::demand::CORRIDOR_LANE_PITCH;
        use crate::layout::kernel::coordinate::main_axis::lane_centers;

        let mut side_counts: BTreeMap<(NodeKey, PortSide), u32> = BTreeMap::new();
        for ep in self.ports.values() {
            if ep.from.node == ep.to.node {
                continue;
            }
            *side_counts
                .entry((ep.from.node.clone(), ep.from.side))
                .or_insert(0) += 1;
            *side_counts
                .entry((ep.to.node.clone(), ep.to.side))
                .or_insert(0) += 1;
        }

        // 先收集要写的值，避免边迭代中双重可变借用
        let mut updates: Vec<(EdgeId, bool, f64)> = Vec::new();
        for (&eid, ep) in &self.ports {
            if ep.from.node == ep.to.node {
                continue;
            }
            for (pr, is_from) in [(&ep.from, true), (&ep.to, false)] {
                let Some(&(_, _, w, h)) = node_rects.get(pr.node.as_str()) else {
                    continue;
                };
                let n = side_counts
                    .get(&(pr.node.clone(), pr.side))
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                let side_len = match pr.side {
                    PortSide::MainLow | PortSide::MainHigh => w,
                    PortSide::CrossLow | PortSide::CrossHigh => h,
                };
                let pitch = if n > 1 {
                    CORRIDOR_LANE_PITCH.min(side_len * 0.6 / (n - 1) as f64)
                } else {
                    0.0
                };
                // base=0 的 lane_centers → 相对中点的偏移
                let centers = lane_centers(0.0, n, pitch);
                let along = centers
                    .get(pr.side_order as usize)
                    .copied()
                    .unwrap_or(0.0);
                updates.push((eid, is_from, along));
            }
        }
        for (eid, is_from, along) in updates {
            let Some(ep) = self.ports.get_mut(&eid) else {
                continue;
            };
            let pr = if is_from { &mut ep.from } else { &mut ep.to };
            pr.along_offset = along;
        }
    }

    /// M3：按端点槽位字典序为每条边的每个 track 分配确定性 lane 下标。
    ///
    /// - Cross track：`(from.order, to.order, eid)`
    /// - Main track：`(from.rank, to.rank, eid)`
    /// - 缺 ports/slots 时回退 EdgeId。
    pub fn assign_lane_indices(&mut self, substrate: &super::channel::Substrate) {
        let mut per_track: BTreeMap<TrackId, Vec<EdgeId>> = BTreeMap::new();
        for (&eid, tracks) in &self.channels {
            for &tid in tracks {
                per_track.entry(tid).or_default().push(eid);
            }
        }
        for (tid, edges) in &mut per_track {
            edges.sort_unstable();
            edges.dedup();
            let orient = substrate.track(*tid).map(|t| t.orient);
            edges.sort_by(|&a, &b| {
                self.lane_sort_key(a, orient)
                    .cmp(&self.lane_sort_key(b, orient))
            });
        }

        let mut lane_indices: BTreeMap<EdgeId, Vec<u32>> = BTreeMap::new();
        for (&eid, tracks) in &self.channels {
            lane_indices.insert(eid, vec![0u32; tracks.len()]);
        }
        for (tid, edges) in &per_track {
            for (lane, &eid) in edges.iter().enumerate() {
                let Some(tracks) = self.channels.get(&eid) else {
                    continue;
                };
                let Some(lanes) = lane_indices.get_mut(&eid) else {
                    continue;
                };
                for (i, &t) in tracks.iter().enumerate() {
                    if t == *tid {
                        lanes[i] = lane as u32;
                    }
                }
            }
        }
        self.lane_indices = lane_indices;
    }

    fn lane_sort_key(
        &self,
        eid: EdgeId,
        orient: Option<super::channel::TrackOrient>,
    ) -> (usize, usize, EdgeId) {
        let Some(ep) = self.ports.get(&eid) else {
            return (0, 0, eid);
        };
        let from = self.node_slots.get(&ep.from.node);
        let to = self.node_slots.get(&ep.to.node);
        match orient {
            Some(super::channel::TrackOrient::Cross) => (
                from.map(|s| s.order).unwrap_or(0),
                to.map(|s| s.order).unwrap_or(0),
                eid,
            ),
            Some(super::channel::TrackOrient::Main) => (
                from.map(|s| s.rank).unwrap_or(0),
                to.map(|s| s.rank).unwrap_or(0),
                eid,
            ),
            None => (0, 0, eid),
        }
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
            a.node == b.node
                && a.side == b.side
                && a.slot_index == b.slot_index
                && a.side_order == b.side_order
                && a.along_offset.to_bits() == b.along_offset.to_bits()
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
