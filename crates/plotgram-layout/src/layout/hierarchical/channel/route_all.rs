//! Route every non-bus edge on the Channel graph (D1.2 Gate + D1.3.3 RouteOrder).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use plotgram_engine_api::LayoutError;
use plotgram_model::diagnostics::Relaxation;

use super::derive::derive_substrate;
use super::graph::{ChannelGraph, Occupancy};
use super::search::{
    end_candidates, path_used_outer_overflow, route_edge, ChannelPath, CostWeights, EndCandidate,
    LexCost, RouteHints, ScopeMask, SpanAffinity,
};
use super::substrate::{derive_root_substrate, BlueprintIndex, PortSide, Substrate, TrackId};
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

fn layer_order(layer_pos: &[usize], elem: usize) -> usize {
    layer_pos[elem]
}

fn endpoint_side_rank_order(
    plan: &PlanGraph,
    graph: &RealGraph,
    edge_of: &BTreeMap<String, usize>,
    layer_pos: &[usize],
    ports: &BTreeMap<String, EdgePorts>,
    edge_id: &str,
    at_source: bool,
) -> Result<(PortSide, usize, usize), LayoutError> {
    let &ei = edge_of.get(edge_id).ok_or_else(|| {
        LayoutError::message(format!("channel: unknown edge `{edge_id}`"))
    })?;
    let edge = &graph.edges[ei];
    let node_idx = if at_source {
        edge.original_source
    } else {
        edge.original_target
    };
    let id = &graph.ids[node_idx];
    let elem = plan.index_of[&ElemKey::Real(id.clone())];
    let rank = plan.elems[elem].rank as usize;
    let order = layer_order(layer_pos, elem);
    let rp = &ports[edge_id];
    let side = if at_source {
        PortSide::from_algo_side(rp.source.side)
    } else {
        PortSide::from_algo_side(rp.target.side)
    };
    Ok((side, rank, order))
}

/// Real-sibling orders of `elem`'s rank that block a straight E/W escape
/// (dummies never block — the edge may pass its own chain column).
fn rank_real_sibling_orders(
    plan: &PlanGraph,
    layer_pos: &[usize],
    elem: usize,
) -> Vec<usize> {
    let rank = plan.elems[elem].rank as usize;
    plan.layers[rank]
        .iter()
        .copied()
        .filter(|&e| e != elem)
        .filter(|&e| matches!(plan.elems[e].key, ElemKey::Real(_)))
        .map(|e| layer_pos[e])
        .collect()
}

