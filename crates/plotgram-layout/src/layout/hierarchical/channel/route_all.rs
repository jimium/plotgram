//! Route every non-bus edge on the Channel graph (D1.2: Gate + rip-up).

use std::collections::BTreeMap;

use plotgram_algo::orientation::Side;
use plotgram_engine_api::LayoutError;
use plotgram_model::diagnostics::Relaxation;

use super::derive::derive_substrate;
use super::graph::{ChannelGraph, Occupancy};
use super::search::{route_edge, ChannelPath, LexCost, RouteHints, ScopeMask};
use super::substrate::{BlueprintIndex, PortSide, Substrate, TrackId};
use crate::layout::hierarchical::compose::bundle::{end_bus_edge_ids, BundlePlan};
use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealEdge, RealGraph};
use crate::layout::hierarchical::params::HierarchicalParams;

const MAX_RIPUP_ROUNDS: u32 = 2;
const MAX_RIPUP_EDGES: usize = 8;

/// Orthogonal Channel topology for one edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteTopology {
    Orthogonal(ChannelPath),
}

/// Full D1.2 route plan: substrate + per-edge topology + BundlePlan facts.
#[derive(Debug, Clone)]
pub struct ChannelRoutePlan {
    pub substrate: Substrate,
    pub index: BlueprintIndex,
    /// edge_id → topology (end-bus members omitted — Ink joins BundlePlan).
    pub routes: BTreeMap<String, RouteTopology>,
    /// End-bus bundles from Compose (`auto_edge_grouping`).
    pub bundles: Vec<BundlePlan>,
    /// Soft relaxations produced by bounded rip-up.
    pub relaxations: Vec<Relaxation>,
    /// True when group-cut Gate IR was used (false = root-scope fallback).
    pub used_gates: bool,
}

fn layer_order(plan: &PlanGraph, elem: usize) -> usize {
    let rank = plan.elems[elem].rank as usize;
    plan.layers[rank]
        .iter()
        .position(|&e| e == elem)
        .expect("elem must be in its layer")
}

fn host_track_for_end(
    index: &BlueprintIndex,
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    edge_id: &str,
    at_source: bool,
) -> Result<TrackId, LayoutError> {
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
    let _ = Side::North;
    index.resolve_host_track(rank, order, side).ok_or_else(|| {
        LayoutError::message(format!(
            "channel: no host track for edge `{edge_id}` {} side {:?}",
            if at_source { "source" } else { "target" },
            side
        ))
    })
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

fn hints_for_edge(params: &HierarchicalParams, edge: &RealEdge, order_count: usize) -> RouteHints {
    let pitch = params.edge_gap.max(1e-9);
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
        prefer_outer_main: edge.reversed,
        order_count,
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

struct EdgeRouteState {
    path: ChannelPath,
    cost: LexCost,
    failure_count: u32,
    critical: bool,
    decl_index: usize,
    start: TrackId,
    goal: TrackId,
    mask: ScopeMask,
    hints: RouteHints,
}

/// Derive substrate and route all non-end-bus edges (declaration order + rip-up).
pub fn route_edges_channel(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    end_bundles: &[BundlePlan],
    params: &HierarchicalParams,
) -> Result<ChannelRoutePlan, LayoutError> {
    let (substrate, index, used_gates) = derive_substrate(plan, graph)?;
    let channel_graph = ChannelGraph::from_substrate(&substrate);
    let mut occupancy = Occupancy::new();

    let bus_edges = end_bus_edge_ids(end_bundles);

    let mut states: BTreeMap<String, EdgeRouteState> = BTreeMap::new();
    for (decl_index, e) in graph.edges.iter().enumerate() {
        if bus_edges.contains(&e.edge_id) {
            continue;
        }
        let start = host_track_for_end(&index, plan, graph, ports, &e.edge_id, true)?;
        let goal = host_track_for_end(&index, plan, graph, ports, &e.edge_id, false)?;
        let mask = scope_mask_for_edge(&substrate, &index, graph, e);
        let hints = hints_for_edge(params, e, index.order_count);
        let outcome = route_edge(
            &channel_graph,
            start,
            goal,
            &occupancy,
            true,
            &mask,
            hints,
        );
        if !outcome.feasible {
            return Err(LayoutError::message(format!(
                "channel: no path for edge `{}` (Infeasible)",
                e.edge_id
            )));
        }
        occupancy.commit(&outcome.path.tracks, &outcome.path.gates);
        states.insert(
            e.edge_id.clone(),
            EdgeRouteState {
                path: outcome.path,
                cost: outcome.cost,
                failure_count: 0,
                critical: e.critical,
                decl_index,
                start,
                goal,
                mask,
                hints,
            },
        );
    }

    let relaxations = bounded_ripup(
        &channel_graph,
        &substrate,
        &mut occupancy,
        &mut states,
    );

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
        used_gates,
    })
}

fn bounded_ripup(
    graph: &ChannelGraph<'_>,
    substrate: &Substrate,
    occupancy: &mut Occupancy,
    states: &mut BTreeMap<String, EdgeRouteState>,
) -> Vec<Relaxation> {
    let mut relaxations = Vec::new();
    let (peak0, _) = occupancy_peak_sum(occupancy, substrate);
    if peak0 <= 1 {
        return relaxations;
    }

    for round in 0..MAX_RIPUP_ROUNDS {
        let (peak, _) = occupancy_peak_sum(occupancy, substrate);
        if peak <= 1 {
            break;
        }

        // Peak tracks → candidates on them, sorted by D1.2 key then LexCost.
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
        on_peak.sort_by(|a, b| {
            let sa = &states[a];
            let sb = &states[b];
            // Prefer ripping non-critical first so critical stay on preferred paths;
            // selection key: failure_count desc, edge_priority(critical) desc, decl, id.
            sb.failure_count
                .cmp(&sa.failure_count)
                .then_with(|| (sb.critical as u8).cmp(&(sa.critical as u8)))
                .then_with(|| sa.decl_index.cmp(&sb.decl_index))
                .then_with(|| a.cmp(b))
                .then_with(|| sb.cost.cmp(&sa.cost))
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

    relaxations
}

#[cfg(test)]
mod tests {
    use super::MAX_RIPUP_ROUNDS;

    #[test]
    fn ripup_budget_is_bounded() {
        assert!(MAX_RIPUP_ROUNDS <= 4, "rip-up must stay bounded");
        assert!(MAX_RIPUP_ROUNDS >= 1);
    }
}
