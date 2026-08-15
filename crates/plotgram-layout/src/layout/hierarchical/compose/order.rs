//! P6 ordering: flat median + transpose + sifting + best-snapshot.
//!
//! Objective `J_order` (composition.md §6), flat extension:
//! `(weighted_crossings, source_moment, total_span, lex_layers)`.
//! Chain-block sifting (non-grouped) uses a local key
//! `(crossings, endpoint_inversion, source_moment, total_span)` so a
//! corridor can park on the endpoint side when crossings stay flat.
//! `source_moment` = Σ positions of **forward** mid-rank sources (no upward
//! proper neighbor, has downward, not a FAS-reverse spine head). After
//! crossing minimization, flat plans also pick the global left/right
//! orientation that minimizes `source_moment` (crossings are flip-invariant)
//! and order the branch-source pocket so forward sources precede reverse
//! heads — expectations: forward branch sources sit toward the reading-start
//! side of the cross axis. Grouped plans zero span/moment.
//!
//! Median prefers real neighbors over virtuals, then falls back to the
//! opposite sweep side, then `prev_pos` (composition.md / Graphviz wmedian).
//! FAS-reversed segments carry the ordinary 1/2/8 weights — after P1 the
//! reverse bit is only a direction.
//!
//! Group containment is enforced upstream by
//! [`super::boundary::insert_group_boundaries`] (Left/Right clamps + high-weight
//! cross-rank segments). Crossing minimization is fully group-agnostic.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::layout::hierarchical::model::{Elem, PlanGraph};

const EPS: f64 = 1e-9;
const MAX_SWEEPS: usize = 24;
const NO_IMPROVE_STOP: usize = 4;

/// real-real / real-virtual / virtual-virtual base weight (composition.md §6).
fn edge_weight(a: &Elem, b: &Elem) -> f64 {
    match (a.key.is_virtual(), b.key.is_virtual()) {
        (false, false) => 1.0,
        (true, true) => 8.0,
        _ => 2.0,
    }
}

/// Segment weight: both ends boundary clamps (group `gb:` or partition
/// `pb:`) → `group_boundary_weight`; else base × author weight (vv corridor
/// stays 8.0, weight does not apply). FAS-reversed segments use the same
/// 1/2/8 recipe as forward ones: after P1, `reversed` is only a direction
/// bit (yfiles/01 §1/§3); the 1/2/8 recipe is what straightens long edges.
fn segment_weight(a: &Elem, b: &Elem, weight: f64, group_boundary_weight: f64) -> f64 {
    if a.key.is_boundary() && b.key.is_boundary() {
        return group_boundary_weight;
    }
    let base = edge_weight(a, b);
    if matches!((a.key.is_virtual(), b.key.is_virtual()), (true, true)) {
        base
    } else {
        base * weight
    }
}

struct Adjacency {
    /// elem -> (neighbor elem, segment weight) one rank above.
    up: Vec<Vec<(usize, f64)>>,
    /// elem -> (neighbor elem, segment weight) one rank below.
    down: Vec<Vec<(usize, f64)>>,
}

fn build_adjacency(
    plan: &PlanGraph,
    edge_weights: &BTreeMap<String, f64>,
    group_boundary_weight: f64,
) -> Adjacency {
    let n = plan.elems.len();
    let mut up = vec![Vec::new(); n];
    let mut down = vec![Vec::new(); n];
    for s in &plan.segments {
        let w = segment_weight(
            &plan.elems[s.from],
            &plan.elems[s.to],
            edge_weights.get(&s.edge_id).copied().unwrap_or(1.0),
            group_boundary_weight,
        );
        down[s.from].push((s.to, w));
        up[s.to].push((s.from, w));
    }
    for v in up.iter_mut().chain(down.iter_mut()) {
        v.sort_unstable_by_key(|&(e, _)| e);
    }
    Adjacency { up, down }
}

/// Precomputed bipartite segments for crossing counts (gb: / pb: excluded).
/// Valid for the whole ordering phase — segments are immutable then.
struct CrossingIndex {
    /// `segs_by_pair[r]` = edges between layers `r` and `r+1` as `(from, to)`.
    segs_by_pair: Vec<Vec<(usize, usize)>>,
    /// elem → neighbors one rank above (sources of incoming segs).
    up: Vec<Vec<usize>>,
    /// elem → neighbors one rank below (targets of outgoing segs).
    down: Vec<Vec<usize>>,
}

fn build_crossing_index(plan: &PlanGraph) -> CrossingIndex {
    let n = plan.elems.len();
    let pairs = plan.layers.len().saturating_sub(1);
    let mut segs_by_pair = vec![Vec::new(); pairs];
    let mut up = vec![Vec::new(); n];
    let mut down = vec![Vec::new(); n];
    for s in &plan.segments {
        if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
            continue;
        }
        let r = plan.elems[s.from].rank as usize;
        if r < pairs && plan.elems[s.to].rank as usize == r + 1 {
            segs_by_pair[r].push((s.from, s.to));
            down[s.from].push(s.to);
            up[s.to].push(s.from);
        }
    }
    for v in up.iter_mut().chain(down.iter_mut()) {
        v.sort_unstable();
    }
    CrossingIndex {
        segs_by_pair,
        up,
        down,
    }
}

pub fn order_layers(
    plan: &mut PlanGraph,
    edge_weights: &BTreeMap<String, f64>,
    group_boundary_weight: f64,
    reversed_edges: &BTreeSet<String>,
) {
    if plan.layers.len() < 2 {
        return; // nothing to reorder
    }
    // Span/sift/source_moment are flat-DAG levers. Gate on **group boundary
    // dummies** (Weak continuous-block clamps), not mere `group_path`.
    let grouped = plan.elems.iter().any(|e| e.key.is_group_boundary());
    let empty = BTreeSet::new();
    let reversed = if grouped { &empty } else { reversed_edges };
    let adj = build_adjacency(plan, edge_weights, group_boundary_weight);
    let xidx = build_crossing_index(plan);
    let rev_head = reversed_heads(plan, reversed);

    let mut best = plan.layers.clone();
    let mut best_score = order_score(plan, &xidx, !grouped, &rev_head);
    let mut no_improve = 0usize;

    for sweep in 0..MAX_SWEEPS {
        if sweep % 2 == 0 {
            for r in 1..plan.layers.len() {
                reorder_layer(plan, &adj, r, Direction::Up);
            }
        } else {
            for r in (0..plan.layers.len() - 1).rev() {
                reorder_layer(plan, &adj, r, Direction::Down);
            }
        }
        transpose_pass(plan, &xidx, /*use_span*/ !grouped);
        for r in 0..plan.layers.len() {
            restore_partition_clamps(plan, r);
            restore_group_clamps(plan, r);
        }
        if !grouped {
            order_branch_source_pocket(plan, &xidx, &rev_head);
        }

        let score = order_score(plan, &xidx, !grouped, &rev_head);
        let lex_better = score.crossings == best_score.crossings
            && score.source_moment == best_score.source_moment
            && score.total_span == best_score.total_span
            && plan.layers < best;
        if score < best_score || lex_better {
            best_score = score;
            best.clone_from(&plan.layers);
            no_improve = 0;
        } else {
            no_improve += 1;
        }
        if no_improve >= NO_IMPROVE_STOP {
            break;
        }
    }

    plan.layers = best;
    for r in 0..plan.layers.len() {
        restore_partition_clamps(plan, r);
        restore_group_clamps(plan, r);
    }
    if !grouped {
        // Crossing-minimal orders are flip-invariant; pick the side that
        // parks forward branch sources toward the reading-start (left in TB).
        choose_layer_orientation(plan, &xidx, &rev_head);
        order_branch_source_pocket(plan, &xidx, &rev_head);
    }
    if !super::super::CHANNEL_FORCE_ROOT.get() {
        super::boundary::align_group_left_pads(plan);
    }
    tighten_one_to_one(plan, &xidx);
    if !grouped {
        for _ in 0..8 {
            let before_layers = plan.layers.clone();
            let before = order_score(plan, &xidx, true, &rev_head);
            sift_pass(plan, &xidx, &rev_head);
            chain_block_sift_pass(plan, &xidx, &rev_head);
            for r in 0..plan.layers.len() {
                restore_partition_clamps(plan, r);
                restore_group_clamps(plan, r);
            }
            order_branch_source_pocket(plan, &xidx, &rev_head);
            let after = order_score(plan, &xidx, true, &rev_head);
            // Chain-block trials may keep crossings flat while trading a
            // little `total_span` for lower endpoint inversion.
            // Revert only when crossings rose; a span-only regression is the
            // cost of parking the corridor on the endpoint side.
            if after.crossings > before.crossings {
                plan.layers = before_layers;
                break;
            }
            if after == before || plan.layers == before_layers {
                break;
            }
        }
        choose_layer_orientation(plan, &xidx, &rev_head);
        order_branch_source_pocket(plan, &xidx, &rev_head);
    }
}

