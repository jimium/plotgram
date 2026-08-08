//! Lexicographic Dijkstra on the ChannelGraph
//! (`bends ≻ length ≻ span_affinity ≻ congestion`).
//!
//! ScopeMask hard-filters foreign group scopes (L8). Gate full → transition gone.
//! Span affinity (D1.3.1): prefer tracks near the edge's rank/order span so
//! Cross line 0 / far Main corridors are not free when an interior seam exists.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use super::graph::{ChannelGraph, Occupancy, Via};
use super::substrate::{GateId, GroupId, Substrate, TrackId, TrackOrient};

/// Ordered path of substrate tracks (L2 topology).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPath {
    pub tracks: Vec<TrackId>,
    /// Gates traversed in order (deduped per edge at occupancy commit).
    pub gates: Vec<GateId>,
}

/// L8 scope mask: allow `{None} ∪ chain(u) ∪ chain(v)` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeMask {
    allowed: BTreeSet<Option<GroupId>>,
}

impl ScopeMask {
    pub fn unrestricted() -> Self {
        // Empty set means "no filter" for root-scope graphs with no groups.
        // When groups exist, use [`Self::for_scopes`].
        Self {
            allowed: BTreeSet::new(),
        }
    }

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

    pub fn allows(&self, scope: Option<GroupId>) -> bool {
        // Unrestricted sentinel: no groups registered → allow all.
        if self.allowed.is_empty() {
            return true;
        }
        self.allowed.contains(&scope)
    }
}

/// Endpoint-induced corridor band for span affinity (D1.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanAffinity {
    pub src_rank: usize,
    pub tgt_rank: usize,
    pub src_order: usize,
    pub tgt_order: usize,
}

/// Soft search preferences (D1.2 params + D1.3 span / inner-corridor).
#[derive(Debug, Clone, Copy, Default)]
pub struct RouteHints {
    /// Minimum first-segment length in logical span units (0 = off).
    pub min_first_span: f64,
    /// Minimum last-segment length in logical span units (0 = off).
    pub min_last_span: f64,
    /// When true (default for production routes), outer Main corridors
    /// (`og == 0` / `og == order_count`) are overflow capacity: penalized
    /// while any inner band gap is still free (D1.3.2). Replaces the old
    /// `prefer_outer_main` preference.
    pub outer_main_as_overflow: bool,
    /// Order-gap count: Main lines are `0..=order_count`.
    pub order_count: usize,
    /// When set, accumulate per-track span distance into [`LexCost::span_affinity`].
    pub span: Option<SpanAffinity>,
}

/// Soft congestion added to an outer Main track while an inner band gap is free.
const OUTER_OVERFLOW_PENALTY: f64 = 4.0;
/// Stronger overflow when both ends share one order (leaf column): packing
/// places Main og=0 / og=order_count on the canvas rim even though affinity
/// still treats them as in-band.
const SAME_ORDER_OUTER_OVERFLOW_PENALTY: f64 = 24.0;
/// Inner Main gap is "occupied" once demand reaches this (overflow unlocked).
const INNER_SATURATION_DEMAND: u32 = 1;

/// Lexicographic cost: bends ≻ length ≻ span_affinity ≻ congestion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LexCost {
    pub bends: u32,
    pub length: f64,
    /// Sum of per-track distances from the edge's natural rank/order band.
    pub span_affinity: u32,
    pub congestion: f64,
}

impl Default for LexCost {
    fn default() -> Self {
        Self {
            bends: 0,
            length: 0.0,
            span_affinity: 0,
            congestion: 0.0,
        }
    }
}

impl Eq for LexCost {}

impl PartialOrd for LexCost {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LexCost {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bends
            .cmp(&other.bends)
            .then_with(|| self.length.total_cmp(&other.length))
            .then_with(|| self.span_affinity.cmp(&other.span_affinity))
            .then_with(|| self.congestion.total_cmp(&other.congestion))
    }
}

