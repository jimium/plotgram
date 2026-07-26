//! Stage 3：Channel 进度量相——Plan.channels → lane Demand → 层缝净空。
//!
//! 把相 I 的通道决策编译成度量相间距需求：
//! - Cross 走向 track（层间缝）→ 抬高 `per_layer_gaps`
//! - lane 带宽 = `2 * CLEARANCE + lanes * CORRIDOR_LANE_PITCH`（23 号文 3.4）
//!
//! **刻意**：不换选路；Plan 由通道图选路产出。

use std::collections::BTreeMap;

use super::channel::{
    derive_node_ports, derive_substrate, route_candidates, ChannelBlueprint, ChannelGraph,
    DerivePortsOptions, GroupSpec, NodeSpec, Occupancy, RouteOutcome, Substrate, TrackId,
    TrackOrient,
};
use super::plan::{Plan, Slot, SubstrateSketch};
use super::space::{Axis, ChannelOccupant, Demand, Occupant, OccupantId, OccupantKind};
use crate::ast::Diagram;
use crate::layout::demand::CORRIDOR_LANE_PITCH;
use crate::layout::kernel::cost::SolverStatus;
use crate::layout::kernel::layered::graph::LayerNodeKind;
use crate::layout::kernel::layered::layered_kernel::LayeredDraft;

/// 通道净空额外边距（lane 带两侧的硬间隙）。
pub const CHANNEL_CLEARANCE: f64 = 8.0;

/// 度量相通道输入：基底 + 占用账 + Plan。
pub struct ChannelMetric {
    pub plan: Plan,
    pub substrate: Substrate,
    pub occupancy: Occupancy,
}

impl ChannelMetric {
    /// 按 track 汇总的 lane Demand（刚性）。
    pub fn track_demands(&self) -> BTreeMap<TrackId, Demand> {
        let mut out = BTreeMap::new();
        for t in self.substrate.tracks() {
            let lanes = self.occupancy.lane_demand(t.id);
            if lanes == 0 {
                continue;
            }
            out.insert(t.id, Demand::rigid(channel_band_width(lanes)));
        }
        out
    }

    /// 构造 Channel Occupant 列表（供诊断 / 后续 attach_channel_ir）。
    pub fn occupants(&self) -> Vec<ChannelOccupant> {
        let mut out = Vec::new();
        for t in self.substrate.tracks() {
            let lanes = self.occupancy.lane_demand(t.id);
            if lanes == 0 {
                continue;
            }
            out.push(ChannelOccupant::from_lanes(t.id, t.orient, lanes));
        }
        out.sort_by_key(|o| o.track_id);
        out
    }

    /// Cross 走向 track → 层间缝需求。
    ///
    /// Cross `line = k`（`1 <= k < rank_count`）对应 `per_layer_gaps[k - 1]`
    /// （rank `k-1` 与 `k` 之间）。外框线 `0` / `rank_count` 不进层缝表。
    pub fn cross_gap_demands(&self) -> BTreeMap<usize, f64> {
        let rank_count = self.plan.substrate.rank_count;
        let mut out: BTreeMap<usize, f64> = BTreeMap::new();
        for t in self.substrate.tracks() {
            if t.orient != TrackOrient::Cross {
                continue;
            }
            let lanes = self.occupancy.lane_demand(t.id);
            if lanes == 0 {
                continue;
            }
            let line = t.line;
            if line == 0 || line >= rank_count {
                continue;
            }
            let gap_idx = line - 1;
            let need = channel_band_width(lanes);
            let e = out.entry(gap_idx).or_insert(0.0);
            *e = e.max(need);
        }
        out
    }

    /// Main 走向 track → 层内 order 缝需求（Cross 轴分离）。
    ///
    /// Main `line = k`（`1 <= k < order_count`）对应 order `k-1` 与 `k` 之间。
    pub fn main_gap_demands(&self) -> BTreeMap<usize, f64> {
        let order_count = self.plan.substrate.order_count;
        let mut out: BTreeMap<usize, f64> = BTreeMap::new();
        for t in self.substrate.tracks() {
            if t.orient != TrackOrient::Main {
                continue;
            }
            let lanes = self.occupancy.lane_demand(t.id);
            if lanes == 0 {
                continue;
            }
            let line = t.line;
            if line == 0 || line >= order_count {
                continue;
            }
            let gap_idx = line - 1;
            let need = channel_band_width(lanes);
            let e = out.entry(gap_idx).or_insert(0.0);
            *e = e.max(need);
        }
        out
    }
}

/// `lanes` 条车道需要的法向带宽（含两侧 clearance）。
pub fn channel_band_width(lanes: u32) -> f64 {
    if lanes == 0 {
        return 0.0;
    }
    CHANNEL_CLEARANCE * 2.0 + (lanes as f64) * CORRIDOR_LANE_PITCH
}