/// Lexicographic ordering objective (composition.md §6 + flat source bias).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OrderScore {
    crossings: u64,
    /// Σ positions of **forward** branch sources (empty up, non-empty down,
    /// and not the working head of a FAS-reversed edge). Reverse-spine heads
    /// are excluded so they do not outrank pure sources for the reading-start
    /// column. Zeroed when `use_span` is false.
    source_moment: u64,
    /// Σ |layer_pos(u) − layer_pos(v)| over proper segments (straightness).
    /// Zeroed when `use_span` is false (grouped plans).
    total_span: u64,
}

fn order_score(
    plan: &PlanGraph,
    xidx: &CrossingIndex,
    use_span: bool,
    rev_head: &[bool],
) -> OrderScore {
    OrderScore {
        crossings: total_crossings(plan, xidx),
        source_moment: if use_span {
            source_moment(plan, xidx, rev_head)
        } else {
            0
        },
        total_span: if use_span {
            total_order_span(plan, xidx)
        } else {
            0
        },
    }
}

fn is_branch_source(plan: &PlanGraph, xidx: &CrossingIndex, e: usize) -> bool {
    use crate::layout::hierarchical::model::ElemKey;
    matches!(plan.elems[e].key, ElemKey::Real(_))
        && plan.elems[e].rank > 0
        && xidx.up[e].is_empty()
        && !xidx.down[e].is_empty()
}

/// Precomputed `heads_reversed_edge` per elem: one O(M) pass replacing the
/// per-elem segment scans (O(N·M) per `source_moment` / pocket call).
/// `plan.segments` and the reversed set are immutable while ordering runs.
fn reversed_heads(plan: &PlanGraph, reversed_edges: &BTreeSet<String>) -> Vec<bool> {
    let mut head = vec![false; plan.elems.len()];
    if reversed_edges.is_empty() {
        return head;
    }
    for s in &plan.segments {
        if reversed_edges.contains(&s.edge_id) {
            head[s.from] = true;
        }
    }
    head
}

fn is_forward_branch_source(
    plan: &PlanGraph,
    xidx: &CrossingIndex,
    e: usize,
    rev_head: &[bool],
) -> bool {
    is_branch_source(plan, xidx, e) && !rev_head[e]
}

fn source_moment(plan: &PlanGraph, xidx: &CrossingIndex, rev_head: &[bool]) -> u64 {
    let mut sum = 0u64;
    for layer in &plan.layers {
        for (i, &e) in layer.iter().enumerate() {
            if is_forward_branch_source(plan, xidx, e, rev_head) {
                sum += i as u64;
            }
        }
    }
    sum
}

/// Within each layer's branch-source positions, put forward sources before
/// FAS-reverse heads (stable within each class). Does not move sources
/// across non-sources — preserves global orientation while preferring
/// forward sources ahead of reverse-spine heads inside the source pocket.
fn order_branch_source_pocket(plan: &mut PlanGraph, xidx: &CrossingIndex, rev_head: &[bool]) {
    for r in 0..plan.layers.len() {
        let layer = &plan.layers[r];
        if layer.len() < 2 {
            continue;
        }
        let mut idxs = Vec::new();
        let mut forward = Vec::new();
        let mut reverse_heads = Vec::new();
        for (i, &e) in layer.iter().enumerate() {
            if !is_branch_source(plan, xidx, e) {
                continue;
            }
            idxs.push(i);
            if rev_head[e] {
                reverse_heads.push(e);
            } else {
                forward.push(e);
            }
        }
        if idxs.len() < 2 || forward.is_empty() || reverse_heads.is_empty() {
            continue;
        }
        let mut ordered = forward;
        ordered.append(&mut reverse_heads);
        let mut next = plan.layers[r].clone();
        for (slot, elem) in idxs.into_iter().zip(ordered) {
            next[slot] = elem;
        }
        if next != plan.layers[r] {
            plan.layers[r] = next;
        }
    }
}

/// Prefer the global left/right orientation with smaller `source_moment`.
/// Bipartite crossings (and total span under a full mirror) are invariant;
/// only the reading-direction of branch sources changes.
fn choose_layer_orientation(plan: &mut PlanGraph, xidx: &CrossingIndex, rev_head: &[bool]) {
    let has_branch = (0..plan.elems.len()).any(|e| is_branch_source(plan, xidx, e));
    if !has_branch {
        return;
    }
    let normal = order_score(plan, xidx, true, rev_head);
    let saved = plan.layers.clone();
    for layer in &mut plan.layers {
        layer.reverse();
    }
    for r in 0..plan.layers.len() {
        restore_partition_clamps(plan, r);
        restore_group_clamps(plan, r);
    }
    let mirrored = order_score(plan, xidx, true, rev_head);
    // Equal score → keep the lexicographically smaller layer vector.
    if mirrored > normal || (mirrored == normal && plan.layers > saved) {
        plan.layers = saved;
    }
}
fn total_order_span(plan: &PlanGraph, xidx: &CrossingIndex) -> u64 {
    let n = plan.elems.len();
    let mut span = 0u64;
    for (r, segs) in xidx.segs_by_pair.iter().enumerate() {
        if segs.is_empty() {
            continue;
        }
        let pos_a = reference_positions(n, &plan.layers[r]);
        let pos_b = reference_positions(n, &plan.layers[r + 1]);
        for &(u, v) in segs {
            let pa = pos_a[u];
            let pb = pos_b[v];
            if pa < 0 || pb < 0 {
                continue;
            }
            span += pa.abs_diff(pb) as u64;
        }
    }
    span
}

