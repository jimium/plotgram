//! Publish main/cross coordinates for Channel TrackOrder lanes.

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::Size;
use plotgram_model::diagnostics::Relaxation;

use crate::layout::hierarchical::channel::{ChannelRoutePlan, TrackId, TrackOrient};
use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
use crate::layout::hierarchical::group_frame::GroupShellBands;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph};

/// Per-edge facts a Main corridor needs to place itself (Metric-owned, read
/// from upstream writers only): the two endpoint port X's, the reserved
/// dummy-chain columns, and the rank interval the corridor spans.
#[derive(Debug, Clone, Default)]
pub struct MainLaneFacts {
    /// edge id → (source port X, target port X).
    pub endpoint_x: BTreeMap<String, (f64, f64)>,
    /// edge id → inclusive endpoint rank interval (lane overlap test).
    pub rank_span: BTreeMap<String, (usize, usize)>,
    /// edge id → inclusive ranks the vertical corridor actually crosses.
    ///
    /// An N/S end leaves through the layer gap, so its own rank is never
    /// crossed; an E/W end runs alongside its row and is. `None` = the
    /// corridor crosses no row at all.
    pub corridor_ranks: BTreeMap<String, Option<(usize, usize)>>,
    /// edge id → endpoint elem indices (never obstacles for their own edge).
    pub endpoint_elems: BTreeMap<String, (usize, usize)>,
    /// edge id → groups either endpoint sits in (own frames are never
    /// obstacles — an edge must be able to run inside its own group).
    pub own_groups: BTreeMap<String, BTreeSet<String>>,
    /// edge id → corridor X interval the edge's port sides admit.
    ///
    /// An E/W port fixes which side of its own face the rail must live on;
    /// a rail on the wrong side makes the first segment ride the node face
    /// (ink-and-verification §4). Compose owns the side, Metric only obeys.
    pub x_bounds: BTreeMap<String, (f64, f64)>,
}

/// A bend costs this many pixels of horizontal run (`w_bend / w_len`).
///
/// Mirrors the Channel search ratio so corridor placement and corridor
/// selection agree on what a bend is worth.
const LANE_BEND_FACTOR: f64 = 10.0;
/// Weight of staying on the reserved chain column, relative to run length.
/// Below 1 so a jog-free corridor always beats a merely well-centered one.
const LANE_CHAIN_WEIGHT: f64 = 0.5;

/// Per-substrate-track lane coordinates (main Y for Cross, cross X for Main).
#[derive(Debug, Default, Clone)]
pub struct TrackCoords {
    pub coords: BTreeMap<TrackId, Vec<f64>>,
}

impl TrackCoords {
    pub fn lane(&self, track: TrackId, index: u32) -> Option<f64> {
        self.coords
            .get(&track)
            .and_then(|v| v.get(index as usize))
            .copied()
    }
}

