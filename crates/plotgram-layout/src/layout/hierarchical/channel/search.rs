//! Weighted-scalar Dijkstra on the ChannelGraph (P5-4).
//!
//! Primary key: `scalar = w_bend·bends + w_len·length + w_cross·crossings
//! + w_span·span_affinity + w_cong·congestion`. Tiebreak keeps the old
//! lexicographic order for determinism.
//!
//! ScopeMask hard-filters foreign group scopes (L8). Gates are unbounded.
//! Span affinity (D1.3.1): prefer tracks near the edge's rank/order span so
//! Cross line 0 / far Main corridors are not free when an interior seam exists.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use super::graph::{ChannelGraph, Occupancy, Via};
use super::substrate::{BlueprintIndex, GateId, GroupId, Substrate, TrackId, TrackOrient};
use plotgram_algo::orientation::Side;

/// How Ink enters/leaves the first/last corridor (P5-3). Channel search
/// picks the per-end option that minimizes `J` (write-authority §2.2);
/// Ink only matches — no frame scanning.
///
/// Side-aware semantics (Ink inserts a `port_stub` normal jog where the
/// escape move itself runs tangential to the node face):
/// - `ViaGap` on E/W ports: normal stub → drop to the gap line;
/// - `ViaGap` on N/S ports: the gap move already follows the normal;
/// - `AtPortNormal` on N/S ports: normal stub → jog onto the rail;
/// - `AtPortNormal` on E/W ports: the rail move already follows the normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeEnd {
    /// Leave/arrive along the port normal (E/W horizontal at port Y, N/S vertical).
    AtPortNormal,
    /// Drop to a layer-gap Cross line first (sibling escape).
    ViaGap(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EscapePlan {
    pub source: EscapeEnd,
    pub target: EscapeEnd,
}

impl EscapePlan {
    pub fn both_normal() -> Self {
        Self {
            source: EscapeEnd::AtPortNormal,
            target: EscapeEnd::AtPortNormal,
        }
    }
}

/// One selectable endpoint landing: host track + escape mode + the discrete
/// Ink expansion cost the option adds (`leave_to_main`/`arrive_from_main`
/// mirror, in gap/span units — only relative ordering matters; the existing
/// typed weights `w_bend`/`w_len` price it into `J`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndCandidate {
    pub track: TrackId,
    pub escape: EscapeEnd,
    /// Extra bends Ink emits at this end (stub + gap detour).
    pub extra_bends: u32,
    /// Extra escape length in order/rank-gap units.
    pub extra_len: f64,
}

impl Eq for EndCandidate {}

impl Ord for EndCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.track
            .cmp(&other.track)
            .then_with(|| self.extra_bends.cmp(&other.extra_bends))
            .then_with(|| self.extra_len.total_cmp(&other.extra_len))
            .then_with(|| escape_key(&self.escape).cmp(&escape_key(&other.escape)))
    }
}

impl PartialOrd for EndCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn escape_key(end: &EscapeEnd) -> (u8, usize) {
    match end {
        EscapeEnd::AtPortNormal => (0, 0),
        EscapeEnd::ViaGap(line) => (1, *line),
    }
}