/// G4: pull 1:1 real leaves under their only neighbor by adjacent swaps that
/// do not increase crossings (and prefer reducing |Δorder|).
fn tighten_one_to_one(plan: &mut PlanGraph, xidx: &CrossingIndex) {
    let n = plan.elems.len();
    let mut down_real = vec![Vec::new(); n];
    let mut up_real = vec![Vec::new(); n];
    for s in &plan.segments {
        if !plan.elems[s.from].key.is_virtual()
            && !plan.elems[s.from].key.is_boundary()
            && !plan.elems[s.to].key.is_virtual()
            && !plan.elems[s.to].key.is_boundary()
        {
            down_real[s.from].push(s.to);
            up_real[s.to].push(s.from);
        }
    }
    let base = total_crossings(plan, xidx);
    let mut cur = base;
    for r in 0..plan.layers.len() {
        let layer_len = plan.layers[r].len();
        for i in 0..layer_len.saturating_sub(1) {
            let a = plan.layers[r][i];
            let b = plan.layers[r][i + 1];
            if plan.elems[a].key.is_zero_width() || plan.elems[b].key.is_zero_width() {
                continue;
            }
            let a_up = up_real[a].len() == 1;
            let b_up = up_real[b].len() == 1;
            let a_dn = down_real[a].len() == 1;
            let b_dn = down_real[b].len() == 1;
            if !(a_up && b_up) && !(a_dn && b_dn) {
                continue;
            }
            let order_of = |p: &PlanGraph, e: usize| -> isize {
                let rank = p.elems[e].rank as usize;
                p.layers[rank].iter().position(|&x| x == e).unwrap_or(0) as isize
            };
            let score = |p: &PlanGraph| -> isize {
                let mut s = 0isize;
                for &e in &p.layers[r] {
                    if up_real[e].len() == 1 {
                        s += (order_of(p, e) - order_of(p, up_real[e][0])).abs();
                    }
                    if down_real[e].len() == 1 {
                        s += (order_of(p, e) - order_of(p, down_real[e][0])).abs();
                    }
                }
                s
            };
            let before = score(plan);
            let d = delta_swap(plan, xidx, r, i);
            plan.layers[r].swap(i, i + 1);
            if cur as i64 + d > base as i64 || score(plan) > before {
                plan.layers[r].swap(i, i + 1);
            } else {
                cur = (cur as i64 + d) as u64;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// Reference layer is `r - 1` (used during the down-sweep).
    Up,
    /// Reference layer is `r + 1` (used during the up-sweep).
    Down,
}

/// elem → position lookup for one layer, dense over `plan.elems` (−1 = not
/// in this layer). O(n) build, O(1) probes — same answers as the old
/// `BTreeMap` build per call.
fn reference_positions(n: usize, layer: &[usize]) -> Vec<i64> {
    let mut pos = vec![-1i64; n];
    for (i, &e) in layer.iter().enumerate() {
        pos[e] = i as i64;
    }
    pos
}

struct Key {
    median: Option<f64>,
    barycenter: Option<f64>,
    /// Position within the layer *before* this sort (stable fallback).
    prev_pos: usize,
    /// Declaration index (final tie-break).
    repr_decl: usize,
}

/// Quantize neighbor positions for a total-order sort key (avoids ε-threshold
/// non-transitivity that panics driftsort at wide layers).
const QUANT: f64 = 1024.0;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    /// Quantized median; missing → `prev_pos` (keep place).
    median: i64,
    barycenter: i64,
    prev_pos: usize,
    repr_decl: usize,
}

fn sort_key(k: &Key) -> SortKey {
    SortKey {
        median: (k.median.unwrap_or(k.prev_pos as f64) * QUANT).round() as i64,
        barycenter: (k.barycenter.unwrap_or(k.prev_pos as f64) * QUANT).round() as i64,
        prev_pos: k.prev_pos,
        repr_decl: k.repr_decl,
    }
}

fn median_of(mut positions: Vec<f64>) -> Option<f64> {
    if positions.is_empty() {
        return None;
    }
    positions.sort_by(f64::total_cmp);
    let m = positions.len();
    if m % 2 == 1 {
        return Some(positions[m / 2]);
    }
    if m == 2 {
        return Some((positions[0] + positions[1]) / 2.0);
    }
    let left = positions[m / 2 - 1];
    let right = positions[m / 2];
    let left_spread = right - positions[0];
    let right_spread = positions[m - 1] - left;
    if left_spread + right_spread <= EPS {
        Some((left + right) / 2.0)
    } else {
        Some((left * right_spread + right * left_spread) / (left_spread + right_spread))
    }
}

fn weighted_barycenter(pairs: &[(f64, f64)]) -> Option<f64> {
    let wsum: f64 = pairs.iter().map(|(_, w)| w).sum();
    if wsum <= EPS {
        return None;
    }
    Some(pairs.iter().map(|(p, w)| p * w).sum::<f64>() / wsum)
}

fn reorder_layer(plan: &mut PlanGraph, adj: &Adjacency, r: usize, dir: Direction) {
    let n = plan.elems.len();
    let primary_pos = match dir {
        Direction::Up => reference_positions(n, &plan.layers[r - 1]),
        Direction::Down => reference_positions(n, &plan.layers[r + 1]),
    };
    let fallback_pos = match dir {
        Direction::Up => plan.layers.get(r + 1).map(|l| reference_positions(n, l)),
        Direction::Down if r > 0 => Some(reference_positions(n, &plan.layers[r - 1])),
        Direction::Down => None,
    };
    let (primary_n, fallback_n): (&[Vec<(usize, f64)>], &[Vec<(usize, f64)>]) = match dir {
        Direction::Up => (&adj.up, &adj.down),
        Direction::Down => (&adj.down, &adj.up),
    };

    let layer = plan.layers[r].clone();
    let mut keyed: Vec<(Key, usize)> = layer
        .iter()
        .enumerate()
        .map(|(prev_pos, &e)| {
            let mut pooled = Vec::new();
            // Prefer real neighbors so FAS reverse dummies do not outrank
            // forward parent order when placing reals.
            for &(n, w) in &primary_n[e] {
                if plan.elems[n].key.is_virtual() {
                    continue;
                }
                let p = primary_pos[n];
                if p >= 0 {
                    pooled.push((p as f64, w));
                }
            }
            if pooled.is_empty() {
                for &(n, w) in &primary_n[e] {
                    let p = primary_pos[n];
                    if p >= 0 {
                        pooled.push((p as f64, w));
                    }
                }
            }
            if pooled.is_empty() {
                if let Some(ref fpos) = fallback_pos {
                    for &(n, w) in &fallback_n[e] {
                        if plan.elems[n].key.is_virtual() {
                            continue;
                        }
                        let p = fpos[n];
                        if p >= 0 {
                            pooled.push((p as f64, w));
                        }
                    }
                    if pooled.is_empty() {
                        for &(n, w) in &fallback_n[e] {
                            let p = fpos[n];
                            if p >= 0 {
                                pooled.push((p as f64, w));
                            }
                        }
                    }
                }
            }
            let positions: Vec<f64> = pooled.iter().map(|(p, _)| *p).collect();
            let key = Key {
                median: median_of(positions),
                barycenter: weighted_barycenter(&pooled),
                prev_pos,
                repr_decl: plan.decl_index[e],
            };
            (key, e)
        })
        .collect();

    keyed.sort_by_key(|(k, _)| sort_key(k));
    plan.layers[r] = keyed.into_iter().map(|(_, e)| e).collect();
    restore_partition_clamps(plan, r);
    restore_group_clamps(plan, r);
}

/// Re-establish partition column blocks after a group-agnostic median /
/// transpose pass (partition-grid.md PG-1): column blocks in DECLARATION
/// order, owned elems ([`PlanGraph::partition_elem_col`]) inside their
/// column's L/R clamps, everything else ejected to the free zones — same
/// eject policy as [`restore_group_clamps`]. Runs OUTSIDE group restore:
/// a column block contains whole group blocks, and group restore then
/// re-seats the nested clamps.
fn restore_partition_clamps(plan: &mut PlanGraph, r: usize) {
    use crate::layout::hierarchical::model::{BoundarySide, ElemKey};

    if plan.partition_columns.is_empty() {
        return;
    }
    let columns = plan.partition_columns.clone();
    let col_pos: BTreeMap<String, usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.clone(), i))
        .collect();
    let clamp_of = |column: &str, side: BoundarySide| -> Option<usize> {
        plan.index_of
            .get(&ElemKey::PartitionBoundary {
                axis: column.to_string(),
                rank: r as u32,
                side,
            })
            .copied()
    };

    // Normalize inverted clamps first (a sweep may flip L/R) so the scan
    // below sees one well-formed block per column.
    for col in &columns {
        let (Some(l), Some(rr)) = (
            clamp_of(col, BoundarySide::Left),
            clamp_of(col, BoundarySide::Right),
        ) else {
            continue;
        };
        let layer = &mut plan.layers[r];
        let (Some(lp), Some(rp)) = (
            layer.iter().position(|&e| e == l),
            layer.iter().position(|&e| e == rr),
        ) else {
            continue;
        };
        if lp > rp {
            layer.swap(lp, rp);
        }
    }

    let old = plan.layers[r].clone();
    let mut blocks: Vec<Vec<usize>> = vec![Vec::new(); columns.len()];
    let mut ejected: Vec<Vec<usize>> = vec![Vec::new(); columns.len()];
    let mut free_lead = Vec::new();
    let mut free_trail = Vec::new();
    let mut current: Option<usize> = None;
    let mut seen_block = false;
    for &e in &old {
        match &plan.elems[e].key {
            ElemKey::PartitionBoundary {
                axis,
                side: BoundarySide::Left,
                ..
            } => {
                current = col_pos.get(axis).copied();
                seen_block = true;
            }
            ElemKey::PartitionBoundary {
                side: BoundarySide::Right,
                ..
            } => {
                current = None;
            }
            _ => {
                // Owned content (assigned members, single-column group
                // clamps) ALWAYS re-enters its band — a sweep may push an
                // elem outside the blocks, and free-zone treatment would
                // strand it there. Only unowned elems (virtuals, unassigned
                // reals) follow the in-block / eject / free-zone policy.
                let owned = plan.partition_elem_col.get(e).copied().flatten();
                match owned {
                    Some(ci) => blocks[ci].push(e),
                    None => match current {
                        Some(ci) => ejected[ci].push(e),
                        None if seen_block => free_trail.push(e),
                        None => free_lead.push(e),
                    },
                }
            }
        }
    }

    // Keep the leading free zone at its pre-eject size (same cross-rank
    // alignment argument as `restore_group_clamps`).
    let mut new_layer = Vec::with_capacity(old.len());
    new_layer.extend(free_lead);
    for (ci, col) in columns.iter().enumerate() {
        let (Some(l), Some(rr)) = (
            clamp_of(col, BoundarySide::Left),
            clamp_of(col, BoundarySide::Right),
        ) else {
            continue;
        };
        new_layer.push(l);
        new_layer.extend(blocks[ci].iter().copied());
        new_layer.push(rr);
        new_layer.extend(ejected[ci].iter().copied());
    }
    new_layer.extend(free_trail);
    plan.layers[r] = new_layer;
}

