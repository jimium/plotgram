//! Route every non-bus edge on the Channel graph (D1.2 Gate + D1.3.3 RouteOrder).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::diagnostics::Relaxation;

use super::derive::derive_substrate;
use super::graph::{ChannelGraph, Occupancy};
use super::search::{
    path_used_outer_overflow, route_edge, ChannelPath, LexCost, RouteHints, ScopeMask, SpanAffinity,
};
use super::substrate::{BlueprintIndex, PortSide, Substrate, TrackId};
use crate::layout::hierarchical::compose::bundle::{end_bus_edge_ids, BundlePlan};
use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealEdge, RealGraph};
use crate::layout::hierarchical::params::HierarchicalParams;

const MAX_RIPUP_ROUNDS: u32 = 4;
const MAX_RIPUP_EDGES: usize = 8;

/// Orthogonal Channel topology for one edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteTopology {
    Orthogonal(ChannelPath),
}

/// Full Channel route plan: substrate + per-edge topology + BundlePlan facts.
#[derive(Debug, Clone)]
pub struct ChannelRoutePlan {
    pub substrate: Substrate,
    pub index: BlueprintIndex,
    /// edge_id → topology (end-bus members omitted — Ink joins BundlePlan).
    pub routes: BTreeMap<String, RouteTopology>,
    /// End-bus bundles from Compose (`auto_edge_grouping`).
    pub bundles: Vec<BundlePlan>,
    /// Soft relaxations produced by bounded rip-up / outer-overflow.
    pub relaxations: Vec<Relaxation>,
    /// Number of rip-up rounds entered (0 when peak ≤ 1).
    pub ripup_rounds: u32,
    /// True when group-cut Gate IR was used (false = root-scope fallback).
    pub used_gates: bool,
    /// D1.3.3 deterministic commit order (RouteOrderWriter).
    pub route_order: Vec<String>,
}

fn layer_order(plan: &PlanGraph, elem: usize) -> usize {
    let rank = plan.elems[elem].rank as usize;
    plan.layers[rank]
        .iter()
        .position(|&e| e == elem)
        .expect("elem must be in its layer")
}

fn endpoint_side_rank_order(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    edge_id: &str,
    at_source: bool,
) -> Result<(PortSide, usize, usize), LayoutError> {
    let edge = graph
        .edges
        .iter()
        .find(|e| e.edge_id == edge_id)
        .ok_or_else(|| LayoutError::message(format!("channel: unknown edge `{edge_id}`")))?;
    let node_idx = if at_source {
        edge.original_source
    } else {
        edge.original_target
    };
    let id = &graph.ids[node_idx];
    let elem = plan.index_of[&ElemKey::Real(id.clone())];
    let rank = plan.elems[elem].rank as usize;
    let order = layer_order(plan, elem);
    let rp = &ports[edge_id];
    let side = if at_source {
        PortSide::from_algo_side(rp.source.side)
    } else {
        PortSide::from_algo_side(rp.target.side)
    };
    Ok((side, rank, order))
}