/// Cross line `k` distance to the edge's natural layer-gap band.
///
/// `k=0` is above L0; `k∈[1,R]` sits between L(k-1) and L(k); `k=R+1` is below
/// the last layer. Strict interior seams `[lo+1, hi]` cost 0; endpoint-adjacent
/// seams `{lo, hi+1}` cost 1; farther lines cost `1 + dist` to `[lo, hi+1]`.
pub fn cross_span_dist(line: usize, src_rank: usize, tgt_rank: usize) -> u32 {
    let lo = src_rank.min(tgt_rank);
    let hi = src_rank.max(tgt_rank);
    if line >= lo.saturating_add(1) && line <= hi {
        return 0;
    }
    let band_lo = lo;
    let band_hi = hi + 1;
    if line == band_lo || line == band_hi {
        return 1;
    }
    if line < band_lo {
        1 + (band_lo - line) as u32
    } else {
        1 + (line - band_hi) as u32
    }
}

/// Main order-gap `og` distance to the interval covering both endpoint columns.
///
/// Zero when `og ∈ [order_lo, order_hi+1]` (inner faces inclusive).
pub fn main_span_dist(order_gap: usize, src_order: usize, tgt_order: usize) -> u32 {
    let order_lo = src_order.min(tgt_order);
    let order_hi = src_order.max(tgt_order);
    let band_hi = order_hi + 1;
    if order_gap >= order_lo && order_gap <= band_hi {
        return 0;
    }
    if order_gap < order_lo {
        (order_lo - order_gap) as u32
    } else {
        (order_gap - band_hi) as u32
    }
}

fn track_span_affinity(substrate: &Substrate, track: TrackId, hints: &RouteHints) -> u32 {
    let Some(span) = hints.span else {
        return 0;
    };
    let Some(t) = substrate.track(track) else {
        return 0;
    };
    match t.orient {
        TrackOrient::Cross => cross_span_dist(t.line, span.src_rank, span.tgt_rank),
        TrackOrient::Main => main_span_dist(t.line, span.src_order, span.tgt_order),
    }
}

/// True when every *inner* Main gap in the edge's order band already has demand.
///
/// Inner = `og ∈ [order_lo, order_hi+1]` excluding the stack-outer lines
/// `0` and `order_count`. No such gap → treat as saturated (outer is the only
/// face in band; do not penalize).
fn inner_main_saturated(
    substrate: &Substrate,
    occupancy: &Occupancy,
    span: SpanAffinity,
    order_count: usize,
) -> bool {
    let order_lo = span.src_order.min(span.tgt_order);
    let order_hi = span.src_order.max(span.tgt_order);
    for og in order_lo..=order_hi.saturating_add(1) {
        if og == 0 || og == order_count {
            continue;
        }
        if main_line_demand(substrate, occupancy, og) < INNER_SATURATION_DEMAND {
            return false;
        }
    }
    true
}

