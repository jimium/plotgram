//! P4 symmetry objective: minimize edge straightness + hub-to-fan-center,
//! subject only to layer separation and VV dummy-chain equalities.
//!
//! Replaces the greedy SymmetryPlan (claimed / FanPack / rigid class) with an
//! iterative weighted-median + VPSC loop (review §4.3).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::Size;
use plotgram_algo::vpsc::{self, Constraint, Variable, VpscError};
use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::group_frame::{GROUP_FRAME_GAP, GROUP_LABEL_TOP_PAD, GROUP_PAD};
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::metric::bk;
use crate::layout::hierarchical::metric::partition_bands::{
    PartitionBandPlan, PARTITION_EMPTY_BAND_MIN,
};
use crate::layout::hierarchical::metric::symmetry::{
    axis_from_neighbors, degrees_of, fan_pitch, forward_real_adjacency, slot_multipliers,
    twin_plan_pairs, unique_min_span_primary,
};
use crate::layout::hierarchical::model::{BoundarySide, Elem, ElemKey, PlanGraph, RealGraph};
use crate::layout::hierarchical::params::{GroupPolicy, HierarchicalParams};

const REAL_WEIGHT: f64 = 1.0;
const VIRTUAL_WEIGHT: f64 = 4.0;

/// Solve cross-axis centers via J(x) iteration (review §4.3.2).
pub fn solve_symmetry_objective(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
    main: &[f64],
    params: &HierarchicalParams,
) -> Result<Vec<f64>, VpscError> {
    let n = plan.elems.len();
    if n == 0 {
        return Ok(Vec::new());
    }

    let bk = bk::bk_ideal(plan, size_of, params.node_gap);
    let nbs = segment_neighbors(plan, graph);
    let (down_nbs, up_nbs) = forward_real_adjacency(plan, graph);
    let down_deg = degrees_of(&down_nbs);
    let up_deg = degrees_of(&up_nbs);
    let twins = twin_plan_pairs(plan, graph);
    let primary = primary_arm_pairs(plan, &down_nbs, &up_nbs, &down_deg, &up_deg, &twins);
    // Constraint degradation chain: full (twin hard + cross-rank clamp
    // equalities) → twin soft → no clamp equalities. The gb equalities make
    // each clamp column the cross-rank frame edge; they can only cycle when
    // sibling group intervals swap order across ranks, in which case frames
    // cannot be disjoint anyway and we degrade gracefully. Partition band
    // separation rides EVERY level and never degrades — an infeasible mix
    // surfaces as `Infeasible`, never silently (partition-grid.md PG-1).
    let bands = PartitionBandPlan::build(plan, params.node_gap);
    let bands = bands.as_ref();
    let chains = [
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &twins,
            true,
            true,
            bands,
        ),
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &twins,
            false,
            true,
            bands,
        ),
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &twins,
            false,
            false,
            bands,
        ),
    ];
    let weights = vpsc_weights(plan, graph);
    let hubs: Vec<usize> = (0..n)
        .filter(|&e| {
            !plan.elems[e].key.is_virtual()
                && !plan.elems[e].key.is_zero_width()
                && (down_deg[e] >= 2 || up_deg[e] >= 2)
        })
        .collect();
    let segs_by_edge = plan.segments_by_edge();
    let layer_pos = plan.layer_positions();

    // Prefer the full chain; degrade only when it cycles with separation.
    let mut chain_idx = 0usize;
    let mut last_infeasible: Option<VpscError> = None;
    for (i, c) in chains.iter().enumerate() {
        match solve_once(n, &bk.ideal, &weights, c) {
            Ok(_) => {
                chain_idx = i;
                last_infeasible = None;
                break;
            }
            Err(e @ VpscError::Infeasible { .. }) => last_infeasible = Some(e),
            Err(e) => return Err(e),
        }
    }
    if let Some(e) = last_infeasible {
        return Err(e);
    }
    let hard = &chains[chain_idx];
    let hard_fallback = &chains[(chain_idx + 1).min(2)];

    // Feasible start: raw BK ideal may violate separation, so its J is not
    // comparable to post-VPSC iterates (would permanently win the snapshot).
    let mut x = solve_once(n, &bk.ideal, &weights, hard)?;
    let mut best = x.clone();
    let mut best_j = objective_j(
        plan,
        &x,
        &hubs,
        &down_nbs,
        &up_nbs,
        &down_deg,
        &up_deg,
        params.lambda_sym,
    );

    let k = params.symmetry_iters.max(1);
    for _ in 0..k {
        let mut desired = vec![0.0; n];
        for e in 0..n {
            desired[e] = weighted_median_desired(
                e,
                &x,
                &nbs,
                &twins,
                &primary,
                params,
                &layer_pos,
                plan,
            );
        }
        let alpha = params.lambda_sym / (1.0 + params.lambda_sym);
        for &h in &hubs {
            let c = center_h(&x, h, &down_nbs, &up_nbs, &down_deg, &up_deg);
            desired[h] = desired[h] * (1.0 - alpha) + c * alpha;
        }
        let mut iter_weights = weights.clone();
        for &h in &hubs {
            iter_weights[h] = REAL_WEIGHT * (1.0 + params.lambda_sym);
        }
        let owner_x = desired.clone();
        apply_port_anchor_desired(
            plan,
            graph,
            ports,
            size_of,
            main,
            &owner_x,
            &segs_by_edge,
            &mut desired,
            &mut iter_weights,
        );

        x = solve_once(n, &desired, &iter_weights, hard)?;
        let j = objective_j(
            plan,
            &x,
            &hubs,
            &down_nbs,
            &up_nbs,
            &down_deg,
            &up_deg,
            params.lambda_sym,
        );
        if j < best_j - 1e-9 {
            best_j = j;
            best.clone_from(&x);
        }
    }

    let mut cross = snap_fan_pack_style(
        plan,
        graph,
        ports,
        size_of,
        main,
        params,
        &best,
        &weights,
        hard,
        hard_fallback,
        &hubs,
        &down_nbs,
        &up_nbs,
        &down_deg,
        &up_deg,
        &twins,
        &primary,
        &layer_pos,
        &segs_by_edge,
    )?;
    // Sibling frame separation is guaranteed by the hard constraint set
    // (unrelated boundary-clamp pairs reserve GROUP_FRAME_GAP — D₂.1): the
    // post-VPSC step never pushes groups apart, it only rigidly closes
    // leftover excess slack down to GROUP_FRAME_GAP (separation is a lower
    // bound; median desired leaves slack that would balloon the canvas —
    // the J-side compactness term is the D₂.2 backlog item). Clamps are then
    // snapped onto the member-derived frame edges (boundary follows members;
    // frames already include pad — option B ruling). Weak-only: under
    // StrongMacro the macro-block writer owns frame geometry and local plans
    // carry no boundary clamps (strong-macro.md §5.3 / §8).
    if params.group_policy == GroupPolicy::Weak {
        close_sibling_frame_slack(plan, size_of, params.node_gap, main, &mut cross);
        snap_boundaries_to_members(plan, size_of, &mut cross);
        snap_partition_clamps_to_members(plan, size_of, params.node_gap, &mut cross);
    }
    Ok(cross)
}