/// Enumerate per-end landing candidates (host track × escape) for one edge.
///
/// E/W ends enumerate every Main corridor on their normal side; `end_candidates`
/// allows straight escapes only on the outermost corridor (the only lane
/// Metric folds past every node face, hence normal-safe for any endpoint)
/// and prices the ViaGap fallback everywhere else, so the solver alone owns
/// the escape decision. Same-face edges must share **one** Main line: both
/// ends are restricted to corridors on the normal side of **both** faces
/// (E–E: `og ≥ max(order)+1`, W–W: `og ≤ min(order)`), and the returned
/// `couple_main_corridor` flag makes search solve per-`og` rather than
/// stitching distinct corridors. N/S ends keep their adjacent gap-line host.
fn edge_end_candidates(
    index: &BlueprintIndex,
    plan: &PlanGraph,
    graph: &RealGraph,
    edge_of: &BTreeMap<String, usize>,
    layer_pos: &[usize],
    ports: &BTreeMap<String, EdgePorts>,
    edge_id: &str,
) -> Result<(Vec<EndCandidate>, Vec<EndCandidate>, bool), LayoutError> {
    let (src_side, src_rank, src_order) =
        endpoint_side_rank_order(plan, graph, edge_of, layer_pos, ports, edge_id, true)?;
    let (tgt_side, tgt_rank, tgt_order) =
        endpoint_side_rank_order(plan, graph, edge_of, layer_pos, ports, edge_id, false)?;

    let &edge_idx = edge_of.get(edge_id).ok_or_else(|| {
        LayoutError::message(format!("channel: unknown edge `{edge_id}`"))
    })?;
    let blocked =
        |elem: usize| -> (std::collections::BTreeSet<usize>, std::collections::BTreeSet<usize>) {
            let mut east = std::collections::BTreeSet::new();
            let mut west = std::collections::BTreeSet::new();
            for s in rank_real_sibling_orders(plan, layer_pos, elem) {
                if s > layer_pos[elem] {
                    east.insert(s);
                } else {
                    west.insert(s);
                }
            }
            (east, west)
        };
    let (src_be, src_bw) = blocked(graph.edges[edge_idx].original_source);
    let (tgt_be, tgt_bw) = blocked(graph.edges[edge_idx].original_target);

    // Facing gap line per end (baseline decide_escape semantics, mirrors
    // Ink's gap_y default).
    let src_toward_higher = tgt_rank >= src_rank;
    let tgt_toward_higher = src_rank > tgt_rank;
    let mut starts = end_candidates(
        index,
        port_side_to_algo(src_side),
        src_rank,
        src_order,
        &src_be,
        &src_bw,
        index.order_count,
        src_toward_higher,
    );
    let mut goals = end_candidates(
        index,
        port_side_to_algo(tgt_side),
        tgt_rank,
        tgt_order,
        &tgt_be,
        &tgt_bw,
        index.order_count,
        tgt_toward_higher,
    );

    // Same-face edges share one Main corridor: keep only corridors on the
    // normal side of both faces, and couple search per og.
    let shared_ogs: Option<std::ops::RangeInclusive<usize>> = match (src_side, tgt_side) {
        (PortSide::CrossHigh, PortSide::CrossHigh) => {
            Some((src_order.max(tgt_order) + 1)..=index.order_count)
        }
        (PortSide::CrossLow, PortSide::CrossLow) => Some(0..=src_order.min(tgt_order)),
        _ => None,
    };
    let couple_main_corridor = shared_ogs.is_some();
    if let Some(ogs) = shared_ogs {
        let allowed: std::collections::BTreeSet<TrackId> = ogs
            .flat_map(|og| index.main_lines.get(&og))
            .flatten()
            .map(|sg| sg.id)
            .collect();
        starts.retain(|c| allowed.contains(&c.track));
        goals.retain(|c| allowed.contains(&c.track));
    }

    if starts.is_empty() || goals.is_empty() {
        return Err(LayoutError::message(format!(
            "channel: no host candidates for edge `{edge_id}` (src {src_side:?} rank{src_rank}, \
             tgt {tgt_side:?} rank{tgt_rank})"
        )));
    }
    Ok((starts, goals, couple_main_corridor))
}