/// 用通道 Demand 抬高层间缝（取 max，不缩小启发式基线）。
pub fn inflate_layer_gaps(base_gaps: &[f64], gap_demands: &BTreeMap<usize, f64>) -> Vec<f64> {
    base_gaps
        .iter()
        .enumerate()
        .map(|(i, &base)| base.max(gap_demands.get(&i).copied().unwrap_or(0.0)))
        .collect()
}

/// 从 Plan.channels / gates 回放占用。
pub fn lane_occupancy_from_plan(plan: &Plan) -> Occupancy {
    let mut occ = Occupancy::new();
    for (edge, tracks) in &plan.channels {
        let gates = plan.gates.get(edge).map(|g| g.as_slice()).unwrap_or(&[]);
        occ.commit(tracks, gates);
    }
    occ
}

/// 相 I 边选路顺序（L2 放松：反向重试争用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhaseIEdgeOrder {
    #[default]
    Forward,
    Reverse,
}

/// 从 Diagram + LayeredDraft 构建度量相通道输入（不依赖像素边几何）。
pub fn build_channel_metric(
    diagram: &Diagram,
    draft: &LayeredDraft,
) -> Result<ChannelMetric, String> {
    build_channel_metric_with_opts(diagram, draft, None, PhaseIEdgeOrder::Forward)
}

/// 带增量 / 边序选项的 draft 入口。
pub fn build_channel_metric_with_opts(
    diagram: &Diagram,
    draft: &LayeredDraft,
    prev_plan: Option<&Plan>,
    edge_order: PhaseIEdgeOrder,
) -> Result<ChannelMetric, String> {
    let mut node_slots: BTreeMap<String, Slot> = BTreeMap::new();
    for (rank, layer) in draft.layers.iter().enumerate() {
        let mut order = 0usize;
        for &node in layer {
            if let LayerNodeKind::Real(dag_node) = draft.proper_graph[node].kind {
                let entity_id = draft.dag[dag_node].clone();
                let r = draft.sugiyama_ranks.get(&entity_id).copied().unwrap_or(rank);
                node_slots.insert(entity_id, Slot { rank: r, order });
                order += 1;
            }
        }
    }
    build_channel_metric_from_slots_with_opts(diagram, node_slots, prev_plan, edge_order)
}

/// 从全局 node → Slot 构建（分治路径用 `sugiyama_ranks` + 同 rank 确定性 order）。
pub fn build_channel_metric_from_slots(
    diagram: &Diagram,
    node_slots: BTreeMap<String, Slot>,
) -> Result<ChannelMetric, String> {
    build_channel_metric_from_slots_with_opts(
        diagram,
        node_slots,
        None,
        PhaseIEdgeOrder::Forward,
    )
}

/// 判断 prev Plan 是否可跳过相 I 选路（槽位一致且每条非自环边已有 channel）。
pub fn can_skip_phase_i_search(
    prev: &Plan,
    node_slots: &BTreeMap<String, Slot>,
    diagram: &Diagram,
) -> bool {
    if prev.node_slots != *node_slots || prev.channels.is_empty() {
        return false;
    }
    for (edge_idx, rel) in diagram.relations.iter().enumerate() {
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        if !node_slots.contains_key(rel.from.as_str()) || !node_slots.contains_key(rel.to.as_str())
        {
            continue;
        }
        if !prev.channels.contains_key(&edge_idx) {
            return false;
        }
    }
    true
}