/// Snap each partition column clamp onto its members' cross-axis extremes
/// ± band pad (partition-grid.md PG-1 post-solve step).
///
/// Mirrors [`snap_boundaries_to_members`]: clamp-only writer, never
/// re-solves. Members are hard-confined between their column's clamps and
/// adjacent bands are hard-separated by ≥ gap = pad, so the snapped edges
/// keep declaration order. Empty columns keep the solved position (the
/// `PARTITION_EMPTY_BAND_MIN` floor already shapes them).
fn snap_partition_clamps_to_members(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    band_pad: f64,
    cross: &mut [f64],
) {
    if plan.partition_columns.is_empty() {
        return;
    }
    // column index -> (min member left, max member right) across all ranks.
    let mut span: Vec<Option<(f64, f64)>> = vec![None; plan.partition_columns.len()];
    for (e_idx, elem) in plan.elems.iter().enumerate() {
        if !matches!(&elem.key, ElemKey::Real(_)) {
            continue;
        }
        let Some(&Some(ci)) = plan.partition_elem_col.get(e_idx) else {
            continue;
        };
        let w = size_of(e_idx).width;
        let left = cross[e_idx] - w / 2.0;
        let right = cross[e_idx] + w / 2.0;
        match &mut span[ci] {
            Some((lo, hi)) => {
                *lo = lo.min(left);
                *hi = hi.max(right);
            }
            slot => *slot = Some((left, right)),
        }
    }
    for (e_idx, elem) in plan.elems.iter().enumerate() {
        if let ElemKey::PartitionBoundary { column, side, .. } = &elem.key {
            let Some(ci) = plan.partition_columns.iter().position(|c| c == column) else {
                continue;
            };
            let Some(&(min_left, max_right)) = span[ci].as_ref() else {
                continue;
            };
            cross[e_idx] = match side {
                BoundarySide::Left => min_left - band_pad,
                BoundarySide::Right => max_right + band_pad,
            };
        }
    }
}

/// Final FanPack-style placement from an iterated seed (review §4.3.3 + §4.3.4).
fn snap_fan_pack_style(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
    main: &[f64],
    params: &HierarchicalParams,
    best: &[f64],
    weights: &[f64],
    hard: &[Constraint],
    hard_fallback: &[Constraint],
    hubs: &[usize],
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    twins: &BTreeSet<(usize, usize)>,
    primary: &BTreeSet<(usize, usize)>,
    layer_pos: &[usize],
    segs_by_edge: &BTreeMap<String, Vec<usize>>,
) -> Result<Vec<f64>, VpscError> {
    let n = plan.elems.len();
    let mut hub_order: Vec<usize> = hubs.to_vec();
    hub_order.sort_by(|&a, &b| {
        plan.elems[b]
            .rank
            .cmp(&plan.elems[a].rank)
            .then(a.cmp(&b))
    });

    let mut desired = best.to_vec();
    let mut iter_weights = weights.to_vec();
    for &h in &hub_order {
        let axis = center_h(best, h, down_nbs, up_nbs, down_deg, up_deg);
        desired[h] = axis;
        iter_weights[h] = REAL_WEIGHT * (1.0 + params.lambda_sym) * 8.0;
        pull_spine_to_axis(
            h, axis, plan, down_nbs, up_nbs, down_deg, up_deg, &mut desired, &mut iter_weights,
        );
        for side in [
            (down_deg[h] >= 2).then_some(&down_nbs[h]),
            (up_deg[h] >= 2).then_some(&up_nbs[h]),
        ]
        .into_iter()
        .flatten()
        {
            let mut leaves: Vec<usize> = side
                .iter()
                .copied()
                .filter(|&leaf| {
                    !plan.elems[leaf].key.is_virtual()
                        && !plan.elems[leaf].key.is_zero_width()
                        && !twins.contains(&undirected(h, leaf))
                        && !primary.contains(&undirected(h, leaf))
                })
                .collect();
            if leaves.is_empty() {
                continue;
            }
            leaves.sort_by(|&a, &b| layer_pos[a].cmp(&layer_pos[b]).then(a.cmp(&b)));
            if leaves.len() == 1 {
                let leaf = leaves[0];
                let pitch = (params.node_gap + size_of(h).width / 2.0 + size_of(leaf).width / 2.0)
                    .max(params.node_gap);
                let sign = if best[leaf] + 1e-9 < axis { -1.0 } else { 1.0 };
                desired[leaf] = axis + sign * pitch;
                iter_weights[leaf] = iter_weights[leaf].max(REAL_WEIGHT * 8.0);
            } else {
                let pitch = fan_pitch(&leaves, best, size_of, params.node_gap);
                let mults = slot_multipliers(leaves.len());
                for (i, &leaf) in leaves.iter().enumerate() {
                    desired[leaf] = axis + mults[i] * pitch;
                    iter_weights[leaf] = iter_weights[leaf].max(REAL_WEIGHT * 8.0);
                }
            }
        }
        for &peer in down_nbs[h].iter().chain(up_nbs[h].iter()) {
            if twins.contains(&undirected(h, peer)) || primary.contains(&undirected(h, peer)) {
                desired[peer] = axis;
                iter_weights[peer] = iter_weights[peer].max(1.0e6);
            }
        }
        // Primary occupies axis → remaining leaf must leave the axis.
        if [
            (down_deg[h] >= 2).then_some(&down_nbs[h]),
            (up_deg[h] >= 2).then_some(&up_nbs[h]),
        ]
        .into_iter()
        .flatten()
        .any(|side| side.iter().any(|&l| primary.contains(&undirected(h, l))))
        {
            for side in [
                (down_deg[h] >= 2).then_some(&down_nbs[h]),
                (up_deg[h] >= 2).then_some(&up_nbs[h]),
            ]
            .into_iter()
            .flatten()
            {
                for &leaf in side.iter() {
                    if plan.elems[leaf].key.is_virtual()
                        || plan.elems[leaf].key.is_zero_width()
                        || twins.contains(&undirected(h, leaf))
                        || primary.contains(&undirected(h, leaf))
                    {
                        continue;
                    }
                    if (desired[leaf] - axis).abs() < 1.0 {
                        let pitch = (params.node_gap
                            + size_of(h).width / 2.0
                            + size_of(leaf).width / 2.0)
                            .max(params.node_gap);
                        desired[leaf] = axis - pitch; // long "no" prefers left
                        iter_weights[leaf] = iter_weights[leaf].max(REAL_WEIGHT * 16.0);
                    }
                }
            }
        }
    }
    let owner_x = desired.clone();
    apply_port_anchor_desired(
        plan, graph, ports, size_of, main, &owner_x, segs_by_edge, &mut desired, &mut iter_weights,
    );
    for layer in &plan.layers {
        for &e in layer {
            if plan.elems[e].key.is_virtual() {
                desired[e] = crate::layout::hierarchical::metric::cross_axis::exteriorize_dummy_desired(
                    plan, size_of, params.node_gap, &desired, e, desired[e],
                );
            }
        }
    }
    let placed = solve_or_fallback(n, &desired, &iter_weights, hard, hard_fallback)?;

    // Second pass: followers + bottom-up spine reclaim.
    let mut desired = placed.clone();
    let mut iter_weights = weights.to_vec();
    for &h in &hub_order {
        let axis = placed[h];
        desired[h] = axis;
        iter_weights[h] = REAL_WEIGHT * (1.0 + params.lambda_sym) * 8.0;
        pull_spine_to_axis(
            h, axis, plan, down_nbs, up_nbs, down_deg, up_deg, &mut desired, &mut iter_weights,
        );
        for side in [
            (down_deg[h] >= 2).then_some(&down_nbs[h]),
            (up_deg[h] >= 2).then_some(&up_nbs[h]),
        ]
        .into_iter()
        .flatten()
        {
            let skip: BTreeSet<usize> = side.iter().copied().collect();
            let side_has_primary = side
                .iter()
                .any(|&leaf| primary.contains(&undirected(h, leaf)));
            for &leaf in side.iter() {
                if plan.elems[leaf].key.is_virtual()
                    || plan.elems[leaf].key.is_zero_width()
                    || twins.contains(&undirected(h, leaf))
                    || primary.contains(&undirected(h, leaf))
                {
                    continue;
                }
                if side_has_primary && (desired[leaf] - axis).abs() < 1.0 {
                    let pitch = (params.node_gap
                        + size_of(h).width / 2.0
                        + size_of(leaf).width / 2.0)
                        .max(params.node_gap);
                    desired[leaf] = axis - pitch;
                }
                iter_weights[leaf] = iter_weights[leaf].max(REAL_WEIGHT * 8.0);
                let toward_up = up_nbs[leaf].contains(&h);
                pull_exclusive_chain(
                    leaf, !toward_up, desired[leaf], plan, down_nbs, up_nbs, down_deg, up_deg,
                    &mut desired, &mut iter_weights, &skip,
                );
            }
            for &peer in side.iter() {
                if twins.contains(&undirected(h, peer)) || primary.contains(&undirected(h, peer)) {
                    desired[peer] = axis;
                    iter_weights[peer] = iter_weights[peer].max(1.0e6);
                }
            }
        }
    }
    for &h in &hub_order {
        pull_spine_to_axis(
            h, desired[h], plan, down_nbs, up_nbs, down_deg, up_deg, &mut desired, &mut iter_weights,
        );
    }
    let owner_x = desired.clone();
    apply_port_anchor_desired(
        plan, graph, ports, size_of, main, &owner_x, segs_by_edge, &mut desired, &mut iter_weights,
    );
    for layer in &plan.layers {
        for &e in layer {
            if plan.elems[e].key.is_virtual() {
                desired[e] = crate::layout::hierarchical::metric::cross_axis::exteriorize_dummy_desired(
                    plan, size_of, params.node_gap, &desired, e, desired[e],
                );
            }
        }
    }
    solve_or_fallback(n, &desired, &iter_weights, hard, hard_fallback)
}

