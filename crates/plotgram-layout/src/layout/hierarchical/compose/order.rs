//! P6 ordering: flat median + transpose + sifting + best-snapshot.
//!
//! Objective `J_order` (composition.md §6):
//! `(weighted_crossings, total_span, lex_layers)` — crossings first, then
//! proper-segment order displacement (straightness), then stable lex.
//!
//! **Reverse-edge corridor barriers:** FAS-reversed long-edge dummies are
//! side-corridor anchors (G3 / back-loop). Sifting and transpose must not
//! move reals across them — otherwise a single bipartite crossing win can
//! destroy the leaf | corridor | spine pattern (yFiles Layout Styles L3:
//! `n23 [e30] n6`). Forward long-edge dummies stay permeable so sifting can
//! still clear ordinary crossings.
//!
//! Group containment is enforced upstream by
//! [`super::boundary::insert_group_boundaries`] (Left/Right clamps + high-weight
//! cross-rank segments). Crossing minimization is fully group-agnostic.

use std::collections::{BTreeMap, BTreeSet};
use std::cmp::Ordering;

use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph};

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
/// stays 8.0, weight does not apply).
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
    let adj = build_adjacency(plan, edge_weights, group_boundary_weight);
    let xidx = build_crossing_index(plan);
    // Span / sift / reverse-corridor are flat-DAG levers. Gate on **group
    // boundary dummies** (Weak continuous-block clamps), not mere `group_path`:
    // StrongMacro intra copies path onto members but inserts no Left/Right
    // boundaries — that block is a local flat DAG and must share this stack.
    // Weak global plans with boundaries keep the restricted path (Channel gate).
    let grouped = plan
        .elems
        .iter()
        .any(|e| e.key.is_group_boundary());
    let empty_rev = BTreeSet::new();
    let reversed = if grouped {
        &empty_rev
    } else {
        reversed_edges
    };

    let mut best = plan.layers.clone();
    let mut best_score = order_score(plan, &xidx, !grouped, reversed);
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
        transpose_pass(plan, &xidx, /*use_span*/ !grouped, reversed);
        for r in 0..plan.layers.len() {
            restore_partition_clamps(plan, r);
            restore_group_clamps(plan, r);
        }
        restore_reverse_corridors(plan, &xidx, reversed);

        let score = order_score(plan, &xidx, !grouped, reversed);
        let lex_better = score.corridor_breaks == best_score.corridor_breaks
            && score.crossings == best_score.crossings
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
    restore_reverse_corridors(plan, &xidx, reversed);
    if !super::super::CHANNEL_FORCE_ROOT.get() {
        super::boundary::align_group_left_pads(plan);
    }
    tighten_one_to_one(plan, &xidx);
    if !grouped {
        for _ in 0..8 {
            let before_layers = plan.layers.clone();
            let before = order_score(plan, &xidx, true, reversed);
            sift_pass(plan, &xidx, reversed);
            for r in 0..plan.layers.len() {
                restore_partition_clamps(plan, r);
                restore_group_clamps(plan, r);
            }
            restore_reverse_corridors(plan, &xidx, reversed);
            let after = order_score(plan, &xidx, true, reversed);
            if after > before {
                plan.layers = before_layers;
                break;
            }
            if after == before {
                break;
            }
        }
    }
}

/// Lexicographic ordering objective (composition.md §6).
///
/// `corridor_breaks` is primary on flat DAGs: FAS reverse dummies anchor a
/// side corridor (`leaf* | rev-dummy* | spine-cont*`); clearing a bipartite
/// crossing by pulling the spine across that dummy is not progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OrderScore {
    corridor_breaks: u64,
    crossings: u64,
    /// Σ |layer_pos(u) − layer_pos(v)| over proper segments (straightness).
    /// Zeroed when `use_span` is false (grouped plans).
    total_span: u64,
}