/// 从 slots 构建；可选复用 prev Plan 的相 I 决策；可选反向边序选路。
pub fn build_channel_metric_from_slots_with_opts(
    diagram: &Diagram,
    node_slots: BTreeMap<String, Slot>,
    prev_plan: Option<&Plan>,
    edge_order: PhaseIEdgeOrder,
) -> Result<ChannelMetric, String> {
    let mut bp = ChannelBlueprint::default();
    for (node, slot) in &node_slots {
        bp.nodes.insert(
            node.clone(),
            NodeSpec {
                rank: slot.rank,
                order: slot.order,
            },
        );
    }
    for group in &diagram.groups {
        bp.groups.insert(
            group.id.as_str().to_string(),
            GroupSpec {
                members: group
                    .entity_ids
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                parent: group.parent_id.as_ref().map(|p| p.as_str().to_string()),
            },
        );
    }
    for rel in &diagram.relations {
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        bp.edges.push((
            rel.from.as_str().to_string(),
            rel.to.as_str().to_string(),
        ));
    }
    super::probe::sanitize_overlapping_groups(&mut bp);

    let (mut substrate, mut index) =
        derive_substrate(&bp).map_err(|e| format!("derive_substrate: {e:?}"))?;
    derive_node_ports(
        &mut substrate,
        &bp,
        &mut index,
        &DerivePortsOptions::default(),
    )
    .map_err(|e| format!("derive_node_ports: {e:?}"))?;

    let rank_count = node_slots.values().map(|s| s.rank + 1).max().unwrap_or(0);
    let order_count = node_slots.values().map(|s| s.order + 1).max().unwrap_or(0);

    // PlanDiff 空（槽位+边通道齐全）→ 跳过相 I 选路搜索，只重建 substrate / occupancy
    if matches!(edge_order, PhaseIEdgeOrder::Forward) {
        if let Some(prev) = prev_plan {
            if can_skip_phase_i_search(prev, &node_slots, diagram) {
                crate::perf_log!("[atlas] phase I search skipped (PlanDiff empty vs prev)");
                let mut plan = prev.clone();
                plan.substrate = SubstrateSketch {
                    rank_count,
                    order_count,
                };
                plan.node_slots = node_slots;
                let occupancy = lane_occupancy_from_plan(&plan);
                return Ok(ChannelMetric {
                    plan,
                    substrate,
                    occupancy,
                });
            }
        }
    }

    let graph = ChannelGraph::from_substrate(&substrate);
    let mut plan = Plan {
        substrate: SubstrateSketch {
            rank_count,
            order_count,
        },
        node_slots,
        ..Plan::default()
    };

    // 顺序选路并 commit——度量相需要真实争用下的 lane_demand
    // EdgeId = diagram.relations 下标（与渲染 Vec 对齐；自环跳过选路）
    let mut edge_indices: Vec<usize> = diagram
        .relations
        .iter()
        .enumerate()
        .filter_map(|(i, rel)| {
            if rel.from.as_str() == rel.to.as_str() {
                None
            } else {
                Some(i)
            }
        })
        .collect();
    if matches!(edge_order, PhaseIEdgeOrder::Reverse) {
        edge_indices.reverse();
    }

    let mut occupancy = Occupancy::new();
    for &edge_idx in &edge_indices {
        let rel = &diagram.relations[edge_idx];
        let from = rel.from.as_str();
        let to = rel.to.as_str();
        if !bp.nodes.contains_key(from) || !bp.nodes.contains_key(to) {
            continue;
        }
        let mask = index.scope_mask_for_edge(&substrate, from, to);
        let from_ports = index.node_ports.get(from).cloned().unwrap_or_default();
        let to_ports = index.node_ports.get(to).cloned().unwrap_or_default();
        if from_ports.is_empty() || to_ports.is_empty() {
            continue;
        }
        let outcome = route_candidates(&graph, &from_ports, &to_ports, &occupancy, &mask)
            .unwrap_or_else(|_| RouteOutcome::infeasible());
        if outcome.status == SolverStatus::Converged {
            let _ = plan.record_route(edge_idx, &outcome);
            plan.record_ports_from_outcome(edge_idx, &outcome, &substrate);
            occupancy.commit(&outcome.tracks, &outcome.gates);
        }
    }
    plan.assign_lane_indices();
    let _ = plan.detect_and_set_bundles(2);

    Ok(ChannelMetric {
        plan,
        substrate,
        occupancy,
    })
}

impl Occupant for ChannelOccupant {
    fn id(&self) -> OccupantId {
        OccupantId(self.track_id.0)
    }

    fn kind(&self) -> OccupantKind {
        OccupantKind::Channel
    }

    fn demand(&self, axis: Axis) -> Demand {
        match (self.orient, axis) {
            (TrackOrient::Cross, Axis::Y) | (TrackOrient::Main, Axis::X) => self.band,
            _ => Demand::rigid(0.0),
        }
    }

    fn provenance(&self) -> &super::provenance::Provenance {
        &self.provenance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_band_width_scales_with_lanes() {
        assert_eq!(channel_band_width(0), 0.0);
        let one = channel_band_width(1);
        let two = channel_band_width(2);
        assert!(two > one);
        assert!((one - (CHANNEL_CLEARANCE * 2.0 + CORRIDOR_LANE_PITCH)).abs() < 1e-9);
    }

    #[test]
    fn inflate_takes_max_not_sum() {
        let base = vec![40.0, 40.0, 40.0];
        let mut dem = BTreeMap::new();
        dem.insert(1, 80.0);
        let out = inflate_layer_gaps(&base, &dem);
        assert_eq!(out[0], 40.0);
        assert_eq!(out[1], 80.0);
        assert_eq!(out[2], 40.0);
    }

    #[test]
    fn can_skip_requires_channels_and_matching_slots() {
        let mut slots = BTreeMap::new();
        slots.insert("a".into(), Slot { rank: 0, order: 0 });
        slots.insert("b".into(), Slot { rank: 1, order: 0 });
        let mut plan = Plan {
            node_slots: slots.clone(),
            ..Plan::default()
        };
        // 无 channel → 不可跳过
        let diagram = crate::ast::Diagram::default();
        assert!(!can_skip_phase_i_search(&plan, &slots, &diagram));
        plan.channels.insert(0, vec![]);
        // 空图无边 → 有 channel map 即可（无非自环边要覆盖）
        assert!(can_skip_phase_i_search(&plan, &slots, &diagram));
    }
}