/// Enumerate one endpoint's landing candidates (host track × escape).
///
/// Hard constraints drop infeasible options instead of predicate tables
/// (write-authority §2.2):
/// - `AtPortNormal` on E/W needs (a) no real sibling between the node and
///   `og` (the straight escape would pierce it) and (b) `og` to be the
///   outermost corridor (`order_count` east / `0` west): Metric folds the
///   rim lane over **all** ranks (max/min), so it sits past every node face
///   and is normal-safe for any endpoint; inner lanes sit at the *average*
///   of per-rank faces and can land on a port's anti-normal side;
/// - `ViaGap` on the gap line facing the other endpoint (`rank+1` when the
///   other end sits at a higher or equal rank, else `rank` — mirrors Ink's
///   `gap_y` default) is always feasible: the horizontal approach runs on the
///   rank-gap line, clear of node bodies. The opposite-facing line is never
///   enumerated: it is weakly dominated (longer approach, identical cost);
/// - N/S hosts sit on the adjacent gap line: the escape is already normal.
///
/// `blocked_east`/`blocked_west` list real-sibling orders of this endpoint's
/// rank (dummies do not block — Ink runs around them on the shared rail).
/// `toward_higher` says whether the other endpoint sits at a higher or equal
/// rank (selects the facing gap line, baseline `decide_escape` semantics).
pub fn end_candidates(
    index: &BlueprintIndex,
    side: Side,
    rank: usize,
    order: usize,
    blocked_east: &std::collections::BTreeSet<usize>,
    blocked_west: &std::collections::BTreeSet<usize>,
    order_count: usize,
    toward_higher: bool,
) -> Vec<EndCandidate> {
    let mut out: Vec<EndCandidate> = Vec::new();
    match side {
        Side::North | Side::South => {
            if let Some(tid) = index.resolve_host_track(
                rank,
                order,
                if side == Side::North {
                    super::substrate::PortSide::MainLow
                } else {
                    super::substrate::PortSide::MainHigh
                },
            ) {
                out.push(EndCandidate {
                    track: tid,
                    escape: EscapeEnd::AtPortNormal,
                    extra_bends: 0,
                    extra_len: 0.0,
                });
            }
        }
        Side::East => {
            for og in (order + 1)..=order_count {
                let Some(tid) = index.main_at(og, rank) else {
                    continue;
                };
                let face_dist = (og - (order + 1)) as f64;
                let pierced = ((order + 1)..og).any(|o| blocked_east.contains(&o));
                let rim = og == order_count;
                if !pierced && rim {
                    out.push(EndCandidate {
                        track: tid,
                        escape: EscapeEnd::AtPortNormal,
                        extra_bends: 0,
                        extra_len: face_dist,
                    });
                }
                let line = if toward_higher { rank + 1 } else { rank };
                out.push(EndCandidate {
                    track: tid,
                    escape: EscapeEnd::ViaGap(line),
                    extra_bends: 2,
                    extra_len: 1.0 + face_dist,
                });
            }
        }
        Side::West => {
            for og in 0..=order {
                let Some(tid) = index.main_at(og, rank) else {
                    continue;
                };
                let face_dist = (order - og) as f64;
                let pierced = ((og + 1)..=order).any(|o| blocked_west.contains(&o));
                let rim = og == 0;
                if !pierced && rim {
                    out.push(EndCandidate {
                        track: tid,
                        escape: EscapeEnd::AtPortNormal,
                        extra_bends: 0,
                        extra_len: face_dist,
                    });
                }
                let line = if toward_higher { rank + 1 } else { rank };
                out.push(EndCandidate {
                    track: tid,
                    escape: EscapeEnd::ViaGap(line),
                    extra_bends: 2,
                    extra_len: 1.0 + face_dist,
                });
            }
        }
    }
    out.sort();
    out
}

/// Ordered path of substrate tracks (L2 topology).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPath {
    pub tracks: Vec<TrackId>,
    /// Gates traversed in order (deduped per edge at occupancy commit).
    pub gates: Vec<GateId>,
    pub escape: EscapePlan,
}

impl ChannelPath {
    pub fn new(tracks: Vec<TrackId>, gates: Vec<GateId>) -> Self {
        Self {
            tracks,
            gates,
            escape: EscapePlan::both_normal(),
        }
    }
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
#[derive(Debug, Clone, Copy)]
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
    /// Weighted-scalar cost weights (P5-4).
    pub weights: CostWeights,
}

impl Default for RouteHints {
    fn default() -> Self {
        Self {
            min_first_span: 0.0,
            min_last_span: 0.0,
            outer_main_as_overflow: false,
            order_count: 0,
            span: None,
            weights: CostWeights::default(),
        }
    }
}

/// Soft congestion added to an outer Main track while an inner band gap is free.
const OUTER_OVERFLOW_PENALTY: f64 = 4.0;
/// Stronger overflow when both ends share one order (leaf column): packing
/// places Main og=0 / og=order_count on the canvas rim even though affinity
/// still treats them as in-band.
const SAME_ORDER_OUTER_OVERFLOW_PENALTY: f64 = 24.0;
/// Inner Main gap is "occupied" once demand reaches this (overflow unlocked).
const INNER_SATURATION_DEMAND: u32 = 1;

/// Typed route cost weights (defaults: `w_bend = 10·edge_gap`, `w_len = 1`,
/// `w_cross = 3·edge_gap`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostWeights {
    pub w_bend: f64,
    pub w_len: f64,
    pub w_cross: f64,
    pub w_span: f64,
    pub w_cong: f64,
}