fn port_side_to_algo(side: PortSide) -> plotgram_algo::orientation::Side {
    use plotgram_algo::orientation::Side;
    match side {
        PortSide::MainLow => Side::North,
        PortSide::MainHigh => Side::South,
        PortSide::CrossLow => Side::West,
        PortSide::CrossHigh => Side::East,
    }
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

fn endpoint_rank_order(
    plan: &PlanGraph,
    graph: &RealGraph,
    layer_pos: &[usize],
    node_idx: usize,
) -> (usize, usize) {
    let id = &graph.ids[node_idx];
    let elem = plan.index_of[&ElemKey::Real(id.clone())];
    let rank = plan.elems[elem].rank as usize;
    let order = layer_order(layer_pos, elem);
    (rank, order)
}

fn edge_rank_span(
    plan: &PlanGraph,
    graph: &RealGraph,
    layer_pos: &[usize],
    edge: &RealEdge,
) -> usize {
    let (sr, _) = endpoint_rank_order(plan, graph, layer_pos, edge.original_source);
    let (tr, _) = endpoint_rank_order(plan, graph, layer_pos, edge.original_target);
    sr.abs_diff(tr)
}

fn dummy_chain_len(segs_by_edge: &BTreeMap<String, Vec<usize>>, edge_id: &str) -> usize {
    segs_by_edge
        .get(edge_id)
        .map(|v| v.len().saturating_sub(1))
        .unwrap_or(0)
}

fn hints_for_edge(
    plan: &PlanGraph,
    graph: &RealGraph,
    layer_pos: &[usize],
    params: &HierarchicalParams,
    edge: &RealEdge,
    order_count: usize,
) -> RouteHints {
    let pitch = params.edge_gap.max(1e-9);
    let (src_rank, src_order) = endpoint_rank_order(plan, graph, layer_pos, edge.original_source);
    let (tgt_rank, tgt_order) = endpoint_rank_order(plan, graph, layer_pos, edge.original_target);
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
        couple_main_corridor: false,
        order_count,
        span: Some(SpanAffinity {
            src_rank,
            tgt_rank,
            src_order,
            tgt_order,
        }),
        weights: CostWeights::from_params(
            params.edge_gap,
            params.route_w_bend,
            params.route_w_len,
            params.route_w_cross,
        ),
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
    starts: Vec<EndCandidate>,
    goals: Vec<EndCandidate>,
    mask: ScopeMask,
    hints: RouteHints,
}

struct PreparedEdge {
    entry: RouteOrderEntry,
    starts: Vec<EndCandidate>,
    goals: Vec<EndCandidate>,
    mask: ScopeMask,
    hints: RouteHints,
}

/// Derive substrate and route all non-end-bus edges (RouteOrder + rip-up).
///
/// When group-cut gates make an edge infeasible, fall back to root-scope
/// substrate (same soft path as impure group rects) so the diagram still
/// layouts; `used_gates` is false and a `channel-group-fallback` relaxation
/// records the reason. (A per-edge scope widening was trialed in D₂.2b and
/// reverted: widened edges re-use root tracks on the group substrate, so
/// penetration counts stayed while bends/canvas regressed — see
/// group-frame-d2.md §8.12 落地记录.)
pub fn route_edges_channel(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    end_bundles: &[BundlePlan],
    params: &HierarchicalParams,
) -> Result<ChannelRoutePlan, LayoutError> {
    let (substrate, index, used_gates, relaxations) =
        if crate::layout::hierarchical::CHANNEL_FORCE_ROOT.get() {
            let (s, idx) = derive_root_substrate(plan);
            (
                s,
                idx,
                false,
                vec![Relaxation {
                    rule: "channel-group-fallback".into(),
                    detail: "forced root-scope after gate-route ink penetration".into(),
                }],
            )
        } else {
            derive_substrate(plan, graph)?
        };
    let try_gates = used_gates;
    match route_on_substrate(
        plan,
        graph,
        ports,
        end_bundles,
        params,
        substrate,
        index,
        used_gates,
        relaxations,
    ) {
        Ok(p) => Ok(p),
        Err(err) if try_gates => {
            let (substrate, index) = derive_root_substrate(plan);
            let relaxations = vec![Relaxation {
                rule: "channel-group-fallback".into(),
                detail: format!(
                    "group-gate routing infeasible ({err}); fell back to d1.3-root-scope"
                ),
            }];
            route_on_substrate(
                plan,
                graph,
                ports,
                end_bundles,
                params,
                substrate,
                index,
                false,
                relaxations,
            )
        }
        Err(err) => Err(err),
    }
}

fn route_on_substrate(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    end_bundles: &[BundlePlan],
    params: &HierarchicalParams,
    substrate: Substrate,
    index: BlueprintIndex,
    used_gates: bool,
    mut relaxations: Vec<Relaxation>,
) -> Result<ChannelRoutePlan, LayoutError> {
    let channel_graph = ChannelGraph::from_substrate(&substrate);
    let mut occupancy = Occupancy::new();

    let bus_edges = end_bus_edge_ids(end_bundles);
    let edge_of = graph.edge_index_map();
    let layer_pos = plan.layer_positions();
    let segs_by_edge = plan.segments_by_edge();

    let mut prepared: Vec<PreparedEdge> = Vec::new();
    for (decl_index, e) in graph.edges.iter().enumerate() {
        if bus_edges.contains(&e.edge_id) {
            continue;
        }
        let (starts, goals, couple_main_corridor) = edge_end_candidates(
            &index, plan, graph, &edge_of, &layer_pos, ports, &e.edge_id,
        )?;
        let mask = scope_mask_for_edge(&substrate, &index, graph, e);
        let mut hints = hints_for_edge(plan, graph, &layer_pos, params, e, index.order_count);
        hints.couple_main_corridor = couple_main_corridor;
        prepared.push(PreparedEdge {
            entry: RouteOrderEntry {
                edge_id: e.edge_id.clone(),
                critical: e.weight > 1.0,
                span: edge_rank_span(plan, graph, &layer_pos, e),
                reversed: e.reversed,
                dummy_len: dummy_chain_len(&segs_by_edge, &e.edge_id),
                decl_index,
            },
            starts,
            goals,
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
            &prep.starts,
            &prep.goals,
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
        occupancy.commit(&substrate, &outcome.path.tracks, &outcome.path.gates);
        states.insert(
            entry.edge_id.clone(),
            EdgeRouteState {
                path: outcome.path,
                cost: outcome.cost,
                failure_count: 0,
                critical: entry.critical,
                span: entry.span,
                decl_index: entry.decl_index,
                starts: prep.starts,
                goals: prep.goals,
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

        let mut peak_tracks = BTreeSet::new();
        for t in substrate.tracks() {
            if occupancy.lane_demand(t.id) == peak {
                peak_tracks.insert(t.id);
            }
        }

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
            let starts = st.starts.clone();
            let goals = st.goals.clone();
            let mask = st.mask.clone();
            let hints = st.hints;

            let (peak_before, sum_before) = occupancy_peak_sum(occupancy, substrate);
            occupancy.release(substrate, &old_path.tracks, &old_path.gates);

            let outcome = route_edge(graph, &starts, &goals, occupancy, true, &mask, hints);
            if !outcome.feasible {
                occupancy.commit(substrate, &old_path.tracks, &old_path.gates);
                if let Some(st) = states.get_mut(&eid) {
                    st.failure_count = st.failure_count.saturating_add(1);
                }
                continue;
            }

            let old_path_load = path_lane_load(occupancy, &old_path.tracks);
            let new_path_load = path_lane_load(occupancy, &outcome.path.tracks);
            occupancy.commit(substrate, &outcome.path.tracks, &outcome.path.gates);

            let (peak_after, sum_after) = occupancy_peak_sum(occupancy, substrate);
            // P5-4: accept also when scalar cost improves without raising peak.
            let accept = peak_after < peak_before
                || (peak_after == peak_before
                    && (sum_after < sum_before || outcome.cost < old_cost))
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
                occupancy.release(substrate, &outcome.path.tracks, &outcome.path.gates);
                occupancy.commit(substrate, &old_path.tracks, &old_path.gates);
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
    use super::{compute_route_order, edge_end_candidates, RouteOrderEntry, MAX_RIPUP_ROUNDS};
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
                weight: 1.0,
                ..Default::default()
            }],
            self_loops: vec![],
            ..Default::default()
        };
        (plan, graph)
    }

    #[test]
    fn ripup_budget_is_bounded() {
        assert!(MAX_RIPUP_ROUNDS <= 4, "rip-up must stay bounded");
        assert!(MAX_RIPUP_ROUNDS >= 1);
    }

    #[test]
    fn same_face_east_candidates_only_normal_side_corridors() {
        let (plan, graph) = two_col_plan_graph();
        let (sub, idx) = derive_root_substrate(&plan);
        let mut ports = BTreeMap::new();
        ports.insert("ew".into(), east_ports());
        let (starts, goals, couple) = edge_end_candidates(
            &idx,
            &plan,
            &graph,
            &graph.edge_index_map(),
            &plan.layer_positions(),
            &ports,
            "ew",
        ).unwrap();
        assert!(couple, "E–E must couple Main corridors");
        assert!(!starts.is_empty() && !goals.is_empty());
        // max(0,1)+1 = 2 is the only shared normal-side corridor (order_count=2).
        for c in starts.iter().chain(goals.iter()) {
            let t = sub.track(c.track).unwrap();
            assert_eq!(t.orient, TrackOrient::Main);
            assert_eq!(t.line, 2, "E–E candidates must stay on og ≥ max+1: {c:?}");
        }
    }

    #[test]
    fn same_face_west_candidates_only_normal_side_corridors() {
        let (plan, graph) = two_col_plan_graph();
        let (sub, idx) = derive_root_substrate(&plan);
        let mut ports = BTreeMap::new();
        ports.insert("ew".into(), west_ports());
        let (starts, goals, couple) = edge_end_candidates(
            &idx,
            &plan,
            &graph,
            &graph.edge_index_map(),
            &plan.layer_positions(),
            &ports,
            "ew",
        ).unwrap();
        assert!(couple, "W–W must couple Main corridors");
        assert!(!starts.is_empty() && !goals.is_empty());
        // min(0,1) = 0 is the only shared normal-side corridor.
        for c in starts.iter().chain(goals.iter()) {
            let t = sub.track(c.track).unwrap();
            assert_eq!(t.orient, TrackOrient::Main);
            assert_eq!(t.line, 0, "W–W candidates must stay on og ≤ min: {c:?}");
        }
    }

    #[test]
    fn same_face_east_with_multiple_ogs_routes_on_one_corridor() {
        // 3 columns → order_count=3; E–E from order0→order1 allows og∈{2,3}.
        // Coupled search must pick one shared line (no Main-line stitch).
        let elems = (0..6)
            .map(|i| Elem {
                key: ElemKey::Real(format!("n{i}")),
                group_path: vec![],
                rank: i / 3,
            })
            .collect::<Vec<_>>();
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: (0..6).collect(),
            segments: vec![],
            layers: vec![vec![0, 1, 2], vec![3, 4, 5]],
        };
        let mut ids = BTreeMap::new();
        for i in 0..6 {
            ids.insert(format!("n{i}"), i);
        }
        let graph = RealGraph {
            ids: (0..6).map(|i| format!("n{i}")).collect(),
            index_of: ids,
            group_path: vec![vec![]; 6],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 6],
            edges: vec![RealEdge {
                edge_id: "ew".into(),
                original_source: 0, // rank0 order0
                original_target: 4, // rank1 order1
                working_source: 0,
                working_target: 4,
                reversed: false,
                from_port: None,
                to_port: None,
                weight: 1.0,
                ..Default::default()
            }],
            self_loops: vec![],
            ..Default::default()
        };
        let (sub, idx) = derive_root_substrate(&plan);
        assert_eq!(idx.order_count, 3);
        let mut ports = BTreeMap::new();
        ports.insert("ew".into(), east_ports());
        let (starts, goals, couple) = edge_end_candidates(
            &idx,
            &plan,
            &graph,
            &graph.edge_index_map(),
            &plan.layer_positions(),
            &ports,
            "ew",
        )
        .unwrap();
        assert!(couple);
        let lines: std::collections::BTreeSet<usize> = starts
            .iter()
            .chain(goals.iter())
            .map(|c| sub.track(c.track).unwrap().line)
            .collect();
        assert_eq!(
            lines,
            [2usize, 3].into_iter().collect(),
            "normal-side pool must include both og=2 and rim og=3"
        );
        use super::super::graph::{ChannelGraph, Occupancy};
        use super::super::search::{route_edge, RouteHints, ScopeMask, SpanAffinity};
        let g = ChannelGraph::from_substrate(&sub);
        let hints = RouteHints {
            couple_main_corridor: true,
            outer_main_as_overflow: true,
            order_count: idx.order_count,
            span: Some(SpanAffinity {
                src_rank: 0,
                tgt_rank: 1,
                src_order: 0,
                tgt_order: 1,
            }),
            ..RouteHints::default()
        };
        let out = route_edge(
            &g,
            &starts,
            &goals,
            &Occupancy::new(),
            true,
            &ScopeMask::unrestricted(),
            hints,
        );
        assert!(out.feasible, "{out:?}");
        let main_lines: std::collections::BTreeSet<usize> = out
            .path
            .tracks
            .iter()
            .filter_map(|&tid| {
                let t = sub.track(tid).unwrap();
                (t.orient == TrackOrient::Main).then_some(t.line)
            })
            .collect();
        assert_eq!(
            main_lines.len(),
            1,
            "coupled E–E must use exactly one Main og, got {main_lines:?} path={:?}",
            out.path.tracks
        );
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