/// Place lanes inside each used substrate track.
///
/// Cross lanes sample a **per-corridor-line shared pitch grid**: group cuts
/// split one rank gap into several scope-cut tracks, and every track on the
/// line must map the same lane index to the same Y so gate crossings stay
/// collinear (channel-d1 §5.1). Outer lines (`k=0` / past last rank) park
/// clear of every group frame so an around-path seam cannot sit in the pad.
/// Returns soft relaxations for shell-band fallbacks (lanes could not stay
/// inside the band-free sub-interval).
///
/// `group_obstacles`: group envelope frames (finalize pad contract,
/// canonical space). Skip-over Main lanes (every member straddles a
/// foreign frame) clear **those straddled frames** directionally — the
/// remnant's backbone vs the straddled union midpoint picks the face —
/// so an around-path cannot collapse both verticals onto one X (Ink
/// spike-fold would then punch through). Other foreign frames and node
/// bodies stay nearest-side; a local skip around one group must not be
/// yanked to the canvas rim.
pub fn assign_track_coords(
    plan: &PlanGraph,
    main: &[f64],
    cross_centers: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    track_order: &TrackOrderPlan,
    route_plan: &ChannelRoutePlan,
    shell: &GroupShellBands,
    edge_gap: f64,
    groups: &[(String, (f64, f64, f64, f64))],
    facts: &MainLaneFacts,
) -> (TrackCoords, Vec<Relaxation>) {
    // Real-node frames for Main-lane clearance (InkVerifier node-penetration).
    let mut obstacles: Vec<(f64, f64, f64, f64)> = Vec::new(); // l,t,r,b
    for layer in &plan.layers {
        for &e in layer {
            if plan.elems[e].key.is_zero_width() {
                continue;
            }
            let s = size_of(e);
            let cx = cross_centers[e];
            let y = main[e];
            obstacles.push((cx - s.width / 2.0, y, cx + s.width / 2.0, y + s.height));
        }
    }

    // (track, lane) → routed member edges (TrackOrder is the sole writer of
    // lane indices; Metric only reads the inverse map).
    let mut lane_members: BTreeMap<(TrackId, u32), Vec<String>> = BTreeMap::new();
    for ((edge_id, tid), hop) in &track_order.assignments {
        lane_members
            .entry((*tid, hop.track_index))
            .or_default()
            .push(edge_id.clone());
    }

    let mut out = TrackCoords::default();
    let mut relaxations = Vec::new();

    // One shared lane grid per Cross corridor line.
    let mut cross_grids: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    let mut line_max: BTreeMap<usize, usize> = BTreeMap::new();
    for (&tid, &count) in &track_order.track_counts {
        let Some(t) = route_plan.substrate.track(tid) else {
            continue;
        };
        if matches!(t.orient, TrackOrient::Cross) {
            let e = line_max.entry(t.line).or_insert(0);
            *e = (*e).max(count);
        }
    }
    for (&line, &track_max) in &line_max {
        let count = track_order
            .cross_line_lane_counts
            .get(&line)
            .copied()
            .unwrap_or(track_max);
        if count == 0 {
            continue;
        }
        let ys = if line == 0 || line >= plan.layers.len() {
            // Outside the stack — park near adjacent layer; still separate
            // lanes (P5-5). Keep one `edge_gap` clear of the group shell
            // band that extends past the outer rank, and of every group
            // frame (around-path bottom/top seams otherwise sit in the pad).
            let below = line > 0;
            let mut park_obs = obstacles.clone();
            park_obs.extend(groups.iter().map(|(_, r)| *r));
            let base = if line == 0 {
                let mut top = plan.layers[0]
                    .iter()
                    .map(|&e| main[e])
                    .fold(f64::INFINITY, f64::min);
                for &(_, (_, t, _, _)) in groups {
                    top = top.min(t);
                }
                if !top.is_finite() {
                    top = 0.0;
                }
                top - edge_gap.max(shell.outer_top + edge_gap)
            } else {
                let last = plan.layers.len() - 1;
                let mut bot = plan.layers[last]
                    .iter()
                    .map(|&e| main[e] + size_of(e).height)
                    .fold(0.0_f64, f64::max);
                for &(_, (_, _, _, b)) in groups {
                    bot = bot.max(b);
                }
                bot + edge_gap.max(shell.outer_bottom + edge_gap)
            };
            let mid = (count.saturating_sub(1) as f64) * 0.5;
            (0..count)
                .map(|i| {
                    clear_parked_y(
                        base + (i as f64 - mid) * edge_gap,
                        &park_obs,
                        edge_gap,
                        below,
                    )
                })
                .collect()
        } else {
            let r = line - 1;
            let thickness = plan.layers[r]
                .iter()
                .map(|&e| size_of(e).height)
                .fold(0.0_f64, f64::max);
            let gap_top = main[plan.layers[r][0]] + thickness;
            let gap_bot = main[plan.layers[r + 1][0]];
            let (below_band, above_band) = shell.gap.get(r).copied().unwrap_or((0.0, 0.0));
            let (start, fallback) =
                cross_lane_start(gap_top, gap_bot, below_band, above_band, count, edge_gap);
            if fallback {
                relaxations.push(Relaxation {
                    rule: "channel-shell-band-fallback".into(),
                    detail: format!(
                        "Cross line {line}: {count}-lane fan wider than the \
                         shell-band-free gap; lanes fall back to the full rank gap"
                    ),
                });
            }
            (0..count).map(|i| start + i as f64 * edge_gap).collect()
        };
        cross_grids.insert(line, ys);
    }

    for (&tid, &count) in &track_order.track_counts {
        if count == 0 {
            continue;
        }
        let Some(t) = route_plan.substrate.track(tid) else {
            continue;
        };
        let ys = match t.orient {
            TrackOrient::Cross => {
                // Lane index is corridor-wide (TrackOrder); sample the line
                // grid so scope-cut tracks stay collinear across gates.
                let grid = &cross_grids[&t.line];
                grid.iter().take(count).copied().collect()
            }
            TrackOrient::Main => {
                // Vertical corridor X: the corridor an edge actually runs in
                // must be the column Order/Metric already reserved for it —
                // its dummy chain — snapped onto an endpoint port whenever
                // that is reachable, since that is what erases a jog. The
                // order-gap midpoint is only the fallback for corridors with
                // no routed member (`main_line_backbone_x` folds every rank,
                // so it means little for a corridor that spans a few).
                let og = t.line;
                let backbone = main_line_backbone_x(plan, cross_centers, size_of, og, edge_gap);

                let mut xs: Vec<f64> = Vec::with_capacity(count);
                let mut bands: Vec<Option<(usize, usize)>> = Vec::with_capacity(count);
                for lane in 0..count {
                    let members: &[String] = lane_members
                        .get(&(tid, lane as u32))
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let band = lane_rank_band(facts, members);
                    let lane_obstacles =
                        band_obstacles(plan, main, cross_centers, size_of, facts, members);
                    let foreign = foreign_group_frames(groups, facts, members);
                    let straddled = straddled_frames(facts, members, &foreign);
                    let dir = skip_over_clear_dir(backbone, facts, members, &straddled);
                    let other: Vec<(f64, f64, f64, f64)> = foreign
                        .iter()
                        .copied()
                        .filter(|f| !straddled.iter().any(|s| frames_eq(*s, *f)))
                        .collect();
                    let settle = |x: f64| -> f64 {
                        match dir {
                            Some(rightward) => clear_skip_over(
                                x,
                                &lane_obstacles,
                                &other,
                                &straddled,
                                edge_gap,
                                rightward,
                            ),
                            None => clear_outside(x, &lane_obstacles, &foreign, edge_gap),
                        }
                    };
                    let (blo, bhi) = lane_x_bounds(facts, members);
                    let settle = |x: f64| settle_within(x, blo, bhi, edge_gap, &settle);
                    let x = if members.is_empty() {
                        settle(backbone - (lane as f64) * if og == 0 { edge_gap } else { 0.0 })
                    } else {
                        best_lane_x(
                            plan,
                            cross_centers,
                            facts,
                            members,
                            backbone,
                            edge_gap,
                            &settle,
                        )
                    };
                    xs.push(x);
                    bands.push(band);
                }
                separate_overlapping_lanes(&mut xs, &bands, edge_gap);
                xs
            }
        };
        out.coords.insert(tid, ys);
    }
    (out, relaxations)
}