/// Re-pack after a group-agnostic median/transpose: keep Left/Right where
/// the soft weights placed them (cross-rank alignment), eject foreign elems
/// from the open interval, and pull any escaped members back inside.
fn restore_group_clamps(plan: &mut PlanGraph, r: usize) {
    use crate::layout::hierarchical::model::{BoundarySide, ElemKey};

    let mut clamps: Vec<(usize, String, usize, usize)> = Vec::new();
    for &ei in &plan.layers[r] {
        if let ElemKey::GroupBoundary {
            group,
            side: BoundarySide::Left,
            ..
        } = &plan.elems[ei].key
        {
            let right_key = ElemKey::GroupBoundary {
                group: group.clone(),
                rank: r as u32,
                side: BoundarySide::Right,
            };
            let Some(&right_ei) = plan.index_of.get(&right_key) else {
                continue;
            };
            let depth = plan.elems[ei].group_path.len();
            clamps.push((depth, group.clone(), ei, right_ei));
        }
    }
    clamps.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    for (_, group, left_ei, right_ei) in clamps {
        let layer = plan.layers[r].clone();
        let Some(mut lpos) = layer.iter().position(|&e| e == left_ei) else {
            continue;
        };
        let Some(mut rpos) = layer.iter().position(|&e| e == right_ei) else {
            continue;
        };
        if lpos > rpos {
            plan.layers[r].swap(lpos, rpos);
            std::mem::swap(&mut lpos, &mut rpos);
        }
        if lpos == rpos {
            continue;
        }

        let is_member = |e: usize| {
            e != left_ei && e != right_ei && plan.elems[e].group_path.iter().any(|g| g == &group)
        };

        let mut before = Vec::new();
        let mut interior = Vec::new();
        let mut foreign = Vec::new();
        let mut after = Vec::new();
        for (i, &e) in layer.iter().enumerate() {
            if e == left_ei || e == right_ei {
                continue;
            }
            if is_member(e) {
                interior.push(e);
            } else if i < lpos {
                before.push(e);
            } else if i > rpos {
                after.push(e);
            } else {
                foreign.push(e);
            }
        }

        // Keep Left at `before.len()` (same index as before the eject) so
        // cross-rank high-weight alignment survives the pack.
        let mut new_layer = Vec::with_capacity(layer.len());
        new_layer.extend(before);
        new_layer.push(left_ei);
        new_layer.extend(interior);
        new_layer.push(right_ei);
        new_layer.extend(foreign);
        new_layer.extend(after);
        plan.layers[r] = new_layer;
    }
}

fn total_crossings(plan: &PlanGraph, xidx: &CrossingIndex) -> u64 {
    let mut total = 0u64;
    for r in 0..xidx.segs_by_pair.len() {
        let segs = &xidx.segs_by_pair[r];
        if segs.is_empty() {
            continue;
        }
        total += plotgram_algo::crossing::count_bipartite_crossings(
            &plan.layers[r],
            &plan.layers[r + 1],
            segs,
        );
    }
    total
}

/// Crossing delta if adjacent elems at `layers[r][i]` and `[i+1]` are swapped.
/// O(deg(u)·deg(v)) against the two neighboring bipartite interfaces.
fn delta_swap(plan: &PlanGraph, xidx: &CrossingIndex, r: usize, i: usize) -> i64 {
    let u = plan.layers[r][i];
    let v = plan.layers[r][i + 1];
    let n = plan.elems.len();
    let mut delta = 0i64;

    if r > 0 {
        let pos = reference_positions(n, &plan.layers[r - 1]);
        for &a in &xidx.up[u] {
            let pa = pos[a];
            if pa < 0 {
                continue;
            }
            for &b in &xidx.up[v] {
                let pb = pos[b];
                if pb < 0 {
                    continue;
                }
                delta += match pa.cmp(&pb) {
                    Ordering::Greater => -1,
                    Ordering::Less => 1,
                    Ordering::Equal => 0,
                };
            }
        }
    }
    if r + 1 < plan.layers.len() {
        let pos = reference_positions(n, &plan.layers[r + 1]);
        for &c in &xidx.down[u] {
            let pc = pos[c];
            if pc < 0 {
                continue;
            }
            for &d in &xidx.down[v] {
                let pd = pos[d];
                if pd < 0 {
                    continue;
                }
                delta += match pc.cmp(&pd) {
                    Ordering::Greater => -1,
                    Ordering::Less => 1,
                    Ordering::Equal => 0,
                };
            }
        }
    }
    delta
}

