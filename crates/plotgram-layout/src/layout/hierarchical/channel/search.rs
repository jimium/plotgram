//! Lexicographic Dijkstra on the ChannelGraph (bends > length > congestion).
//!
//! ScopeMask hard-filters foreign group scopes (L8). Gate full → transition gone.

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

/// Soft search preferences (D1.2 params).
#[derive(Debug, Clone, Copy, Default)]
pub struct RouteHints {
    /// Minimum first-segment length in logical span units (0 = off).
    pub min_first_span: f64,
    /// Minimum last-segment length in logical span units (0 = off).
    pub min_last_span: f64,
    /// Prefer outer Main corridors (back-edge / reversed edges).
    pub prefer_outer_main: bool,
    /// Order-gap count for outer preference (0..=order_count).
    pub order_count: usize,
}

/// Lexicographic cost: q3 bends, q4 length, q5 congestion soft preference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LexCost {
    pub bends: u32,
    pub length: f64,
    pub congestion: f64,
}

impl Default for LexCost {
    fn default() -> Self {
        Self {
            bends: 0,
            length: 0.0,
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
            .then_with(|| {
                self.length
                    .total_cmp(&other.length)
                    .then_with(|| self.congestion.total_cmp(&other.congestion))
            })
    }
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
        congestion: if congestion_bias {
            occupancy.lane_demand(start) as f64
        } else {
            0.0
        },
        ..LexCost::default()
    };
    start_cost.congestion += soft_penalties(substrate, start, &hints, true);

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
            if congestion_bias {
                next.congestion += occupancy.lane_demand(tr.to) as f64;
            }
            next.congestion += soft_penalties(substrate, tr.to, &hints, false);
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

fn soft_penalties(
    substrate: &Substrate,
    track: TrackId,
    hints: &RouteHints,
    _is_start: bool,
) -> f64 {
    if !hints.prefer_outer_main {
        return 0.0;
    }
    let Some(t) = substrate.track(track) else {
        return 0.0;
    };
    if t.orient != TrackOrient::Main {
        return 0.0;
    }
    // Prefer outer order-gaps 0 and order_count; penalize middle.
    let outer = t.line == 0 || t.line == hints.order_count;
    if outer {
        0.0
    } else {
        0.25
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
        let out = route_edge(
            &g,
            start,
            goal,
            &Occupancy::new(),
            true,
            &ScopeMask::unrestricted(),
            RouteHints::default(),
        );
        assert!(out.feasible, "long span must find a path");
        assert!(
            out.path.tracks.len() >= 3,
            "expected Cross→Main→Cross, got {:?}",
            out.path.tracks
        );
        assert!(out.cost.bends >= 2);
    }
}
