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
use super::substrate::{
    BlueprintIndex, GateId, GroupId, GroupScope, Substrate, Track, TrackId, TrackOrient,
};
use tautcore_algo::orientation::Side;

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
///   `og` (East: `(order+1)..og`; West: `og..order`) and (b) `og` to be the
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
            // Column and overflow Mains: the facing Cross cell can belong
            // to a sibling group after per-rank shift, making the normal
            // host ScopeMask-infeasible. ViaGap onto a Main keeps
            // around-routing connected without widening the mask.
            // `edge_end_candidates` drops these when the Cross host is
            // in-scope, so they do not steal the zero-bend vertical.
            let line = if toward_higher { rank + 1 } else { rank };
            for og in 0..=order_count {
                let Some(tid) = index.main_at(og, rank) else {
                    continue;
                };
                let face_dist = if og <= order {
                    (order - og) as f64
                } else {
                    (og - (order + 1)) as f64
                };
                out.push(EndCandidate {
                    track: tid,
                    escape: EscapeEnd::ViaGap(line),
                    extra_bends: 2,
                    extra_len: 1.0 + face_dist,
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
                // Mirror East's `(order+1)..og`: nodes strictly between the
                // west rim corridor `og` and this endpoint. The old
                // `(og+1)..=order` missed the leftmost sibling (order `og`)
                // and wrongly included the endpoint itself — AtPortNormal
                // then drew a same-band horizontal through that sibling.
                let pierced = (og..order).any(|o| blocked_west.contains(&o));
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

/// Architecture §6.3「同线直穿」: a Cross on a line a foreign group would
/// cut (`r0 < k ≤ r1`) that is either a covering highway (ext spans the
/// group's order interior) or a **root-scope remnant** on that interior
/// line. Group-scope tracks of an allowed group stay (sibling groups may
/// share a rank band). Root remnants on a foreign interior line are unsafe:
/// Metric may place the two Mains on opposite sides of the frame, and Ink
/// expands a Cross as an infinite-width horizontal. Top/bottom seams
/// (`k=r0` / `k=r1+1`) stay legal because derive does not cut them.
pub(crate) fn cross_covers_foreign_in(t: &Track, foreign: &[&GroupScope]) -> bool {
    if t.orient != TrackOrient::Cross {
        return false;
    }
    for g in foreign {
        let (r0, r1) = g.ranks;
        if !(r0 < t.line && t.line <= r1) {
            continue;
        }
        if t.scope.is_none() {
            return true;
        }
        let (o0, o1) = g.orders;
        let glo = 2 * o0 + 1;
        let ghi = 2 * o1 + 1;
        if t.ext.0 <= glo && ghi <= t.ext.1 {
            return true;
        }
    }
    false
}

/// Groups the mask does not allow, in substrate (BTreeMap) order.
pub(crate) fn foreign_groups<'a>(
    substrate: &'a Substrate,
    allowed: &ScopeMask,
) -> Vec<&'a GroupScope> {
    substrate
        .groups()
        .filter(|g| !allowed.allows(Some(g.id)))
        .collect()
}

/// True when a Main at `og` sits on the opposite side of a foreign group
/// from `endpoint_order` on a rank that group occupies. Landing there
/// makes Ink's first/last horizontal punch through the sibling frame;
/// the around-path must stay on this side and cross at `k=r0` / `k=r1+1`.
pub(crate) fn main_straddles_in(
    foreign: &[&GroupScope],
    endpoint_order: usize,
    endpoint_rank: usize,
    og: usize,
) -> bool {
    for g in foreign {
        let (r0, r1) = g.ranks;
        if endpoint_rank < r0 || endpoint_rank > r1 {
            continue;
        }
        let (o0, o1) = g.orders;
        let between = if og <= endpoint_order {
            og <= o0 && endpoint_order >= o1 + 1
        } else {
            endpoint_order < o0 && og >= o1 + 1
        };
        if between {
            return true;
        }
    }
    false
}

/// Endpoint-induced corridor band for span affinity (D1.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanAffinity {
    pub src_rank: usize,
    pub tgt_rank: usize,
    pub src_order: usize,
    pub tgt_order: usize,
    /// Median order of the edge's own dummy-chain column (`None` = span 1).
    ///
    /// The endpoint band alone is flat across every interior gap, so a long
    /// edge could take any corridor between its two columns while its chain
    /// — the element Order/Metric already reserved for it — sat elsewhere.
    /// Keeping the corridor on the chain column is what makes the routed ink
    /// and the reserved column the same object.
    pub chain_order: Option<usize>,
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
    /// Same-face E–E / W–W: force start and goal onto one shared Main line
    /// (run one sub-search per `og`, pick min `J`). Opposite-face Main pairs
    /// must leave this false — they legitimately use distinct corridors.
    pub couple_main_corridor: bool,
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
            couple_main_corridor: false,
            order_count: 0,
            span: None,
            weights: CostWeights::default(),
        }
    }
}