/// Snap each group-boundary clamp to the GLOBAL member-derived frame edge.
///
/// Sole post-solve writer of clamp positions (D₂.1): separation between
/// sibling frames is already hard-constrained in the VPSC set, so this only
/// moves clamps inward onto the frame edge their members imply — it never
/// widens or narrows a frame gap.
fn snap_boundaries_to_members(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    cross: &mut [f64],
) {
    let mut global_span: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for (e_idx, elem) in plan.elems.iter().enumerate() {
        if !matches!(&elem.key, ElemKey::Real(_)) {
            continue;
        }
        let w = size_of(e_idx).width;
        let left = cross[e_idx] - w / 2.0;
        let right = cross[e_idx] + w / 2.0;
        for g in &elem.group_path {
            global_span
                .entry(g.as_str())
                .and_modify(|(lo, hi)| {
                    *lo = lo.min(left);
                    *hi = hi.max(right);
                })
                .or_insert((left, right));
        }
    }
    for (e_idx, elem) in plan.elems.iter().enumerate() {
        if let ElemKey::GroupBoundary { group, side, .. } = &elem.key {
            let Some(&(min_left, max_right)) = global_span.get(group.as_str()) else {
                continue;
            };
            cross[e_idx] = match side {
                BoundarySide::Left => min_left - GROUP_PAD,
                BoundarySide::Right => max_right + GROUP_PAD,
            };
        }
    }
}

/// Rigidly close excess slack between sibling group frames after VPSC
/// (D₂.1 writer: pull-left only).
///
/// Push-apart is the hard constraint set's job (unrelated clamp pairs
/// reserve GROUP_FRAME_GAP), so this pass only slides a trailing sibling set
/// LEFT when its frame sits further right than frontier + GROUP_FRAME_GAP —
/// it never widens a gap and never overrides the solve's separation. Slack
/// closure stays here until J(x) grows a compactness term (D₂.2 backlog).
/// Only pairs whose member **rank spans overlap** (true 2D frame collision
/// risk) are adjusted. Vertically stacked siblings on disjoint ranks keep
/// their VPSC x-alignment.
fn close_sibling_frame_slack(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    main: &[f64],
    cross: &mut [f64],
) {
    let mut group_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut group_members: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut group_ranks: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for (e_idx, elem) in plan.elems.iter().enumerate() {
        if !matches!(&elem.key, ElemKey::Real(_)) {
            continue;
        }
        for (depth, g) in elem.group_path.iter().enumerate() {
            group_path
                .entry(g.clone())
                .or_insert_with(|| elem.group_path[..=depth].to_vec());
            group_members.entry(g.clone()).or_default().push(e_idx);
            group_ranks.entry(g.clone()).or_default().insert(elem.rank);
        }
    }
    if group_members.len() < 2 {
        return;
    }
    let max_depth = group_path.values().map(|p| p.len()).max().unwrap_or(0);
    for depth in (1..=max_depth).rev() {
        let mut siblings_of: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
        for (g, path) in &group_path {
            if path.len() != depth {
                continue;
            }
            siblings_of
                .entry(path[..depth - 1].to_vec())
                .or_default()
                .insert(g.clone());
        }
        for siblings in siblings_of.values() {
            if siblings.len() < 2 {
                continue;
            }
            let mut drawn = drawn_frame_x_extents(&group_path, &group_members, size_of, cross);
            close_slack_one_sibling_set(
                siblings,
                &mut drawn,
                &group_ranks,
                &group_path,
                &group_members,
                plan,
                size_of,
                node_gap,
                main,
                cross,
            );
        }
    }
}

