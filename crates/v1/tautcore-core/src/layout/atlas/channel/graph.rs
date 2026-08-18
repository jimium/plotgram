//! 通道图：从 [`Substrate`] 导出的可搜索转移图 + 占用账本。
//!
//! 与 [`crate::layout::kernel::route::graph::ResourceGraph`]（几何可见图）的本质区别：
//! 顶点是 **轨道** 而非坐标点，转移是 **link / gate** 而非可见段。
//! 坐标在本模型中不存在，因此路径不会因坐标变动而失效。

use super::substrate::{GateCapacity, GateId, PortSlotId, Substrate, SubstrateError, TrackId};
use std::collections::{BTreeMap, BTreeSet};

/// 转移方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// 同 scope 轨道相交转移。
    Link,
    /// 跨 scope 闸口转移（容量硬约束）。
    Gate(GateId),
}

/// 有向转移（构建时双向注册）。
#[derive(Debug, Clone, Copy)]
pub struct Transition {
    pub to: TrackId,
    pub via: Via,
}

/// 通道图：确定性邻接表（BTreeMap + 排序邻居，AGENTS.md §2）。
#[derive(Debug, Clone)]
pub struct ChannelGraph<'s> {
    substrate: &'s Substrate,
    adjacency: BTreeMap<TrackId, Vec<Transition>>,
}

impl<'s> ChannelGraph<'s> {
    /// 从基底导出通道图。基底合法性已在构建期保证，此处不再校验。
    pub fn from_substrate(substrate: &'s Substrate) -> Self {
        let mut adjacency: BTreeMap<TrackId, Vec<Transition>> = BTreeMap::new();
        for t in substrate.tracks() {
            adjacency.entry(t.id).or_default();
        }
        for &(a, b) in substrate.links() {
            adjacency
                .entry(a)
                .or_default()
                .push(Transition { to: b, via: Via::Link });
            adjacency
                .entry(b)
                .or_default()
                .push(Transition { to: a, via: Via::Link });
        }
        for g in substrate.gates() {
            // gate 按 crossings 逐对展开（L2：一一配对，非笛卡尔积），共享同一容量池
            for &(inner, outer) in &g.crossings {
                adjacency.entry(inner).or_default().push(Transition {
                    to: outer,
                    via: Via::Gate(g.id),
                });
                adjacency.entry(outer).or_default().push(Transition {
                    to: inner,
                    via: Via::Gate(g.id),
                });
            }
        }
        // 邻居按 (to, via 序) 排序：BinaryHeap 平局时展开序仍确定
        for neighbors in adjacency.values_mut() {
            neighbors.sort_by_key(|t| {
                let via_key = match t.via {
                    Via::Link => (0u8, 0u32),
                    Via::Gate(g) => (1u8, g.0),
                };
                (t.to, via_key)
            });
        }
        Self {
            substrate,
            adjacency,
        }
    }

    pub fn substrate(&self) -> &Substrate {
        self.substrate
    }

    pub fn neighbors(&self, track: TrackId) -> &[Transition] {
        self.adjacency.get(&track).map_or(&[], Vec::as_slice)
    }
}

/// 占用账本：轨道 lane 占用、闸口容量池占用、端口侧占用。
///
/// **轨道占用无上限**——lane 数是输出给度量相的 Demand，不是可行性约束（B1）；
/// **闸口 / 端口占用有上限**——达到容量后对应转移 / 端点从图上消失（H2/H4 与 I.5 硬约束）。
#[derive(Debug, Clone, Default)]
pub struct Occupancy {
    track_usage: BTreeMap<TrackId, u32>,
    gate_usage: BTreeMap<GateId, u32>,
    port_usage: BTreeMap<PortSlotId, u32>,
}

impl Occupancy {
    pub fn new() -> Self {
        Self::default()
    }

    /// 该轨道当前需要的 lane 数（选路结果 → 度量相 Demand 的来源）。
    pub fn lane_demand(&self, track: TrackId) -> u32 {
        self.track_usage.get(&track).copied().unwrap_or(0)
    }

    /// 该闸口当前的穿越数——即 `crossing_demand`（27 号文 L4：相 I 的输出，
    /// 交由度量相把边界段撑开；`Unbounded` 生产路径下它不是约束而是度量）。
    pub fn gate_load(&self, gate: GateId) -> u32 {
        self.gate_usage.get(&gate).copied().unwrap_or(0)
    }

    /// 闸口是否仍有空槽（L4：`Unbounded` 恒开；`Fixed` 仅诊断扫描时收紧）。
    pub fn gate_open(&self, substrate: &Substrate, gate: GateId) -> bool {
        substrate.gate(gate).is_some_and(|g| match g.capacity {
            GateCapacity::Unbounded => true,
            GateCapacity::Fixed(c) => self.gate_load(gate) < c,
        })
    }

    pub fn port_load(&self, port: PortSlotId) -> u32 {
        self.port_usage.get(&port).copied().unwrap_or(0)
    }

    /// 端口侧是否仍有空槽（`capacity = 0` 视为不限）。
    pub fn port_open(&self, substrate: &Substrate, port: PortSlotId) -> bool {
        substrate.port(port).is_some_and(|p| {
            p.capacity == 0 || self.port_load(port) < p.capacity
        })
    }

    /// 登记端点占用（与 [`commit`](Self::commit) 配套，回溯时与 [`release`](Self::release) 配套）。
    pub fn commit_ports(&mut self, ports: &[PortSlotId]) {
        for &p in ports {
            *self.port_usage.entry(p).or_insert(0) += 1;
        }
    }

    /// 撤销端点占用。
    pub fn release_ports(&mut self, ports: &[PortSlotId]) {
        for &p in ports {
            if let Some(u) = self.port_usage.get_mut(&p) {
                *u = u.saturating_sub(1);
            }
        }
    }

    /// 登记一条已选路径的占用。
    ///
    /// **gate 按边去重**：同一条边多次穿越同一 gate（嵌套组往返）只占 1 单位容量池。
    pub fn commit(&mut self, tracks: &[TrackId], gates: &[GateId]) {
        for &t in tracks {
            *self.track_usage.entry(t).or_insert(0) += 1;
        }
        let mut seen: BTreeSet<GateId> = BTreeSet::new();
        for &g in gates {
            if seen.insert(g) {
                *self.gate_usage.entry(g).or_insert(0) += 1;
            }
        }
    }

    /// 撤销一条路径的占用（词典序有界回溯用）。与 [`commit`](Self::commit) 严格互逆。
    pub fn release(&mut self, tracks: &[TrackId], gates: &[GateId]) {
        for &t in tracks {
            if let Some(u) = self.track_usage.get_mut(&t) {
                *u = u.saturating_sub(1);
            }
        }
        let mut seen: BTreeSet<GateId> = BTreeSet::new();
        for &g in gates {
            if seen.insert(g) {
                if let Some(u) = self.gate_usage.get_mut(&g) {
                    *u = u.saturating_sub(1);
                }
            }
        }
    }
}

/// 选路端点解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointError {
    UnknownPort(PortSlotId),
    /// 候选端点选路时某一端候选集为空（24 号文 R3，不得 panic）。
    EmptyCandidates,
}

impl From<EndpointError> for SubstrateError {
    fn from(e: EndpointError) -> Self {
        match e {
            EndpointError::UnknownPort(p) => SubstrateError::UnknownPort(p),
            EndpointError::EmptyCandidates => SubstrateError::EmptyCandidates,
        }
    }
}