/// First lane Y for an interior Cross corridor: center the lane fan in the
/// group-shell-free sub-interval of the rank gap (demand reserves room for
/// it); fall back to centering in the full gap when the free interval is too
/// narrow (degraded group chains / tight params) — flagged in the second
/// return value so the caller can surface a relaxation.
fn cross_lane_start(
    gap_top: f64,
    gap_bot: f64,
    below_band: f64,
    above_band: f64,
    count: usize,
    edge_gap: f64,
) -> (f64, bool) {
    let span = (count.saturating_sub(1) as f64) * edge_gap;
    let free_top = gap_top + below_band;
    let free_bot = gap_bot - above_band;
    let fits_free = free_bot - free_top >= span;
    let (lo, hi) = if fits_free {
        (free_top, free_bot)
    } else {
        (gap_top, gap_bot)
    };
    let usable = (hi - lo).max(0.0);
    let fallback = !fits_free && (below_band > 0.0 || above_band > 0.0);
    (lo + (usable - span) * 0.5, fallback)
}

/// Union of the member edges' inclusive endpoint rank intervals.
fn lane_rank_band(facts: &MainLaneFacts, members: &[String]) -> Option<(usize, usize)> {
    let mut band: Option<(usize, usize)> = None;
    for id in members {
        let Some(&(lo, hi)) = facts.rank_span.get(id) else {
            continue;
        };
        band = Some(match band {
            None => (lo, hi),
            Some((a, b)) => (a.min(lo), b.max(hi)),
        });
    }
    band
}

/// Node bodies a corridor on this lane can actually hit: the rows its member
/// edges cross, minus the members' own endpoints (an edge may always touch
/// those). Rows outside the crossed band must not constrain the corridor —
/// folding all of them into one wall is what used to push corridors onto the
/// canvas rim.
fn band_obstacles(
    plan: &PlanGraph,
    main: &[f64],
    cross_centers: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    facts: &MainLaneFacts,
    members: &[String],
) -> Vec<(f64, f64, f64, f64)> {
    let mut own: BTreeSet<usize> = BTreeSet::new();
    let mut ranks: BTreeSet<usize> = BTreeSet::new();
    for id in members {
        if let Some(&(s, t)) = facts.endpoint_elems.get(id) {
            own.insert(s);
            own.insert(t);
        }
        match facts.corridor_ranks.get(id) {
            Some(Some((lo, hi))) => ranks.extend(*lo..=*hi),
            Some(None) => {}
            // Unknown member (no facts): stay conservative.
            None => ranks.extend(0..plan.layers.len()),
        }
    }
    let mut out = Vec::new();
    for rank in ranks {
        let Some(layer) = plan.layers.get(rank) else {
            continue;
        };
        for &e in layer {
            if plan.elems[e].key.is_zero_width() || own.contains(&e) {
                continue;
            }
            let s = size_of(e);
            out.push((
                cross_centers[e] - s.width / 2.0,
                main[e],
                cross_centers[e] + s.width / 2.0,
                main[e] + s.height,
            ));
        }
    }
    out
}