fn order_score(
    plan: &PlanGraph,
    xidx: &CrossingIndex,
    use_span: bool,
    reversed_edges: &BTreeSet<String>,
) -> OrderScore {
    OrderScore {
        corridor_breaks: count_corridor_breaks(plan, xidx, reversed_edges),
        crossings: total_crossings(plan, xidx),
        total_span: if use_span {
            total_order_span(plan, xidx)
        } else {
            0
        },
    }
}

/// Count hubs whose reverse-corridor pattern is broken on a layer:
/// some continuing spine sits left of a leaf/rev-dummy of the same hub.
///
/// Only scored when the hub has **both** leaves and continuing spines on
/// that layer — otherwise the reverse dummy's side is left to crossings/span
/// (east vs west back-loop), matching flowchart feedback edges.
fn count_corridor_breaks(
    plan: &PlanGraph,
    xidx: &CrossingIndex,
    reversed_edges: &BTreeSet<String>,
) -> u64 {
    if reversed_edges.is_empty() {
        return 0;
    }
    let mut breaks = 0u64;
    for layer in &plan.layers {
        let mut dummies_by_hub: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &e in layer {
            if !is_reverse_corridor_dummy(plan, e, reversed_edges) {
                continue;
            }
            let Some(hub) = climb_to_real(plan, xidx, e, true) else {
                continue;
            };
            dummies_by_hub.entry(hub).or_default().push(e);
        }
        for (hub, dummies) in dummies_by_hub {
            let pos = |e: usize| layer.iter().position(|&x| x == e).unwrap_or(usize::MAX);
            let mut leaf_max = 0usize;
            let mut has_leaf = false;
            let mut spine_min = usize::MAX;
            for &e in layer {
                if !matches!(plan.elems[e].key, ElemKey::Real(_)) {
                    continue;
                }
                if !xidx.up[e].contains(&hub) {
                    continue;
                }
                if has_real_down(plan, xidx, e) {
                    spine_min = spine_min.min(pos(e));
                } else {
                    has_leaf = true;
                    leaf_max = leaf_max.max(pos(e));
                }
            }
            if !has_leaf || spine_min == usize::MAX {
                continue;
            }
            let left_max = leaf_max.max(dummies.iter().map(|&e| pos(e)).max().unwrap_or(0));
            if left_max > spine_min {
                breaks += 1;
            }
        }
    }
    breaks
}