/// Cross-axis drawn frame edges matching finalize:
/// `union(descendant member edges ∪ child drawn frames) ± GROUP_PAD`.
///
/// Child frames already include their own pad, so a parent that only contains
/// nested groups is one pad wider per side than `union(members)±pad`.
fn drawn_frame_x_extents(
    group_path: &BTreeMap<String, Vec<String>>,
    group_members: &BTreeMap<String, Vec<usize>>,
    size_of: &dyn Fn(usize) -> Size,
    cross: &[f64],
) -> BTreeMap<String, (f64, f64)> {
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut max_depth = 0usize;
    for (g, path) in group_path {
        max_depth = max_depth.max(path.len());
        if path.len() >= 2 {
            children
                .entry(path[path.len() - 2].as_str())
                .or_default()
                .push(g.as_str());
        }
    }
    let mut by_depth: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    for (g, path) in group_path {
        by_depth.entry(path.len()).or_default().push(g.as_str());
    }
    let mut drawn: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for depth in (1..=max_depth).rev() {
        let Some(groups) = by_depth.get(&depth) else {
            continue;
        };
        for &g in groups {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            if let Some(members) = group_members.get(g) {
                for &e in members {
                    let w = size_of(e).width;
                    lo = lo.min(cross[e] - w / 2.0);
                    hi = hi.max(cross[e] + w / 2.0);
                }
            }
            if let Some(kids) = children.get(g) {
                for &child in kids {
                    if let Some(&(clo, chi)) = drawn.get(child) {
                        lo = lo.min(clo);
                        hi = hi.max(chi);
                    }
                }
            }
            if lo.is_finite() {
                drawn.insert(g.to_string(), (lo - GROUP_PAD, hi + GROUP_PAD));
            }
        }
    }
    drawn
}