/// Resolve Channel start/goal hosts for one edge.
///
/// Same-face East–East / West–West share one outer Main corridor:
/// - both CrossHigh (East) → `Main(max(order)+1)` at each end's rank
/// - both CrossLow (West) → `Main(min(order))` at each end's rank
///
/// N/S and mixed faces keep per-endpoint `resolve_host_track`.
fn host_tracks_for_edge(
    index: &BlueprintIndex,
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    edge_id: &str,
) -> Result<(TrackId, TrackId), LayoutError> {
    let (src_side, src_rank, src_order) =
        endpoint_side_rank_order(plan, graph, ports, edge_id, true)?;
    let (tgt_side, tgt_rank, tgt_order) =
        endpoint_side_rank_order(plan, graph, ports, edge_id, false)?;

    let shared_og = match (src_side, tgt_side) {
        (PortSide::CrossHigh, PortSide::CrossHigh) => Some(src_order.max(tgt_order) + 1),
        (PortSide::CrossLow, PortSide::CrossLow) => Some(src_order.min(tgt_order)),
        _ => None,
    };

    if let Some(og) = shared_og {
        let start = index.main_at(og, src_rank).ok_or_else(|| {
            LayoutError::message(format!(
                "channel: no shared East/West Main host og={og} for edge `{edge_id}` source"
            ))
        })?;
        let goal = index.main_at(og, tgt_rank).ok_or_else(|| {
            LayoutError::message(format!(
                "channel: no shared East/West Main host og={og} for edge `{edge_id}` target"
            ))
        })?;
        return Ok((start, goal));
    }

    let start = index
        .resolve_host_track(src_rank, src_order, src_side)
        .ok_or_else(|| {
            LayoutError::message(format!(
                "channel: no host track for edge `{edge_id}` source side {src_side:?}"
            ))
        })?;
    let goal = index
        .resolve_host_track(tgt_rank, tgt_order, tgt_side)
        .ok_or_else(|| {
            LayoutError::message(format!(
                "channel: no host track for edge `{edge_id}` target side {tgt_side:?}"
            ))
        })?;
    Ok((start, goal))
}

fn scope_mask_for_edge(
    substrate: &Substrate,
    index: &BlueprintIndex,
    graph: &RealGraph,
    edge: &RealEdge,
) -> ScopeMask {
    if index.group_ids.is_empty() {
        return ScopeMask::unrestricted();
    }
    let from = &graph.ids[edge.original_source];
    let to = &graph.ids[edge.original_target];
    ScopeMask::for_scopes(substrate, index.node_scope(from), index.node_scope(to))
}

fn endpoint_rank_order(plan: &PlanGraph, graph: &RealGraph, node_idx: usize) -> (usize, usize) {
    let id = &graph.ids[node_idx];
    let elem = plan.index_of[&ElemKey::Real(id.clone())];
    let rank = plan.elems[elem].rank as usize;
    let order = layer_order(plan, elem);
    (rank, order)
}

fn edge_rank_span(plan: &PlanGraph, graph: &RealGraph, edge: &RealEdge) -> usize {
    let (sr, _) = endpoint_rank_order(plan, graph, edge.original_source);
    let (tr, _) = endpoint_rank_order(plan, graph, edge.original_target);
    sr.abs_diff(tr)
}

fn dummy_chain_len(plan: &PlanGraph, edge_id: &str) -> usize {
    plan.segments
        .iter()
        .filter(|s| s.edge_id == edge_id)
        .count()
        .saturating_sub(1)
}