/// Global `total_order_span` change from swapping adjacent `u = layers[r][i]`
/// and `v = layers[r][i+1]`: only segments incident to u/v at the two
/// neighboring interfaces contribute, and the arithmetic is integer-exact,
/// so `delta < 0` decides exactly like the old before/after full recount.
fn span_swap_delta(plan: &PlanGraph, xidx: &CrossingIndex, r: usize, i: usize) -> i64 {
    let u = plan.layers[r][i];
    let v = plan.layers[r][i + 1];
    let n = plan.elems.len();
    // Old positions: u→i, v→i+1; after the swap: u→i+1, v→i.
    let (old_u, new_u) = (i as i64, i as i64 + 1);
    let (old_v, new_v) = (i as i64 + 1, i as i64);
    let mut delta = 0i64;
    if r > 0 {
        let pos = reference_positions(n, &plan.layers[r - 1]);
        for &(e, old_p, new_p) in &[(u, old_u, new_u), (v, old_v, new_v)] {
            for &a in &xidx.up[e] {
                let pa = pos[a];
                if pa < 0 {
                    continue;
                }
                delta += (new_p - pa).abs() - (old_p - pa).abs();
            }
        }
    }
    if r + 1 < plan.layers.len() {
        let pos = reference_positions(n, &plan.layers[r + 1]);
        for &(e, old_p, new_p) in &[(u, old_u, new_u), (v, old_v, new_v)] {
            for &c in &xidx.down[e] {
                let pc = pos[c];
                if pc < 0 {
                    continue;
                }
                delta += (new_p - pc).abs() - (old_p - pc).abs();
            }
        }
    }
    delta
}

/// Adjacent swaps: always accept crossing reductions; when `use_span` and
/// crossings are flat, accept span reductions.
fn transpose_pass(plan: &mut PlanGraph, xidx: &CrossingIndex, use_span: bool) {
    let budget = plan.elems.len() + plan.layers.len() * 4 + 32;
    for _ in 0..budget {
        let mut improved = false;
        for r in 0..plan.layers.len() {
            let mut i = 0;
            while i + 1 < plan.layers[r].len() {
                let d = delta_swap(plan, xidx, r, i);
                if d < 0 {
                    plan.layers[r].swap(i, i + 1);
                    improved = true;
                } else if d == 0 && use_span {
                    if span_swap_delta(plan, xidx, r, i) < 0 {
                        plan.layers[r].swap(i, i + 1);
                        improved = true;
                    }
                }
                i += 1;
            }
        }
        if !improved {
            break;
        }
    }
}

/// Chain blocks (yfiles/16 §4, Bachmaier 2010): all dummies of one long
/// edge form ONE ordering unit. Every member sits in a different layer, so
/// a side switch needs all covered layers to move together — per-layer
/// local moves each look flat and the local optimum never exits.
fn chain_blocks(plan: &PlanGraph) -> Vec<Vec<usize>> {
    use crate::layout::hierarchical::model::ElemKey;
    let mut by_edge: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (e, elem) in plan.elems.iter().enumerate() {
        if let ElemKey::Virtual { edge_id, .. } = &elem.key {
            by_edge.entry(edge_id.clone()).or_default().push(e);
        }
    }
    by_edge
        .into_values()
        .filter(|c| c.len() >= 2)
        .map(|mut c| {
            c.sort_by_key(|&e| (plan.elems[e].rank, e));
            c
        })
        .collect()
}

/// Real endpoints of a dummy chain (the two `ElemKey::Real` ends of the
/// chain's `edge_id`). `None` if the chain is malformed.
fn chain_real_ends(plan: &PlanGraph, chain: &[usize]) -> Option<(usize, usize)> {
    use crate::layout::hierarchical::model::ElemKey;
    let ElemKey::Virtual { edge_id, .. } = &plan.elems[chain[0]].key else {
        return None;
    };
    let mut reals = Vec::new();
    for s in &plan.segments {
        if s.edge_id != *edge_id {
            continue;
        }
        for end in [s.from, s.to] {
            if matches!(plan.elems[end].key, ElemKey::Real(_)) {
                reals.push(end);
            }
        }
    }
    reals.sort_unstable();
    reals.dedup();
    match reals.as_slice() {
        &[a, b] => Some((a, b)),
        _ => None,
    }
}

/// How many real nodes sit strictly between each dummy and the slot that
/// matches the two real endpoints' layer-relative positions (lerp by rank).
/// Lower = corridor parked on the endpoint side. Block-trial only — not
/// part of global `J_order`.
fn chain_endpoint_inversion(plan: &PlanGraph, chain: &[usize]) -> u64 {
    use crate::layout::hierarchical::model::ElemKey;
    let Some((src, tgt)) = chain_real_ends(plan, chain) else {
        return 0;
    };
    let pos = plan.layer_positions();
    let src_r = plan.elems[src].rank as usize;
    let tgt_r = plan.elems[tgt].rank as usize;
    let src_len = plan.layers.get(src_r).map(|l| l.len()).unwrap_or(0);
    let tgt_len = plan.layers.get(tgt_r).map(|l| l.len()).unwrap_or(0);
    if src_len == 0 || tgt_len == 0 {
        return 0;
    }
    let src_frac = (pos[src] as f64 + 0.5) / src_len as f64;
    let tgt_frac = (pos[tgt] as f64 + 0.5) / tgt_len as f64;
    let rs = plan.elems[src].rank as i64;
    let rt = plan.elems[tgt].rank as i64;
    let mut inv = 0u64;
    for &d in chain {
        let r = plan.elems[d].rank as usize;
        let layer = &plan.layers[r];
        let n = layer.len();
        if n == 0 {
            continue;
        }
        let p = pos[d] as i64;
        let rd = plan.elems[d].rank as i64;
        let end_frac = if rs == rt {
            src_frac
        } else {
            let t = (rd - rs) as f64 / (rt - rs) as f64;
            src_frac + t * (tgt_frac - src_frac)
        };
        let target = ((end_frac * n as f64).floor() as i64).clamp(0, n as i64 - 1);
        let lo = p.min(target);
        let hi = p.max(target);
        for (i, &e) in layer.iter().enumerate() {
            let i = i as i64;
            if i > lo && i < hi && matches!(plan.elems[e].key, ElemKey::Real(_)) {
                inv += 1;
            }
        }
    }
    inv
}

/// Block-trial lexicographic key. `inversion` is the chain-local secondary
/// key when crossings tie; `source_moment` / `total_span` stay later
/// tie-breaks. Not used by barycenter / element sifting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ChainBlockScore {
    crossings: u64,
    inversion: u64,
    source_moment: u64,
    total_span: u64,
}

/// Block-level sifting: slide each chain block through every layer it
/// covers simultaneously at the same **relative** slot (quantile `k/max_k`,
/// not a shared absolute index). A trial is eligible when crossings do not
/// increase; among eligible trials (including the start) pick by
/// [`ChainBlockScore`], then lexicographic layer order.
fn chain_block_sift_pass(plan: &mut PlanGraph, xidx: &CrossingIndex, rev_head: &[bool]) {
    for chain in chain_blocks(plan) {
        let base = order_score(plan, xidx, true, rev_head);
        let start = plan.layers.clone();
        let mut best_score = ChainBlockScore {
            crossings: base.crossings,
            inversion: chain_endpoint_inversion(plan, &chain),
            source_moment: base.source_moment,
            total_span: base.total_span,
        };
        let mut best_layers = start.clone();
        // Per member: its rank and the layer minus that member.
        let without: Vec<(usize, Vec<usize>)> = chain
            .iter()
            .map(|&e| {
                let r = plan.elems[e].rank as usize;
                let rest = start[r].iter().copied().filter(|&x| x != e).collect();
                (r, rest)
            })
            .collect();
        let max_k = without
            .iter()
            .map(|(_, rest)| rest.len())
            .max()
            .unwrap_or(0);
        for k in 0..=max_k {
            let mut trial = start.clone();
            for ((r, rest), &e) in without.iter().zip(&chain) {
                let n = rest.len();
                let at = if max_k == 0 { 0 } else { (k * n) / max_k };
                let mut layer = rest.clone();
                layer.insert(at.min(n), e);
                trial[*r] = layer;
            }
            plan.layers = trial;
            for &(r, _) in &without {
                restore_partition_clamps(plan, r);
                restore_group_clamps(plan, r);
            }
            let score = order_score(plan, xidx, true, rev_head);
            if score.crossings > base.crossings {
                continue;
            }
            let cand = ChainBlockScore {
                crossings: score.crossings,
                inversion: chain_endpoint_inversion(plan, &chain),
                source_moment: score.source_moment,
                total_span: score.total_span,
            };
            if cand < best_score || (cand == best_score && plan.layers < best_layers) {
                best_score = cand;
                best_layers = plan.layers.clone();
            }
        }
        plan.layers = best_layers;
    }
}