/// Group frames a corridor on this lane must stay out of: every group frame
/// except the ones the member edges' own endpoints live in.
fn foreign_group_frames(
    groups: &[(String, (f64, f64, f64, f64))],
    facts: &MainLaneFacts,
    members: &[String],
) -> Vec<(f64, f64, f64, f64)> {
    let mut own: BTreeSet<&str> = BTreeSet::new();
    for id in members {
        if let Some(set) = facts.own_groups.get(id) {
            own.extend(set.iter().map(String::as_str));
        }
    }
    groups
        .iter()
        .filter(|(id, _)| !own.contains(id.as_str()))
        .map(|(_, r)| *r)
        .collect()
}

/// `Some(rightward)` when every member of this lane skips over a foreign
/// group: park on the face of the **straddled** union that this remnant's
/// backbone already sits on. `best_lane_x` otherwise ties to the smaller
/// X and both around-verticals collapse (Ink spike-fold punches through).
/// Direction is vs the straddled midpoint — not the canvas / all-foreign
/// union — so a local skip around one middle group stays local.
fn skip_over_clear_dir(
    backbone: f64,
    facts: &MainLaneFacts,
    members: &[String],
    straddled: &[(f64, f64, f64, f64)],
) -> Option<bool> {
    if straddled.is_empty() {
        return None;
    }
    if !lane_is_skip_over_only(facts, members, straddled) {
        return None;
    }
    let left = straddled
        .iter()
        .map(|&(l, _, _, _)| l)
        .fold(f64::INFINITY, f64::min);
    let right = straddled
        .iter()
        .map(|&(_, _, r, _)| r)
        .fold(f64::NEG_INFINITY, f64::max);
    Some(backbone >= (left + right) * 0.5)
}

fn lane_is_skip_over_only(
    facts: &MainLaneFacts,
    members: &[String],
    foreign: &[(f64, f64, f64, f64)],
) -> bool {
    !members.is_empty()
        && members
            .iter()
            .all(|id| edge_straddles_foreign(facts, id, foreign))
}

fn straddled_frames(
    facts: &MainLaneFacts,
    members: &[String],
    foreign: &[(f64, f64, f64, f64)],
) -> Vec<(f64, f64, f64, f64)> {
    foreign
        .iter()
        .copied()
        .filter(|&frame| {
            members
                .iter()
                .any(|id| edge_straddles_foreign(facts, id, &[frame]))
        })
        .collect()
}

fn frames_eq(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    const EPS: f64 = 1e-9;
    (a.0 - b.0).abs() < EPS
        && (a.1 - b.1).abs() < EPS
        && (a.2 - b.2).abs() < EPS
        && (a.3 - b.3).abs() < EPS
}

fn edge_straddles_foreign(
    facts: &MainLaneFacts,
    id: &str,
    foreign: &[(f64, f64, f64, f64)],
) -> bool {
    let Some(&(a, b)) = facts.endpoint_x.get(id) else {
        return false;
    };
    let (min_x, max_x) = (a.min(b), a.max(b));
    foreign.iter().any(|&(l, _, r, _)| min_x < l && max_x > r)
}

/// Intersection of the member edges' admissible corridor X intervals.
fn lane_x_bounds(facts: &MainLaneFacts, members: &[String]) -> (f64, f64) {
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    for id in members {
        if let Some(&(l, h)) = facts.x_bounds.get(id) {
            lo = lo.max(l);
            hi = hi.min(h);
        }
    }
    if lo > hi {
        // Contradictory sides (a corridor shared by opposite normals): the
        // clearance pass alone decides, as before.
        return (f64::NEG_INFINITY, f64::INFINITY);
    }
    (lo, hi)
}

/// Clear `x` of node bodies **without** leaving `[lo, hi]`: clearance may push
/// a rail across the bound, so step back inward until both hold.
fn settle_within(x: f64, lo: f64, hi: f64, edge_gap: f64, settle: &dyn Fn(f64) -> f64) -> f64 {
    let clamp = |v: f64| v.clamp(lo, hi);
    let first = settle(clamp(x));
    if first >= lo - 1e-9 && first <= hi + 1e-9 {
        return first;
    }
    // Walk inward from the violated bound until clearance is a fixpoint.
    let (start, step) = if first < lo {
        (lo, edge_gap)
    } else {
        (hi, -edge_gap)
    };
    for i in 0..32 {
        let cand = start + step * i as f64;
        if cand < lo - 1e-9 || cand > hi + 1e-9 {
            break;
        }
        if (settle(cand) - cand).abs() < 1e-9 {
            return cand;
        }
    }
    clamp(first)
}

