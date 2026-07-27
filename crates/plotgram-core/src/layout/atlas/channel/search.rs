//! 词典序 Dijkstra：在 [`ChannelGraph`] 上为单条边选 track 序列。
//!
//! 复用 [`crate::layout::kernel::cost::LexCost`]（中立共享词汇，不依赖待删的旧路由内核），本层单边搜索只计：
//! - **Q3 折点**：link 转移必为折点（Main × Cross，构造保证）；gate 转移看两侧轨道是否异向。
//! - **Q4 长度**：路径上全部轨道的 `span_weight` 之和（含起点轨道）。
//!
//! Q2 交叉 / Q5 对齐 / Q6 对称是**跨边**量，属于上层全局协调（有界块坐标下降）的
//! 职责，单边搜索不近似——避免像素级 OVG 那种「用重叠长度冒充交叉」的失真。
//!
//! 硬约束语义：闸口满容 → 转移从图上消失；作用域越界 → 转移被 [`ScopeMask`]
//! 硬过滤（L8，无代价旁路）；无路可走 → 显式 `Infeasible`，
//! **没有几何兜底**（本模型无坐标可兜）。这正是相 I 可行率探针需要的诚实信号。

use super::graph::{ChannelGraph, EndpointError, Occupancy, Via};
use super::substrate::{GateId, GroupId, PortSide, PortSlotId, Substrate, TrackId};
use crate::layout::kernel::cost::{LexCost, OrderedF64, SolverStatus};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// L8 作用域掩码：单边搜索只允许进入 `{None} ∪ chain(scope(u)) ∪ chain(scope(v))`
/// 内的段——路径不得借道与两端无关的组（A2 归零的唯一机制，硬过滤非计价）。
///
/// 静态面（构建期拒绝跨 scope link）保证进组必经 gate；本掩码是动态面：
/// 即使经 gate 合法进出，无关组也不得作为过道。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeMask {
    allowed: BTreeSet<Option<GroupId>>,
}

impl ScopeMask {
    /// 由两端作用域构建：`{None} ∪ chain(u_scope) ∪ chain(v_scope)`
    /// （chain = 自身及全部祖先组，[`Substrate::scope_chain`]）。
    pub fn for_scopes(
        substrate: &Substrate,
        u_scope: Option<GroupId>,
        v_scope: Option<GroupId>,
    ) -> Self {
        let mut allowed: BTreeSet<Option<GroupId>> = BTreeSet::new();
        allowed.insert(None);
        for g in substrate.scope_chain(u_scope) {
            allowed.insert(Some(g));
        }
        for g in substrate.scope_chain(v_scope) {
            allowed.insert(Some(g));
        }
        Self { allowed }
    }

    /// 便利构建：从两端口的宿主轨道 scope 推导（探针 / 手工基底测试用）。
    pub fn for_ports(
        substrate: &Substrate,
        from: PortSlotId,
        to: PortSlotId,
    ) -> Result<Self, EndpointError> {
        let scope_of = |p: PortSlotId| -> Result<Option<GroupId>, EndpointError> {
            let track = substrate.port(p).ok_or(EndpointError::UnknownPort(p))?.track;
            Ok(substrate.track(track).and_then(|t| t.scope))
        };
        Ok(Self::for_scopes(substrate, scope_of(from)?, scope_of(to)?))
    }

    /// 该作用域的段是否允许进入。
    pub fn allows(&self, scope: Option<GroupId>) -> bool {
        self.allowed.contains(&scope)
    }
}

/// 单边选路结果。
#[derive(Debug, Clone, PartialEq)]
pub struct RouteOutcome {
    /// track 序列（含起止宿主轨道）——这就是 Plan.channels 的一项。
    pub tracks: Vec<TrackId>,
    /// 途经闸口序列（按穿越顺序）。
    pub gates: Vec<GateId>,
    /// 胜出起点端口（`Infeasible` 时为 None）。
    pub from_port: Option<PortSlotId>,
    /// 胜出终点端口。
    pub to_port: Option<PortSlotId>,
    pub cost: LexCost,
    pub status: SolverStatus,
}

impl RouteOutcome {
    /// 不可行结果：空路径 + `Infeasible`（无几何兜底，不伪造路径）。
    pub fn infeasible() -> Self {
        Self {
            tracks: Vec::new(),
            gates: Vec::new(),
            from_port: None,
            to_port: None,
            cost: LexCost {
                q1_hard_residual: OrderedF64(1.0),
                ..LexCost::default()
            },
            status: SolverStatus::Infeasible,
        }
    }
}