fn close_slack_one_sibling_set(
    groups: &BTreeSet<String>,
    drawn: &mut BTreeMap<String, (f64, f64)>,
    group_ranks: &BTreeMap<String, BTreeSet<u32>>,
    group_path: &BTreeMap<String, Vec<String>>,
    group_members: &BTreeMap<String, Vec<usize>>,
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    main: &[f64],
    cross: &mut [f64],
) {
    // Main-axis (y) bands of drawn frames — static in this pass (x-only
    // writer). Bottom-up like `drawn_frame_x_extents`: child frames are
    // already padded, the parent wraps their union + its own pads. Top pad
    // is conservatively the labeled value (label presence is not visible
    // here; over-approximating the band only blocks more closures).
    let mut y_band: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let max_depth = group_path.values().map(|p| p.len()).max().unwrap_or(0);
    for depth in (1..=max_depth).rev() {
        for (g, path) in group_path {
            if path.len() != depth {
                continue;
            }
            let mut top = f64::INFINITY;
            let mut bottom = f64::NEG_INFINITY;
            if let Some(members) = group_members.get(g) {
                for &e in members {
                    top = top.min(main[e]);
                    bottom = bottom.max(main[e] + size_of(e).height);
                }
            }
            for (child, cpath) in group_path {
                if cpath.len() < 2 || cpath[cpath.len() - 2] != g.as_str() {
                    continue;
                }
                if let Some(&(ct, cb)) = y_band.get(child) {
                    top = top.min(ct);
                    bottom = bottom.max(cb);
                }
            }
            if top.is_finite() {
                y_band.insert(
                    g.clone(),
                    (top - GROUP_LABEL_TOP_PAD, bottom + GROUP_PAD),
                );
            }
        }
    }
    let mut frames: Vec<(String, f64, f64)> = Vec::new();
    for g in groups {
        let Some(&(left, right)) = drawn.get(g) else {
            continue;
        };
        frames.push((g.clone(), left, right));
    }
    frames.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    // Per-rank right frontier: a group only clears prior groups that share a
    // rank (y-overlap). Disjoint-rank siblings are free to share an x-band.
    let mut max_right_per_rank: BTreeMap<u32, f64> = BTreeMap::new();

    for i in 0..frames.len() {
        let g = frames[i].0.clone();
        let left = frames[i].1;
        let Some(ranks) = group_ranks.get(&g) else {
            continue;
        };
        let relevant_max = ranks
            .iter()
            .filter_map(|r| max_right_per_rank.get(r))
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        if relevant_max.is_finite() {
            let target = relevant_max + GROUP_FRAME_GAP;
            // >0 only: too far right of target → slide left. The shortfall
            // direction (too close) is infeasible by construction since the
            // hard set reserves GROUP_FRAME_GAP, and this pass never pushes.
            let adjust = left - target;
            if adjust > 1e-6 {
                let shift_groups: BTreeSet<String> =
                    frames[i..].iter().map(|(name, _, _)| name.clone()).collect();
                let virt_gate = left;
                let move_it: Vec<bool> = plan
                    .elems
                    .iter()
                    .enumerate()
                    .map(|(e_idx, elem)| match &elem.key {
                        ElemKey::Real(_) => elem
                            .group_path
                            .iter()
                            .any(|gg| shift_groups.contains(gg)),
                        ElemKey::GroupBoundary { group, .. } => shift_groups.contains(group),
                        // Partition clamps never join a group-frame slide;
                        // the post-solve partition snap re-seats them on
                        // member extremes ± pad (partition-grid PG-1).
                        ElemKey::PartitionBoundary { .. } => false,
                        ElemKey::Virtual { .. } => cross[e_idx] >= virt_gate - 1e-6,
                        ElemKey::OrderPad { .. } => false,
                    })
                    .collect();
                // Safety clamp: the rigid shift set tracks groups, not
                // group-less / non-moving same-rank elements, so a full
                // left-pull can run a mover into a stationary neighbor.
                // Judge collisions on RANK-LOCAL surfaces: a group only
                // occupies ranks where it has members (its global frame
                // rectangle spans the rank union and would false-block), and
                // boundary clamps are zero-width ghosts the final snap
                // rewrites. Capping only reduces the left shift, so no
                // separation shrinks below what the solve already
                // guaranteed.
                let mut max_ok = adjust;
                for (e_idx, elem) in plan.elems.iter().enumerate() {
                    // Only real members drive collisions: clamps snap onto
                    // members afterwards, dummies are zero-width.
                    if !move_it[e_idx] || !matches!(&elem.key, ElemKey::Real(_)) {
                        continue;
                    }
                    let rank = elem.rank;
                    // The one shifting-set group this mover belongs to.
                    let mover_group = elem
                        .group_path
                        .iter()
                        .find(|gg| shift_groups.contains(*gg));
                    // Mover surface: left edge of its (moving) frame on this
                    // rank, else its own left edge.
                    let mut e_surface = cross[e_idx] - size_of(e_idx).width / 2.0;
                    let mut e_framed = false;
                    for gg in &elem.group_path {
                        if !shift_groups.contains(gg) {
                            continue;
                        }
                        if let Some(lo) = group_rank_edge(
                            gg, false, rank, group_path, group_members, plan, size_of, cross,
                        ) {
                            e_surface = e_surface.min(lo);
                            e_framed = true;
                        }
                    }
                    for (o_idx, other) in plan.elems.iter().enumerate() {
                        if move_it[o_idx] || !matches!(&other.key, ElemKey::Real(_)) {
                            continue;
                        }
                        // Stationary obstacles are real members only: clamps
                        // are snap-rewritten ghosts, dummies/pads zero-width
                        // (the retired compact pass crossed them freely and
                        // stayed green; edges re-route in compose).
                        // A pull-left only collides with stationary elements
                        // on the mover's left; right-side ones open up.
                        if cross[o_idx] >= cross[e_idx] {
                            continue;
                        }
                        if other.rank != rank {
                            // Different rank: members never collide; the only
                            // risk is the sibling gate — frames whose
                            // y-bands intersect must keep GLOBAL x-gap ≥
                            // GROUP_FRAME_GAP. y-bands are static here.
                            let Some(mg) = mover_group else {
                                continue;
                            };
                            let Some(&(et, eb)) = y_band.get(mg.as_str()) else {
                                continue;
                            };
                            // Outermost stationary ancestor of `other` (its
                            // band superset-contains all inner frames).
                            let mut o_hi: Option<f64> = None;
                            let mut o_intersects = false;
                            for gg in &other.group_path {
                                if shift_groups.contains(gg) {
                                    continue;
                                }
                                if let Some(&(_, hi)) = drawn.get(gg.as_str()) {
                                    o_hi = Some(o_hi.map_or(hi, |c| c.max(hi)));
                                }
                                if let Some(&(ot, ob)) = y_band.get(gg.as_str()) {
                                    o_intersects |= et < ob && ot < eb;
                                }
                                break;
                            }
                            if !o_intersects {
                                continue;
                            }
                            let Some(hi) = o_hi else {
                                continue;
                            };
                            let e_lo = drawn
                                .get(mg.as_str())
                                .map(|&(lo, _)| lo)
                                .unwrap_or(e_surface);
                            let limit = e_lo - hi - GROUP_FRAME_GAP;
                            if limit < max_ok {
                                max_ok = limit;
                            }
                            continue;
                        }
                        // Same rank: rank-local surfaces (a group only
                        // physically occupies ranks with members).
                        let mut o_surface = cross[o_idx] + size_of(o_idx).width / 2.0;
                        let mut o_framed = false;
                        for gg in &other.group_path {
                            if shift_groups.contains(gg) {
                                continue;
                            }
                            if let Some(hi) = group_rank_edge(
                                gg, true, rank, group_path, group_members, plan, size_of, cross,
                            ) {
                                o_surface = o_surface.max(hi);
                                o_framed = true;
                            }
                        }
                        let req = match (e_framed, o_framed) {
                            (true, true) => GROUP_FRAME_GAP,
                            // Frame edge already carries GROUP_PAD beyond its
                            // members, so pad + node_gap keeps the bare
                            // element clear of the frame's members too.
                            (true, false) | (false, true) => node_gap + GROUP_PAD,
                            (false, false) => node_gap,
                        };
                        let limit = e_surface - o_surface - req;
                        if limit < max_ok {
                            max_ok = limit;
                        }
                    }
                }
                if max_ok > 1e-6 {
                    for (e_idx, &mv) in move_it.iter().enumerate() {
                        if mv {
                            cross[e_idx] -= max_ok;
                        }
                    }
                    for f in frames.iter_mut().skip(i) {
                        f.1 -= max_ok;
                        f.2 -= max_ok;
                    }
                    // Keep the drawn map coherent: every shifted group and its
                    // descendants travel rigidly with the movers, so later
                    // clamps must see the new edges.
                    for (g, (lo, hi)) in drawn.iter_mut() {
                        if group_path.get(g.as_str()).is_some_and(|p| {
                            p.iter().any(|gg| shift_groups.contains(gg))
                        }) {
                            *lo -= max_ok;
                            *hi -= max_ok;
                        }
                    }
                }
            }
        }
        let right_now = frames[i].2;
        for r in ranks {
            max_right_per_rank
                .entry(*r)
                .and_modify(|v| *v = v.max(right_now))
                .or_insert(right_now);
        }
    }
}

/// Rank-local drawn edge of group `g` on `rank`: union of member edges on
/// that rank plus nested children's rank-local edges (+pad), matching the
/// finalize semantics one rank at a time. `None` when the group occupies no
/// member (directly or via descendants) on `rank` — a frame rectangle spans
/// its rank union, but on a rank without members it is no physical obstacle.
fn group_rank_edge(
    g: &str,
    side_right: bool,
    rank: u32,
    group_path: &BTreeMap<String, Vec<String>>,
    group_members: &BTreeMap<String, Vec<usize>>,
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    cross: &[f64],
) -> Option<f64> {
    fn pick(acc: Option<f64>, v: f64, side_right: bool) -> f64 {
        match acc {
            Some(cur) => {
                if side_right {
                    cur.max(v)
                } else {
                    cur.min(v)
                }
            }
            None => v,
        }
    }
    let mut edge: Option<f64> = None;
    if let Some(members) = group_members.get(g) {
        for &e in members {
            if plan.elems[e].rank != rank {
                continue;
            }
            let half = size_of(e).width / 2.0;
            let v = if side_right {
                cross[e] + half
            } else {
                cross[e] - half
            };
            edge = Some(pick(edge, v, side_right));
        }
    }
    for (child, path) in group_path {
        if path.len() < 2 || path[path.len() - 2] != g {
            continue;
        }
        if let Some(child_edge) = group_rank_edge(
            child, side_right, rank, group_path, group_members, plan, size_of, cross,
        ) {
            let padded = if side_right {
                child_edge + GROUP_PAD
            } else {
                child_edge - GROUP_PAD
            };
            edge = Some(pick(edge, padded, side_right));
        }
    }
    edge
}

fn solve_once(
    n: usize,
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
) -> Result<Vec<f64>, VpscError> {
    let vars: Vec<Variable> = (0..n)
        .map(|e| Variable {
            desired: desired[e],
            weight: weights[e],
        })
        .collect();
    vpsc::solve(&vars, constraints)
}