fn hints_for_edge(
    plan: &PlanGraph,
    graph: &RealGraph,
    params: &HierarchicalParams,
    edge: &RealEdge,
    order_count: usize,
) -> RouteHints {
    let pitch = params.edge_gap.max(1e-9);
    let (src_rank, src_order) = endpoint_rank_order(plan, graph, edge.original_source);
    let (tgt_rank, tgt_order) = endpoint_rank_order(plan, graph, edge.original_target);
    RouteHints {
        min_first_span: if params.min_first_segment > 0.0 {
            (params.min_first_segment / pitch).max(1.0)
        } else {
            0.0
        },
        min_last_span: if params.min_last_segment > 0.0 {
            (params.min_last_segment / pitch).max(1.0)
        } else {
            0.0
        },
        // D1.3.2: outer Main is overflow for every edge, not a reversed-edge preference.
        outer_main_as_overflow: true,
        order_count,
        span: Some(SpanAffinity {
            src_rank,
            tgt_rank,
            src_order,
            tgt_order,
        }),
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

fn path_lane_load(occupancy: &Occupancy, tracks: &[TrackId]) -> u32 {
    tracks
        .iter()
        .map(|&t| occupancy.lane_demand(t))
        .sum::<u32>()
}

/// Sort key for RouteOrderWriter (D1.3.3). Higher priority sorts first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteOrderEntry {
    pub edge_id: String,
    pub critical: bool,
    pub span: usize,
    pub reversed: bool,
    pub dummy_len: usize,
    pub decl_index: usize,
}

impl RouteOrderEntry {
    fn cmp_priority(&self, other: &Self) -> Ordering {
        other
            .critical
            .cmp(&self.critical)
            .then_with(|| other.span.cmp(&self.span))
            .then_with(|| other.reversed.cmp(&self.reversed))
            .then_with(|| other.dummy_len.cmp(&self.dummy_len))
            .then_with(|| self.decl_index.cmp(&other.decl_index))
            .then_with(|| self.edge_id.cmp(&other.edge_id))
    }
}

/// Deterministic commit order: critical ↓, span ↓, reversed ↓, dummy ↓, decl ↑.
pub fn compute_route_order(mut entries: Vec<RouteOrderEntry>) -> Vec<RouteOrderEntry> {
    entries.sort_by(|a, b| a.cmp_priority(b));
    entries
}

struct EdgeRouteState {
    path: ChannelPath,
    cost: LexCost,
    failure_count: u32,
    critical: bool,
    span: usize,
    decl_index: usize,
    start: TrackId,
    goal: TrackId,
    mask: ScopeMask,
    hints: RouteHints,
}

struct PreparedEdge {
    entry: RouteOrderEntry,
    start: TrackId,
    goal: TrackId,
    mask: ScopeMask,
    hints: RouteHints,
}

/// Derive substrate and route all non-end-bus edges (RouteOrder + rip-up).
pub fn route_edges_channel(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    end_bundles: &[BundlePlan],
    params: &HierarchicalParams,
) -> Result<ChannelRoutePlan, LayoutError> {
    let (substrate, index, used_gates, mut relaxations) = derive_substrate(plan, graph)?;
    let channel_graph = ChannelGraph::from_substrate(&substrate);
    let mut occupancy = Occupancy::new();

    let bus_edges = end_bus_edge_ids(end_bundles);

    let mut prepared: Vec<PreparedEdge> = Vec::new();
    for (decl_index, e) in graph.edges.iter().enumerate() {
        if bus_edges.contains(&e.edge_id) {
            continue;
        }
        let (start, goal) = host_tracks_for_edge(&index, plan, graph, ports, &e.edge_id)?;
        let mask = scope_mask_for_edge(&substrate, &index, graph, e);
        let hints = hints_for_edge(plan, graph, params, e, index.order_count);
        prepared.push(PreparedEdge {
            entry: RouteOrderEntry {
                edge_id: e.edge_id.clone(),
                critical: e.critical,
                span: edge_rank_span(plan, graph, e),
                reversed: e.reversed,
                dummy_len: dummy_chain_len(plan, &e.edge_id),
                decl_index,
            },
            start,
            goal,
            mask,
            hints,
        });
    }

    let ordered = compute_route_order(prepared.iter().map(|p| p.entry.clone()).collect());
    let route_order: Vec<String> = ordered.iter().map(|e| e.edge_id.clone()).collect();
    let mut by_id: BTreeMap<String, PreparedEdge> = prepared
        .into_iter()
        .map(|p| (p.entry.edge_id.clone(), p))
        .collect();

    let mut states: BTreeMap<String, EdgeRouteState> = BTreeMap::new();
    for entry in &ordered {
        let prep = by_id
            .remove(&entry.edge_id)
            .expect("prepared edge present");
        let outcome = route_edge(
            &channel_graph,
            prep.start,
            prep.goal,
            &occupancy,
            true,
            &prep.mask,
            prep.hints,
        );
        if !outcome.feasible {
            return Err(LayoutError::message(format!(
                "channel: no path for edge `{}` (Infeasible)",
                entry.edge_id
            )));
        }
        occupancy.commit(&outcome.path.tracks, &outcome.path.gates);
        states.insert(
            entry.edge_id.clone(),
            EdgeRouteState {
                path: outcome.path,
                cost: outcome.cost,
                failure_count: 0,
                critical: entry.critical,
                span: entry.span,
                decl_index: entry.decl_index,
                start: prep.start,
                goal: prep.goal,
                mask: prep.mask,
                hints: prep.hints,
            },
        );
    }

    let (ripup_relaxations, ripup_rounds) = bounded_ripup(
        &channel_graph,
        &substrate,
        &mut occupancy,
        &mut states,
    );
    relaxations.extend(ripup_relaxations);

    // Outer-overflow: record when the final path sits on outer Main while an
    // inner band gap is free (search soft-penalty already applied; this is
    // diagnostics only — geometry unchanged).
    for (eid, st) in &states {
        if path_used_outer_overflow(&substrate, &st.path, &st.hints, &occupancy) {
            relaxations.push(Relaxation {
                rule: "channel-outer-overflow".into(),
                detail: format!("edge `{eid}` routed on outer Main while inner band free"),
            });
        }
    }

    // Path-level scope verifier (hard FAIL on foreign scope).
    for (eid, st) in &states {
        if let Err(msg) = super::verify::verify_route_scope(&substrate, &st.mask, &st.path) {
            return Err(LayoutError::message(format!(
                "channel: edge `{eid}` failed scope verify: {msg}"
            )));
        }
    }

    let mut routes = BTreeMap::new();
    for (eid, st) in &states {
        routes.insert(eid.clone(), RouteTopology::Orthogonal(st.path.clone()));
    }

    Ok(ChannelRoutePlan {
        substrate,
        index,
        routes,
        bundles: end_bundles.to_vec(),
        relaxations,
        ripup_rounds,
        used_gates,
        route_order,
    })
}

fn bounded_ripup(
    graph: &ChannelGraph<'_>,
    substrate: &Substrate,
    occupancy: &mut Occupancy,
    states: &mut BTreeMap<String, EdgeRouteState>,
) -> (Vec<Relaxation>, u32) {
    let mut relaxations = Vec::new();
    let (peak0, _) = occupancy_peak_sum(occupancy, substrate);
    if peak0 <= 1 {
        return (relaxations, 0);
    }

    let mut rounds_entered = 0u32;
    for round in 0..MAX_RIPUP_ROUNDS {
        let (peak, _) = occupancy_peak_sum(occupancy, substrate);
        if peak <= 1 {
            break;
        }
        rounds_entered += 1;

        let mut peak_tracks = Vec::new();
        for t in substrate.tracks() {
            if occupancy.lane_demand(t.id) == peak {
                peak_tracks.push(t.id);
            }
        }
        peak_tracks.sort();

        let mut on_peak: Vec<String> = states
            .iter()
            .filter(|(_, st)| st.path.tracks.iter().any(|t| peak_tracks.contains(t)))
            .map(|(id, _)| id.clone())
            .collect();
        // Sacrifice: non-critical first, shorter span first, then failure/decl.
        on_peak.sort_by(|a, b| {
            let sa = &states[a];
            let sb = &states[b];
            (sa.critical as u8)
                .cmp(&(sb.critical as u8))
                .then_with(|| sa.span.cmp(&sb.span))
                .then_with(|| sb.failure_count.cmp(&sa.failure_count))
                .then_with(|| sa.decl_index.cmp(&sb.decl_index))
                .then_with(|| a.cmp(b))
        });
        on_peak.truncate(MAX_RIPUP_EDGES);

        let mut round_ripped = 0u32;
        for eid in on_peak {
            let Some(st) = states.get(&eid) else {
                continue;
            };
            let old_path = st.path.clone();
            let old_cost = st.cost;
            let start = st.start;
            let goal = st.goal;
            let mask = st.mask.clone();
            let hints = st.hints;

            let (peak_before, sum_before) = occupancy_peak_sum(occupancy, substrate);
            occupancy.release(&old_path.tracks, &old_path.gates);

            let outcome = route_edge(graph, start, goal, occupancy, true, &mask, hints);
            if !outcome.feasible {
                occupancy.commit(&old_path.tracks, &old_path.gates);
                if let Some(st) = states.get_mut(&eid) {
                    st.failure_count = st.failure_count.saturating_add(1);
                }
                continue;
            }

            let old_path_load = path_lane_load(occupancy, &old_path.tracks);
            let new_path_load = path_lane_load(occupancy, &outcome.path.tracks);
            occupancy.commit(&outcome.path.tracks, &outcome.path.gates);

            let (peak_after, sum_after) = occupancy_peak_sum(occupancy, substrate);
            let accept = peak_after < peak_before
                || (peak_after == peak_before && sum_after < sum_before)
                || (outcome.path.tracks != old_path.tracks && new_path_load < old_path_load);

            if accept {
                if let Some(st) = states.get_mut(&eid) {
                    st.path = outcome.path;
                    st.cost = outcome.cost;
                }
                round_ripped += 1;
                relaxations.push(Relaxation {
                    rule: "channel-rip-up".into(),
                    detail: format!("edge `{eid}` re-routed in round {round}"),
                });
            } else {
                occupancy.release(&outcome.path.tracks, &outcome.path.gates);
                occupancy.commit(&old_path.tracks, &old_path.gates);
                if let Some(st) = states.get_mut(&eid) {
                    st.path = old_path;
                    st.cost = old_cost;
                    st.failure_count = st.failure_count.saturating_add(1);
                }
            }
        }

        if round_ripped == 0 {
            break;
        }
    }

    let (peak_final, _) = occupancy_peak_sum(occupancy, substrate);
    if peak_final > 1 {
        relaxations.push(Relaxation {
            rule: "channel-rip-up-budget".into(),
            detail: format!(
                "peak occupancy {peak_final} remains after {MAX_RIPUP_ROUNDS} rounds"
            ),
        });
    }

    (relaxations, rounds_entered)
}

#[cfg(test)]
mod tests {
    use super::{compute_route_order, host_tracks_for_edge, RouteOrderEntry, MAX_RIPUP_ROUNDS};
    use super::super::substrate::{derive_root_substrate, TrackOrient};
    use crate::layout::hierarchical::compose::ports::{EdgePorts, ResolvedPort};
    use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph, RealEdge, RealGraph};
    use plotgram_algo::orientation::Side;
    use plotgram_model::port::AlongSpec;
    use std::collections::BTreeMap;

    fn east_ports() -> EdgePorts {
        EdgePorts {
            source: ResolvedPort {
                side: Side::East,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
            target: ResolvedPort {
                side: Side::East,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
            source_cluster: None,
            target_cluster: None,
        }
    }

    fn west_ports() -> EdgePorts {
        EdgePorts {
            source: ResolvedPort {
                side: Side::West,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
            target: ResolvedPort {
                side: Side::West,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
            source_cluster: None,
            target_cluster: None,
        }
    }

    /// Two ranks × two orders: left/right columns, one edge spanning ranks.
    fn two_col_plan_graph() -> (PlanGraph, RealGraph) {
        let elems = vec![
            Elem {
                key: ElemKey::Real("a0".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("a1".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b0".into()),
                group_path: vec![],
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("b1".into()),
                group_path: vec![],
                rank: 1,
            },
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: (0..4).collect(),
            segments: vec![],
            layers: vec![vec![0, 1], vec![2, 3]],
        };
        let mut index_of_ids = BTreeMap::new();
        for (i, id) in ["a0", "a1", "b0", "b1"].iter().enumerate() {
            index_of_ids.insert((*id).into(), i);
        }
        let graph = RealGraph {
            ids: vec!["a0".into(), "a1".into(), "b0".into(), "b1".into()],
            index_of: index_of_ids,
            group_path: vec![vec![]; 4],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 4],
            edges: vec![RealEdge {
                edge_id: "ew".into(),
                // a0 (order0) → b1 (order1): cross-column same-face
                original_source: 0,
                original_target: 3,
                working_source: 0,
                working_target: 3,
                reversed: false,
                from_port: None,
                to_port: None,
                critical: false,
            }],
            self_loops: vec![],
        };
        (plan, graph)
    }

    #[test]
    fn ripup_budget_is_bounded() {
        assert!(MAX_RIPUP_ROUNDS <= 4, "rip-up must stay bounded");
        assert!(MAX_RIPUP_ROUNDS >= 1);
    }

    #[test]
    fn same_face_east_hosts_share_max_order_plus_one_main() {
        let (plan, graph) = two_col_plan_graph();
        let (sub, idx) = derive_root_substrate(&plan);
        let mut ports = BTreeMap::new();
        ports.insert("ew".into(), east_ports());
        let (start, goal) = host_tracks_for_edge(&idx, &plan, &graph, &ports, "ew").unwrap();
        let start_t = sub.track(start).unwrap();
        let goal_t = sub.track(goal).unwrap();
        assert_eq!(start_t.orient, TrackOrient::Main);
        assert_eq!(goal_t.orient, TrackOrient::Main);
        // max(0,1)+1 = 2
        assert_eq!(start_t.line, 2);
        assert_eq!(goal_t.line, 2);
        // Per-end legacy would split Main1 vs Main2.
        let legacy_src = idx.resolve_host_track(0, 0, super::super::substrate::PortSide::CrossHigh);
        let legacy_tgt = idx.resolve_host_track(1, 1, super::super::substrate::PortSide::CrossHigh);
        assert_ne!(legacy_src, legacy_tgt);
        assert_eq!(start_t.line, legacy_tgt.and_then(|id| sub.track(id)).unwrap().line);
    }

    #[test]
    fn same_face_west_hosts_share_min_order_main() {
        let (plan, graph) = two_col_plan_graph();
        let (sub, idx) = derive_root_substrate(&plan);
        let mut ports = BTreeMap::new();
        ports.insert("ew".into(), west_ports());
        let (start, goal) = host_tracks_for_edge(&idx, &plan, &graph, &ports, "ew").unwrap();
        let start_t = sub.track(start).unwrap();
        let goal_t = sub.track(goal).unwrap();
        assert_eq!(start_t.orient, TrackOrient::Main);
        assert_eq!(goal_t.orient, TrackOrient::Main);
        // min(0,1) = 0
        assert_eq!(start_t.line, 0);
        assert_eq!(goal_t.line, 0);
    }

    #[test]
    fn route_order_ignores_declaration_shuffle() {
        // Same topology keys, opposite declaration indices → identical order.
        let a = vec![
            RouteOrderEntry {
                edge_id: "short".into(),
                critical: false,
                span: 1,
                reversed: false,
                dummy_len: 0,
                decl_index: 0,
            },
            RouteOrderEntry {
                edge_id: "long_rev".into(),
                critical: false,
                span: 4,
                reversed: true,
                dummy_len: 3,
                decl_index: 1,
            },
        ];
        let b = vec![
            RouteOrderEntry {
                edge_id: "long_rev".into(),
                critical: false,
                span: 4,
                reversed: true,
                dummy_len: 3,
                decl_index: 0,
            },
            RouteOrderEntry {
                edge_id: "short".into(),
                critical: false,
                span: 1,
                reversed: false,
                dummy_len: 0,
                decl_index: 1,
            },
        ];
        let oa: Vec<_> = compute_route_order(a)
            .into_iter()
            .map(|e| e.edge_id)
            .collect();
        let ob: Vec<_> = compute_route_order(b)
            .into_iter()
            .map(|e| e.edge_id)
            .collect();
        assert_eq!(oa, ob);
        assert_eq!(oa, vec!["long_rev".to_string(), "short".to_string()]);
    }

    #[test]
    fn route_order_critical_outranks_span() {
        let entries = vec![
            RouteOrderEntry {
                edge_id: "long".into(),
                critical: false,
                span: 9,
                reversed: false,
                dummy_len: 8,
                decl_index: 0,
            },
            RouteOrderEntry {
                edge_id: "crit".into(),
                critical: true,
                span: 1,
                reversed: false,
                dummy_len: 0,
                decl_index: 1,
            },
        ];
        let ids: Vec<_> = compute_route_order(entries)
            .into_iter()
            .map(|e| e.edge_id)
            .collect();
        assert_eq!(ids, vec!["crit".to_string(), "long".to_string()]);
    }
}