#[derive(Clone)]
struct State {
    cost: LexCost,
    track: TrackId,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.track == other.track
    }
}
impl Eq for State {}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap 是最大堆；反转使最小 LexCost 优先，平局按 TrackId（确定性）
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| other.track.cmp(&self.track))
    }
}

/// 在通道图上为一条边选路（词典序 Dijkstra，确定性全展开）。
///
/// `occupancy` 只影响硬约束（闸口容量）；轨道占用不设上限——lane 数是
/// 输出给度量相的 Demand，不是可行性约束（B1）。
/// `allowed` 为 L8 作用域掩码：目标段 scope 不在掩码内的转移直接跳过。
///
/// **自环不建模（L7-T5 定案）**：两端口属同一节点 → 显式 `Infeasible`，
/// 不返回单轨道平凡解伪装成功（现行管线自环走节点旁小环，不经通道；
/// 未来若建模走专门绕行构造）。不同节点同宿主轨道的边（如 B7 退化组
/// 的组内边）仍合法返回单轨道解。
pub fn route(
    graph: &ChannelGraph<'_>,
    from: PortSlotId,
    to: PortSlotId,
    occupancy: &Occupancy,
    allowed: &ScopeMask,
) -> Result<RouteOutcome, EndpointError> {
    route_inner(graph, from, to, occupancy, allowed, false)
}

/// M2：在 bends/length 之后用 `lane_demand` 作软偏好（写入 LexCost.q5）。
pub fn route_congested(
    graph: &ChannelGraph<'_>,
    from: PortSlotId,
    to: PortSlotId,
    occupancy: &Occupancy,
    allowed: &ScopeMask,
) -> Result<RouteOutcome, EndpointError> {
    route_inner(graph, from, to, occupancy, allowed, true)
}

fn route_inner(
    graph: &ChannelGraph<'_>,
    from: PortSlotId,
    to: PortSlotId,
    occupancy: &Occupancy,
    allowed: &ScopeMask,
    congestion_bias: bool,
) -> Result<RouteOutcome, EndpointError> {
    let substrate = graph.substrate();
    let from_port = substrate.port(from).ok_or(EndpointError::UnknownPort(from))?;
    let to_port = substrate.port(to).ok_or(EndpointError::UnknownPort(to))?;

    // 自环（同节点）未建模：显式不可行，不伪造平凡解（L7-T5）。
    if from_port.node == to_port.node {
        return Ok(RouteOutcome::infeasible());
    }
    let start = from_port.track;
    let goal = to_port.track;

    // 端口侧容量硬约束（I.5 PORT_SIDE_CAPACITY）：任一端点满容 → 该边此刻不可布
    if !occupancy.port_open(substrate, from) || !occupancy.port_open(substrate, to) {
        return Ok(RouteOutcome::infeasible());
    }

    let mut start_cost = LexCost {
        q4_length: OrderedF64(substrate.track(start).map_or(0.0, |t| t.span_weight)),
        ..LexCost::default()
    };
    if congestion_bias {
        start_cost.q5_alignment =
            OrderedF64(occupancy.lane_demand(start) as f64);
    }
    if start == goal {
        // 不同节点、同宿主轨道（如 B7 退化组的边界缝）：单轨道解合法。
        return Ok(RouteOutcome {
            tracks: vec![start],
            gates: Vec::new(),
            from_port: Some(from),
            to_port: Some(to),
            cost: start_cost,
            status: SolverStatus::Converged,
        });
    }

    let mut open = BinaryHeap::new();
    open.push(State {
        cost: start_cost,
        track: start,
    });
    // best[track] = (cost, parent, via)
    let mut best: BTreeMap<TrackId, (LexCost, Option<TrackId>, Option<Via>)> = BTreeMap::new();
    best.insert(start, (start_cost, None, None));

    while let Some(State { cost, track }) = open.pop() {
        if let Some((bc, _, _)) = best.get(&track) {
            if &cost > bc {
                continue;
            }
        }
        if track == goal {
            let (tracks, gates) = reconstruct(&best, goal);
            return Ok(RouteOutcome {
                tracks,
                gates,
                from_port: Some(from),
                to_port: Some(to),
                cost,
                status: SolverStatus::Converged,
            });
        }
        for tr in graph.neighbors(track) {
            // 硬约束：满容闸口的转移不存在
            if let Via::Gate(g) = tr.via {
                if !occupancy.gate_open(substrate, g) {
                    continue;
                }
            }
            // L8 作用域掩码：无关组的段不得进入（硬过滤，非计价）
            let target_scope = match substrate.track(tr.to) {
                Some(t) => t.scope,
                None => continue,
            };
            if !allowed.allows(target_scope) {
                continue;
            }
            let next_cost = step_cost(
                graph,
                &cost,
                track,
                tr.to,
                tr.via,
                congestion_bias.then_some(occupancy),
            );
            let better = match best.get(&tr.to) {
                None => true,
                Some((bc, _, _)) => next_cost < *bc,
            };
            if better {
                best.insert(tr.to, (next_cost, Some(track), Some(tr.via)));
                open.push(State {
                    cost: next_cost,
                    track: tr.to,
                });
            }
        }
    }

    Ok(RouteOutcome::infeasible())
}