fn main_line_demand(substrate: &Substrate, occupancy: &Occupancy, order_gap: usize) -> u32 {
    substrate
        .tracks()
        .filter(|t| t.orient == TrackOrient::Main && t.line == order_gap)
        .map(|t| occupancy.lane_demand(t.id))
        .max()
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteOutcome {
    pub path: ChannelPath,
    pub cost: LexCost,
    pub feasible: bool,
}

impl RouteOutcome {
    pub fn infeasible() -> Self {
        Self {
            path: ChannelPath {
                tracks: Vec::new(),
                gates: Vec::new(),
            },
            cost: LexCost::default(),
            feasible: false,
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
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| other.track.cmp(&self.track))
    }
}

/// Route from start track to goal track under ScopeMask + hints.
pub fn route_edge(
    graph: &ChannelGraph<'_>,
    start: TrackId,
    goal: TrackId,
    occupancy: &Occupancy,
    congestion_bias: bool,
    allowed: &ScopeMask,
    hints: RouteHints,
) -> RouteOutcome {
    let substrate = graph.substrate();
    let Some(start_t) = substrate.track(start) else {
        return RouteOutcome::infeasible();
    };
    if !allowed.allows(start_t.scope) {
        return RouteOutcome::infeasible();
    }
    let mut start_cost = LexCost {
        length: start_t.span_weight,
        span_affinity: track_span_affinity(substrate, start, &hints),
        congestion: if congestion_bias {
            occupancy.lane_demand(start) as f64
        } else {
            0.0
        },
        ..LexCost::default()
    };
    start_cost.congestion += soft_penalties(substrate, start, &hints, occupancy);

    if start == goal {
        let mut cost = start_cost;
        if hints.min_last_span > 0.0 && start_t.span_weight + 1e-9 < hints.min_last_span {
            cost.congestion += 1e6;
        }
        return RouteOutcome {
            path: ChannelPath {
                tracks: vec![start],
                gates: Vec::new(),
            },
            cost,
            feasible: true,
        };
    }

    let mut open = BinaryHeap::new();
    open.push(State {
        cost: start_cost,
        track: start,
    });
    // best: track → (cost, parent track, via used to enter)
    let mut best: BTreeMap<TrackId, (LexCost, Option<TrackId>, Option<Via>)> = BTreeMap::new();
    best.insert(start, (start_cost, None, None));

    while let Some(State { cost, track }) = open.pop() {
        if let Some((bc, _, _)) = best.get(&track) {
            if cost > *bc {
                continue;
            }
        }
        if track == goal {
            let (tracks, gates) = reconstruct(&best, goal);
            let mut final_cost = cost;
            if hints.min_last_span > 0.0 {
                if let Some(gt) = substrate.track(goal) {
                    if gt.span_weight + 1e-9 < hints.min_last_span {
                        final_cost.congestion += 1e6;
                    }
                }
            }
            return RouteOutcome {
                path: ChannelPath { tracks, gates },
                cost: final_cost,
                feasible: true,
            };
        }
        let from_orient = match substrate.track(track) {
            Some(t) => t.orient,
            None => continue,
        };
        let leaving_start = track == start && cost.bends == 0;
        for tr in graph.neighbors(track) {
            if let Via::Gate(g) = tr.via {
                if !occupancy.gate_open(substrate, g) {
                    continue;
                }
            }
            let Some(to_t) = substrate.track(tr.to) else {
                continue;
            };
            if !allowed.allows(to_t.scope) {
                continue;
            }
            let mut next = cost;
            match tr.via {
                Via::Link if from_orient != to_t.orient => {
                    next.bends += 1;
                    if leaving_start
                        && hints.min_first_span > 0.0
                        && start_t.span_weight + 1e-9 < hints.min_first_span
                    {
                        next.congestion += 1e6;
                    }
                }
                Via::Gate(_) if from_orient != to_t.orient => {
                    next.bends += 1;
                }
                _ => {}
            }
            next.length += to_t.span_weight;
            next.span_affinity =
                next.span_affinity.saturating_add(track_span_affinity(substrate, tr.to, &hints));
            if congestion_bias {
                next.congestion += occupancy.lane_demand(tr.to) as f64;
            }
            next.congestion += soft_penalties(substrate, tr.to, &hints, occupancy);
            let replace = match best.get(&tr.to) {
                None => true,
                Some((bc, _, _)) => next < *bc,
            };
            if replace {
                best.insert(tr.to, (next, Some(track), Some(tr.via)));
                open.push(State {
                    cost: next,
                    track: tr.to,
                });
            }
        }
    }
    RouteOutcome::infeasible()
}

/// True when `path` uses an outer Main corridor while an inner band gap is
/// still free (the soft overflow penalty would fire). Diagnostics only —
/// does not change search costs or geometry.
pub fn path_used_outer_overflow(
    substrate: &Substrate,
    path: &ChannelPath,
    hints: &RouteHints,
    occupancy: &Occupancy,
) -> bool {
    path.tracks
        .iter()
        .any(|&tid| soft_penalties(substrate, tid, hints, occupancy) > 0.0)
}

fn soft_penalties(
    substrate: &Substrate,
    track: TrackId,
    hints: &RouteHints,
    occupancy: &Occupancy,
) -> f64 {
    if !hints.outer_main_as_overflow {
        return 0.0;
    }
    let Some(span) = hints.span else {
        return 0.0;
    };
    let Some(t) = substrate.track(track) else {
        return 0.0;
    };
    if t.orient != TrackOrient::Main {
        return 0.0;
    }
    let outer = t.line == 0 || t.line == hints.order_count;
    if !outer {
        return 0.0;
    }
    if inner_main_saturated(substrate, occupancy, span, hints.order_count) {
        0.0
    } else if span.src_order == span.tgt_order {
        SAME_ORDER_OUTER_OVERFLOW_PENALTY
    } else {
        OUTER_OVERFLOW_PENALTY
    }
}

fn reconstruct(
    best: &BTreeMap<TrackId, (LexCost, Option<TrackId>, Option<Via>)>,
    goal: TrackId,
) -> (Vec<TrackId>, Vec<GateId>) {
    let mut cur = goal;
    let mut rev_tracks = vec![cur];
    let mut rev_gates = Vec::new();
    while let Some((_, Some(parent), via)) = best.get(&cur) {
        if let Some(Via::Gate(g)) = via {
            rev_gates.push(*g);
        }
        cur = *parent;
        rev_tracks.push(cur);
    }
    rev_tracks.reverse();
    rev_gates.reverse();
    (rev_tracks, rev_gates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::graph::{ChannelGraph, Occupancy};
    use super::super::substrate::{derive_root_substrate, PortSide};
    use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph};

    fn plan_chain() -> PlanGraph {
        let elems = (0..3)
            .map(|i| Elem {
                key: ElemKey::Real(format!("n{i}")),
                group_path: vec![],
                rank: i,
            })
            .collect::<Vec<_>>();
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        PlanGraph {
            elems,
            index_of,
            decl_index: (0..3).collect(),
            segments: vec![],
            layers: vec![vec![0], vec![1], vec![2]],
        }
    }

    #[test]
    fn adjacent_layer_same_column_is_single_cross_track() {
        let plan = plan_chain();
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        let start = idx.resolve_host_track(0, 0, PortSide::MainHigh).unwrap();
        let goal = idx.resolve_host_track(1, 0, PortSide::MainLow).unwrap();
        assert_eq!(start, goal);
        let out = route_edge(
            &g,
            start,
            goal,
            &Occupancy::new(),
            true,
            &ScopeMask::unrestricted(),
            RouteHints::default(),
        );
        assert!(out.feasible);
        assert_eq!(out.path.tracks, vec![start]);
        assert_eq!(out.cost.bends, 0);
    }

    #[test]
    fn long_span_routes_via_main_corridor() {
        let plan = plan_chain();
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        let start = idx.resolve_host_track(0, 0, PortSide::MainHigh).unwrap();
        let goal = idx.resolve_host_track(2, 0, PortSide::MainLow).unwrap();
        assert_ne!(start, goal);
        let hints = RouteHints {
            span: Some(SpanAffinity {
                src_rank: 0,
                tgt_rank: 2,
                src_order: 0,
                tgt_order: 0,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
            ..RouteHints::default()
        };
        let out = route_edge(
            &g,
            start,
            goal,
            &Occupancy::new(),
            true,
            &ScopeMask::unrestricted(),
            hints,
        );
        assert!(out.feasible, "long span must find a path");
        assert!(
            out.path.tracks.len() >= 3,
            "expected Cross→Main→Cross, got {:?}",
            out.path.tracks
        );
        assert!(out.cost.bends >= 2);
        // Interior Cross lines only (not stack-top k=0).
        for &tid in &out.path.tracks {
            let t = sub.track(tid).unwrap();
            if t.orient == TrackOrient::Cross {
                assert_ne!(t.line, 0, "must not rush to Cross line 0: {:?}", out.path.tracks);
            }
        }
    }

    #[test]
    fn cross_span_dist_table() {
        // Mid-graph multi-rank: lo=1, hi=5 → interior {2,3,4,5}, adjacent {1,6}.
        let cases = [
            (0usize, 2u32), // above band
            (1, 1),
            (2, 0),
            (5, 0),
            (6, 1),
            (7, 2),
        ];
        for (line, want) in cases {
            assert_eq!(
                cross_span_dist(line, 1, 5),
                want,
                "cross_span_dist({line}, 1, 5)"
            );
        }
        // Adjacent ranks lo=0, hi=1 → interior {1}, adjacent {0,2}.
        assert_eq!(cross_span_dist(0, 0, 1), 1);
        assert_eq!(cross_span_dist(1, 0, 1), 0);
        assert_eq!(cross_span_dist(2, 0, 1), 1);
        assert_eq!(cross_span_dist(3, 0, 1), 2);
    }

    #[test]
    fn main_span_dist_table() {
        // Orders 0 and 2 → zero band [0, 3].
        assert_eq!(main_span_dist(0, 0, 2), 0);
        assert_eq!(main_span_dist(1, 0, 2), 0);
        assert_eq!(main_span_dist(3, 0, 2), 0);
        assert_eq!(main_span_dist(4, 0, 2), 1);
        // Same column order=1 → faces {1, 2}.
        assert_eq!(main_span_dist(0, 1, 1), 1);
        assert_eq!(main_span_dist(1, 1, 1), 0);
        assert_eq!(main_span_dist(2, 1, 1), 0);
        assert_eq!(main_span_dist(3, 1, 1), 1);
    }

    #[test]
    fn empty_graph_prefers_interior_cross_over_stack_top() {
        // Two columns × three ranks: route MainHigh@L0 → MainLow@L2 via a
        // Cross bridge; with span affinity the bridge must not be line 0.
        let elems = (0..6)
            .map(|i| Elem {
                key: ElemKey::Real(format!("n{i}")),
                group_path: vec![],
                rank: i / 2,
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
            layers: vec![vec![0, 1], vec![2, 3], vec![4, 5]],
        };
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        // Right column bottom face of L0 → left column top face of L2:
        // forces a Cross bridge between divergent Main hosts.
        let start = idx.resolve_host_track(0, 1, PortSide::MainHigh).unwrap();
        let goal = idx.resolve_host_track(2, 0, PortSide::MainLow).unwrap();
        let hints = RouteHints {
            span: Some(SpanAffinity {
                src_rank: 0,
                tgt_rank: 2,
                src_order: 1,
                tgt_order: 0,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
            ..RouteHints::default()
        };
        let out = route_edge(
            &g,
            start,
            goal,
            &Occupancy::new(),
            true,
            &ScopeMask::unrestricted(),
            hints,
        );
        assert!(out.feasible, "{out:?}");
        let cross_lines: Vec<usize> = out
            .path
            .tracks
            .iter()
            .filter_map(|&tid| {
                let t = sub.track(tid)?;
                (t.orient == TrackOrient::Cross).then_some(t.line)
            })
            .collect();
        assert!(
            !cross_lines.is_empty(),
            "expected a Cross bridge, tracks={:?}",
            out.path.tracks
        );
        assert!(
            cross_lines.iter().all(|&k| k != 0),
            "empty-graph span affinity must avoid Cross line 0, got {cross_lines:?}"
        );
    }

    #[test]
    fn same_column_long_edge_prefers_inner_main_not_og0() {
        // Left column of a 2-col × 3-rank grid: order-band [0,1] has outer og=0
        // and inner face og=1. Empty occupancy → InnerCorridorFirst must pick
        // Main og=1, not the canvas-left outer corridor.
        let elems = (0..6)
            .map(|i| Elem {
                key: ElemKey::Real(format!("n{i}")),
                group_path: vec![],
                rank: i / 2,
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
            layers: vec![vec![0, 1], vec![2, 3], vec![4, 5]],
        };
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        let start = idx.resolve_host_track(0, 0, PortSide::MainHigh).unwrap();
        let goal = idx.resolve_host_track(2, 0, PortSide::MainLow).unwrap();
        let hints = RouteHints {
            span: Some(SpanAffinity {
                src_rank: 0,
                tgt_rank: 2,
                src_order: 0,
                tgt_order: 0,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
            ..RouteHints::default()
        };
        let out = route_edge(
            &g,
            start,
            goal,
            &Occupancy::new(),
            true,
            &ScopeMask::unrestricted(),
            hints,
        );
        assert!(out.feasible, "{out:?}");
        let main_lines: Vec<usize> = out
            .path
            .tracks
            .iter()
            .filter_map(|&tid| {
                let t = sub.track(tid)?;
                (t.orient == TrackOrient::Main).then_some(t.line)
            })
            .collect();
        assert!(
            !main_lines.is_empty(),
            "expected a Main bridge, tracks={:?}",
            out.path.tracks
        );
        assert!(
            main_lines.iter().all(|&og| og != 0),
            "empty-graph inner-corridor must avoid Main og=0, got {main_lines:?}"
        );
    }

    #[test]
    fn same_order_outer_overflow_penalty_exceeds_generic() {
        // Leaf-column (same order) must punish packing-rim Mains harder than
        // cross-column edges; lex still bends≻length≻affinity≻congestion.
        assert!(SAME_ORDER_OUTER_OVERFLOW_PENALTY > OUTER_OVERFLOW_PENALTY);
        assert!(SAME_ORDER_OUTER_OVERFLOW_PENALTY >= 6.0 * OUTER_OVERFLOW_PENALTY);
    }

    #[test]
    fn same_order_avoids_right_outer_main_when_inner_free() {
        let elems = (0..6)
            .map(|i| Elem {
                key: ElemKey::Real(format!("n{i}")),
                group_path: vec![],
                rank: i / 2,
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
            layers: vec![vec![0, 1], vec![2, 3], vec![4, 5]],
        };
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        // Right column hosts: South of order-1 → North of order-1.
        let start = idx.resolve_host_track(0, 1, PortSide::MainHigh).unwrap();
        let goal = idx.resolve_host_track(2, 1, PortSide::MainLow).unwrap();
        let hints = RouteHints {
            span: Some(SpanAffinity {
                src_rank: 0,
                tgt_rank: 2,
                src_order: 1,
                tgt_order: 1,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
            ..RouteHints::default()
        };
        let out = route_edge(
            &g,
            start,
            goal,
            &Occupancy::new(),
            true,
            &ScopeMask::unrestricted(),
            hints,
        );
        assert!(out.feasible, "{out:?}");
        let main_lines: Vec<usize> = out
            .path
            .tracks
            .iter()
            .filter_map(|&tid| {
                let t = sub.track(tid)?;
                (t.orient == TrackOrient::Main).then_some(t.line)
            })
            .collect();
        assert!(
            main_lines.iter().all(|&og| og != idx.order_count),
            "same-order right column must avoid Main og=order_count, got {main_lines:?}"
        );
    }

    #[test]
    fn outer_main_free_when_inner_band_saturated() {
        // Same setup as above, but Main og=1 already occupied → outer og=0
        // is legitimate overflow (no hard failure; may select og=0).
        let elems = (0..6)
            .map(|i| Elem {
                key: ElemKey::Real(format!("n{i}")),
                group_path: vec![],
                rank: i / 2,
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
            layers: vec![vec![0, 1], vec![2, 3], vec![4, 5]],
        };
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        let inner = idx.main_at(1, 0).expect("inner Main og=1");
        let mut occ = Occupancy::new();
        occ.commit(&[inner], &[]);
        let start = idx.resolve_host_track(0, 0, PortSide::MainHigh).unwrap();
        let goal = idx.resolve_host_track(2, 0, PortSide::MainLow).unwrap();
        let hints = RouteHints {
            span: Some(SpanAffinity {
                src_rank: 0,
                tgt_rank: 2,
                src_order: 0,
                tgt_order: 0,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
            ..RouteHints::default()
        };
        let out = route_edge(
            &g,
            start,
            goal,
            &occ,
            true,
            &ScopeMask::unrestricted(),
            hints,
        );
        assert!(out.feasible, "overflow to outer Main must remain feasible");
    }
}