fn solve_or_fallback(
    n: usize,
    desired: &[f64],
    weights: &[f64],
    hard: &[Constraint],
    hard_fallback: &[Constraint],
) -> Result<Vec<f64>, VpscError> {
    match solve_once(n, desired, weights, hard) {
        Ok(v) => Ok(v),
        Err(VpscError::Infeasible { .. }) if !std::ptr::eq(hard.as_ptr(), hard_fallback.as_ptr()) => {
            solve_once(n, desired, weights, hard_fallback)
        }
        Err(e) => Err(e),
    }
}

fn vpsc_weights(plan: &PlanGraph, graph: &RealGraph) -> Vec<f64> {
    let weights: BTreeMap<&str, f64> = graph
        .edges
        .iter()
        .map(|e| (e.edge_id.as_str(), e.weight))
        .collect();
    (0..plan.elems.len())
        .map(|e| match &plan.elems[e].key {
            ElemKey::Virtual { edge_id, .. } => {
                VIRTUAL_WEIGHT * weights.get(edge_id.as_str()).copied().unwrap_or(1.0)
            }
            ElemKey::GroupBoundary { .. } | ElemKey::PartitionBoundary { .. } | ElemKey::OrderPad { .. } => 4.0,
            ElemKey::Real(_) => REAL_WEIGHT,
        })
        .collect()
}

fn hard_constraints(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    primary_blocks: &[Vec<usize>],
    twins: &BTreeSet<(usize, usize)>,
    twin_hard: bool,
    gb_hard: bool,
    bands: Option<&PartitionBandPlan>,
) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    // Group-boundary clamps participate in the hard separation chain: with a
    // `GROUP_PAD` extra on boundary-adjacent pairs, each clamp sits exactly on
    // the frame edge its rank implies (frames are drawn at member ± pad).
    // Unrelated sibling clamp pairs additionally reserve `GROUP_FRAME_GAP`:
    // the drawn frame gap equals that pair's extra (member edge distance −
    // 2×pad), so the solve itself guarantees sibling frame separation (D₂.1 —
    // the post-VPSC step only re-solves tightened desired, never pushes).
    // Nested parent/child and same-group clamp pairs keep the pad extra:
    // nesting is containment, not sibling separation.
    for layer in &plan.layers {
        let geometric: Vec<usize> = layer
            .iter()
            .copied()
            .filter(|&e| !matches!(&plan.elems[e].key, ElemKey::OrderPad { .. }))
            .collect();
        for i in 0..geometric.len().saturating_sub(1) {
            let (l, r) = (geometric[i], geometric[i + 1]);
            let extra = pair_extra(&plan.elems[l], &plan.elems[r], node_gap);
            let gap = size_of(l).width / 2.0 + size_of(r).width / 2.0 + extra;
            constraints.push(Constraint::new(l, r, gap));
        }
    }
    for block in primary_blocks {
        for pair in block.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if plan.elems[a].key.is_virtual() && plan.elems[b].key.is_virtual() {
                constraints.push(Constraint::new(a, b, 0.0));
                constraints.push(Constraint::new(b, a, 0.0));
            }
        }
    }
    if twin_hard {
        for &(a, b) in twins {
            if plan.elems[a].rank != plan.elems[b].rank {
                constraints.push(Constraint::new(a, b, 0.0));
                constraints.push(Constraint::new(b, a, 0.0));
            }
        }
    }
    if gb_hard {
        // Cross-rank hard equalities tie same-side clamps of one group into a
        // single column = the cross-rank union edge = the drawn frame edge.
        // Partition `pb:` segments get the same treatment: a column band is
        // full-height, so its L/R edges stay straight vertical lines
        // (partition-grid.md PG-1).
        for s in &plan.segments {
            if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
                constraints.push(Constraint::new(s.from, s.to, 0.0));
                constraints.push(Constraint::new(s.to, s.from, 0.0));
            }
        }
    }
    if let Some(bands) = bands {
        // Column bands (partition-grid.md PG-1), present on EVERY chain
        // level: (1) adjacent declared columns stay globally separated on
        // every rank — members are confined to their band by the layer
        // separation chain, so this yields whole-band disjointness and
        // declaration order; (2) a column with no members anywhere keeps a
        // minimum strip for its title.
        for rank in 0..plan.layers.len() {
            for pair in bands.columns.windows(2) {
                let Some(r) = bands.clamp(plan, &pair[0], rank as u32, BoundarySide::Right)
                else {
                    continue;
                };
                let Some(l) = bands.clamp(plan, &pair[1], rank as u32, BoundarySide::Left)
                else {
                    continue;
                };
                constraints.push(Constraint::new(r, l, bands.gap));
            }
        }
        for (ci, col) in bands.columns.iter().enumerate() {
            if !bands.empty[ci] {
                continue;
            }
            for rank in 0..plan.layers.len() {
                let (Some(l), Some(r)) = (
                    bands.clamp(plan, col, rank as u32, BoundarySide::Left),
                    bands.clamp(plan, col, rank as u32, BoundarySide::Right),
                ) else {
                    continue;
                };
                constraints.push(Constraint::new(l, r, PARTITION_EMPTY_BAND_MIN));
            }
        }
    }
    constraints
}

/// Separation extra between an adjacent (or clamped-against) pair: unrelated
/// sibling boundary-clamp pairs reserve `GROUP_FRAME_GAP` (their extra IS the
/// drawn frame gap), any pair touching a group boundary clamp reserves
/// `GROUP_PAD`, a pair touching a partition clamp reserves `node_gap` (band
/// pad = inter-column gap, partition-grid PG-1), everything else `node_gap`.
fn pair_extra(l: &Elem, r: &Elem, node_gap: f64) -> f64 {
    match (&l.key, &r.key) {
        (ElemKey::GroupBoundary { .. }, ElemKey::GroupBoundary { .. })
            if !boundary_related(l, r) =>
        {
            GROUP_FRAME_GAP
        }
        _ if l.key.is_partition_boundary() || r.key.is_partition_boundary() => node_gap,
        _ if l.key.is_group_boundary() || r.key.is_group_boundary() => GROUP_PAD,
        _ => node_gap,
    }
}

/// True when two boundary clamps belong to the same group or to a nested
/// parent/child pair (one `group_path` is a prefix of the other).
fn boundary_related(a: &Elem, b: &Elem) -> bool {
    fn is_prefix(short: &[String], long: &[String]) -> bool {
        short.len() <= long.len() && long[..short.len()] == *short
    }
    is_prefix(&a.group_path, &b.group_path) || is_prefix(&b.group_path, &a.group_path)
}

/// Neighbor list entry: (neighbor, base edge weight 1/2/8, author weight).
type Nb = (usize, f64, f64);