/// Max bends the spike-fold terminal discount may erase (pair-level).
const SPIKE_FOLD_BEND_DISCOUNT: u32 = 4;

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

/// Main order-gap `og` distance to the two gaps flanking the edge's own
/// dummy-chain column. Zero when the corridor runs along the chain.
pub fn main_chain_dist(order_gap: usize, chain_order: Option<usize>) -> u32 {
    let Some(k) = chain_order else {
        return 0;
    };
    if order_gap <= k {
        (k - order_gap) as u32
    } else if order_gap > k + 1 {
        (order_gap - (k + 1)) as u32
    } else {
        0
    }
}

fn track_span_affinity(t: Option<&Track>, hints: &RouteHints) -> u32 {
    let Some(span) = hints.span else {
        return 0;
    };
    let Some(t) = t else {
        return 0;
    };
    match t.orient {
        TrackOrient::Cross => cross_span_dist(t.line, span.src_rank, span.tgt_rank),
        TrackOrient::Main => main_span_dist(t.line, span.src_order, span.tgt_order)
            .saturating_add(main_chain_dist(t.line, span.chain_order)),
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
///
/// When [`RouteHints::couple_main_corridor`] is set, candidates are partitioned
/// by Main `og` and each partition is solved independently; the min-`J`
/// feasible outcome wins. That preserves the same-face single-corridor
/// invariant without letting Dijkstra stitch distinct Main lines together.
pub fn route_edge(
    graph: &ChannelGraph<'_>,
    starts: &[EndCandidate],
    goals: &[EndCandidate],
    occupancy: &Occupancy,
    congestion_bias: bool,
    allowed: &ScopeMask,
    hints: RouteHints,
) -> RouteOutcome {
    if hints.couple_main_corridor {
        return route_edge_coupled_main(
            graph,
            starts,
            goals,
            occupancy,
            congestion_bias,
            allowed,
            hints,
        );
    }
    route_edge_ungated(
        graph,
        starts,
        goals,
        occupancy,
        congestion_bias,
        allowed,
        hints,
    )
}

fn route_edge_coupled_main(
    graph: &ChannelGraph<'_>,
    starts: &[EndCandidate],
    goals: &[EndCandidate],
    occupancy: &Occupancy,
    congestion_bias: bool,
    allowed: &ScopeMask,
    hints: RouteHints,
) -> RouteOutcome {
    let substrate = graph.substrate();
    let line_of = |tid: TrackId| -> Option<usize> { substrate.track(tid).map(|t| t.line) };
    let mut lines = BTreeSet::new();
    for c in starts.iter().chain(goals.iter()) {
        if let Some(line) = line_of(c.track) {
            lines.insert(line);
        }
    }
    let mut best: Option<RouteOutcome> = None;
    // Disable nesting: sub-searches must not re-enter this partitioner.
    let mut sub_hints = hints;
    sub_hints.couple_main_corridor = false;
    for line in lines {
        let s: Vec<EndCandidate> = starts
            .iter()
            .copied()
            .filter(|c| line_of(c.track) == Some(line))
            .collect();
        let g: Vec<EndCandidate> = goals
            .iter()
            .copied()
            .filter(|c| line_of(c.track) == Some(line))
            .collect();
        if s.is_empty() || g.is_empty() {
            continue;
        }
        let out = route_edge_ungated(
            graph,
            &s,
            &g,
            occupancy,
            congestion_bias,
            allowed,
            sub_hints,
        );
        if !out.feasible {
            continue;
        }
        let replace = match &best {
            None => true,
            Some(cur) => out.cost < cur.cost,
        };
        if replace {
            best = Some(out);
        }
    }
    best.unwrap_or_else(RouteOutcome::infeasible)
}

/// Per-call dense/memoized view of the search inputs that are immutable
/// within one `route_edge` call (substrate, mask, occupancy, hints). Pure
/// cache of the free-function queries — identical results, fewer map walks.
struct SearchEnv<'a> {
    substrate: &'a Substrate,
    /// Dense track table by `TrackId.0` (None = id hole).
    tracks: Vec<Option<&'a Track>>,
    /// Groups the mask does not allow, in substrate (BTreeMap) order.
    foreign: Vec<&'a GroupScope>,
    /// Memoized `cross_covers_foreign_group` per track id.
    cross_foreign: Vec<Option<bool>>,
    /// Memoized `soft_penalties` per track id.
    soft: Vec<Option<f64>>,
    /// Memoized `inner_main_saturated` (constant within a call).
    inner_saturated: Option<bool>,
    /// Memoized `main_line_demand` per order-gap line.
    main_demand: Vec<Option<u32>>,
}

impl<'a> SearchEnv<'a> {
    fn new(substrate: &'a Substrate, allowed: &ScopeMask) -> Self {
        let n = substrate
            .tracks()
            .map(|t| t.id.0 as usize + 1)
            .max()
            .unwrap_or(0);
        let mut tracks: Vec<Option<&'a Track>> = vec![None; n];
        for t in substrate.tracks() {
            tracks[t.id.0 as usize] = Some(t);
        }
        Self {
            substrate,
            tracks,
            foreign: foreign_groups(substrate, allowed),
            cross_foreign: vec![None; n],
            soft: vec![None; n],
            inner_saturated: None,
            main_demand: Vec::new(),
        }
    }

    /// Dense track lookup (the borrow is tied to the substrate, not `self`).
    fn track(&self, id: TrackId) -> Option<&'a Track> {
        self.tracks.get(id.0 as usize).copied().flatten()
    }

    fn cross_covers_foreign(&mut self, track: TrackId) -> bool {
        let i = track.0 as usize;
        if let Some(Some(v)) = self.cross_foreign.get(i) {
            return *v;
        }
        let v = match self.track(track) {
            None => false,
            Some(t) => cross_covers_foreign_in(t, &self.foreign),
        };
        self.cross_foreign[i] = Some(v);
        v
    }

    /// Cross→Main (or reverse) whose order span jumps a foreign group: Ink
    /// draws one horizontal at the Cross Y through the sibling frame.
    fn hop_straddles(&self, from: TrackId, to: TrackId) -> bool {
        let (a, b) = (self.track(from), self.track(to));
        let (Some(a), Some(b)) = (a, b) else {
            return false;
        };
        let (cross, main_og) = match (a.orient, b.orient) {
            (TrackOrient::Cross, TrackOrient::Main) => (a, b.line),
            (TrackOrient::Main, TrackOrient::Cross) => (b, a.line),
            _ => return false,
        };
        let mid = (cross.ext.0 / 2 + cross.ext.1 / 2) / 2;
        let rank = cross.line.saturating_sub(1);
        main_straddles_in(&self.foreign, mid, rank, main_og)
    }

    fn span_affinity(&self, track: TrackId, hints: &RouteHints) -> u32 {
        track_span_affinity(self.track(track), hints)
    }

    fn soft_penalty(&mut self, track: TrackId, hints: &RouteHints, occupancy: &Occupancy) -> f64 {
        let i = track.0 as usize;
        if let Some(Some(v)) = self.soft.get(i) {
            return *v;
        }
        let t = self.track(track);
        let v = soft_penalties_core(t, hints, || {
            let span = hints.span.expect("core checks span before inner_sat");
            self.inner_saturated(hints, occupancy, span)
        });
        self.soft[i] = Some(v);
        v
    }

    fn inner_saturated(
        &mut self,
        hints: &RouteHints,
        occupancy: &Occupancy,
        span: SpanAffinity,
    ) -> bool {
        if let Some(v) = self.inner_saturated {
            return v;
        }
        let order_lo = span.src_order.min(span.tgt_order);
        let order_hi = span.src_order.max(span.tgt_order);
        let mut saturated = true;
        for og in order_lo..=order_hi.saturating_add(1) {
            if og == 0 || og == hints.order_count {
                continue;
            }
            if self.main_demand_at(og, occupancy) < INNER_SATURATION_DEMAND {
                saturated = false;
                break;
            }
        }
        self.inner_saturated = Some(saturated);
        saturated
    }

    fn main_demand_at(&mut self, og: usize, occupancy: &Occupancy) -> u32 {
        if let Some(Some(v)) = self.main_demand.get(og) {
            return *v;
        }
        let v = main_line_demand(self.substrate, occupancy, og);
        if og >= self.main_demand.len() {
            self.main_demand.resize(og + 1, None);
        }
        self.main_demand[og] = Some(v);
        v
    }
}