fn step_cost(
    graph: &ChannelGraph<'_>,
    prev: &LexCost,
    from: TrackId,
    to: TrackId,
    via: Via,
    occupancy: Option<&Occupancy>,
) -> LexCost {
    let substrate = graph.substrate();
    let mut c = *prev;
    let bend = match via {
        // link 只连异向轨道（构建期保证）→ 必为折点
        Via::Link => 1,
        // gate 两侧同向 = 直穿组边界，无折点。
        // L2 后 crossings 配对恒同向（G-inv-1/2 构建期保证），异向分支
        // 实际不会命中；保留作防御性兜底（L7-T6）。
        Via::Gate(_) => {
            let differs = match (substrate.track(from), substrate.track(to)) {
                (Some(a), Some(b)) => a.orient != b.orient,
                _ => false,
            };
            u32::from(differs)
        }
    };
    c.q3_bends = c.q3_bends.saturating_add(bend);
    c.q4_length = OrderedF64(c.q4_length.0 + substrate.track(to).map_or(0.0, |t| t.span_weight));
    if let Some(occ) = occupancy {
        // q5：低于 bends/length 的拥塞软偏好（M2 rip-up）
        c.q5_alignment = OrderedF64(c.q5_alignment.0 + occ.lane_demand(to) as f64);
    }
    c
}

fn reconstruct(
    best: &BTreeMap<TrackId, (LexCost, Option<TrackId>, Option<Via>)>,
    goal: TrackId,
) -> (Vec<TrackId>, Vec<GateId>) {
    let mut tracks = Vec::new();
    let mut gates = Vec::new();
    let mut cur = goal;
    loop {
        tracks.push(cur);
        match best.get(&cur) {
            // via 记录的是「进入 cur 时走的转移」
            Some((_, Some(parent), via)) => {
                if let Some(Via::Gate(g)) = via {
                    gates.push(*g);
                }
                cur = *parent;
            }
            // 起点（无 parent）
            _ => break,
        }
    }
    tracks.reverse();
    gates.reverse();
    (tracks, gates)
}