/// Pick the lane X minimising `J = w_bend·jogs + run_length + w_chain·chain_drift`
/// over the candidate columns the upstream writers already produced (each
/// endpoint port X, each reserved chain column, and the order-gap backbone).
///
/// The `J` minimum sits on an endpoint port X whenever that column is
/// reachable — which is exactly the corridor that needs only one jog.
fn best_lane_x(
    plan: &PlanGraph,
    cross_centers: &[f64],
    facts: &MainLaneFacts,
    members: &[String],
    backbone: f64,
    edge_gap: f64,
    settle: &dyn Fn(f64) -> f64,
) -> f64 {
    let chains: Vec<Vec<f64>> = members
        .iter()
        .map(|id| chain_columns(plan, cross_centers, id))
        .collect();

    let mut candidates: Vec<f64> = vec![backbone];
    for id in members {
        if let Some(&(sx, tx)) = facts.endpoint_x.get(id) {
            candidates.push(sx);
            candidates.push(tx);
        }
    }
    for c in chains.iter().flatten() {
        candidates.push(*c);
    }

    let w_bend = LANE_BEND_FACTOR * edge_gap.max(1e-9);
    let cost = |x: f64| -> f64 {
        let mut j = 0.0;
        for (id, chain) in members.iter().zip(&chains) {
            if let Some(&(sx, tx)) = facts.endpoint_x.get(id) {
                for e in [sx, tx] {
                    let d = (x - e).abs();
                    j += d;
                    if d > 1e-6 {
                        j += w_bend;
                    }
                }
            }
            // Mean, not sum: a long chain must not outweigh the run-length
            // term just by having more links.
            if !chain.is_empty() {
                let drift: f64 = chain.iter().map(|c| (x - c).abs()).sum();
                j += LANE_CHAIN_WEIGHT * drift / chain.len() as f64;
            }
        }
        j
    };

    let mut best: Option<(f64, f64)> = None;
    for cand in candidates {
        let x = settle(cand);
        let j = cost(x);
        // Ties break on the smaller X for determinism.
        if best.is_none_or(|(bj, bx)| j < bj - 1e-9 || (j < bj + 1e-9 && x < bx)) {
            best = Some((j, x));
        }
    }
    best.map(|(_, x)| x).unwrap_or(backbone)
}

/// Cross coordinates of `edge_id`'s dummy chain (its reserved columns).
fn chain_columns(plan: &PlanGraph, cross_centers: &[f64], edge_id: &str) -> Vec<f64> {
    plan.elems
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(&e.key, ElemKey::Virtual { edge_id: id, .. } if id == edge_id))
        .map(|(i, _)| cross_centers[i])
        .collect()
}