fn segment_neighbors(plan: &PlanGraph, graph: &RealGraph) -> Vec<Vec<Nb>> {
    let weights: BTreeMap<&str, f64> = graph
        .edges
        .iter()
        .map(|e| (e.edge_id.as_str(), e.weight))
        .collect();
    let n = plan.elems.len();
    let mut out = vec![Vec::new(); n];
    for s in &plan.segments {
        if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
            continue;
        }
        let base = edge_weight_base(&plan.elems[s.from], &plan.elems[s.to]);
        let w = weights.get(s.edge_id.as_str()).copied().unwrap_or(1.0);
        out[s.from].push((s.to, base, w));
        out[s.to].push((s.from, base, w));
    }
    out
}

fn edge_weight_base(a: &Elem, b: &Elem) -> f64 {
    match (a.key.is_virtual(), b.key.is_virtual()) {
        (false, false) => 1.0,
        (true, true) => 8.0,
        _ => 2.0,
    }
}

fn undirected(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn primary_arm_pairs(
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    twins: &BTreeSet<(usize, usize)>,
) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    let n = plan.elems.len();
    for h in 0..n {
        if plan.elems[h].key.is_virtual() || plan.elems[h].key.is_zero_width() {
            continue;
        }
        // At most one primary arm per hub (prefer down fan, else up).
        let nb = if down_deg[h] >= 2 {
            unique_min_span_primary(h, &down_nbs[h], plan, twins)
        } else if up_deg[h] >= 2 {
            unique_min_span_primary(h, &up_nbs[h], plan, twins)
        } else {
            None
        };
        if let Some(nb) = nb {
            out.insert(undirected(h, nb));
        }
    }
    out
}

fn desired_weight(
    u: usize,
    v: usize,
    base: f64,
    weight: f64,
    twins: &BTreeSet<(usize, usize)>,
    primary: &BTreeSet<(usize, usize)>,
    params: &HierarchicalParams,
) -> f64 {
    let mut w = base;
    if twins.contains(&undirected(u, v)) {
        w *= params.twin_spine_boost;
    }
    if primary.contains(&undirected(u, v)) {
        w *= params.primary_arm_boost;
    }
    w *= weight;
    w
}

fn weighted_median_desired(
    e: usize,
    x: &[f64],
    nbs: &[Vec<Nb>],
    twins: &BTreeSet<(usize, usize)>,
    primary: &BTreeSet<(usize, usize)>,
    params: &HierarchicalParams,
    layer_pos: &[usize],
    plan: &PlanGraph,
) -> f64 {
    let mut weighted: Vec<(f64, f64, usize)> = Vec::new();
    for &(nb, base, w) in &nbs[e] {
        let dw = desired_weight(e, nb, base, w, twins, primary, params);
        weighted.push((x[nb], dw, nb));
    }
    if weighted.is_empty() {
        return x[e];
    }
    weighted.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(plan.elems[a.2].rank.cmp(&plan.elems[b.2].rank))
            .then(layer_pos[a.2].cmp(&layer_pos[b.2]))
            .then(a.2.cmp(&b.2))
    });
    let total_w: f64 = weighted.iter().map(|(_, w, _)| *w).sum();
    if total_w <= 1e-12 {
        return x[e];
    }
    let half = total_w / 2.0;
    let mut acc = 0.0;
    for &(pos, w, _) in &weighted {
        acc += w;
        if acc >= half {
            return pos;
        }
    }
    weighted.last().map(|(p, _, _)| *p).unwrap_or(x[e])
}

/// Fan center via neighbor median / mid-of-two-middle (`axis_from_neighbors`).
/// Equal-spaced packs match review §4.3.1 `(bbox_mid+median)/2`.
fn center_h(
    x: &[f64],
    h: usize,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
) -> f64 {
    let mut parts = Vec::new();
    if down_deg[h] >= 2 {
        parts.push(axis_from_neighbors(&down_nbs[h], x));
    }
    if up_deg[h] >= 2 {
        parts.push(axis_from_neighbors(&up_nbs[h], x));
    }
    if parts.is_empty() {
        return x[h];
    }
    parts.iter().sum::<f64>() / parts.len() as f64
}

fn objective_j(
    plan: &PlanGraph,
    x: &[f64],
    hubs: &[usize],
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    lambda_sym: f64,
) -> f64 {
    let mut j = 0.0;
    for s in &plan.segments {
        if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
            continue;
        }
        let w = edge_weight_base(&plan.elems[s.from], &plan.elems[s.to]);
        j += w * (x[s.from] - x[s.to]).abs();
    }
    for &h in hubs {
        let c = center_h(x, h, down_nbs, up_nbs, down_deg, up_deg);
        j += lambda_sym * (x[h] - c).abs();
    }
    j
}

/// Pull exclusive 1:1 real spines onto `axis` (replaces RigidColumnClass walk).
fn pull_exclusive_chain(
    start: usize,
    toward_up: bool,
    axis: f64,
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    desired: &mut [f64],
    weights: &mut [f64],
    skip: &BTreeSet<usize>,
) {
    let mut cur = start;
    loop {
        let nbs = if toward_up {
            &up_nbs[cur]
        } else {
            &down_nbs[cur]
        };
        if nbs.len() != 1 {
            break;
        }
        let next = nbs[0];
        if skip.contains(&next) {
            break;
        }
        if plan.elems[next].key.is_virtual() || plan.elems[next].key.is_zero_width() {
            break;
        }
        if down_deg[next] >= 2 || up_deg[next] >= 2 {
            // Absorb unique parent fan hub (ticket-triage handle above resolve_gate).
            if toward_up && up_nbs[cur].len() == 1 && down_deg[next] == 1 {
                desired[next] = axis;
                weights[next] = weights[next].max(REAL_WEIGHT * 32.0);
            }
            break;
        }
        let deg = down_deg[next] + up_deg[next];
        if deg > 2 {
            break;
        }
        desired[next] = axis;
        weights[next] = weights[next].max(REAL_WEIGHT * 8.0);
        cur = next;
    }
}

fn pull_spine_to_axis(
    hub: usize,
    axis: f64,
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    desired: &mut [f64],
    weights: &mut [f64],
) {
    let skip = BTreeSet::new();
    for toward_up in [true, false] {
        pull_exclusive_chain(
            hub,
            toward_up,
            axis,
            plan,
            down_nbs,
            up_nbs,
            down_deg,
            up_deg,
            desired,
            weights,
            &skip,
        );
    }
}