/// 候选端点选路（24 号文 R3）：在 `from × to` 笛卡尔积上取 LexCost 最优可行对。
///
/// 行为规格：
/// 1. 任一端候选集为空 → `Err(EmptyCandidates)`，不 panic；
/// 2. 笛卡尔积含同 track 候选对的单轨道平凡解（仅限两端不同节点；同节点
///    自环由 [`route`] 判 `Infeasible`，L7-T5）；
/// 3. 跳过：未知 id、任一端 `!port_open`、`route` 返回 `Infeasible`（由 [`route`] 内部处理）；
/// 4. 在所有 `Converged` 结果中取 **LexCost 最小**；平局按 `(from_id, to_id)` 升序
///    （候选按 id 升序遍历 + 严格 `<` 替换 → 首个最小代价对获胜，确定性）；
/// 5. **不修改** `occupancy`（与 [`route`] 一致，调用方决定是否 `commit`）；
/// 6. 全部不可行 → `Ok(RouteOutcome::infeasible())`，不伪造路径；
/// 7. 候选规模预期很小（每端 ≤4），朴素双重循环，不上启发式剪枝。
pub fn route_candidates(
    graph: &ChannelGraph<'_>,
    from_candidates: &[PortSlotId],
    to_candidates: &[PortSlotId],
    occupancy: &Occupancy,
    allowed: &ScopeMask,
) -> Result<RouteOutcome, EndpointError> {
    if from_candidates.is_empty() || to_candidates.is_empty() {
        return Err(EndpointError::EmptyCandidates);
    }
    // 升序去重遍历：保证平局时 (from_id, to_id) 最小者获胜（确定性）。
    let mut froms: Vec<PortSlotId> = from_candidates.to_vec();
    froms.sort();
    froms.dedup();
    let mut tos: Vec<PortSlotId> = to_candidates.to_vec();
    tos.sort();
    tos.dedup();

    let mut best: Option<(RouteOutcome, PortSlotId, PortSlotId)> = None;
    for &f in &froms {
        for &t in &tos {
            if let Ok(mut out) = route(graph, f, t, occupancy, allowed) {
                if out.status == SolverStatus::Converged
                    && best
                        .as_ref()
                        .is_none_or(|(b, _, _)| out.cost < b.cost)
                {
                    out.from_port = Some(f);
                    out.to_port = Some(t);
                    best = Some((out, f, t));
                }
            }
        }
    }
    Ok(best.map(|(out, _, _)| out).unwrap_or_else(RouteOutcome::infeasible))
}

/// 路径上各 track 的 `lane_demand` 之和（rip-up 次级键；不修改 occupancy）。
pub fn path_lane_load(occupancy: &Occupancy, tracks: &[TrackId]) -> u32 {
    tracks.iter().map(|&t| occupancy.lane_demand(t)).sum()
}

/// M2 rip-up 用：Dijkstra 内以 `lane_demand` 写入 LexCost.q5（低于 bends/length）。
///
/// 端点笛卡尔积上取完整 LexCost 最小；平局 `(from_id, to_id)`。
pub fn route_candidates_congested(
    graph: &ChannelGraph<'_>,
    from_candidates: &[PortSlotId],
    to_candidates: &[PortSlotId],
    occupancy: &Occupancy,
    allowed: &ScopeMask,
) -> Result<RouteOutcome, EndpointError> {
    if from_candidates.is_empty() || to_candidates.is_empty() {
        return Err(EndpointError::EmptyCandidates);
    }
    let mut froms: Vec<PortSlotId> = from_candidates.to_vec();
    froms.sort();
    froms.dedup();
    let mut tos: Vec<PortSlotId> = to_candidates.to_vec();
    tos.sort();
    tos.dedup();

    let mut best: Option<(RouteOutcome, PortSlotId, PortSlotId)> = None;
    for &f in &froms {
        for &t in &tos {
            if let Ok(mut out) = route_congested(graph, f, t, occupancy, allowed) {
                if out.status == SolverStatus::Converged
                    && best
                        .as_ref()
                        .is_none_or(|(b, _, _)| out.cost < b.cost)
                {
                    out.from_port = Some(f);
                    out.to_port = Some(t);
                    best = Some((out, f, t));
                }
            }
        }
    }
    Ok(best.map(|(out, _, _)| out).unwrap_or_else(RouteOutcome::infeasible))
}

/// 便利 API：按节点×侧候选集选路（24 号文 R3 可选）。
///
/// 内部：查 [`Substrate::ports_of_node_side`](super::substrate::Substrate::ports_of_node_side) 得候选 id → [`route_candidates`]。
/// 任一端无匹配端口（空候选）→ `Err(EmptyCandidates)`。
pub fn route_node_sides(
    graph: &ChannelGraph<'_>,
    from_node: &str,
    from_sides: &[PortSide],
    to_node: &str,
    to_sides: &[PortSide],
    occupancy: &Occupancy,
    allowed: &ScopeMask,
) -> Result<RouteOutcome, EndpointError> {
    let substrate = graph.substrate();
    let froms: Vec<PortSlotId> = from_sides
        .iter()
        .flat_map(|&side| substrate.ports_of_node_side(from_node, side))
        .map(|p| p.id)
        .collect();
    let tos: Vec<PortSlotId> = to_sides
        .iter()
        .flat_map(|&side| substrate.ports_of_node_side(to_node, side))
        .map(|p| p.id)
        .collect();
    route_candidates(graph, &froms, &tos, occupancy, allowed)
}