impl CostWeights {
    pub fn from_edge_gap(edge_gap: f64) -> Self {
        let g = edge_gap.max(1e-9);
        Self {
            w_bend: 10.0 * g,
            w_len: 1.0,
            w_cross: 3.0 * g,
            w_span: g,
            w_cong: 1.0,
        }
    }

    pub fn from_params(edge_gap: f64, bend_factor: f64, w_len: f64, cross_factor: f64) -> Self {
        let g = edge_gap.max(1e-9);
        Self {
            w_bend: bend_factor.max(0.0) * g,
            w_len: w_len.max(0.0),
            w_cross: cross_factor.max(0.0) * g,
            w_span: g,
            w_cong: 1.0,
        }
    }
}

impl Default for CostWeights {
    fn default() -> Self {
        Self::from_edge_gap(16.0)
    }
}

/// Weighted scalar + lexicographic tiebreak (P5-4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LexCost {
    /// `w_bend·bends + w_len·length + w_cross·crossings + w_span·span + w_cong·cong`.
    pub scalar: f64,
    pub bends: u32,
    pub length: f64,
    /// Sum of per-track distances from the edge's natural rank/order band.
    pub span_affinity: u32,
    pub congestion: f64,
    /// Overlaps with committed track intervals encountered while expanding.
    pub crossings: u32,
}

impl LexCost {
    pub fn recompute(&mut self, w: &CostWeights) {
        self.scalar = w.w_bend * f64::from(self.bends)
            + w.w_len * self.length
            + w.w_cross * f64::from(self.crossings)
            + w.w_span * f64::from(self.span_affinity)
            + w.w_cong * self.congestion;
    }
}