fn route_edge_ungated(
    graph: &ChannelGraph<'_>,
    starts: &[EndCandidate],
    goals: &[EndCandidate],
    occupancy: &Occupancy,
    congestion_bias: bool,
    allowed: &ScopeMask,
    hints: RouteHints,
) -> RouteOutcome {
    let substrate = graph.substrate();
    let mut env = SearchEnv::new(substrate, allowed);
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

    // Adjacent endpoint hosts still go through ScopeMask + through-highway
    // filters (verify_route_scope audits every committed track). Direct
    // start↔goal only skips the Dijkstra expansion, not the legality checks.
    let direct_via = |a: TrackId, b: TrackId| -> Option<Via> {
        graph
            .neighbors(a)
            .iter()
            .find(|tr| tr.to == b)
            .map(|tr| tr.via)
    };

    let mut open = BinaryHeap::new();
    // best: dense by TrackId.0 → (cost, parent track, via used to enter,
    // start candidate idx). Track ids come from the same substrate.
    let mut best: Vec<Option<BestEntry>> = vec![None; env.tracks.len()];

    for (si, sc) in starts.iter().enumerate() {
        let Some(start_t) = env.track(sc.track) else {
            continue;
        };
        if !allowed.allows(start_t.scope) || env.cross_covers_foreign(sc.track) {
            continue;
        }
        let mut start_cost = LexCost {
            bends: sc.extra_bends,
            length: start_t.span_weight + sc.extra_len,
            span_affinity: env.span_affinity(sc.track, &hints),
            congestion: if congestion_bias {
                occupancy.lane_demand(sc.track) as f64
            } else {
                0.0
            },
            crossings: occupancy.crossing_count(sc.track, start_t.ext),
            ..LexCost::default()
        };
        start_cost.congestion += env.soft_penalty(sc.track, &hints, occupancy);
        start_cost.recompute(&hints.weights);
        let replace = match best_at(&best, sc.track) {
            None => true,
            Some((bc, _, _, _)) => start_cost < bc,
        };
        if replace {
            best[sc.track.0 as usize] = Some((start_cost, None, None, si));
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
            let Some(to_t) = env.track(gt) else {
                continue;
            };
            if !allowed.allows(to_t.scope)
                || env.cross_covers_foreign(gt)
                || env.hop_straddles(sc.track, gt)
            {
                continue;
            }
            let mut direct = start_cost;
            if start_t.orient != to_t.orient {
                direct.bends += 1;
            }
            direct.length += to_t.span_weight;
            direct.span_affinity = direct
                .span_affinity
                .saturating_add(env.span_affinity(gt, &hints));
            direct.crossings = direct
                .crossings
                .saturating_add(occupancy.crossing_count(gt, to_t.ext));
            if congestion_bias {
                direct.congestion += occupancy.lane_demand(gt) as f64;
            }
            direct.congestion += env.soft_penalty(gt, &hints, occupancy);
            direct.recompute(&hints.weights);
            let replace = match best_at(&best, gt) {
                None => true,
                Some((bc, _, _, _)) => direct < bc,
            };
            if replace {
                best[gt.0 as usize] = Some((direct, Some(sc.track), Some(via), si));
                open.push(State {
                    cost: direct,
                    track: gt,
                });
            }
        }
    }
    if best.iter().all(|e| e.is_none()) {
        return RouteOutcome::infeasible();
    }

    let mut best_goal: Option<(LexCost, TrackId)> = None;

    while let Some(State { cost, track }) = open.pop() {
        if let Some((bc, _, _, _)) = best_at(&best, track) {
            if cost > bc {
                continue;
            }
        }
        if let Some(gc) = goal_map.get(&track).copied() {
            let mut total = cost;
            total.bends += gc.extra_bends;
            total.length += gc.extra_len;
            if hints.min_last_span > 0.0
                && env
                    .track(track)
                    .is_some_and(|t| t.span_weight + 1e-9 < hints.min_last_span)
            {
                total.congestion += 1e6;
            }
            total.recompute(&hints.weights);
            // Spike-fold discount (pair-level): when both ends escape via the
            // *same* gap line onto the *same* corridor through the minimal
            // Main→Cross→Main chain, Ink's out-and-back rail excursion sits on
            // one shared gap line and collapses to a removable spike — the
            // path renders just the two stub-corner bends (baseline e10
            // shape). The fold erases the 2 transit bends plus one rail-touch
            // bend per end; price the pair accordingly. Rim AtPortNormal
            // pairings never fold and keep their full price.
            if let Some((_, _, _, si)) = best_at(&best, track) {
                if let (EscapeEnd::ViaGap(sl), EscapeEnd::ViaGap(gl)) =
                    (starts[si].escape, gc.escape)
                {
                    let same_corridor = env
                        .track(starts[si].track)
                        .zip(env.track(gc.track))
                        .is_some_and(|(s, g)| s.orient == g.orient && s.line == g.line);
                    // Minimal chain: reach the start entry within 2 hops.
                    let mut hops = 0usize;
                    let mut cur = track;
                    let mut minimal = false;
                    loop {
                        match best_at(&best, cur) {
                            Some((_, None, _, _)) => {
                                minimal = true;
                                break;
                            }
                            Some((_, Some(p), _, _)) if hops < 2 => {
                                cur = p;
                                hops += 1;
                            }
                            _ => break,
                        }
                    }
                    if sl == gl && same_corridor && minimal {
                        total.bends = total.bends.saturating_sub(SPIKE_FOLD_BEND_DISCOUNT);
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
        // Prune: heap pops in ascending prefix scalar. Spike-fold may subtract
        // up to SPIKE_FOLD_BEND_DISCOUNT bends at a goal, so a later prefix can
        // still beat `best_goal` until it exceeds best + that discount.
        if let Some((bt, _)) = &best_goal {
            let fold_slack = hints.weights.w_bend * f64::from(SPIKE_FOLD_BEND_DISCOUNT);
            if cost.scalar >= bt.scalar + fold_slack {
                break;
            }
        }
        let from_orient = match env.track(track) {
            Some(t) => t.orient,
            None => continue,
        };
        // Still on a start seed (no parent). Do not gate on `cost.bends == 0`:
        // ViaGap seeds already carry escape bends, but the first corridor
        // leave must still honor `min_first_span`.
        let leaving_start = best_at(&best, track).is_some_and(|e| e.1.is_none());
        for tr in graph.neighbors(track) {
            if let Via::Gate(g) = tr.via {
                if !occupancy.gate_open(substrate, g) {
                    continue;
                }
            }
            let Some(to_t) = env.track(tr.to) else {
                continue;
            };
            if !allowed.allows(to_t.scope)
                || env.cross_covers_foreign(tr.to)
                || env.hop_straddles(track, tr.to)
            {
                continue;
            }
            let mut next = cost;
            match tr.via {
                Via::Link if from_orient != to_t.orient => {
                    next.bends += 1;
                    if leaving_start
                        && hints.min_first_span > 0.0
                        && env
                            .track(track)
                            .is_some_and(|t| t.span_weight + 1e-9 < hints.min_first_span)
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
            next.span_affinity = next
                .span_affinity
                .saturating_add(env.span_affinity(tr.to, &hints));
            next.crossings = next
                .crossings
                .saturating_add(occupancy.crossing_count(tr.to, to_t.ext));
            if congestion_bias {
                next.congestion += occupancy.lane_demand(tr.to) as f64;
            }
            next.congestion += env.soft_penalty(tr.to, &hints, occupancy);
            next.recompute(&hints.weights);
            let replace = match best_at(&best, tr.to) {
                None => true,
                Some((bc, _, _, _)) => next < bc,
            };
            if replace {
                let start_idx = best_at(&best, track).map(|e| e.3).unwrap_or(0);
                best[tr.to.0 as usize] = Some((next, Some(track), Some(tr.via), start_idx));
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
    let start_idx = best_at(&best, tracks[0]).map(|e| e.3).unwrap_or(0);
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
    soft_penalties_core(substrate.track(track), hints, || {
        let span = hints.span.expect("core checks span before inner_sat");
        inner_main_saturated(substrate, occupancy, span, hints.order_count)
    })
}

/// [`soft_penalties`] core with a lazily-evaluated saturation probe (the
/// search env memoizes the probe; it is constant within one call).
fn soft_penalties_core(
    t: Option<&Track>,
    hints: &RouteHints,
    inner_sat: impl FnOnce() -> bool,
) -> f64 {
    if !hints.outer_main_as_overflow {
        return 0.0;
    }
    let Some(span) = hints.span else {
        return 0.0;
    };
    let Some(t) = t else {
        return 0.0;
    };
    if t.orient != TrackOrient::Main {
        return 0.0;
    }
    let outer = t.line == 0 || t.line == hints.order_count;
    if !outer {
        return 0.0;
    }
    if inner_sat() {
        0.0
    } else if span.src_order == span.tgt_order {
        SAME_ORDER_OUTER_OVERFLOW_PENALTY
    } else {
        OUTER_OVERFLOW_PENALTY
    }
}

fn reconstruct(best: &[Option<BestEntry>], goal: TrackId) -> (Vec<TrackId>, Vec<GateId>) {
    let mut cur = goal;
    let mut rev_tracks = vec![cur];
    let mut rev_gates = Vec::new();
    while let Some((_, Some(parent), via, _)) = best_at(best, cur) {
        if let Some(Via::Gate(g)) = via {
            rev_gates.push(g);
        }
        cur = parent;
        rev_tracks.push(cur);
    }
    rev_tracks.reverse();
    rev_gates.reverse();
    (rev_tracks, rev_gates)
}

/// Best-table entry: `(cost, parent track, via used to enter, start cand idx)`.
type BestEntry = (LexCost, Option<TrackId>, Option<Via>, usize);

#[inline]
fn best_at(best: &[Option<BestEntry>], track: TrackId) -> Option<BestEntry> {
    best.get(track.0 as usize).copied().flatten()
}

#[cfg(test)]
mod tests {
    use super::super::graph::{ChannelGraph, Occupancy};
    use super::super::substrate::{derive_root_substrate, PortSide};
    use super::*;
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
            ..Default::default()
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
                chain_order: None,
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
                assert_ne!(
                    t.line, 0,
                    "must not rush to Cross line 0: {:?}",
                    out.path.tracks
                );
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
            ..Default::default()
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
                chain_order: None,
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
            ..Default::default()
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
                chain_order: None,
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
            ..Default::default()
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
                chain_order: None,
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
            ..Default::default()
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
                chain_order: None,
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
            ..Default::default()
        };
        let (sub, idx) = derive_root_substrate(&plan);
        use tautcore_algo::orientation::Side;

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
                    c.escape == EscapeEnd::AtPortNormal && sub.track(c.track).unwrap().line == line
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
                    .filter(|c| sub.track(c.track).unwrap().line == line)
                    .filter_map(|c| match c.escape {
                        EscapeEnd::ViaGap(g) => Some(g),
                        _ => None,
                    })
                    .collect();
                assert_eq!(gaps, vec![2], "ViaGap fallback on og={line}: {cands:?}");
            }
        }

        // West mirror: endpoint at order 1, sibling at order 0 must block
        // rim AtPortNormal (would draw through that sibling on the node band).
        let west_cases: [(std::collections::BTreeSet<usize>, bool); 2] = [
            (no_blocked(), true),
            ([0usize].into_iter().collect(), false),
        ];
        for (blocked, want_straight_rim) in west_cases {
            let cands = end_candidates(
                &idx,
                Side::West,
                1,
                1,
                &no_blocked(),
                &blocked,
                idx.order_count,
                true,
            );
            assert!(!cands.is_empty(), "West port must have candidates");
            let straight = |line: usize| {
                cands.iter().any(|c| {
                    c.escape == EscapeEnd::AtPortNormal && sub.track(c.track).unwrap().line == line
                })
            };
            assert!(
                !straight(1),
                "inner West lane is not normal-safe: {cands:?}"
            );
            assert_eq!(
                straight(0),
                want_straight_rim,
                "West rim straight exit, blocked={blocked:?}"
            );
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
        use tautcore_algo::orientation::Side;
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
                chain_order: None,
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
            ..Default::default()
        };
        let (sub, idx) = derive_root_substrate(&plan);
        let g = ChannelGraph::from_substrate(&sub);
        use tautcore_algo::orientation::Side;
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
                chain_order: None,
            }),
            order_count: idx.order_count,
            outer_main_as_overflow: true,
            couple_main_corridor: true,
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