fn total_order_span(plan: &PlanGraph, xidx: &CrossingIndex) -> u64 {
    let mut span = 0u64;
    for (r, segs) in xidx.segs_by_pair.iter().enumerate() {
        if segs.is_empty() {
            continue;
        }
        let pos_a = reference_positions(&plan.layers[r]);
        let pos_b = reference_positions(&plan.layers[r + 1]);
        for &(u, v) in segs {
            let Some(&pa) = pos_a.get(&u) else {
                continue;
            };
            let Some(&pb) = pos_b.get(&v) else {
                continue;
            };
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

fn reference_positions(layer: &[usize]) -> BTreeMap<usize, usize> {
    layer.iter().enumerate().map(|(i, &e)| (e, i)).collect()
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
    /// `(false, q)` = has value; `(true, 0)` = no neighbors, sorts after all valued keys.
    median: (bool, i64),
    barycenter: (bool, i64),
    prev_pos: usize,
    repr_decl: usize,
}

fn quant(v: Option<f64>) -> (bool, i64) {
    match v {
        None => (true, 0),
        Some(x) => (false, (x * QUANT).round() as i64),
    }
}

fn sort_key(k: &Key) -> SortKey {
    SortKey {
        median: quant(k.median),
        barycenter: quant(k.barycenter),
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
    let ref_layer = match dir {
        Direction::Up => &plan.layers[r - 1],
        Direction::Down => &plan.layers[r + 1],
    };
    let ref_pos = reference_positions(ref_layer);
    let neighbors_of: &[Vec<(usize, f64)>] = match dir {
        Direction::Up => &adj.up,
        Direction::Down => &adj.down,
    };

    let layer = plan.layers[r].clone();
    let mut keyed: Vec<(Key, usize)> = layer
        .iter()
        .enumerate()
        .map(|(prev_pos, &e)| {
            let mut pooled = Vec::new();
            for &(n, w) in &neighbors_of[e] {
                if let Some(&p) = ref_pos.get(&n) {
                    pooled.push((p as f64, w));
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
                column: column.to_string(),
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
                column,
                side: BoundarySide::Left,
                ..
            } => {
                current = col_pos.get(column).copied();
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
            e != left_ei
                && e != right_ei
                && plan.elems[e].group_path.iter().any(|g| g == &group)
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
    let mut delta = 0i64;

    if r > 0 {
        let pos = reference_positions(&plan.layers[r - 1]);
        for &a in &xidx.up[u] {
            let Some(&pa) = pos.get(&a) else {
                continue;
            };
            for &b in &xidx.up[v] {
                let Some(&pb) = pos.get(&b) else {
                    continue;
                };
                delta += match pa.cmp(&pb) {
                    Ordering::Greater => -1,
                    Ordering::Less => 1,
                    Ordering::Equal => 0,
                };
            }
        }
    }
    if r + 1 < plan.layers.len() {
        let pos = reference_positions(&plan.layers[r + 1]);
        for &c in &xidx.down[u] {
            let Some(&pc) = pos.get(&c) else {
                continue;
            };
            for &d in &xidx.down[v] {
                let Some(&pd) = pos.get(&d) else {
                    continue;
                };
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

/// Adjacent swaps: always accept crossing reductions; when `use_span` and
/// crossings are flat, accept span reductions.
/// Never swap a real across a FAS-reversed corridor dummy.
fn transpose_pass(
    plan: &mut PlanGraph,
    xidx: &CrossingIndex,
    use_span: bool,
    reversed_edges: &BTreeSet<String>,
) {
    let budget = plan.elems.len() + plan.layers.len() * 4 + 32;
    for _ in 0..budget {
        let mut improved = false;
        for r in 0..plan.layers.len() {
            let mut i = 0;
            while i + 1 < plan.layers[r].len() {
                let u = plan.layers[r][i];
                let v = plan.layers[r][i + 1];
                if real_rev_corridor_adjacent(plan, u, v, reversed_edges) {
                    i += 1;
                    continue;
                }
                let d = delta_swap(plan, xidx, r, i);
                if d < 0 {
                    plan.layers[r].swap(i, i + 1);
                    improved = true;
                } else if d == 0 && use_span {
                    let before = total_order_span(plan, xidx);
                    plan.layers[r].swap(i, i + 1);
                    let after = total_order_span(plan, xidx);
                    if after < before {
                        improved = true;
                    } else {
                        plan.layers[r].swap(i, i + 1);
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

fn climb_to_real(plan: &PlanGraph, xidx: &CrossingIndex, start: usize, up: bool) -> Option<usize> {
    let mut e = start;
    for _ in 0..plan.layers.len().saturating_add(1) {
        if matches!(plan.elems[e].key, ElemKey::Real(_)) {
            return Some(e);
        }
        let next = if up {
            xidx.up[e].first().copied()
        } else {
            xidx.down[e].first().copied()
        }?;
        e = next;
    }
    None
}

fn has_real_down(plan: &PlanGraph, xidx: &CrossingIndex, e: usize) -> bool {
    xidx.down[e]
        .iter()
        .any(|&n| matches!(plan.elems[n].key, ElemKey::Real(_)))
}

/// Pack FAS-reversed corridor dummies between leaf children and continuing
/// children of the same hub (working-source real above the dummy).
///
/// Pattern: `… leaf*  [rev-corridor]*  spine-cont* …`
/// Mirrors yFiles back-loop corridors: a single crossing is not worth
/// pulling the downward spine inside the reverse dummy (write-authority:
/// corridor topology stays with Compose, not Ink).
///
/// Reorders **in place** over the slots already occupied by that hub's
/// related elems — unrelated nodes keep their indices (avoids scattering
/// the rest of the layer).
fn restore_reverse_corridors(
    plan: &mut PlanGraph,
    xidx: &CrossingIndex,
    reversed_edges: &BTreeSet<String>,
) {
    if reversed_edges.is_empty() {
        return;
    }
    for r in 0..plan.layers.len() {
        let layer = plan.layers[r].clone();
        let mut dummies_by_hub: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &e in &layer {
            if !is_reverse_corridor_dummy(plan, e, reversed_edges) {
                continue;
            }
            let Some(hub) = climb_to_real(plan, xidx, e, true) else {
                continue;
            };
            dummies_by_hub.entry(hub).or_default().push(e);
        }
        if dummies_by_hub.is_empty() {
            continue;
        }

        let mut new_layer = layer.clone();
        for (hub, dummies) in &dummies_by_hub {
            let mut leaves = Vec::new();
            let mut cont = Vec::new();
            for &e in &layer {
                if !matches!(plan.elems[e].key, ElemKey::Real(_)) {
                    continue;
                }
                if !xidx.up[e].contains(hub) {
                    continue;
                }
                if has_real_down(plan, xidx, e) {
                    cont.push(e);
                } else {
                    leaves.push(e);
                }
            }
            // No leaves ⇒ side of the reverse corridor is free (east/west);
            // do not pin dummies left of the spine.
            if leaves.is_empty() || cont.is_empty() {
                continue;
            }
            // Preserve relative order already present in `layer`.
            let order_of = |ids: &[usize]| -> Vec<usize> {
                layer.iter().copied().filter(|e| ids.contains(e)).collect()
            };
            let packed: Vec<usize> = {
                let mut v = order_of(&leaves);
                v.extend(order_of(dummies));
                v.extend(order_of(&cont));
                v
            };
            if packed.is_empty() {
                continue;
            }
            let mut slots: Vec<usize> = packed
                .iter()
                .filter_map(|&e| layer.iter().position(|&x| x == e))
                .collect();
            slots.sort_unstable();
            slots.dedup();
            if slots.len() != packed.len() {
                continue;
            }
            for (slot, &elem) in slots.iter().zip(packed.iter()) {
                new_layer[*slot] = elem;
            }
        }
        debug_assert_eq!(new_layer.len(), layer.len());
        plan.layers[r] = new_layer;
    }
}

fn is_reverse_corridor_dummy(
    plan: &PlanGraph,
    elem: usize,
    reversed_edges: &BTreeSet<String>,
) -> bool {
    match &plan.elems[elem].key {
        ElemKey::Virtual { edge_id, .. } => reversed_edges.contains(edge_id),
        _ => false,
    }
}

fn real_rev_corridor_adjacent(
    plan: &PlanGraph,
    a: usize,
    b: usize,
    reversed_edges: &BTreeSet<String>,
) -> bool {
    let a_rev = is_reverse_corridor_dummy(plan, a, reversed_edges);
    let b_rev = is_reverse_corridor_dummy(plan, b, reversed_edges);
    let a_real = matches!(plan.elems[a].key, ElemKey::Real(_));
    let b_real = matches!(plan.elems[b].key, ElemKey::Real(_));
    (a_rev && b_real) || (b_rev && a_real)
}

/// Inclusive-exclusive pocket `[lo, hi)` of `layer` containing `pos`, bounded
/// by reverse-corridor dummies (layer ends if none).
fn corridor_pocket(layer: &[usize], pos: usize, plan: &PlanGraph, reversed: &BTreeSet<String>) -> (usize, usize) {
    let mut lo = 0;
    for i in (0..pos).rev() {
        if is_reverse_corridor_dummy(plan, layer[i], reversed) {
            lo = i + 1;
            break;
        }
    }
    let mut hi = layer.len();
    for i in (pos + 1)..layer.len() {
        if is_reverse_corridor_dummy(plan, layer[i], reversed) {
            hi = i;
            break;
        }
    }
    (lo, hi)
}

/// Sifting: slide each non-zero-width elem through its layer, keep best `J_order`.
/// Forward dummies are permeable; FAS-reversed corridor dummies bound the pocket.
fn sift_pass(plan: &mut PlanGraph, xidx: &CrossingIndex, reversed_edges: &BTreeSet<String>) {
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
            let Some(from) = plan.layers[r].iter().position(|&e| e == elem) else {
                continue;
            };
            let (pocket_lo, pocket_hi) =
                corridor_pocket(&plan.layers[r], from, plan, reversed_edges);
            let without: Vec<usize> = plan.layers[r]
                .iter()
                .copied()
                .filter(|&e| e != elem)
                .collect();
            // Map full-layer pocket to insert indices in `without`.
            let insert_lo = pocket_lo;
            let insert_hi = pocket_hi.saturating_sub(1); // exclusive end after removal
            let mut best_layer = plan.layers[r].clone();
            let mut best = order_score(plan, xidx, true, reversed_edges);

            for to in insert_lo..=insert_hi {
                if to > without.len() {
                    break;
                }
                let mut trial = without.clone();
                trial.insert(to, elem);
                plan.layers[r] = trial;
                restore_partition_clamps(plan, r);
                restore_group_clamps(plan, r);
                let score = order_score(plan, xidx, true, reversed_edges);
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
                        ElemKey::PartitionBoundary { column, side: s, .. }
                        if column.as_str() == col && *s == side
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
            plain_elem("n26", 0, &[]),
            plain_elem("n5", 0, &[]),
            plain_elem("n25", 1, &[]),
            plain_elem("n23", 1, &[]),
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e_fwd".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            },
            plain_elem("n6", 1, &[]),
        ];
        // n26→n25, n26→n6, n5→n23, n5→n6 — with order [n25,n23,virt,n6] one crossing.
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

    /// FAS-reversed dummies are side-corridor anchors: reals must not sift
    /// across them even when that would clear a bipartite crossing
    /// (layout-styles L3: keep `n23 [e30] n6`, not `n6 n23 [e30]`).
    #[test]
    fn reverse_corridor_dummy_blocks_sift_across() {
        let elems = vec![
            plain_elem("n26", 0, &[]),
            plain_elem("n5", 0, &[]),
            plain_elem("n25", 1, &[]),
            plain_elem("n23", 1, &[]),
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e30".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            },
            plain_elem("n6", 1, &[]),
            plain_elem("n10", 2, &[]),
        ];
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
            Segment {
                edge_id: "e30".into(),
                ordinal: 0,
                from: 1,
                to: 4,
            },
            Segment {
                edge_id: "spine".into(),
                ordinal: 0,
                from: 5,
                to: 6,
            },
        ];
        let layers = vec![vec![0, 1], vec![2, 3, 4, 5], vec![6]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: (0..7).collect(),
            segments,
            layers,
            ..Default::default()
        };
        let reversed: BTreeSet<String> = ["e30".into()].into_iter().collect();
        order_layers(&mut plan, &BTreeMap::new(), 16.0, &reversed);
        let layer1 = &plan.layers[1];
        let pos = |id: &str| {
            layer1
                .iter()
                .position(|&e| match &plan.elems[e].key {
                    ElemKey::Real(x) => x == id,
                    ElemKey::Virtual { edge_id, .. } => edge_id == id,
                    _ => false,
                })
                .unwrap()
        };
        assert!(
            pos("n23") < pos("e30") && pos("e30") < pos("n6"),
            "leaf | reverse-corridor | spine must hold, got {:?}",
            layer1
                .iter()
                .map(|&e| match &plan.elems[e].key {
                    ElemKey::Real(id) => id.clone(),
                    ElemKey::Virtual { edge_id, .. } => format!("[{edge_id}]"),
                    _ => "?".into(),
                })
                .collect::<Vec<_>>()
        );
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
            order_score(&plan, &xidx, true, &BTreeSet::new()).total_span,
            0
        );
    }
}