impl Default for LexCost {
    fn default() -> Self {
        Self {
            scalar: 0.0,
            bends: 0,
            length: 0.0,
            span_affinity: 0,
            congestion: 0.0,
            crossings: 0,
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
        self.scalar
            .total_cmp(&other.scalar)
            .then_with(|| self.bends.cmp(&other.bends))
            .then_with(|| self.length.total_cmp(&other.length))
            .then_with(|| self.span_affinity.cmp(&other.span_affinity))
            .then_with(|| self.congestion.total_cmp(&other.congestion))
            .then_with(|| self.crossings.cmp(&other.crossings))
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
            path: ChannelPath::new(Vec::new(), Vec::new()),
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

/// Route between endpoint candidate sets under ScopeMask + hints.
///
/// Multi-source / multi-goal: every start candidate seeds the heap with its
/// Ink expansion cost; popping any goal track prices in that goal's candidate
/// cost, and the minimum total wins. The chosen candidates' escapes are
/// written into `ChannelPath.escape` — the search is the sole escape writer.
pub fn route_edge(
    graph: &ChannelGraph<'_>,
    starts: &[EndCandidate],
    goals: &[EndCandidate],
    occupancy: &Occupancy,
    congestion_bias: bool,
    allowed: &ScopeMask,
    hints: RouteHints,
) -> RouteOutcome {
    let substrate = graph.substrate();
    // Per-track best goal candidate (deterministic pick on duplicates).
    let mut goal_map: BTreeMap<TrackId, EndCandidate> = BTreeMap::new();
    for &g in goals {
        match goal_map.get(&g.track) {
            Some(cur) if (g.extra_bends, g.extra_len) >= (cur.extra_bends, cur.extra_len) => {}
            _ => {
                goal_map.insert(g.track, g);
            }
        }
    }
    if goal_map.is_empty() {
        return RouteOutcome::infeasible();
    }

    // Endpoint hosts keep the baseline scope contract (verify_route_scope
    // audits every committed track), but transit between adjacent endpoint
    // hosts is exempt: a host pair joined by one via always connects even
    // when the scope chain between them is cut.
    let direct_via = |a: TrackId, b: TrackId| -> Option<Via> {
        graph
            .neighbors(a)
            .iter()
            .find(|tr| tr.to == b)
            .map(|tr| tr.via)
    };

    let mut open = BinaryHeap::new();
    // best: track → (cost, parent track, via used to enter, start candidate idx)
    let mut best: BTreeMap<TrackId, (LexCost, Option<TrackId>, Option<Via>, usize)> =
        BTreeMap::new();

    for (si, sc) in starts.iter().enumerate() {
        let Some(start_t) = substrate.track(sc.track) else {
            continue;
        };
        if !allowed.allows(start_t.scope) {
            continue;
        }
        let mut start_cost = LexCost {
            bends: sc.extra_bends,
            length: start_t.span_weight + sc.extra_len,
            span_affinity: track_span_affinity(substrate, sc.track, &hints),
            congestion: if congestion_bias {
                occupancy.lane_demand(sc.track) as f64
            } else {
                0.0
            },
            crossings: occupancy.crossing_count(sc.track, start_t.ext),
            ..LexCost::default()
        };
        start_cost.congestion += soft_penalties(substrate, sc.track, &hints, occupancy);
        start_cost.recompute(&hints.weights);
        let replace = match best.get(&sc.track) {
            None => true,
            Some((bc, _, _, _)) => start_cost < *bc,
        };
        if replace {
            best.insert(sc.track, (start_cost, None, None, si));
            open.push(State {
                cost: start_cost,
                track: sc.track,
            });
        }
        // Direct start↔goal connection bypasses the transit scope filter.
        for (gt, via) in goal_map
            .keys()
            .filter(|&&gt| gt != sc.track)
            .filter_map(|&gt| direct_via(sc.track, gt).map(|via| (gt, via)))
        {
            if let Via::Gate(g) = via {
                if !occupancy.gate_open(substrate, g) {
                    continue;
                }
            }
            let Some(to_t) = substrate.track(gt) else {
                continue;
            };
            let mut direct = start_cost;
            if start_t.orient != to_t.orient {
                direct.bends += 1;
            }
            direct.length += to_t.span_weight;
            direct.span_affinity = direct
                .span_affinity
                .saturating_add(track_span_affinity(substrate, gt, &hints));
            direct.crossings = direct
                .crossings
                .saturating_add(occupancy.crossing_count(gt, to_t.ext));
            if congestion_bias {
                direct.congestion += occupancy.lane_demand(gt) as f64;
            }
            direct.congestion += soft_penalties(substrate, gt, &hints, occupancy);
            direct.recompute(&hints.weights);
            let replace = match best.get(&gt) {
                None => true,
                Some((bc, _, _, _)) => direct < *bc,
            };
            if replace {
                best.insert(gt, (direct, Some(sc.track), Some(via), si));
                open.push(State {
                    cost: direct,
                    track: gt,
                });
            }
        }
    }
    if best.is_empty() {
        return RouteOutcome::infeasible();
    }

    let finish_goal = |track: TrackId, mut cost: LexCost, goal: &EndCandidate| -> LexCost {
        cost.bends += goal.extra_bends;
        cost.length += goal.extra_len;
        if hints.min_last_span > 0.0
            && substrate
                .track(track)
                .is_some_and(|t| t.span_weight + 1e-9 < hints.min_last_span)
        {
            cost.congestion += 1e6;
        }
        cost.recompute(&hints.weights);
        cost
    };

    let mut best_goal: Option<(LexCost, TrackId)> = None;

    while let Some(State { cost, track }) = open.pop() {
        if let Some((bc, _, _, _)) = best.get(&track) {
            if cost > *bc {
                continue;
            }
        }
        if let Some(gc) = goal_map.get(&track) {
            let mut total = finish_goal(track, cost, gc);
            // Spike-fold discount (pair-level): when both ends escape via the
            // *same* gap line onto the *same* corridor through the minimal
            // Main→Cross→Main chain, Ink's out-and-back rail excursion sits on
            // one shared gap line and collapses to a removable spike — the
            // path renders just the two stub-corner bends (baseline e10
            // shape). The fold erases the 2 transit bends plus one rail-touch
            // bend per end; price the pair accordingly. Rim AtPortNormal
            // pairings never fold and keep their full price.
            if let Some(si) = best.get(&track).map(|e| e.3) {
                if let (EscapeEnd::ViaGap(sl), EscapeEnd::ViaGap(gl)) =
                    (starts[si].escape, gc.escape)
                {
                    let same_corridor = substrate
                        .track(starts[si].track)
                        .zip(substrate.track(gc.track))
                        .is_some_and(|(s, g)| s.orient == g.orient && s.line == g.line);
                    // Minimal chain: reach the start entry within 2 hops.
                    let mut hops = 0usize;
                    let mut cur = track;
                    let mut minimal = false;
                    loop {
                        match best.get(&cur) {
                            Some((_, None, _, _)) => {
                                minimal = true;
                                break;
                            }
                            Some((_, Some(p), _, _)) if hops < 2 => {
                                cur = *p;
                                hops += 1;
                            }
                            _ => break,
                        }
                    }
                    if sl == gl && same_corridor && minimal {
                        total.bends = total.bends.saturating_sub(4);
                        total.recompute(&hints.weights);
                    }
                }
            }
            let better = match &best_goal {
                None => true,
                Some((bt, _)) => total < *bt,
            };
            if better {
                best_goal = Some((total, track));
            }
        }
        // Prune: heap pops in ascending scalar, so nothing below remains.
        if let Some((bt, _)) = &best_goal {
            if cost.scalar >= bt.scalar {
                break;
            }
        }
        let from_orient = match substrate.track(track) {
            Some(t) => t.orient,
            None => continue,
        };
        let leaving_start = cost.bends == 0
            && best.get(&track).is_some_and(|e| e.1.is_none());
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
                        && substrate.track(track).is_some_and(|t| {
                            t.span_weight + 1e-9 < hints.min_first_span
                        })
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
            next.crossings = next
                .crossings
                .saturating_add(occupancy.crossing_count(tr.to, to_t.ext));
            if congestion_bias {
                next.congestion += occupancy.lane_demand(tr.to) as f64;
            }
            next.congestion += soft_penalties(substrate, tr.to, &hints, occupancy);
            next.recompute(&hints.weights);
            let replace = match best.get(&tr.to) {
                None => true,
                Some((bc, _, _, _)) => next < *bc,
            };
            if replace {
                let start_idx = best.get(&track).map(|e| e.3).unwrap_or(0);
                best.insert(tr.to, (next, Some(track), Some(tr.via), start_idx));
                open.push(State {
                    cost: next,
                    track: tr.to,
                });
            }
        }
    }

    let Some((final_cost, goal_track)) = best_goal else {
        return RouteOutcome::infeasible();
    };
    let (tracks, gates) = reconstruct(&best, goal_track);
    let start_idx = best.get(&tracks[0]).map(|e| e.3).unwrap_or(0);
    let path = ChannelPath {
        tracks,
        gates,
        escape: EscapePlan {
            source: starts[start_idx].escape,
            target: goal_map[&goal_track].escape,
        },
    };
    RouteOutcome {
        path,
        cost: final_cost,
        feasible: true,
    }
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
    best: &BTreeMap<TrackId, (LexCost, Option<TrackId>, Option<Via>, usize)>,
    goal: TrackId,
) -> (Vec<TrackId>, Vec<GateId>) {
    let mut cur = goal;
    let mut rev_tracks = vec![cur];
    let mut rev_gates = Vec::new();
    while let Some((_, Some(parent), via, _)) = best.get(&cur) {
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

    /// Single-candidate adapter for host-resolved tracks (legacy shape).
    fn normal_cand(tid: TrackId) -> Vec<EndCandidate> {
        vec![EndCandidate {
            track: tid,
            escape: EscapeEnd::AtPortNormal,
            extra_bends: 0,
            extra_len: 0.0,
        }]
    }

    fn no_blocked() -> std::collections::BTreeSet<usize> {
        std::collections::BTreeSet::new()
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
            &normal_cand(start),
            &normal_cand(goal),
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
            &normal_cand(start),
            &normal_cand(goal),
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
            &normal_cand(start),
            &normal_cand(goal),
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
            &normal_cand(start),
            &normal_cand(goal),
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
            &normal_cand(start),
            &normal_cand(goal),
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
        occ.commit(&sub, &[inner], &[]);
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
            &normal_cand(start),
            &normal_cand(goal),
            &occ,
            true,
            &ScopeMask::unrestricted(),
            hints,
        );
        assert!(out.feasible, "overflow to outer Main must remain feasible");
    }

    #[test]
    fn end_candidates_escape_choice_table() {
        // Escape option enumeration: normal-side + sibling hard constraints.
        // 2-col × 3-rank grid, endpoint order 0, East port.
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
        use plotgram_algo::orientation::Side;

        // Straight exits exist only on the rim corridor (og = order_count =
        // 2), gated by sibling piercing; inner corridors keep ViaGap only.
        let cases: [(std::collections::BTreeSet<usize>, bool); 2] = [
            (no_blocked(), true),
            ([1usize].into_iter().collect(), false),
        ];
        for (blocked, want_straight_rim) in cases {
            let cands = end_candidates(
                &idx,
                Side::East,
                1,
                0,
                &blocked,
                &no_blocked(),
                idx.order_count,
                true,
            );
            assert!(!cands.is_empty(), "East port must have candidates");
            let straight = |line: usize| {
                cands.iter().any(|c| {
                    c.escape == EscapeEnd::AtPortNormal
                        && sub.track(c.track).unwrap().line == line
                })
            };
            assert!(!straight(1), "inner lane is not normal-safe: {cands:?}");
            assert_eq!(
                straight(2),
                want_straight_rim,
                "rim straight exit, blocked={blocked:?}"
            );
            // ViaGap stays feasible (the fallback escape) on the facing gap
            // line (rank+1 for toward_higher) of every corridor.
            for line in [1usize, 2] {
                let gaps: Vec<usize> = cands
                    .iter()
                    .filter(|c| {
                        sub.track(c.track).unwrap().line == line
                    })
                    .filter_map(|c| match c.escape {
                        EscapeEnd::ViaGap(g) => Some(g),
                        _ => None,
                    })
                    .collect();
                assert_eq!(gaps, vec![2], "ViaGap fallback on og={line}: {cands:?}");
            }
        }
    }

    #[test]
    fn solver_prefers_normal_escape_over_gap_detour() {
        // e3 shape: single-node ranks, E–E back edge spanning all ranks.
        // Both ends have a free normal exit → the solver must pick
        // AtPortNormal at both ends (zero extra bends), not ViaGap detours.
        let plan = plan_chain();
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        use plotgram_algo::orientation::Side;
        // 1-column chain: the only Main line is og=1.
        let starts = end_candidates(
            &idx,
            Side::East,
            2,
            0,
            &no_blocked(),
            &no_blocked(),
            idx.order_count,
            false, // tgt rank 0 < src rank 2
        );
        let goals = end_candidates(
            &idx,
            Side::East,
            0,
            0,
            &no_blocked(),
            &no_blocked(),
            idx.order_count,
            true, // src rank 2 >= tgt rank 0
        );
        let hints = RouteHints {
            span: Some(SpanAffinity {
                src_rank: 2,
                tgt_rank: 0,
                src_order: 0,
                tgt_order: 0,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
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
        assert_eq!(
            out.path.escape.source,
            EscapeEnd::AtPortNormal,
            "single-node rank needs no gap detour: {:?}",
            out.path
        );
        assert_eq!(out.path.escape.target, EscapeEnd::AtPortNormal);
    }

    #[test]
    fn solver_picks_normal_side_corridor_for_same_face_back_edge() {
        // e7 shape: E–E edge whose endpoints sit at different orders; the
        // shared corridor must land on the normal side of both faces
        // (og ≥ max(order)+1) and escape straight when no sibling blocks.
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
        use plotgram_algo::orientation::Side;
        // Shared corridor of the E–E edge: og ≥ max(order)+1 = 2 (mirrors the
        // route_all filter). Both ends escape straight when no sibling blocks.
        let normal_side = |cs: Vec<EndCandidate>| {
            cs.into_iter()
                .filter(|c| sub.track(c.track).unwrap().line >= 2)
                .collect::<Vec<_>>()
        };
        let starts = normal_side(end_candidates(
            &idx,
            Side::East,
            0,
            0,
            &no_blocked(),
            &no_blocked(),
            idx.order_count,
            true, // tgt rank 2 >= src rank 0
        ));
        let goals = normal_side(end_candidates(
            &idx,
            Side::East,
            2,
            1,
            &no_blocked(),
            &no_blocked(),
            idx.order_count,
            false, // src rank 0 < tgt rank 2
        ));
        let hints = RouteHints {
            span: Some(SpanAffinity {
                src_rank: 0,
                tgt_rank: 2,
                src_order: 0,
                tgt_order: 1,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
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
        assert_eq!(out.path.escape.source, EscapeEnd::AtPortNormal);
        assert_eq!(out.path.escape.target, EscapeEnd::AtPortNormal);
        for &tid in &out.path.tracks {
            let t = sub.track(tid).unwrap();
            if t.orient == TrackOrient::Main {
                assert!(t.line >= 2, "corridor must stay normal-side: {t:?}");
            }
        }
    }
}