/// Sifting: slide each non-zero-width elem through its layer, keep best `J_order`.
/// Crosses zero-width dummies that block adjacent transpose.
fn sift_pass(plan: &mut PlanGraph, xidx: &CrossingIndex, rev_head: &[bool]) {
    let layer_count = plan.layers.len();
    for r in 0..layer_count {
        if plan.layers[r].len() < 2 {
            continue;
        }
        let mut candidates: Vec<usize> = plan.layers[r]
            .iter()
            .copied()
            .filter(|&e| !plan.elems[e].key.is_zero_width())
            .collect();
        candidates.sort_by_key(|&e| plan.decl_index[e]);

        for elem in candidates {
            if !plan.layers[r].iter().any(|&e| e == elem) {
                continue;
            }
            let without: Vec<usize> = plan.layers[r]
                .iter()
                .copied()
                .filter(|&e| e != elem)
                .collect();
            let mut best_layer = plan.layers[r].clone();
            let mut best = order_score(plan, xidx, true, rev_head);

            for to in 0..=without.len() {
                let mut trial = without.clone();
                trial.insert(to, elem);
                plan.layers[r] = trial;
                restore_partition_clamps(plan, r);
                restore_group_clamps(plan, r);
                let score = order_score(plan, xidx, true, rev_head);
                if score < best || (score == best && plan.layers[r] < best_layer) {
                    best = score;
                    best_layer = plan.layers[r].clone();
                }
            }
            plan.layers[r] = best_layer;
            restore_partition_clamps(plan, r);
            restore_group_clamps(plan, r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::boundary::insert_group_boundaries;
    use crate::layout::hierarchical::model::{BoundarySide, ElemKey, Segment};

    fn plain_elem(id: &str, rank: u32, group: &[&str]) -> Elem {
        Elem {
            key: ElemKey::Real(id.to_string()),
            group_path: group.iter().map(|s| s.to_string()).collect(),
            rank,
        }
    }

    /// K3,3-ish crossing graph: layer0 = [a0,a1], layer1 = [b0,b1],
    /// edges wired so the identity order has crossings and a rearrangement
    /// removes them.
    fn crossing_plan() -> PlanGraph {
        let elems = vec![
            plain_elem("a0", 0, &[]),
            plain_elem("a1", 0, &[]),
            plain_elem("b0", 1, &[]),
            plain_elem("b1", 1, &[]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 3,
            }, // a0-b1
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 1,
                to: 2,
            }, // a1-b0
        ];
        let layers = vec![vec![0, 1], vec![2, 3]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = vec![0, 1, 2, 3];
        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
            ..Default::default()
        }
    }

    #[test]
    fn ordering_removes_a_crossing() {
        let mut plan = crossing_plan();
        let xidx = build_crossing_index(&plan);
        assert_eq!(total_crossings(&plan, &xidx), 1);
        order_layers(&mut plan, &BTreeMap::new(), 16.0, &BTreeSet::new());
        let xidx = build_crossing_index(&plan);
        assert_eq!(total_crossings(&plan, &xidx), 0);
    }

    #[test]
    fn group_members_stay_contiguous_after_ordering() {
        // Layer 1 has a 2-member group {g0,g1} interleaved (by declaration)
        // with an ungrouped node u. After boundary insert + ordering, members
        // must sit between the group's Left/Right clamps.
        let elems = vec![
            plain_elem("s0", 0, &[]),
            plain_elem("s1", 0, &[]),
            plain_elem("s2", 0, &[]),
            plain_elem("g0", 1, &["g"]),
            plain_elem("u", 1, &[]),
            plain_elem("g1", 1, &["g"]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 3,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 2,
                to: 5,
            },
            Segment {
                edge_id: "e2".into(),
                ordinal: 0,
                from: 1,
                to: 4,
            },
        ];
        let layers = vec![vec![0, 1, 2], vec![3, 4, 5]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = vec![0, 1, 2, 3, 4, 5];
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
            ..Default::default()
        };

        insert_group_boundaries(&mut plan);
        order_layers(&mut plan, &BTreeMap::new(), 16.0, &BTreeSet::new());

        let layer1 = &plan.layers[1];
        let left = layer1.iter().position(|&e| {
            matches!(
                &plan.elems[e].key,
                ElemKey::GroupBoundary {
                    group,
                    side: BoundarySide::Left,
                    ..
                } if group == "g"
            )
        });
        let right = layer1.iter().position(|&e| {
            matches!(
                &plan.elems[e].key,
                ElemKey::GroupBoundary {
                    group,
                    side: BoundarySide::Right,
                    ..
                } if group == "g"
            )
        });
        let (left, right) = (
            left.expect("g Left boundary"),
            right.expect("g Right boundary"),
        );
        assert!(left < right, "Left must precede Right: {layer1:?}");
        let members: Vec<usize> = layer1
            .iter()
            .enumerate()
            .filter(|(_, &e)| {
                matches!(&plan.elems[e].key, ElemKey::Real(id) if id == "g0" || id == "g1")
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(members.len(), 2);
        assert!(
            members.iter().all(|&i| i > left && i < right),
            "members must sit between clamps: left={left} right={right} members={members:?}"
        );
    }

    #[test]
    fn deterministic_rerun() {
        let mut p1 = crossing_plan();
        let mut p2 = crossing_plan();
        order_layers(&mut p1, &BTreeMap::new(), 16.0, &BTreeSet::new());
        order_layers(&mut p2, &BTreeMap::new(), 16.0, &BTreeSet::new());
        assert_eq!(p1.layers, p2.layers);
    }

    /// Critical marks double the ordering pull of real-real / real-virtual
    /// segments (edge-parameters §2.5).
    #[test]
    fn critical_edge_doubles_ordering_weight() {
        let elems = vec![
            plain_elem("a0", 0, &[]),
            plain_elem("a1", 0, &[]),
            plain_elem("m0", 1, &[]),
            plain_elem("m1", 1, &[]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 2,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 1,
                to: 2,
            },
        ];
        let layers = vec![vec![0, 1], vec![2, 3]];
        let index_of: BTreeMap<ElemKey, usize> = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan_of = || PlanGraph {
            elems: elems.clone(),
            index_of: index_of.clone(),
            decl_index: vec![0, 1, 2, 3],
            segments: segments.clone(),
            layers: layers.clone(),
            ..Default::default()
        };

        let mut p = plan_of();
        order_layers(&mut p, &BTreeMap::new(), 16.0, &BTreeSet::new());
        assert_eq!(p.layers[1], vec![2, 3]);

        let weights: BTreeMap<String, f64> = [("e0".to_string(), 2.0)].into_iter().collect();
        let adj = build_adjacency(&plan_of(), &weights, 16.0);
        let ups: Vec<f64> = adj.up[2].iter().map(|&(_, w)| w).collect();
        assert_eq!(ups, vec![2.0, 1.0], "weighted real-real must weigh 2x");

        let real_a = plain_elem("ra", 0, &[]);
        let virt_a = Elem {
            key: ElemKey::Virtual {
                edge_id: "va".into(),
                ordinal: 0,
            },
            group_path: Vec::new(),
            rank: 1,
        };
        let virt_b = Elem {
            key: ElemKey::Virtual {
                edge_id: "vb".into(),
                ordinal: 1,
            },
            group_path: Vec::new(),
            rank: 2,
        };
        assert_eq!(
            segment_weight(&virt_a, &virt_b, 2.0, 16.0),
            8.0,
            "virtual-virtual corridor weight must not scale"
        );
        // Reversed segments are ordinary edges after P1: the 1/2/8 recipe
        // applies, no down-weight (2026-08-12 notes §10 D2).
        assert_eq!(
            segment_weight(&real_a, &virt_a, 2.0, 16.0),
            4.0,
            "real-virtual keeps the 2x base regardless of direction"
        );
    }

    /// Wide layers (≥18 elems) used to panic driftsort when `cmp_key` was not a
    /// total order; regression for review §2.1.
    fn wide_layer_plan(width: usize) -> PlanGraph {
        let n = width;
        let mut elems: Vec<Elem> = Vec::with_capacity(2 * n + 4);
        for i in 0..n {
            let id = format!("n{i}");
            elems.push(plain_elem(&id, 0, &[]));
        }
        for i in 0..n {
            let id = format!("m{i}");
            elems.push(plain_elem(&id, 1, &[]));
        }
        elems.push(plain_elem("iso_l0_a", 0, &[]));
        elems.push(plain_elem("iso_l0_b", 0, &[]));
        elems.push(plain_elem("iso_l1_a", 1, &[]));
        elems.push(plain_elem("iso_l1_b", 1, &[]));

        let iso_l0_a = 2 * n;
        let iso_l0_b = 2 * n + 1;
        let iso_l1_a = 2 * n + 2;
        let iso_l1_b = 2 * n + 3;

        let segments: Vec<Segment> = (0..n)
            .map(|i| Segment {
                edge_id: format!("e{i}"),
                ordinal: 0,
                from: i,
                to: n + ((i * 7 + 3) % n),
            })
            .collect();

        let layer0: Vec<usize> = (0..n).chain([iso_l0_a, iso_l0_b]).collect();
        let layer1: Vec<usize> = (n..2 * n).chain([iso_l1_a, iso_l1_b]).collect();
        let layers = vec![layer0, layer1];

        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index: Vec<usize> = (0..elems.len()).collect();

        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
            ..Default::default()
        }
    }

    #[test]
    fn ordering_survives_wide_layers() {
        for width in [18, 32, 64] {
            let mut plan = wide_layer_plan(width);
            let layer_len = width + 2;
            order_layers(&mut plan, &BTreeMap::new(), 16.0, &BTreeSet::new());
            assert_eq!(plan.layers[0].len(), layer_len, "width={width}");
            assert_eq!(plan.layers[1].len(), layer_len, "width={width}");
        }
    }

    /// PG-1: crossing sweeps must not break column blocks — after ordering,
    /// every layer keeps partition clamps in declaration order and assigned
    /// members sit inside their column's L/R clamps.
    #[test]
    fn partition_blocks_survive_crossing_sweeps() {
        use crate::layout::hierarchical::compose::partition_boundary::insert_partition_boundaries;
        use crate::layout::hierarchical::model::RealGraph;
        use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        // `left` declared before `right`; layer 1 declares the right member
        // first, and the criss-cross edges push the sweep to swap them.
        let elems = vec![
            plain_elem("l0", 0, &[]),
            plain_elem("r0", 0, &[]),
            plain_elem("r1", 1, &[]),
            plain_elem("l1", 1, &[]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 2,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 1,
                to: 3,
            },
        ];
        let layers = vec![vec![0, 1], vec![2, 3]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1, 2, 3],
            segments,
            layers,
            ..Default::default()
        };

        let mut real = RealGraph::default();
        real.partition = Some(PartitionGrid {
            columns: vec![PartitionAxis::new("left"), PartitionAxis::new("right")],
            rows: vec![],
        });
        real.ids = vec!["l0".into(), "r0".into(), "r1".into(), "l1".into()];
        for (i, id) in real.ids.iter().enumerate() {
            real.index_of.insert(id.clone(), i);
        }
        real.partition_cell = vec![
            Some(PartitionCell::col("left")),
            Some(PartitionCell::col("right")),
            Some(PartitionCell::col("right")),
            Some(PartitionCell::col("left")),
        ];

        insert_partition_boundaries(&mut plan, &real).unwrap();
        order_layers(&mut plan, &BTreeMap::new(), 16.0, &BTreeSet::new());

        let clamp_pos = |layer: &[usize], col: &str, side: BoundarySide| -> usize {
            layer
                .iter()
                .position(|&e| {
                    matches!(
                        &plan.elems[e].key,
                        ElemKey::PartitionBoundary { axis, side: s, .. }
                        if axis.as_str() == col && *s == side
                    )
                })
                .unwrap_or_else(|| panic!("missing {col} {side:?} clamp in {layer:?}"))
        };
        for layer in &plan.layers {
            let l_l = clamp_pos(layer, "left", BoundarySide::Left);
            let l_r = clamp_pos(layer, "left", BoundarySide::Right);
            let r_l = clamp_pos(layer, "right", BoundarySide::Left);
            let r_r = clamp_pos(layer, "right", BoundarySide::Right);
            assert!(
                l_l < l_r && l_r < r_l && r_l < r_r,
                "column declaration order broken: {layer:?}"
            );
            for (i, &e) in layer.iter().enumerate() {
                let Some(&Some(ci)) = plan.partition_elem_col.get(e) else {
                    continue;
                };
                let (lo, hi) = if ci == 0 { (l_l, l_r) } else { (r_l, r_r) };
                assert!(
                    i > lo && i < hi,
                    "elem {e} escaped its column band: pos {i}, band [{lo},{hi}]"
                );
            }
        }
    }

    /// Dummy between two reals must not freeze a crossing that sifting can clear
    /// (forward long-edge dummy — permeable).
    #[test]
    fn sifting_clears_crossing_across_dummy() {
        let elems = vec![
            plain_elem("src_a", 0, &[]),
            plain_elem("src_b", 0, &[]),
            plain_elem("leaf_a", 1, &[]),
            plain_elem("leaf_b", 1, &[]),
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e_fwd".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            },
            plain_elem("leaf_c", 1, &[]),
        ];
        // src_a→leaf_a, src_a→leaf_c, src_b→leaf_b, src_b→leaf_c —
        // with order [leaf_a, leaf_b, virt, leaf_c] one crossing.
        let segments = vec![
            Segment {
                edge_id: "a".into(),
                ordinal: 0,
                from: 0,
                to: 2,
            },
            Segment {
                edge_id: "b".into(),
                ordinal: 0,
                from: 0,
                to: 5,
            },
            Segment {
                edge_id: "c".into(),
                ordinal: 0,
                from: 1,
                to: 3,
            },
            Segment {
                edge_id: "d".into(),
                ordinal: 0,
                from: 1,
                to: 5,
            },
        ];
        let layers = vec![vec![0, 1], vec![2, 3, 4, 5]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1, 2, 3, 4, 5],
            segments,
            layers,
            ..Default::default()
        };
        let xidx = build_crossing_index(&plan);
        assert_eq!(total_crossings(&plan, &xidx), 1);
        order_layers(&mut plan, &BTreeMap::new(), 16.0, &BTreeSet::new());
        let xidx = build_crossing_index(&plan);
        assert_eq!(
            total_crossings(&plan, &xidx),
            0,
            "sifting should move n6 left of n23 across a forward dummy: {:?}",
            plan.layers[1]
        );
    }

    /// mech-e29 shape: a 2-dummy chain sits LEFT of the reals on both
    /// covered layers. No single-layer move ever beats the base crossing
    /// count (d1 alone is flat, d2 alone makes it worse), so per-layer
    /// transpose / element sifting never exit the local optimum — only the
    /// simultaneous block move reaches the 0-crossing side (2026-08-12
    /// notes §10.1(a)).
    fn chain_side_plan() -> PlanGraph {
        let elems = vec![
            plain_elem("a", 0, &[]), // 0
            plain_elem("b", 0, &[]), // 1
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e_long".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            }, // 2 = d1
            plain_elem("x", 1, &[]), // 3
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e_long".into(),
                    ordinal: 1,
                },
                group_path: Vec::new(),
                rank: 2,
            }, // 4 = d2
            plain_elem("y", 2, &[]), // 5
            plain_elem("c", 3, &[]), // 6
            plain_elem("z", 3, &[]), // 7
        ];
        let segments = vec![
            Segment {
                edge_id: "e_long".into(),
                ordinal: 0,
                from: 1,
                to: 2,
            }, // b → d1
            Segment {
                edge_id: "e_long".into(),
                ordinal: 1,
                from: 2,
                to: 4,
            }, // d1 → d2
            Segment {
                edge_id: "e_long".into(),
                ordinal: 2,
                from: 4,
                to: 6,
            }, // d2 → c
            Segment {
                edge_id: "f".into(),
                ordinal: 0,
                from: 0,
                to: 3,
            }, // a → x
            Segment {
                edge_id: "g".into(),
                ordinal: 0,
                from: 3,
                to: 5,
            }, // x → y
            Segment {
                edge_id: "h".into(),
                ordinal: 0,
                from: 5,
                to: 7,
            }, // y → z
            Segment {
                edge_id: "i".into(),
                ordinal: 0,
                from: 4,
                to: 7,
            }, // d2 → z
            Segment {
                edge_id: "j".into(),
                ordinal: 0,
                from: 5,
                to: 6,
            }, // y → c
        ];
        let layers = vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        PlanGraph {
            elems,
            index_of,
            decl_index: (0..8).collect(),
            segments,
            layers,
            ..Default::default()
        }
    }

    #[test]
    fn chain_block_sift_exits_two_layer_side_optimum() {
        let mut plan = chain_side_plan();
        let xidx = build_crossing_index(&plan);
        assert_eq!(total_crossings(&plan, &xidx), 2);
        // Single-layer probes never improve: moving d1 alone stays flat,
        // moving d2 alone increases crossings.
        let single_move_crossings = |which: usize| {
            let mut p = chain_side_plan();
            let r = p.elems[which].rank as usize;
            let rest: Vec<usize> = p.layers[r]
                .iter()
                .copied()
                .filter(|&e| e != which)
                .collect();
            let mut layer = rest;
            layer.push(which);
            p.layers[r] = layer;
            let xi = build_crossing_index(&p);
            total_crossings(&p, &xi)
        };
        assert_eq!(single_move_crossings(2), 2, "d1 alone must be flat");
        assert!(single_move_crossings(4) > 2, "d2 alone must not improve");

        let no_rev = reversed_heads(&plan, &BTreeSet::new());
        chain_block_sift_pass(&mut plan, &xidx, &no_rev);

        let xidx = build_crossing_index(&plan);
        assert_eq!(
            total_crossings(&plan, &xidx),
            1,
            "block move must reach the lower-crossing side: {:?}",
            plan.layers
        );
        let pos = |r: usize, e: usize| plan.layers[r].iter().position(|&x| x == e).unwrap();
        assert!(
            pos(1, 2) > pos(1, 3),
            "d1 must sit right of x: {:?}",
            plan.layers[1]
        );
        assert!(
            pos(2, 4) > pos(2, 5),
            "d2 must sit right of y: {:?}",
            plan.layers[2]
        );
    }

    /// Grouped plans never run chain-block sifting (gated with `sift_pass`);
    /// block trials on grouped layers must additionally keep group clamps
    /// well-formed — members between their Left/Right clamps — and the whole
    /// ordering stays deterministic.
    #[test]
    fn grouped_plan_keeps_clamps_and_is_deterministic() {
        let build = || {
            let elems = vec![
                plain_elem("s0", 0, &[]),
                plain_elem("s1", 0, &[]),
                plain_elem("g0", 1, &["g"]),
                plain_elem("g1", 1, &["g"]),
                Elem {
                    key: ElemKey::Virtual {
                        edge_id: "e_long".into(),
                        ordinal: 0,
                    },
                    group_path: Vec::new(),
                    rank: 1,
                },
                plain_elem("t0", 2, &[]),
            ];
            let segments = vec![
                Segment {
                    edge_id: "e0".into(),
                    ordinal: 0,
                    from: 0,
                    to: 2,
                },
                Segment {
                    edge_id: "e1".into(),
                    ordinal: 0,
                    from: 1,
                    to: 3,
                },
                Segment {
                    edge_id: "e_long".into(),
                    ordinal: 0,
                    from: 1,
                    to: 4,
                },
                Segment {
                    edge_id: "e_long".into(),
                    ordinal: 1,
                    from: 4,
                    to: 5,
                },
            ];
            let layers = vec![vec![0, 1], vec![2, 3, 4], vec![5]];
            let index_of = elems
                .iter()
                .enumerate()
                .map(|(i, e)| (e.key.clone(), i))
                .collect();
            let mut plan = PlanGraph {
                elems,
                index_of,
                decl_index: (0..6).collect(),
                segments,
                layers,
                ..Default::default()
            };
            insert_group_boundaries(&mut plan);
            plan
        };

        let mut p1 = build();
        let mut p2 = build();
        order_layers(&mut p1, &BTreeMap::new(), 16.0, &BTreeSet::new());
        order_layers(&mut p2, &BTreeMap::new(), 16.0, &BTreeSet::new());
        assert_eq!(
            p1.layers, p2.layers,
            "grouped ordering must be deterministic"
        );

        let layer1 = &p1.layers[1];
        let left = layer1
            .iter()
            .position(|&e| {
                matches!(
                    &p1.elems[e].key,
                    ElemKey::GroupBoundary { group, side: BoundarySide::Left, .. }
                    if group == "g"
                )
            })
            .expect("g Left clamp");
        let right = layer1
            .iter()
            .position(|&e| {
                matches!(
                    &p1.elems[e].key,
                    ElemKey::GroupBoundary { group, side: BoundarySide::Right, .. }
                    if group == "g"
                )
            })
            .expect("g Right clamp");
        assert!(left < right, "clamps well-formed: {layer1:?}");
        for id in ["g0", "g1"] {
            let pos = layer1
                .iter()
                .position(|&e| matches!(&p1.elems[e].key, ElemKey::Real(r) if r == id))
                .unwrap_or_else(|| panic!("{id} present"));
            assert!(
                pos > left && pos < right,
                "{id} must stay inside its clamps: {layer1:?}"
            );
        }
    }

    #[test]
    fn span_tiebreak_prefers_straighter_order() {
        // Two crossings-free orders; span should pull children under parents.
        let elems = vec![
            plain_elem("a0", 0, &[]),
            plain_elem("a1", 0, &[]),
            plain_elem("b0", 1, &[]),
            plain_elem("b1", 1, &[]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 2,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 1,
                to: 3,
            },
        ];
        // Start crossed in position but without crossings if we only had matching —
        // identity [b0,b1] under [a0,a1] is already optimal span 0.
        // Start with reversed children: span=2, crossings=2 → must fix both.
        let layers = vec![vec![0, 1], vec![3, 2]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1, 2, 3],
            segments,
            layers,
            ..Default::default()
        };
        order_layers(&mut plan, &BTreeMap::new(), 16.0, &BTreeSet::new());
        assert_eq!(plan.layers[1], vec![2, 3]);
        let xidx = build_crossing_index(&plan);
        assert_eq!(
            order_score(&plan, &xidx, true, &reversed_heads(&plan, &BTreeSet::new())).total_span,
            0
        );
    }
}