fn apply_port_anchor_desired(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
    main: &[f64],
    owner_x: &[f64],
    segs_by_edge: &BTreeMap<String, Vec<usize>>,
    desired: &mut [f64],
    weights: &mut [f64],
) {
    let frame_of = |e: usize| -> Rect {
        let s = size_of(e);
        Rect::new(owner_x[e] - s.width / 2.0, main[e], s.width, s.height)
    };
    for e in &graph.edges {
        let Some(rp) = ports.get(&e.edge_id) else {
            continue;
        };
        for (real_idx, port) in [
            (e.original_target, rp.target),
            (e.original_source, rp.source),
        ] {
            let real_elem = plan.index_of[&ElemKey::Real(graph.ids[real_idx].clone())];
            let Some(idxs) = segs_by_edge.get(&e.edge_id) else {
                continue;
            };
            let Some(nb) = idxs
                .iter()
                .map(|&i| &plan.segments[i])
                .find(|s| s.from == real_elem || s.to == real_elem)
                .map(|s| if s.from == real_elem { s.to } else { s.from })
            else {
                continue;
            };
            if !plan.elems[nb].key.is_virtual() {
                continue;
            }
            desired[nb] = port_anchor(frame_of(real_elem), port).x;
            // Port-anchor dummies must dominate soft median drift.
            weights[nb] = weights[nb].max(VIRTUAL_WEIGHT * 16.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::BoundarySide;

    fn boundary(group: &str, path: &[&str], side: BoundarySide) -> Elem {
        Elem {
            key: ElemKey::GroupBoundary {
                group: group.to_string(),
                rank: 0,
                side,
            },
            group_path: path.iter().map(|s| (*s).to_string()).collect(),
            rank: 0,
        }
    }

    fn real(id: &str) -> Elem {
        Elem {
            key: ElemKey::Real(id.to_string()),
            group_path: Vec::new(),
            rank: 0,
        }
    }

    /// Single-layer plan; boundary clamps are zero-width by contract.
    fn plan_of(elems: Vec<Elem>) -> PlanGraph {
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let layers = vec![(0..elems.len()).collect()];
        PlanGraph {
            elems,
            index_of,
            decl_index: Vec::new(),
            segments: Vec::new(),
            layers,
            ..Default::default()
        }
    }

    #[test]
    fn hard_separation_extras_by_pair_kind() {
        use BoundarySide::{Left, Right};
        let cases: Vec<(&str, Vec<Elem>, Vec<f64>, Vec<((usize, usize), f64)>)> = vec![
            (
                "unrelated sibling clamp pair reserves GROUP_FRAME_GAP",
                vec![
                    boundary("g1", &["g1"], Left),
                    boundary("g1", &["g1"], Right),
                    boundary("g2", &["g2"], Left),
                    boundary("g2", &["g2"], Right),
                ],
                vec![0.0; 4],
                vec![((0, 1), 16.0), ((1, 2), 24.0), ((2, 3), 16.0)],
            ),
            (
                "nested parent/child and same-group pairs keep GROUP_PAD",
                vec![
                    boundary("o", &["o"], Left),
                    boundary("i", &["o", "i"], Left),
                    boundary("i", &["o", "i"], Right),
                    boundary("o", &["o"], Right),
                ],
                vec![0.0; 4],
                vec![((0, 1), 16.0), ((1, 2), 16.0), ((2, 3), 16.0)],
            ),
            (
                "plain real pair uses node_gap",
                vec![real("a"), real("b")],
                vec![10.0, 10.0],
                vec![((0, 1), 34.0)],
            ),
            (
                "member-to-own-clamp pair uses GROUP_PAD",
                vec![real("a"), boundary("g1", &["g1"], Left)],
                vec![10.0, 0.0],
                vec![((0, 1), 21.0)],
            ),
        ];
        for (name, elems, widths, expect) in cases {
            let plan = plan_of(elems);
            let size_of = |i: usize| Size::new(widths[i], 0.0);
            let constraints = hard_constraints(
                &plan,
                &size_of,
                24.0,
                &[],
                &BTreeSet::new(),
                false,
                false,
                None,
            );
            for ((l, r), gap) in expect {
                let found = constraints
                    .iter()
                    .find(|c| c.left == l && c.right == r)
                    .map(|c| c.gap);
                assert_eq!(found, Some(gap), "{name}: pair ({l},{r})");
            }
        }
    }

    /// PG-1 band hard constraints: adjacent declared columns separate by the
    /// band gap on every rank; a column with no member anywhere keeps the
    /// `PARTITION_EMPTY_BAND_MIN` strip.
    #[test]
    fn partition_band_constraints_separate_columns_and_floor_empty() {
        use crate::layout::hierarchical::compose::partition_boundary::insert_partition_boundaries;
        use crate::layout::hierarchical::metric::partition_bands::PartitionBandPlan;
        use crate::layout::hierarchical::model::RealGraph;
        use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        // Two columns; only `a` has a member → `b` is empty everywhere.
        let mut real = RealGraph::default();
        real.partition = Some(PartitionGrid {
            columns: vec![PartitionAxis::new("a"), PartitionAxis::new("b")],
            rows: vec![],
        });
        let specs: [(&str, Option<&str>); 2] = [("n1", Some("a")), ("n2", None)];
        let mut elems = Vec::new();
        for (i, (id, col)) in specs.iter().enumerate() {
            real.ids.push((*id).to_string());
            real.index_of.insert((*id).to_string(), i);
            real.partition_cell.push(col.map(PartitionCell::col));
            elems.push(Elem {
                key: ElemKey::Real((*id).into()),
                group_path: Vec::new(),
                rank: 0,
            });
        }
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1],
            segments: Vec::new(),
            layers: vec![vec![0, 1]],
            ..Default::default()
        };
        insert_partition_boundaries(&mut plan, &real).unwrap();

        let bands = PartitionBandPlan::build(&plan, 24.0).unwrap();
        // Clamps are zero-width by contract; only reals carry size here
        // (mirrors `elem_size`), so the layer-separation chain does not
        // invent width-based extras on top of the band constraints.
        let size_of = |i: usize| {
            Size::new(if plan.elems[i].key.is_zero_width() { 0.0 } else { 10.0 }, 10.0)
        };
        let constraints = hard_constraints(
            &plan,
            &size_of,
            24.0,
            &[],
            &BTreeSet::new(),
            false,
            false,
            Some(&bands),
        );
        let gap_of = |l: usize, r: usize| {
            // A pair can carry both the layer-separation constraint and a
            // band constraint; VPSC honors the max, so assert on the max.
            constraints
                .iter()
                .filter(|c| c.left == l && c.right == r)
                .map(|c| c.gap)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
        };
        // Adjacent-column separation on rank 0.
        let a_r = bands.clamp(&plan, "a", 0, BoundarySide::Right).unwrap();
        let b_l = bands.clamp(&plan, "b", 0, BoundarySide::Left).unwrap();
        assert_eq!(gap_of(a_r, b_l), Some(24.0), "columns must separate by gap");
        // Empty-column minimum width on rank 0.
        let b_r = bands.clamp(&plan, "b", 0, BoundarySide::Right).unwrap();
        assert_eq!(
            gap_of(b_l, b_r),
            Some(PARTITION_EMPTY_BAND_MIN),
            "empty column keeps its minimum strip"
        );
    }
}
