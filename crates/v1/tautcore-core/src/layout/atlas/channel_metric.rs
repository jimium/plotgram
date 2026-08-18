//! Stage 3：Channel 进度量相——Plan.channels → lane Demand → 层缝净空。
//!
//! 把相 I 的通道决策编译成度量相间距需求：
//! - Cross 走向 track（层间缝）→ 抬高 `per_layer_gaps`
//! - lane 带宽 = `2 * CLEARANCE + lanes * CORRIDOR_LANE_PITCH`（23 号文 3.4）
//!
//! **刻意**：不换选路；Plan 由通道图选路产出。

use std::collections::BTreeMap;

use super::channel::{
    derive_node_ports, derive_substrate, path_lane_load, route_candidates,
    route_candidates_congested, BlueprintIndex, ChannelBlueprint, ChannelGraph, DerivePortsOptions,
    EdgeId, GroupSpec, NodeSpec, Occupancy, RouteOutcome, Substrate, TrackId, TrackOrient,
};
use super::plan::{Plan, Slot, SubstrateSketch};
use super::space::{Axis, ChannelOccupant, Demand, Occupant, OccupantId, OccupantKind};
use crate::ast::Diagram;
use crate::layout::demand::CORRIDOR_LANE_PITCH;
use crate::layout::kernel::cost::{LexCost, SolverStatus};
use crate::layout::kernel::layered::graph::LayerNodeKind;
use crate::layout::kernel::layered::layered_kernel::LayeredDraft;

/// 通道净空额外边距（lane 带两侧的硬间隙）。
pub const CHANNEL_CLEARANCE: f64 = 8.0;

/// M6：Cross 走廊上每条带中段 label 的边预留的法向高度（与 `label_metrics` 高度同口径）。
pub const CROSS_LABEL_HEIGHT: f64 = crate::layout::constants::DEFAULT_LABEL_FONT_SIZE
    + 2.0 * crate::layout::constants::DEFAULT_LABEL_PADDING;

/// 度量相通道输入：基底 + 占用账 + Plan。
pub struct ChannelMetric {
    pub plan: Plan,
    pub substrate: Substrate,
    pub occupancy: Occupancy,
}

impl ChannelMetric {
    /// 按 track 汇总的 Demand（刚性）。Cross 含 M6 label 法向预留，与 `cross_gap_demands` 对齐。
    pub fn track_demands(&self, diagram: &Diagram) -> BTreeMap<TrackId, Demand> {
        let label_band = label_band_by_cross_track(diagram, &self.plan, &self.substrate);
        let mut out = BTreeMap::new();
        for t in self.substrate.tracks() {
            let lanes = self.occupancy.lane_demand(t.id);
            if lanes == 0 {
                continue;
            }
            let need = if t.orient == TrackOrient::Cross {
                cross_track_band_need(lanes, label_band.get(&t.id).copied().unwrap_or(0.0))
            } else {
                channel_band_width(lanes)
            };
            out.insert(t.id, Demand::rigid(need));
        }
        out
    }