/// Push lanes of one corridor apart when their rank bands overlap — lanes
/// with disjoint bands are free to share an X.
fn separate_overlapping_lanes(xs: &mut [f64], bands: &[Option<(usize, usize)>], edge_gap: f64) {
    let overlaps = |a: usize, b: usize| match (bands.get(a), bands.get(b)) {
        (Some(Some((a0, a1))), Some(Some((b0, b1)))) => a0 <= b1 && b0 <= a1,
        _ => true,
    };
    for _ in 0..xs.len().max(1) {
        let mut order: Vec<usize> = (0..xs.len()).collect();
        order.sort_by(|&a, &b| xs[a].total_cmp(&xs[b]).then(a.cmp(&b)));
        let mut moved = false;
        for w in order.windows(2) {
            let (a, b) = (w[0], w[1]);
            if !overlaps(a, b) {
                continue;
            }
            let need = edge_gap - (xs[b] - xs[a]);
            if need > 1e-9 {
                xs[b] += need;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

fn main_line_backbone_x(
    plan: &PlanGraph,
    cross_centers: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    order_gap: usize,
    edge_gap: f64,
) -> f64 {
    let margin = edge_gap.max(1.0) * 0.5;
    let max_cols = plan.layers.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut xs = Vec::new();
    for layer in &plan.layers {
        if layer.is_empty() {
            continue;
        }
        if order_gap == 0 {
            // West outer: fold over every element's west face so the lane
            // clears all bodies of every rank (first/last can be a narrow
            // dummy, and clearance must not push the rim lane inward — the
            // Channel solver relies on rim lanes sitting on port normals).
            let face = layer
                .iter()
                .map(|&e| cross_centers[e] - size_of(e).width / 2.0)
                .fold(f64::INFINITY, f64::min);
            xs.push(face - margin);
        } else if order_gap >= layer.len() {
            // East outer: symmetric fold over every element's east face.
            let face = layer
                .iter()
                .map(|&e| cross_centers[e] + size_of(e).width / 2.0)
                .fold(f64::NEG_INFINITY, f64::max);
            xs.push(face + margin);
        } else {
            let l = layer[order_gap - 1];
            let r = layer[order_gap];
            let left_face = cross_centers[l] + size_of(l).width / 2.0;
            let right_face = cross_centers[r] - size_of(r).width / 2.0;
            xs.push(0.5 * (left_face + right_face));
        }
    }
    if xs.is_empty() {
        0.0
    } else if order_gap == 0 {
        xs.iter().cloned().fold(f64::INFINITY, f64::min)
    } else if order_gap >= max_cols {
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

/// Nudge a candidate Main-lane X until it does not pierce any node interior.
///
/// P5-2 wanted this deleted after substrate node occupancy; P5-1 could not
/// punch Main odd cells without breaking Channel search, so clearance stays
/// in the Metric Main-X writer (sole writer — not Ink).
///
/// Overlapping X-projections (same column / tight neighbors) are merged so we
/// cannot oscillate between faces of two overlapping intervals.
fn clear_main_x(x: f64, obstacles: &[(f64, f64, f64, f64)], edge_gap: f64) -> f64 {
    const EPS: f64 = 1e-6;
    let margin = edge_gap.max(1.0) * 0.5;
    let mut intervals: Vec<(f64, f64)> = obstacles.iter().map(|&(l, _, r, _)| (l, r)).collect();
    intervals.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap()
            .then(a.1.partial_cmp(&b.1).unwrap())
    });
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (l, r) in intervals {
        if let Some(last) = merged.last_mut() {
            // Merge if overlapping or closer than 2*margin (no room for a lane).
            if l <= last.1 + 2.0 * margin {
                last.1 = last.1.max(r);
            } else {
                merged.push((l, r));
            }
        } else {
            merged.push((l, r));
        }
    }
    for &(l, r) in &merged {
        if x > l + EPS && x < r - EPS {
            return if (x - l) <= (r - x) {
                l - margin
            } else {
                r + margin
            };
        }
    }
    x
}

/// Skip-over remnant: nearest-side against nodes and non-straddled foreign
/// frames; directional (including wrong-face yank) against the straddled
/// union only.
fn clear_skip_over(
    mut x: f64,
    nodes: &[(f64, f64, f64, f64)],
    other_foreign: &[(f64, f64, f64, f64)],
    straddled: &[(f64, f64, f64, f64)],
    edge_gap: f64,
    rightward: bool,
) -> f64 {
    for _ in 0..16 {
        let next = clear_main_x_dir(
            clear_main_x(clear_main_x(x, nodes, edge_gap), other_foreign, edge_gap),
            straddled,
            edge_gap,
            rightward,
        );
        if next == x {
            return x;
        }
        x = next;
    }
    x
}

/// Outer-rail clearance: [`clear_main_x`] against node bodies **and** group
/// envelopes, iterated to a fixpoint (pushing out of a group can land
/// inside a node projection further out, and vice versa).
fn clear_outside(
    mut x: f64,
    obstacles: &[(f64, f64, f64, f64)],
    group_obstacles: &[(f64, f64, f64, f64)],
    edge_gap: f64,
) -> f64 {
    for _ in 0..16 {
        let next = clear_main_x(
            clear_main_x(x, obstacles, edge_gap),
            group_obstacles,
            edge_gap,
        );
        if next == x {
            return x;
        }
        x = next;
    }
    x
}

fn clear_main_x_dir(
    x: f64,
    obstacles: &[(f64, f64, f64, f64)],
    edge_gap: f64,
    rightward: bool,
) -> f64 {
    const EPS: f64 = 1e-6;
    let margin = edge_gap.max(1.0) * 0.5;
    let mut intervals: Vec<(f64, f64)> = obstacles.iter().map(|&(l, _, r, _)| (l, r)).collect();
    intervals.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap()
            .then(a.1.partial_cmp(&b.1).unwrap())
    });
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (l, r) in intervals {
        if let Some(last) = merged.last_mut() {
            if l <= last.1 + 2.0 * margin {
                last.1 = last.1.max(r);
            } else {
                merged.push((l, r));
            }
        } else {
            merged.push((l, r));
        }
    }
    for &(l, r) in &merged {
        if x > l + EPS && x < r - EPS {
            return if rightward { r + margin } else { l - margin };
        }
    }
    if merged.is_empty() {
        return x;
    }
    // Already clear of every interval, but possibly parked on the wrong
    // side of the stack (best_lane_x snapped to the far endpoint).
    if rightward {
        let right = merged
            .iter()
            .map(|&(_, r)| r)
            .fold(f64::NEG_INFINITY, f64::max);
        if x < right + margin - EPS {
            return right + margin;
        }
    } else {
        let left = merged.iter().map(|&(l, _)| l).fold(f64::INFINITY, f64::min);
        if x > left - margin + EPS {
            return left - margin;
        }
    }
    x
}

/// Clear a parked Cross lane Y out of every node's Y-projection, always
/// pushing away from the stack (`below` = parked under the last rank).
/// Repeats until clear — the pushed position can land inside a taller row's
/// projection further out.
fn clear_parked_y(y: f64, obstacles: &[(f64, f64, f64, f64)], edge_gap: f64, below: bool) -> f64 {
    const EPS: f64 = 1e-6;
    let margin = edge_gap.max(1.0) * 0.5;
    let mut y = y;
    loop {
        let hit = obstacles
            .iter()
            .find(|&&(_, t, _, b)| y > t + EPS && y < b - EPS)
            .map(|&(_, t, _, b)| (t, b));
        match hit {
            Some((t, b)) => {
                y = if below { b + margin } else { t - margin };
            }
            None => return y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::clear_main_x;
    use super::clear_parked_y;
    use super::cross_lane_start;
    use super::*;

    use crate::layout::hierarchical::channel::substrate::{BlueprintIndex, Substrate};
    use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
    use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph};

    /// Two scope-cut tracks on one interior Cross line sample one shared
    /// pitch grid: the 1-lane track takes its grid slot instead of
    /// re-centering its own fan.
    #[test]
    fn cross_tracks_on_one_line_share_a_pitch_grid() {
        // Table: (below_band, above_band, wide_count, expected ys, fallback).
        // Ranks at y=0 / y=100, node height 10 → interior gap [10, 100].
        let cases: &[(f64, f64, usize, &[f64], bool)] = &[
            // No bands: 3-lane fan centered in the gap.
            (0.0, 0.0, 3, &[39.0, 55.0, 71.0], false),
            // Bands squeeze the free interval below the fan span → full-gap
            // fallback, surfaced as a relaxation.
            (45.0, 45.0, 2, &[47.0, 63.0], true),
        ];
        for &(below, above, count, expected, fallback) in cases {
            let elems = vec![
                Elem {
                    key: ElemKey::Real("a".into()),
                    group_path: vec![],
                    rank: 0,
                },
                Elem {
                    key: ElemKey::Real("b".into()),
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
                decl_index: (0..2).collect(),
                segments: vec![],
                layers: vec![vec![0], vec![1]],
                ..Default::default()
            };
            // One corridor line cut into a wide + narrow scope-cut track.
            let mut sub = Substrate::new();
            let t_wide = sub.alloc_track_id();
            sub.add_track(t_wide, TrackOrient::Cross, None, 1.0, 1, (0, 1))
                .unwrap();
            let t_narrow = sub.alloc_track_id();
            sub.add_track(t_narrow, TrackOrient::Cross, None, 1.0, 1, (2, 3))
                .unwrap();
            let route_plan = ChannelRoutePlan {
                substrate: sub,
                index: BlueprintIndex::default(),
                routes: BTreeMap::new(),
                bundles: vec![],
                relaxations: vec![],
                ripup_rounds: 0,
                used_gates: false,
                route_order: vec![],
            };
            let track_order = TrackOrderPlan {
                assignments: BTreeMap::new(),
                track_counts: [(t_wide, count), (t_narrow, 1)].into_iter().collect(),
                rank_gap_track_counts: BTreeMap::new(),
                cross_line_lane_counts: [(1usize, count)].into_iter().collect(),
            };
            let shell = GroupShellBands {
                gap: vec![(below, above)],
                outer_top: 0.0,
                outer_bottom: 0.0,
            };
            let (coords, relaxations) = assign_track_coords(
                &plan,
                &[0.0, 100.0],
                &[0.0, 0.0],
                &|_| Size::new(20.0, 10.0),
                &track_order,
                &route_plan,
                &shell,
                16.0,
                &[],
                &MainLaneFacts::default(),
            );
            assert_eq!(
                &coords.coords[&t_wide], expected,
                "below={below} above={above} count={count}"
            );
            // The single-lane track takes lane 0 of the same grid — never an
            // independently centered fan.
            assert_eq!(
                coords.coords[&t_narrow],
                vec![expected[0]],
                "narrow track must sample the corridor grid"
            );
            assert_eq!(
                relaxations.len(),
                if fallback { 1 } else { 0 },
                "below={below} above={above} count={count}"
            );
            if fallback {
                assert_eq!(relaxations[0].rule, "channel-shell-band-fallback");
            }
        }
    }

    #[test]
    fn cross_lane_start_avoids_shell_bands() {
        // Table: (below_band, above_band, count, expected start).
        // Gap [100, 180], edge_gap 16.
        let cases: &[(f64, f64, usize, f64)] = &[
            // No bands: center in the full gap (single lane).
            (0.0, 0.0, 1, 140.0),
            // Bands [100,116] / [156,180]: center in free [116,156].
            (16.0, 24.0, 1, 136.0),
            // Two lanes span 16 in free [116,156] → start 128.
            (16.0, 24.0, 2, 128.0),
            // Free interval narrower than the fan → fall back to full gap.
            (40.0, 40.0, 2, 100.0 + (80.0 - 16.0) * 0.5),
        ];
        for &(below, above, count, expected) in cases {
            let (start, fallback) = cross_lane_start(100.0, 180.0, below, above, count, 16.0);
            assert!(
                (start - expected).abs() < 1e-9,
                "below={below} above={above} count={count}: {start} != {expected}"
            );
            if below + above < 80.0 {
                let last = start + (count.saturating_sub(1) as f64) * 16.0;
                assert!(start >= 100.0 + below - 1e-9, "start pierces below band");
                assert!(last <= 180.0 - above + 1e-9, "last lane pierces above band");
                assert!(!fallback, "free interval fits — no fallback expected");
            } else {
                assert!(fallback, "squeezed fan must flag the shell-band fallback");
            }
        }
    }

    #[test]
    fn clear_main_x_pushes_off_node_body() {
        let obstacles = vec![(10.0, 0.0, 30.0, 20.0)];
        let x = clear_main_x(20.0, &obstacles, 8.0);
        assert!(x <= 10.0 - 4.0 + 1e-9 || x >= 30.0 + 4.0 - 1e-9, "x={x}");
    }

    #[test]
    fn clear_main_x_leaves_gap_alone() {
        let obstacles = vec![(10.0, 0.0, 30.0, 20.0), (50.0, 0.0, 70.0, 20.0)];
        let x = clear_main_x(40.0, &obstacles, 8.0);
        assert!((x - 40.0).abs() < 1e-9);
    }

    #[test]
    fn clear_parked_y_pushes_out_of_tall_row() {
        // Row at y 100..140 (taller than edge_gap): parked lanes inside the
        // body must move past it, away from the stack.
        let obstacles = vec![(0.0, 100.0, 50.0, 140.0)];
        let below = clear_parked_y(116.0, &obstacles, 16.0, true);
        assert!(below >= 140.0 + 8.0 - 1e-9, "below={below}");
        let above = clear_parked_y(116.0, &obstacles, 16.0, false);
        assert!(above <= 100.0 - 8.0 + 1e-9, "above={above}");
        // Already-clear lanes stay put.
        let free = clear_parked_y(160.0, &obstacles, 16.0, true);
        assert!((free - 160.0).abs() < 1e-9);
    }

    #[test]
    fn clear_main_x_handles_overlapping_projections() {
        // Two columns whose X-ranges overlap — naive push oscillates.
        let obstacles = vec![(0.0, 0.0, 100.0, 10.0), (90.0, 20.0, 190.0, 30.0)];
        let x = clear_main_x(95.0, &obstacles, 8.0);
        assert!(
            x <= 0.0 - 4.0 + 1e-9 || x >= 190.0 + 4.0 - 1e-9,
            "must escape merged block, got {x}"
        );
    }

    /// SM-4: an East-outer Main rail whose node-only clearance parks it in
    /// a group's pad band must move outside the group envelope.
    #[test]
    fn outer_main_rail_clears_group_envelope() {
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
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
            decl_index: (0..2).collect(),
            segments: vec![],
            layers: vec![vec![0], vec![1]],
            ..Default::default()
        };
        // East-outer Main track (order gap 1 >= max_cols 1).
        let mut sub = Substrate::new();
        let t_main = sub.alloc_track_id();
        sub.add_track(t_main, TrackOrient::Main, None, 1.0, 1, (0, 1))
            .unwrap();
        let route_plan = ChannelRoutePlan {
            substrate: sub,
            index: BlueprintIndex::default(),
            routes: BTreeMap::new(),
            bundles: vec![],
            relaxations: vec![],
            ripup_rounds: 0,
            used_gates: false,
            route_order: vec![],
        };
        let track_order = TrackOrderPlan {
            assignments: BTreeMap::new(),
            track_counts: [(t_main, 1)].into_iter().collect(),
            rank_gap_track_counts: BTreeMap::new(),
            cross_line_lane_counts: BTreeMap::new(),
        };
        let shell = GroupShellBands {
            gap: vec![(0.0, 0.0)],
            outer_top: 0.0,
            outer_bottom: 0.0,
        };
        // Node bodies [0,20]×two rows; group envelope extends to x=36.
        let group_env = vec![("g".to_string(), (0.0, -24.0, 36.0, 130.0))];
        let (with_group, _) = assign_track_coords(
            &plan,
            &[0.0, 100.0],
            &[10.0, 10.0],
            &|_| Size::new(20.0, 10.0),
            &track_order,
            &route_plan,
            &shell,
            16.0,
            &group_env,
            &MainLaneFacts::default(),
        );
        // Node-only backbone is 28 (east face 20 + margin 8) — inside the
        // envelope; clearance pushes past 36 + margin.
        assert_eq!(with_group.coords[&t_main], vec![44.0]);
        // Without group obstacles the rail stays at the node-only backbone.
        let (no_group, _) = assign_track_coords(
            &plan,
            &[0.0, 100.0],
            &[10.0, 10.0],
            &|_| Size::new(20.0, 10.0),
            &track_order,
            &route_plan,
            &shell,
            16.0,
            &[],
            &MainLaneFacts::default(),
        );
        assert_eq!(no_group.coords[&t_main], vec![28.0]);
    }
}