    /// 构造 Channel Occupant 列表（供诊断）；Cross 含 label band，与 gap demand 同口径。
    pub fn occupants(&self, diagram: &Diagram) -> Vec<ChannelOccupant> {
        let label_band = label_band_by_cross_track(diagram, &self.plan, &self.substrate);
        let mut out = Vec::new();
        for t in self.substrate.tracks() {
            let lanes = self.occupancy.lane_demand(t.id);
            if lanes == 0 {
                continue;
            }
            let lb = if t.orient == TrackOrient::Cross {
                label_band.get(&t.id).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            out.push(ChannelOccupant::from_lanes_and_label_band(
                t.id, t.orient, lanes, lb,
            ));
        }
        out.sort_by_key(|o| o.track_id);
        out
    }

    /// Cross 走向 track → 层间缝需求（含 M6 label 法向预留）。
    ///
    /// Cross `line = k`（`1 <= k < rank_count`）对应 `per_layer_gaps[k - 1]`
    /// （rank `k-1` 与 `k` 之间）。外框线 `0` / `rank_count` 不进层缝表。
    ///
    /// `need = channel_band_width(lanes) + Σ label_metrics 高度`，同 gap 多 track 取 max。
    pub fn cross_gap_demands(&self, diagram: &Diagram) -> BTreeMap<usize, f64> {
        cross_gap_demands_from(
            diagram,
            &self.plan,
            &self.substrate,
            &self.occupancy,
        )
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

/// 单条 Cross track 的法向带宽：lane band + label 法向合计高度。
pub fn cross_track_band_need(lanes: u32, label_band: f64) -> f64 {
    if lanes == 0 {
        return 0.0;
    }
    channel_band_width(lanes) + label_band
}

/// 一条边上 Cross 法向 label 预留：主/`head`/`tail` 非空文本取 `label_metrics` 高度之 max。
fn relation_cross_label_band(rel: &crate::ast::Relation) -> f64 {
    use crate::layout::routing::common::label_avoidance::label_metrics;
    let mut h = 0.0f64;
    for text in [
        rel.label.as_deref(),
        rel.head_label.as_deref(),
        rel.tail_label.as_deref(),
    ] {
        if let Some(t) = text.filter(|s| !s.is_empty()) {
            let (_, lh) = label_metrics(t);
            h = h.max(lh);
        }
    }
    h
}

/// 按 Cross track 累加边上的 label 法向高度（与 `cross_gap_demands` / `track_demands` 同口径）。
pub fn label_band_by_cross_track(
    diagram: &Diagram,
    plan: &Plan,
    substrate: &Substrate,
) -> BTreeMap<TrackId, f64> {
    let mut out: BTreeMap<TrackId, f64> = BTreeMap::new();
    for (&eid, tracks) in &plan.channels {
        let Some(rel) = diagram.relations.get(eid) else {
            continue;
        };
        let band = relation_cross_label_band(rel);
        if band <= 0.0 {
            continue;
        }
        for &tid in tracks {
            let Some(t) = substrate.track(tid) else {
                continue;
            };
            if t.orient != TrackOrient::Cross {
                continue;
            }
            *out.entry(tid).or_insert(0.0) += band;
        }
    }
    out
}

/// 按 Cross `TrackId` 统计路径含该 track 且带任一非空 label 的边数。
///
/// 「带 label」= 主 `label` / `head_label` / `tail_label` 任一非空（每边至多计 1）。
pub fn labeled_edge_counts_by_cross_track(
    diagram: &Diagram,
    plan: &Plan,
    substrate: &Substrate,
) -> BTreeMap<TrackId, u32> {
    let mut out: BTreeMap<TrackId, u32> = BTreeMap::new();
    for (&eid, tracks) in &plan.channels {
        let Some(rel) = diagram.relations.get(eid) else {
            continue;
        };
        let has_label = [rel.label.as_deref(), rel.head_label.as_deref(), rel.tail_label.as_deref()]
            .into_iter()
            .any(|s| s.is_some_and(|t| !t.is_empty()));
        if !has_label {
            continue;
        }
        for &tid in tracks {
            let Some(t) = substrate.track(tid) else {
                continue;
            };
            if t.orient != TrackOrient::Cross {
                continue;
            }
            *out.entry(tid).or_insert(0) += 1;
        }
    }
    out
}

/// Cross gap demand 共享实现（供 ChannelMetric / main_axis / 诊断 inflate）。
pub fn cross_gap_demands_from(
    diagram: &Diagram,
    plan: &Plan,
    substrate: &Substrate,
    occupancy: &Occupancy,
) -> BTreeMap<usize, f64> {
    let rank_count = plan.substrate.rank_count;
    let label_band = label_band_by_cross_track(diagram, plan, substrate);
    let mut out: BTreeMap<usize, f64> = BTreeMap::new();
    for t in substrate.tracks() {
        if t.orient != TrackOrient::Cross {
            continue;
        }
        let lanes = occupancy.lane_demand(t.id);
        if lanes == 0 {
            continue;
        }
        let line = t.line;
        if line == 0 || line >= rank_count {
            continue;
        }
        let gap_idx = line - 1;
        let lb = label_band.get(&t.id).copied().unwrap_or(0.0);
        let need = cross_track_band_need(lanes, lb);
        let e = out.entry(gap_idx).or_insert(0.0);
        *e = e.max(need);
    }
    out
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

/// 判断 prev Plan 是否可跳过相 I 选路。
///
/// **实验特性（M8）**：供 `compute_layout_incremental` /
/// `TAUTCORE_ATLAS_PLAN_CACHE` 使用；默认全量布局路径不依赖本函数。
/// 生产勿默认打开增量入口。
///
/// 条件：槽位 rank/order **拓扑同构**（稠密化后相等）且每条非自环边已有 channel。
pub fn can_skip_phase_i_search(
    prev: &Plan,
    node_slots: &BTreeMap<String, Slot>,
    diagram: &Diagram,
) -> bool {
    if prev.channels.is_empty()
        || !slots_topologically_eq(&prev.node_slots, node_slots)
    {
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

/// 将 slots 稠密化：rank → `0..R`；每个 rank 内 order → `0..O`（同 order 平局按 node id）。
pub fn canonicalize_slots(slots: &BTreeMap<String, Slot>) -> BTreeMap<String, Slot> {
    let mut ranks: Vec<usize> = slots.values().map(|s| s.rank).collect();
    ranks.sort_unstable();
    ranks.dedup();
    let rank_map: BTreeMap<usize, usize> = ranks
        .iter()
        .enumerate()
        .map(|(i, &r)| (r, i))
        .collect();

    // rank → [(order, node_id)] 再按 order、id 排序后稠密化 order
    let mut by_rank: BTreeMap<usize, Vec<(usize, String)>> = BTreeMap::new();
    for (id, slot) in slots {
        let cr = *rank_map.get(&slot.rank).unwrap_or(&slot.rank);
        by_rank
            .entry(cr)
            .or_default()
            .push((slot.order, id.clone()));
    }

    let mut out = BTreeMap::new();
    for (cr, mut nodes) in by_rank {
        nodes.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut order_vals: Vec<usize> = nodes.iter().map(|(o, _)| *o).collect();
        order_vals.sort_unstable();
        order_vals.dedup();
        let order_map: BTreeMap<usize, usize> = order_vals
            .iter()
            .enumerate()
            .map(|(i, &o)| (o, i))
            .collect();
        for (ord, id) in nodes {
            let co = *order_map.get(&ord).unwrap_or(&ord);
            out.insert(id, Slot { rank: cr, order: co });
        }
    }
    out
}

/// 键集相同且 [`canonicalize_slots`] 结果相等。
pub fn slots_topologically_eq(
    a: &BTreeMap<String, Slot>,
    b: &BTreeMap<String, Slot>,
) -> bool {
    if a.keys().collect::<Vec<_>>() != b.keys().collect::<Vec<_>>() {
        return false;
    }
    canonicalize_slots(a) == canonicalize_slots(b)
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

    // PlanDiff 空（槽位拓扑同构 + 边通道齐全）→ 跳过相 I 选路搜索，只重建 substrate / occupancy
    // Reverse 边序改争用：禁止 skip（即使传入 prev）。
    if matches!(edge_order, PhaseIEdgeOrder::Forward) {
        if let Some(prev) = prev_plan {
            if can_skip_phase_i_search(prev, &node_slots, diagram) {
                let exact = prev.node_slots == node_slots;
                crate::perf_log!(
                    "[atlas] phase I search skipped ({})",
                    if exact {
                        "slots_exact"
                    } else {
                        "slots_topo_relocated"
                    }
                );
                let mut plan = prev.clone();
                plan.substrate = SubstrateSketch {
                    rank_count,
                    order_count,
                };
                // 重定位：写入本次绝对槽；保留 channels/gates/ports/lanes/bundles
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
    let mut edge_costs: BTreeMap<EdgeId, LexCost> = BTreeMap::new();
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
            edge_costs.insert(edge_idx, outcome.cost);
        }
    }
    bounded_phase_i_ripup(
        diagram,
        &bp,
        &index,
        &graph,
        &substrate,
        &mut plan,
        &mut occupancy,
        &mut edge_costs,
    );
    plan.assign_port_side_orders();
    plan.assign_lane_indices(&substrate);
    let _ = plan.detect_and_set_bundles(2);

    Ok(ChannelMetric {
        plan,
        substrate,
        occupancy,
    })
}

/// 相 I 有界返工：峰值 lane_demand track 上的边 release → 拥塞感知重路（最多 2 轮 × 8 边）。
pub(crate) fn bounded_phase_i_ripup(
    diagram: &Diagram,
    bp: &ChannelBlueprint,
    index: &BlueprintIndex,
    graph: &ChannelGraph<'_>,
    substrate: &Substrate,
    plan: &mut Plan,
    occupancy: &mut Occupancy,
    edge_costs: &mut BTreeMap<EdgeId, LexCost>,
) {
    const MAX_RIPUP_ROUNDS: u32 = 2;
    const MAX_RIPUP_EDGES: usize = 8;

    let (peak0, _) = occupancy_peak_sum(occupancy, substrate);
    if peak0 <= 1 {
        return;
    }

    let mut rounds_run = 0u32;
    let mut ripped = 0u32;

    for round in 0..MAX_RIPUP_ROUNDS {
        let (peak, _) = occupancy_peak_sum(occupancy, substrate);
        if peak <= 1 {
            break;
        }
        rounds_run = round + 1;

        let mut peak_tracks = Vec::new();
        for t in substrate.tracks() {
            if occupancy.lane_demand(t.id) == peak {
                peak_tracks.push(t.id);
            }
        }
        if peak_tracks.is_empty() {
            break;
        }

        let mut candidates: Vec<EdgeId> = plan
            .channels
            .iter()
            .filter_map(|(&eid, tracks)| {
                if tracks.iter().any(|t| peak_tracks.binary_search(t).is_ok()) {
                    Some(eid)
                } else {
                    None
                }
            })
            .collect();
        // LexCost 降序，平局 EdgeId 升序
        candidates.sort_by(|&a, &b| {
            let ca = edge_costs.get(&a);
            let cb = edge_costs.get(&b);
            match (ca, cb) {
                (Some(x), Some(y)) => y.cmp(x).then_with(|| a.cmp(&b)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(&b),
            }
        });
        candidates.truncate(MAX_RIPUP_EDGES);

        let mut round_ripped = 0u32;
        for eid in candidates {
            let Some(old_tracks) = plan.channels.get(&eid).cloned() else {
                continue;
            };
            let old_gates = plan.gates.get(&eid).cloned().unwrap_or_default();
            let old_ports = plan.ports.get(&eid).cloned();
            let old_cost = edge_costs.get(&eid).copied().unwrap_or_default();

            let (peak_before, sum_before) = occupancy_peak_sum(occupancy, substrate);

            occupancy.release(&old_tracks, &old_gates);

            let rel = &diagram.relations[eid];
            let from = rel.from.as_str();
            let to = rel.to.as_str();
            if !bp.nodes.contains_key(from) || !bp.nodes.contains_key(to) {
                occupancy.commit(&old_tracks, &old_gates);
                continue;
            }
            let mask = index.scope_mask_for_edge(substrate, from, to);
            let from_ports = index.node_ports.get(from).cloned().unwrap_or_default();
            let to_ports = index.node_ports.get(to).cloned().unwrap_or_default();
            if from_ports.is_empty() || to_ports.is_empty() {
                occupancy.commit(&old_tracks, &old_gates);
                continue;
            }

            let outcome =
                route_candidates_congested(graph, &from_ports, &to_ports, occupancy, &mask)
                    .unwrap_or_else(|_| RouteOutcome::infeasible());

            if outcome.status != SolverStatus::Converged {
                occupancy.commit(&old_tracks, &old_gates);
                continue;
            }

            let old_path_load = path_lane_load(occupancy, &old_tracks);
            let new_path_load = path_lane_load(occupancy, &outcome.tracks);

            let _ = plan.record_route(eid, &outcome);
            plan.record_ports_from_outcome(eid, &outcome, substrate);
            occupancy.commit(&outcome.tracks, &outcome.gates);

            let (peak_after, sum_after) = occupancy_peak_sum(occupancy, substrate);
            // 全局 peak/sum：走廊分流时常因共享宿主 Cross 不变；
            // path_load（release 后）捕捉「离开高峰走廊」的局部改善。
            let accept = peak_after < peak_before
                || (peak_after == peak_before && sum_after < sum_before)
                || (outcome.tracks != old_tracks && new_path_load < old_path_load);

            if accept {
                edge_costs.insert(eid, outcome.cost);
                ripped += 1;
                round_ripped += 1;
            } else {
                occupancy.release(&outcome.tracks, &outcome.gates);
                // 恢复旧路径
                let restore = RouteOutcome {
                    tracks: old_tracks.clone(),
                    gates: old_gates.clone(),
                    from_port: old_ports.as_ref().and_then(|p| p.from.slot_id),
                    to_port: old_ports.as_ref().and_then(|p| p.to.slot_id),
                    cost: old_cost,
                    status: SolverStatus::Converged,
                };
                let _ = plan.record_route(eid, &restore);
                if let Some(ports) = old_ports {
                    plan.ports.insert(eid, ports);
                }
                occupancy.commit(&old_tracks, &old_gates);
            }
        }

        if round_ripped == 0 {
            break;
        }
    }

    let (peak_final, _) = occupancy_peak_sum(occupancy, substrate);
    if ripped > 0 || peak_final < peak0 {
        crate::perf_log!(
            "[atlas] phase-I ripup: rounds={rounds_run} ripped={ripped} peak {peak0}→{peak_final}"
        );
    }
}

fn occupancy_peak_sum(occupancy: &Occupancy, substrate: &Substrate) -> (u32, u32) {
    let mut peak = 0u32;
    let mut sum = 0u32;
    for t in substrate.tracks() {
        let d = occupancy.lane_demand(t.id);
        peak = peak.max(d);
        sum = sum.saturating_add(d);
    }
    (peak, sum)
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
    fn cross_gap_demands_includes_label_height() {
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation};
        use crate::layout::atlas::channel::{PortSide, PortSlotId};

        let mut substrate = Substrate::default();
        substrate
            .add_track(TrackId(1), TrackOrient::Cross, None, 1.0, 1, (0, 2))
            .unwrap();
        substrate
            .attach_port(PortSlotId(0), "a", PortSide::MainHigh, 0, TrackId(1), 0)
            .unwrap();
        substrate
            .attach_port(PortSlotId(1), "b", PortSide::MainLow, 0, TrackId(1), 0)
            .unwrap();

        let mut plan = Plan {
            substrate: SubstrateSketch {
                rank_count: 2,
                order_count: 1,
            },
            ..Plan::default()
        };
        plan.node_slots.insert("a".into(), Slot { rank: 0, order: 0 });
        plan.node_slots.insert("b".into(), Slot { rank: 1, order: 0 });
        // 两条边同 Cross track：一条有 label
        plan.channels.insert(0, vec![TrackId(1)]);
        plan.channels.insert(1, vec![TrackId(1)]);
        plan.gates.insert(0, vec![]);
        plan.gates.insert(1, vec![]);

        let occupancy = lane_occupancy_from_plan(&plan);
        assert_eq!(occupancy.lane_demand(TrackId(1)), 2);

        let mut diagram = Diagram::default();
        let span = crate::ast::Span::dummy();
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: Some("go".into()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });

        let metric = ChannelMetric {
            plan,
            substrate,
            occupancy,
        };
        let dem = metric.cross_gap_demands(&diagram);
        let (_, label_h) = crate::layout::routing::common::label_avoidance::label_metrics("go");
        let expected = channel_band_width(2) + label_h; // 仅一条带 label
        assert!(
            (dem.get(&0).copied().unwrap_or(0.0) - expected).abs() < 1e-9,
            "got {:?} want {expected}",
            dem.get(&0)
        );

        // 诊断 API 与 gap 同口径
        let td = metric.track_demands(&diagram);
        assert!(
            (td.get(&TrackId(1)).map(|d| d.preferred).unwrap_or(0.0) - expected).abs() < 1e-9,
            "track_demands Cross should match gap need"
        );

        // 去掉主 label，改用 head_label → 仍计 label_metrics 高度
        diagram.relations[0].label = None;
        diagram.relations[0].head_label = Some("H".into());
        let (_, head_h) = crate::layout::routing::common::label_avoidance::label_metrics("H");
        let expected_head = channel_band_width(2) + head_h;
        let dem_head = metric.cross_gap_demands(&diagram);
        assert!(
            (dem_head.get(&0).copied().unwrap_or(0.0) - expected_head).abs() < 1e-9,
            "head_label should count; got {:?}",
            dem_head.get(&0)
        );

        // 清空全部 label → 仅 lane band
        diagram.relations[0].head_label = None;
        let dem2 = metric.cross_gap_demands(&diagram);
        assert!(
            (dem2.get(&0).copied().unwrap_or(0.0) - channel_band_width(2)).abs() < 1e-9,
            "got {:?}",
            dem2.get(&0)
        );
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

    #[test]
    fn slots_topo_eq_and_can_skip_table() {
        let cases: &[(&str, BTreeMap<String, Slot>, BTreeMap<String, Slot>, bool)] = &[
            (
                "exact",
                BTreeMap::from([
                    ("a".into(), Slot { rank: 0, order: 0 }),
                    ("b".into(), Slot { rank: 1, order: 0 }),
                ]),
                BTreeMap::from([
                    ("a".into(), Slot { rank: 0, order: 0 }),
                    ("b".into(), Slot { rank: 1, order: 0 }),
                ]),
                true,
            ),
            (
                "dense_shift_same_topo",
                BTreeMap::from([
                    ("a".into(), Slot { rank: 2, order: 5 }),
                    ("b".into(), Slot { rank: 7, order: 3 }),
                ]),
                BTreeMap::from([
                    ("a".into(), Slot { rank: 0, order: 0 }),
                    ("b".into(), Slot { rank: 1, order: 0 }),
                ]),
                true,
            ),
            (
                "key_set_diff",
                BTreeMap::from([("a".into(), Slot { rank: 0, order: 0 })]),
                BTreeMap::from([
                    ("a".into(), Slot { rank: 0, order: 0 }),
                    ("b".into(), Slot { rank: 1, order: 0 }),
                ]),
                false,
            ),
            (
                "relative_rank_diff",
                BTreeMap::from([
                    ("a".into(), Slot { rank: 0, order: 0 }),
                    ("b".into(), Slot { rank: 1, order: 0 }),
                ]),
                BTreeMap::from([
                    ("a".into(), Slot { rank: 1, order: 0 }),
                    ("b".into(), Slot { rank: 0, order: 0 }),
                ]),
                false,
            ),
            (
                "same_rank_order_swap",
                BTreeMap::from([
                    ("a".into(), Slot { rank: 0, order: 0 }),
                    ("b".into(), Slot { rank: 0, order: 1 }),
                ]),
                BTreeMap::from([
                    ("a".into(), Slot { rank: 0, order: 1 }),
                    ("b".into(), Slot { rank: 0, order: 0 }),
                ]),
                false,
            ),
        ];
        for (name, a, b, want) in cases {
            assert_eq!(
                slots_topologically_eq(a, b),
                *want,
                "topo_eq case {name}"
            );
            assert_eq!(
                slots_topologically_eq(b, a),
                *want,
                "topo_eq symmetric {name}"
            );
        }

        // can_skip：绝对槽不同但拓扑同 → true；channels 保留
        let prev_slots = BTreeMap::from([
            ("a".into(), Slot { rank: 2, order: 5 }),
            ("b".into(), Slot { rank: 7, order: 3 }),
        ]);
        let new_slots = BTreeMap::from([
            ("a".into(), Slot { rank: 0, order: 0 }),
            ("b".into(), Slot { rank: 1, order: 0 }),
        ]);
        let mut plan = Plan {
            node_slots: prev_slots,
            ..Plan::default()
        };
        plan.channels.insert(0, vec![]);
        let diagram = crate::ast::Diagram::default();
        assert!(can_skip_phase_i_search(&plan, &new_slots, &diagram));

        // skip 重定位：clone 后写新槽，channels 不变
        let mut relocated = plan.clone();
        relocated.node_slots = new_slots.clone();
        assert_eq!(relocated.node_slots, new_slots);
        assert_eq!(relocated.channels, plan.channels);
    }

    fn rel_ab(span: crate::ast::Span) -> crate::ast::Relation {
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation};
        Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        }
    }

    fn empty_index_with_ports(
        a: crate::layout::atlas::channel::PortSlotId,
        b: crate::layout::atlas::channel::PortSlotId,
    ) -> BlueprintIndex {
        let mut index = BlueprintIndex {
            cross_lines: BTreeMap::new(),
            main_lines: BTreeMap::new(),
            group_ids: BTreeMap::new(),
            node_region: BTreeMap::new(),
            node_ports: BTreeMap::new(),
        };
        index.node_ports.insert("a".into(), vec![a]);
        index.node_ports.insert("b".into(), vec![b]);
        index
    }

    #[test]
    fn phase_i_ripup_diverts_equal_cost_corridor() {
        use crate::ast::{Position, Span};
        use crate::layout::atlas::channel::{PortSide, PortSlotId, TrackOrient};

        let mut s = Substrate::new();
        s.add_track(TrackId(0), TrackOrient::Cross, None, 1.0, 0, (0, 4))
            .unwrap();
        s.add_track(TrackId(1), TrackOrient::Cross, None, 1.0, 1, (0, 4))
            .unwrap();
        s.add_track(TrackId(10), TrackOrient::Main, None, 1.0, 0, (0, 2))
            .unwrap();
        s.add_track(TrackId(11), TrackOrient::Main, None, 1.0, 1, (0, 2))
            .unwrap();
        for m in [10u32, 11] {
            s.link(TrackId(0), TrackId(m)).unwrap();
            s.link(TrackId(1), TrackId(m)).unwrap();
        }
        s.attach_port(PortSlotId(0), "a", PortSide::MainLow, 0, TrackId(0), 0)
            .unwrap();
        s.attach_port(PortSlotId(1), "b", PortSide::MainLow, 0, TrackId(1), 0)
            .unwrap();

        let graph = ChannelGraph::from_substrate(&s);
        let mut bp = ChannelBlueprint::default();
        bp.nodes.insert("a".into(), NodeSpec { rank: 0, order: 0 });
        bp.nodes.insert("b".into(), NodeSpec { rank: 1, order: 0 });
        let index = empty_index_with_ports(PortSlotId(0), PortSlotId(1));
        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = crate::ast::Diagram::default();
        diagram.relations.push(rel_ab(span));
        diagram.relations.push(rel_ab(span));

        let mask = index.scope_mask_for_edge(&s, "a", "b");
        let mut occupancy = Occupancy::new();
        let mut plan = Plan::default();
        let mut edge_costs = BTreeMap::new();
        for eid in 0..2usize {
            let out = route_candidates(
                &graph,
                &index.node_ports["a"],
                &index.node_ports["b"],
                &occupancy,
                &mask,
            )
            .unwrap();
            let _ = plan.record_route(eid, &out);
            plan.record_ports_from_outcome(eid, &out, &s);
            occupancy.commit(&out.tracks, &out.gates);
            edge_costs.insert(eid, out.cost);
        }
        assert_eq!(occupancy.lane_demand(TrackId(10)), 2);
        assert_eq!(occupancy.lane_demand(TrackId(11)), 0);

        bounded_phase_i_ripup(
            &diagram,
            &bp,
            &index,
            &graph,
            &s,
            &mut plan,
            &mut occupancy,
            &mut edge_costs,
        );
        assert_ne!(plan.channels[&0], plan.channels[&1]);
        assert_eq!(occupancy.lane_demand(TrackId(10)), 1);
        assert_eq!(occupancy.lane_demand(TrackId(11)), 1);
    }

    #[test]
    fn phase_i_ripup_noop_when_peak_le_one() {
        use crate::ast::{Position, Span};
        use crate::layout::atlas::channel::{PortSide, PortSlotId, TrackOrient};

        let mut s = Substrate::new();
        s.add_track(TrackId(0), TrackOrient::Cross, None, 1.0, 0, (0, 4))
            .unwrap();
        s.add_track(TrackId(1), TrackOrient::Cross, None, 1.0, 1, (0, 4))
            .unwrap();
        s.add_track(TrackId(10), TrackOrient::Main, None, 1.0, 0, (0, 2))
            .unwrap();
        s.link(TrackId(0), TrackId(10)).unwrap();
        s.link(TrackId(1), TrackId(10)).unwrap();
        s.attach_port(PortSlotId(0), "a", PortSide::MainLow, 0, TrackId(0), 0)
            .unwrap();
        s.attach_port(PortSlotId(1), "b", PortSide::MainLow, 0, TrackId(1), 0)
            .unwrap();

        let graph = ChannelGraph::from_substrate(&s);
        let mut bp = ChannelBlueprint::default();
        bp.nodes.insert("a".into(), NodeSpec { rank: 0, order: 0 });
        bp.nodes.insert("b".into(), NodeSpec { rank: 1, order: 0 });
        let index = empty_index_with_ports(PortSlotId(0), PortSlotId(1));
        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = crate::ast::Diagram::default();
        diagram.relations.push(rel_ab(span));

        let mask = index.scope_mask_for_edge(&s, "a", "b");
        let mut occupancy = Occupancy::new();
        let mut plan = Plan::default();
        let mut edge_costs = BTreeMap::new();
        let out = route_candidates(
            &graph,
            &index.node_ports["a"],
            &index.node_ports["b"],
            &occupancy,
            &mask,
        )
        .unwrap();
        let _ = plan.record_route(0, &out);
        plan.record_ports_from_outcome(0, &out, &s);
        occupancy.commit(&out.tracks, &out.gates);
        edge_costs.insert(0, out.cost);
        let channels_before = plan.channels.clone();

        bounded_phase_i_ripup(
            &diagram,
            &bp,
            &index,
            &graph,
            &s,
            &mut plan,
            &mut occupancy,
            &mut edge_costs,
        );
        assert_eq!(plan.channels, channels_before);
        assert_eq!(occupancy.lane_demand(TrackId(10)), 1);
    }

    #[test]
    fn phase_i_ripup_restores_when_no_alternate() {
        use crate::ast::{Position, Span};
        use crate::layout::atlas::channel::{PortSide, PortSlotId, TrackOrient};

        let mut s = Substrate::new();
        s.add_track(TrackId(0), TrackOrient::Cross, None, 1.0, 0, (0, 4))
            .unwrap();
        s.add_track(TrackId(1), TrackOrient::Cross, None, 1.0, 1, (0, 4))
            .unwrap();
        s.add_track(TrackId(10), TrackOrient::Main, None, 1.0, 0, (0, 2))
            .unwrap();
        s.link(TrackId(0), TrackId(10)).unwrap();
        s.link(TrackId(1), TrackId(10)).unwrap();
        s.attach_port(PortSlotId(0), "a", PortSide::MainLow, 0, TrackId(0), 0)
            .unwrap();
        s.attach_port(PortSlotId(1), "b", PortSide::MainLow, 0, TrackId(1), 0)
            .unwrap();

        let graph = ChannelGraph::from_substrate(&s);
        let mut bp = ChannelBlueprint::default();
        bp.nodes.insert("a".into(), NodeSpec { rank: 0, order: 0 });
        bp.nodes.insert("b".into(), NodeSpec { rank: 1, order: 0 });
        let index = empty_index_with_ports(PortSlotId(0), PortSlotId(1));
        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = crate::ast::Diagram::default();
        diagram.relations.push(rel_ab(span));
        diagram.relations.push(rel_ab(span));

        let mask = index.scope_mask_for_edge(&s, "a", "b");
        let mut occupancy = Occupancy::new();
        let mut plan = Plan::default();
        let mut edge_costs = BTreeMap::new();
        for eid in 0..2usize {
            let out = route_candidates(
                &graph,
                &index.node_ports["a"],
                &index.node_ports["b"],
                &occupancy,
                &mask,
            )
            .unwrap();
            let _ = plan.record_route(eid, &out);
            plan.record_ports_from_outcome(eid, &out, &s);
            occupancy.commit(&out.tracks, &out.gates);
            edge_costs.insert(eid, out.cost);
        }
        let channels_before = plan.channels.clone();
        assert_eq!(occupancy.lane_demand(TrackId(10)), 2);

        bounded_phase_i_ripup(
            &diagram,
            &bp,
            &index,
            &graph,
            &s,
            &mut plan,
            &mut occupancy,
            &mut edge_costs,
        );
        assert_eq!(plan.channels, channels_before);
        assert_eq!(occupancy.lane_demand(TrackId(10)), 2);
    }
}
