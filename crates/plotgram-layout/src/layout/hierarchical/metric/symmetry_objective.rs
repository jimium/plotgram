//! P4 symmetry objective: minimize edge straightness + hub-to-fan-center,
//! subject only to layer separation and VV dummy-chain equalities.
//!
//! Main path (symmetry-axis.md): IPSEP-style iterate — unconstrained descent on
//! J, then VPSC projection of that step's `x`. The old median-as-desired
//! packer is `symmetry_place: median`.

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::Size;
use plotgram_algo::vpsc::{self, Constraint, Variable, VpscError};

use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::compose::ports::{EdgePorts, ResolvedPort};
use crate::layout::hierarchical::group_frame::{GROUP_FRAME_GAP, GROUP_LABEL_TOP_PAD, GROUP_PAD};
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::metric::bk;
use crate::layout::hierarchical::metric::partition_bands::{
    PartitionBandPlan, PARTITION_EMPTY_BAND_MIN,
};
use crate::layout::hierarchical::metric::symmetry::{
    axis_from_neighbors, degrees_of, descendant_mass, fan_axis_from_children, fan_pitch,
    forward_real_adjacency, slot_multipliers, stem_psi, twin_plan_pairs,
};
use crate::layout::hierarchical::model::{BoundarySide, Elem, ElemKey, PlanGraph, RealGraph};
use crate::layout::hierarchical::params::{GroupPolicy, HierarchicalParams, SymmetryPlace};

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
    let seg_offs = segment_port_offsets(plan, graph, ports, size_of);
    let deltas = AlignDeltas::build(plan, &seg_offs);
    let nbs = segment_neighbors(plan, graph, &seg_offs);
    let (down_nbs, up_nbs) = forward_real_adjacency(plan, graph);
    let down_deg = degrees_of(&down_nbs);
    let up_deg = degrees_of(&up_nbs);
    let twins = twin_plan_pairs(plan, graph);
    let layer_pos = plan.layer_positions();
    // Constraint degradation chain: full (twin hard + cross-rank clamp
    // equalities) → twin soft → no clamp equalities. The gb equalities make
    // each clamp column the cross-rank frame edge; they can only cycle when
    // sibling group intervals swap order across ranks, in which case frames
    // cannot be disjoint anyway and we degrade gracefully. Partition band
    // separation rides EVERY level and never degrades — an infeasible mix
    // surfaces as `Infeasible`, never silently (partition-grid.md PG-1).
    //
    // Real↔virtual BK collinear is intentionally soft (via median / J), not
    // hard: hard RV equality lets a long side-leaf corridor yank the hub off
    // a short exclusive stem. Collinearity stays soft — hard equalities
    // bloat the canvas on dense layers.
    let bands = PartitionBandPlan::build(plan, params.node_gap);
    let bands = bands.as_ref();
    // Linear segment (yfiles/01 §4.5): adjacent-dummy pairs of every long
    // edge, rank order — one variable per chain. Deterministic: BTreeMap
    // iteration + rank/elem sort.
    let chain_pairs: Vec<(usize, usize)> = {
        let mut by_edge: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (e_idx, elem) in plan.elems.iter().enumerate() {
            if let ElemKey::Virtual { edge_id, .. } = &elem.key {
                by_edge.entry(edge_id.clone()).or_default().push(e_idx);
            }
        }
        let mut pairs = Vec::new();
        for mut v in by_edge.into_values() {
            if v.len() < 2 {
                continue;
            }
            v.sort_by_key(|&e| (plan.elems[e].rank, e));
            for w in v.windows(2) {
                pairs.push((w[0], w[1]));
            }
        }
        pairs.sort_unstable();
        pairs
    };
    // Chain identity rides every degradation level and only drops on the
    // final floor — a too-rigid corridor must not surface as Infeasible.
    let chains = [
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &chain_pairs,
            true,
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
            &chain_pairs,
            true,
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
            &chain_pairs,
            true,
            &twins,
            false,
            false,
            bands,
        ),
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &chain_pairs,
            false,
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
    let chain_end_leaves = chain_end_leaves(plan, &down_deg, &up_deg, &down_nbs, &up_nbs);
    let segs_by_edge = plan.segments_by_edge();
    let mass = descendant_mass(plan, &down_nbs, &up_deg);
    let spine = exclusive_spine_pairs(
        plan,
        &down_nbs,
        &up_nbs,
        &down_deg,
        &up_deg,
        &chain_end_leaves,
    );
    let hop_boost = stem_flow_hop_boost(
        plan,
        graph,
        &down_nbs,
        &down_deg,
        &up_deg,
        &spine,
        &mass,
        &segs_by_edge,
        params,
    );
    let chain_end = chain_end_rv_pairs(plan, &chain_end_leaves);

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
    let hard_fallback = &chains[(chain_idx + 1).min(3)];

    // Feasible start: raw BK ideal may violate separation, so its J is not
    // comparable to post-VPSC iterates (would permanently win the snapshot).
    let mut x = solve_once(n, &bk.ideal, &weights, hard)?;
    // A0 diagnostic (§12): BK ideal already projected through the hard set
    // (chain identity included). Skip median-desired packing and fan snap so
    // we can see whether layer slack comes back. Group/partition snaps stay
    // — they are clamp writers, not the packer under test.
    if params.symmetry_place == SymmetryPlace::BkIdeal {
        if params.group_policy == GroupPolicy::Weak {
            close_sibling_frame_slack(plan, size_of, params.node_gap, main, &mut x);
            snap_boundaries_to_members(plan, size_of, &mut x);
            snap_partition_clamps_to_members(plan, size_of, params.node_gap, &mut x);
        }
        return Ok(x);
    }

    let score = |x: &[f64]| {
        objective_j(
            plan, x, &seg_offs, &hubs, &down_nbs, &up_nbs, &down_deg, &up_deg, &twins, &hop_boost,
            &chain_end, &mass, params,
        )
    };
    let mut best = x.clone();
    let mut best_j = score(&x);

    for _ in 0..params.symmetry_iters.max(1) {
        let (desired, iter_weights) = match params.symmetry_place {
            // A1: unconstrained L2 step on J's terms, then VPSC *projects
            // this x* (base weights). Median-as-desired maps many nodes onto
            // a few neighbor columns and the projection packs the layer.
            SymmetryPlace::Ipsep => {
                let desired = unconstrained_l2_step(
                    &x, &nbs, &hubs, &down_nbs, &up_nbs, &down_deg, &up_deg, &twins, &hop_boost,
                    &chain_end, &mass, params,
                );
                (desired, weights.clone())
            }
            SymmetryPlace::Median => {
                let mut desired = vec![0.0; n];
                for (e, slot) in desired.iter_mut().enumerate() {
                    *slot = weighted_median_desired(
                        e, &x, &nbs, &twins, &hop_boost, &chain_end, params, &layer_pos, plan,
                    );
                }
                let alpha = params.lambda_sym / (1.0 + params.lambda_sym);
                let mut iter_weights = weights.clone();
                for &h in &hubs {
                    if stem_fan_out_keep_j(h, &down_nbs, &up_nbs, &down_deg) {
                        continue;
                    }
                    let c = center_h(&x, h, &down_nbs, &up_nbs, &down_deg, &up_deg, &mass);
                    desired[h] = desired[h] * (1.0 - alpha) + c * alpha;
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
                    &chain_end_leaves,
                    &hubs,
                    &down_deg,
                    &mut desired,
                    &mut iter_weights,
                );
                (desired, iter_weights)
            }
            SymmetryPlace::BkIdeal => unreachable!("bk returns before the iterate"),
        };

        x = solve_once(n, &desired, &iter_weights, hard)?;
        let j = score(&x);
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
        &deltas,
        &hubs,
        &down_nbs,
        &up_nbs,
        &down_deg,
        &up_deg,
        &twins,
        &chain_end_leaves,
        &layer_pos,
        &segs_by_edge,
        &mass,
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
        if let ElemKey::PartitionBoundary { axis, side, .. } = &elem.key {
            let Some(ci) = plan.partition_columns.iter().position(|c| c == axis) else {
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

/// Center-to-center floor matching VPSC layer sep — not a layout pitch.
fn min_center_sep(h: usize, leaf: usize, size_of: &dyn Fn(usize) -> Size, node_gap: f64) -> f64 {
    (node_gap + size_of(h).width / 2.0 + size_of(leaf).width / 2.0).max(node_gap)
}

/// Side of `axis` this free leaf belongs on: FanPack slot sign
/// (left half / right half), or the iterate's side when the slot is the mid.
fn leaf_side_sign(leaf: usize, leaves: &[usize], best: &[f64], axis: f64) -> f64 {
    let idx = leaves.iter().position(|&x| x == leaf).unwrap_or(0);
    let mid = (leaves.len() as f64 - 1.0) / 2.0;
    let m = idx as f64 - mid;
    if m.abs() < 1e-9 {
        if best[leaf] + 1e-9 < axis {
            -1.0
        } else {
            1.0
        }
    } else if m < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// C: keep the iterate's x when the leaf is already on the correct side of
/// the hub and clear of min-sep. Only rewrite when J sat on the axis or the
/// wrong side (order-approval long branch left of the hub).
fn place_leaves_keep_j(
    h: usize,
    axis: f64,
    leaves: &[usize],
    best: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    desired: &mut [f64],
    iter_weights: &mut [f64],
) {
    for &leaf in leaves {
        let sign = leaf_side_sign(leaf, leaves, best, axis);
        let min_sep = min_center_sep(h, leaf, size_of, node_gap);
        let dx = best[leaf] - axis;
        if dx * sign > 1.0 && dx.abs() + 1e-9 >= min_sep {
            continue;
        }
        let dist = dx.abs().max(min_sep);
        desired[leaf] = axis + sign * dist;
        iter_weights[leaf] = iter_weights[leaf].max(REAL_WEIGHT * 8.0);
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
    deltas: &AlignDeltas,
    hubs: &[usize],
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    twins: &BTreeSet<(usize, usize)>,
    chain_end_leaves: &BTreeSet<usize>,
    layer_pos: &[usize],
    segs_by_edge: &BTreeMap<String, Vec<usize>>,
    mass: &[f64],
) -> Result<Vec<f64>, VpscError> {
    let n = plan.elems.len();
    let mut hub_order: Vec<usize> = hubs.to_vec();
    hub_order.sort_by(|&a, &b| plan.elems[b].rank.cmp(&plan.elems[a].rank).then(a.cmp(&b)));

    let mut desired = best.to_vec();
    let mut iter_weights = weights.to_vec();
    let long_edge_hubs: BTreeSet<usize> = plan
        .segments
        .iter()
        .filter(|s| plan.elems[s.from].key.is_virtual() ^ plan.elems[s.to].key.is_virtual())
        .flat_map(|s| [s.from, s.to])
        .filter(|&e| matches!(plan.elems[e].key, ElemKey::Real(_)))
        .collect();
    // Snap expands J: dummy-chain hubs and 1:1-stem fan-out hubs already
    // have a column from the iterate. Re-centering them onto child/parent
    // centroids undoes the linear segment / stem.
    let keep_j: BTreeSet<usize> = hub_order
        .iter()
        .copied()
        .filter(|&h| {
            long_edge_hubs.contains(&h) || stem_fan_out_keep_j(h, down_nbs, up_nbs, down_deg)
        })
        .collect();

    for &h in &hub_order {
        let axis = if params.symmetry_place == SymmetryPlace::Ipsep && keep_j.contains(&h) {
            best[h]
        } else {
            center_h(best, h, down_nbs, up_nbs, down_deg, up_deg, mass)
        };
        desired[h] = axis;
        iter_weights[h] = REAL_WEIGHT * (1.0 + params.lambda_sym) * 8.0;
        pull_spine_to_axis(
            h,
            axis,
            plan,
            deltas,
            down_nbs,
            up_nbs,
            down_deg,
            up_deg,
            chain_end_leaves,
            &mut desired,
            &mut iter_weights,
        );
        // Fan-out: pack children around the hub. Fan-in: the hub already
        // sits on `center_h` of its parents — packing the parents would
        // yank 1:1 columns off and leave the sink looking glued to the
        // median parent.
        for side in [(down_deg[h] >= 2).then_some(&down_nbs[h])]
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
                })
                .collect();
            if leaves.is_empty() {
                continue;
            }
            leaves.sort_by(|&a, &b| layer_pos[a].cmp(&layer_pos[b]).then(a.cmp(&b)));
            if params.symmetry_place == SymmetryPlace::Ipsep && leaves.len() == 1 {
                // C: a lone free leaf keeps its J column. The old
                // axis±(node_gap+widths) yank is the remaining packer on
                // dense DAGs (one leaf per hub, whole layer at min pitch).
                place_leaves_keep_j(
                    h,
                    axis,
                    &leaves,
                    best,
                    size_of,
                    params.node_gap,
                    &mut desired,
                    &mut iter_weights,
                );
            } else if leaves.len() == 1 {
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
            snap_spine_peer(
                h,
                peer,
                axis,
                deltas,
                twins,
                params,
                &mut desired,
                &mut iter_weights,
            );
        }
    }
    weld_unique_stems(
        plan,
        down_nbs,
        up_nbs,
        down_deg,
        deltas,
        chain_end_leaves,
        &mut desired,
        &mut iter_weights,
    );
    recenter_fan_in_sinks(
        plan,
        hubs,
        down_nbs,
        up_nbs,
        down_deg,
        up_deg,
        &keep_j,
        params.lambda_sym,
        mass,
        &mut desired,
        &mut iter_weights,
    );
    let owner_x = desired.clone();
    apply_port_anchor_desired(
        plan,
        graph,
        ports,
        size_of,
        main,
        &owner_x,
        segs_by_edge,
        chain_end_leaves,
        hubs,
        down_deg,
        &mut desired,
        &mut iter_weights,
    );
    for layer in &plan.layers {
        for &e in layer {
            if plan.elems[e].key.is_virtual() {
                desired[e] =
                    crate::layout::hierarchical::metric::cross_axis::exteriorize_dummy_desired(
                        plan,
                        size_of,
                        params.node_gap,
                        &desired,
                        e,
                        desired[e],
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
            h,
            axis,
            plan,
            deltas,
            down_nbs,
            up_nbs,
            down_deg,
            up_deg,
            chain_end_leaves,
            &mut desired,
            &mut iter_weights,
        );
        for side in [(down_deg[h] >= 2).then_some(&down_nbs[h])]
            .into_iter()
            .flatten()
        {
            let skip: BTreeSet<usize> = side.iter().copied().collect();
            for &leaf in side.iter() {
                if plan.elems[leaf].key.is_virtual()
                    || plan.elems[leaf].key.is_zero_width()
                    || twins.contains(&undirected(h, leaf))
                {
                    continue;
                }
                iter_weights[leaf] = iter_weights[leaf].max(REAL_WEIGHT * 8.0);
                let toward_up = up_nbs[leaf].contains(&h);
                pull_exclusive_chain(
                    leaf,
                    !toward_up,
                    desired[leaf],
                    plan,
                    deltas,
                    down_nbs,
                    up_nbs,
                    down_deg,
                    up_deg,
                    &mut desired,
                    &mut iter_weights,
                    &skip,
                    chain_end_leaves,
                );
            }
            for &peer in side.iter() {
                snap_spine_peer(
                    h,
                    peer,
                    axis,
                    deltas,
                    twins,
                    params,
                    &mut desired,
                    &mut iter_weights,
                );
            }
        }
    }
    for &h in &hub_order {
        pull_spine_to_axis(
            h,
            desired[h],
            plan,
            deltas,
            down_nbs,
            up_nbs,
            down_deg,
            up_deg,
            chain_end_leaves,
            &mut desired,
            &mut iter_weights,
        );
    }
    weld_unique_stems(
        plan,
        down_nbs,
        up_nbs,
        down_deg,
        deltas,
        chain_end_leaves,
        &mut desired,
        &mut iter_weights,
    );
    recenter_fan_in_sinks(
        plan,
        hubs,
        down_nbs,
        up_nbs,
        down_deg,
        up_deg,
        &keep_j,
        params.lambda_sym,
        mass,
        &mut desired,
        &mut iter_weights,
    );
    let owner_x = desired.clone();
    apply_port_anchor_desired(
        plan,
        graph,
        ports,
        size_of,
        main,
        &owner_x,
        segs_by_edge,
        chain_end_leaves,
        hubs,
        down_deg,
        &mut desired,
        &mut iter_weights,
    );
    for layer in &plan.layers {
        for &e in layer {
            if plan.elems[e].key.is_virtual() {
                desired[e] =
                    crate::layout::hierarchical::metric::cross_axis::exteriorize_dummy_desired(
                        plan,
                        size_of,
                        params.node_gap,
                        &desired,
                        e,
                        desired[e],
                    );
            }
        }
    }
    let placed = solve_or_fallback(n, &desired, &iter_weights, hard, hard_fallback)?;
    adsorb_near_collinear(
        n,
        placed,
        plan,
        deltas,
        chain_end_leaves,
        hard,
        params.node_gap,
    )
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
                y_band.insert(g.clone(), (top - GROUP_LABEL_TOP_PAD, bottom + GROUP_PAD));
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
                let shift_groups: BTreeSet<String> = frames[i..]
                    .iter()
                    .map(|(name, _, _)| name.clone())
                    .collect();
                let virt_gate = left;
                let move_it: Vec<bool> = plan
                    .elems
                    .iter()
                    .enumerate()
                    .map(|(e_idx, elem)| match &elem.key {
                        ElemKey::Real(_) => {
                            elem.group_path.iter().any(|gg| shift_groups.contains(gg))
                        }
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
                    let mover_group = elem.group_path.iter().find(|gg| shift_groups.contains(*gg));
                    // Mover surface: left edge of its (moving) frame on this
                    // rank, else its own left edge.
                    let mut e_surface = cross[e_idx] - size_of(e_idx).width / 2.0;
                    let mut e_framed = false;
                    for gg in &elem.group_path {
                        if !shift_groups.contains(gg) {
                            continue;
                        }
                        if let Some(lo) = group_rank_edge(
                            gg,
                            false,
                            rank,
                            group_path,
                            group_members,
                            plan,
                            size_of,
                            cross,
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
                                gg,
                                true,
                                rank,
                                group_path,
                                group_members,
                                plan,
                                size_of,
                                cross,
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
                        if group_path
                            .get(g.as_str())
                            .is_some_and(|p| p.iter().any(|gg| shift_groups.contains(gg)))
                        {
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
            child,
            side_right,
            rank,
            group_path,
            group_members,
            plan,
            size_of,
            cross,
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
        Err(VpscError::Infeasible { .. })
            if !std::ptr::eq(hard.as_ptr(), hard_fallback.as_ptr()) =>
        {
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
            ElemKey::GroupBoundary { .. }
            | ElemKey::PartitionBoundary { .. }
            | ElemKey::OrderPad { .. } => 4.0,
            ElemKey::Real(_) => REAL_WEIGHT,
        })
        .collect()
}

fn hard_constraints(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    primary_blocks: &[Vec<usize>],
    chain_pairs: &[(usize, usize)],
    chain_hard: bool,
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
    if chain_hard {
        // Linear segment (yfiles/01 §4.5): every long edge's dummies form ONE
        // variable — adjacent-dummy equalities bind the whole chain into a
        // single column, independent of whether BK blocks happen to align.
        // Members sit in distinct ranks, so the equalities never fight
        // in-layer separation. Overlap with primary-block VV equalities is
        // harmless (both demand the same coincidence).
        for &(a, b) in chain_pairs {
            constraints.push(Constraint::new(a, b, 0.0));
            constraints.push(Constraint::new(b, a, 0.0));
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
                let Some(r) = bands.clamp(plan, &pair[0], rank as u32, BoundarySide::Right) else {
                    continue;
                };
                let Some(l) = bands.clamp(plan, &pair[1], rank as u32, BoundarySide::Left) else {
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

/// Cross-axis distance from an element center to the port an edge uses on it.
///
/// A segment is straight when the two **ports** share x, not when the two node
/// centers do — a node carrying several ports on one face must pay the offset
/// with its own center. Only N/S faces shift x; an E/W port leaves sideways, so
/// there is no x to straighten and the offset stays 0.
fn port_offsets(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
) -> BTreeMap<(String, usize), f64> {
    use plotgram_algo::orientation::Side;
    use plotgram_model::port::AlongSpec;

    let mut out = BTreeMap::new();
    for e in &graph.edges {
        let Some(rp) = ports.get(&e.edge_id) else {
            continue;
        };
        for (real_idx, port) in [
            (e.original_source, rp.source),
            (e.original_target, rp.target),
        ] {
            if !matches!(port.side, Side::North | Side::South) {
                continue;
            }
            let AlongSpec::Ordered { order, count } = port.along else {
                continue;
            };
            let Some(&elem) = plan
                .index_of
                .get(&ElemKey::Real(graph.ids[real_idx].clone()))
            else {
                continue;
            };
            let t = (order as f64 + 1.0) / (count as f64 + 1.0);
            out.insert((e.edge_id.clone(), elem), (t - 0.5) * size_of(elem).width);
        }
    }
    out
}

/// Per-segment `(from, to)` port offsets, parallel to `plan.segments`.
fn segment_port_offsets(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
) -> Vec<(f64, f64)> {
    let ends = port_offsets(plan, graph, ports, size_of);
    plan.segments
        .iter()
        .map(|s| {
            let at = |elem: usize| ends.get(&(s.edge_id.clone(), elem)).copied().unwrap_or(0.0);
            (at(s.from), at(s.to))
        })
        .collect()
}

/// Cross-axis shift that makes a segment's two **ports** collinear:
/// `x_to = x_from + get(from, to)`.
///
/// Pairs joined by several segments (twin corridors) have no single answer and
/// stay 0 — PortLane owns those faces.
pub(crate) struct AlignDeltas(BTreeMap<(usize, usize), f64>);

impl AlignDeltas {
    fn build(plan: &PlanGraph, seg_offs: &[(f64, f64)]) -> Self {
        let mut count: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut delta: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for (i, s) in plan.segments.iter().enumerate() {
            if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
                continue;
            }
            let key = undirected(s.from, s.to);
            *count.entry(key).or_insert(0) += 1;
            let (off_from, off_to) = seg_offs[i];
            let low_to_high = if s.from <= s.to {
                off_from - off_to
            } else {
                off_to - off_from
            };
            delta.insert(key, low_to_high);
        }
        delta.retain(|k, v| count[k] == 1 && v.abs() > 1e-9);
        Self(delta)
    }

    fn get(&self, from: usize, to: usize) -> f64 {
        let v = self.0.get(&undirected(from, to)).copied().unwrap_or(0.0);
        if from <= to {
            v
        } else {
            -v
        }
    }
}

/// Neighbor list entry: (neighbor, base edge weight 1/2/8, author weight,
/// `delta` = the shift this element needs so both ports of the segment align).
type Nb = (usize, f64, f64, f64);

fn segment_neighbors(plan: &PlanGraph, graph: &RealGraph, seg_offs: &[(f64, f64)]) -> Vec<Vec<Nb>> {
    let weights: BTreeMap<&str, f64> = graph
        .edges
        .iter()
        .map(|e| (e.edge_id.as_str(), e.weight))
        .collect();
    let n = plan.elems.len();
    let mut out = vec![Vec::new(); n];
    for (i, s) in plan.segments.iter().enumerate() {
        if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
            continue;
        }
        let base = edge_weight_base(&plan.elems[s.from], &plan.elems[s.to]);
        let w = weights.get(s.edge_id.as_str()).copied().unwrap_or(1.0);
        let (off_from, off_to) = seg_offs[i];
        out[s.from].push((s.to, base, w, off_to - off_from));
        out[s.to].push((s.from, base, w, off_from - off_to));
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

/// Fan-out hub on a unique 1:1 parent stem: J already owns the column
/// (exclusive hop). λ_sym must not pull it onto the child barycenter.
fn stem_fan_out_keep_j(
    h: usize,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
) -> bool {
    down_deg[h] >= 2 && up_nbs[h].len() == 1 && down_nbs[up_nbs[h][0]].len() == 1
}

/// Dangling sinks whose only span-1 parent is itself not a fan hub
/// (span-1 degree; long-edge hops do not count). They may leave that 1:1
/// parent to sit on a long corridor. Forward sources, through-nodes, and
/// fan-hub children stay on the stem.
fn chain_end_leaves(
    plan: &PlanGraph,
    down_deg: &[usize],
    up_deg: &[usize],
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
) -> BTreeSet<usize> {
    let span1 = |e: usize, nbs: &[Vec<usize>]| {
        nbs[e]
            .iter()
            .filter(|&&nb| plan.elems[nb].rank.abs_diff(plan.elems[e].rank) == 1)
            .count()
    };
    let mut out = BTreeSet::new();
    for (e, elem) in plan.elems.iter().enumerate() {
        if !matches!(elem.key, ElemKey::Real(_)) {
            continue;
        }
        if down_deg[e] != 0 || up_deg[e] != 1 {
            continue;
        }
        let parent = up_nbs[e][0];
        if span1(parent, down_nbs) >= 2 || span1(parent, up_nbs) >= 2 {
            continue;
        }
        let has_virt = plan.segments.iter().any(|s| {
            (s.from == e && plan.elems[s.to].key.is_virtual())
                || (s.to == e && plan.elems[s.from].key.is_virtual())
        });
        if has_virt {
            out.insert(e);
        }
    }
    out
}

fn chain_end_rv_pairs(plan: &PlanGraph, leaves: &BTreeSet<usize>) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for s in &plan.segments {
        if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
            continue;
        }
        let a_v = plan.elems[s.from].key.is_virtual();
        let b_v = plan.elems[s.to].key.is_virtual();
        if a_v == b_v {
            continue;
        }
        let real = if a_v { s.to } else { s.from };
        if leaves.contains(&real) {
            out.insert(undirected(s.from, s.to));
        }
    }
    out
}

/// Exclusive 1:1 real hops (same walk as [`pull_exclusive_chain`]), from every
/// real — not only hubs — so a leaf's own continuation (fan `b→b2`) is in J.
/// Stops at hubs without inserting into a fan-in target (stem-flow: fan-in
/// hops stay unboosted; λ_sym writes the sink).
fn exclusive_spine_pairs(
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    chain_end_leaves: &BTreeSet<usize>,
) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for (e, elem) in plan.elems.iter().enumerate() {
        if elem.key.is_virtual() || elem.key.is_zero_width() {
            continue;
        }
        for toward_up in [true, false] {
            let mut cur = e;
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
                if plan.elems[next].key.is_virtual() || plan.elems[next].key.is_zero_width() {
                    break;
                }
                if chain_end_leaves.contains(&next) || chain_end_leaves.contains(&cur) {
                    break;
                }
                if down_deg[next] >= 2 || up_deg[next] >= 2 {
                    // Unique parent into a fan-out hub: 1:1 stem ends at the
                    // fan (ticket-triage handle→resolve_gate). Do not insert
                    // into a fan-in target.
                    if !toward_up && down_nbs[cur].len() == 1 && up_deg[next] == 1 {
                        out.insert(undirected(cur, next));
                    }
                    break;
                }
                if down_deg[next] + up_deg[next] > 2 {
                    break;
                }
                out.insert(undirected(cur, next));
                cur = next;
            }
        }
    }
    out
}

/// φ(role)·ψ(L) on every plan segment of a real edge (stem-flow J).
fn stem_flow_hop_boost(
    plan: &PlanGraph,
    graph: &RealGraph,
    down_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    exclusive: &BTreeSet<(usize, usize)>,
    mass: &[f64],
    segs_by_edge: &BTreeMap<String, Vec<usize>>,
    params: &HierarchicalParams,
) -> BTreeMap<(usize, usize), f64> {
    let mut out = BTreeMap::new();
    for e in &graph.edges {
        if e.edge_id.starts_with("gb:") || e.edge_id.starts_with("pb:") {
            continue;
        }
        let Some(&src) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[e.working_source].clone()))
        else {
            continue;
        };
        let Some(&tgt) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[e.working_target].clone()))
        else {
            continue;
        };
        let Some(seg_idx) = segs_by_edge.get(&e.edge_id) else {
            continue;
        };
        if seg_idx.is_empty() {
            continue;
        }
        let phi = hop_phi(
            src, tgt, down_nbs, down_deg, up_deg, exclusive, mass, params,
        );
        let mut elems = BTreeSet::new();
        for &i in seg_idx {
            let s = &plan.segments[i];
            elems.insert(s.from);
            elems.insert(s.to);
        }
        let psi = if exclusive.contains(&undirected(src, tgt)) {
            stem_psi(elems.len().max(2), params.stem_length_gain)
        } else {
            1.0
        };
        let boost = phi * psi;
        for &i in seg_idx {
            let s = &plan.segments[i];
            let p = undirected(s.from, s.to);
            let slot = out.entry(p).or_insert(1.0);
            if boost > *slot {
                *slot = boost;
            }
        }
    }
    out
}

fn hop_phi(
    src: usize,
    tgt: usize,
    down_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    exclusive: &BTreeSet<(usize, usize)>,
    mass: &[f64],
    params: &HierarchicalParams,
) -> f64 {
    if up_deg[tgt] >= 2 {
        return 1.0;
    }
    if down_deg[src] >= 2 && down_deg[tgt] >= 2 {
        return 1.0;
    }
    if exclusive.contains(&undirected(src, tgt)) {
        return params.primary_arm_boost;
    }
    if down_deg[src] >= 2 {
        let mut sum = 0.0;
        for &c in &down_nbs[src] {
            sum += mass[c].max(1e-12);
        }
        if sum <= 1e-12 {
            return 1.0;
        }
        return 1.0 + params.fan_mass_gain * mass[tgt].max(1e-12) / sum;
    }
    1.0
}

fn align_boost(
    u: usize,
    v: usize,
    twins: &BTreeSet<(usize, usize)>,
    hop_boost: &BTreeMap<(usize, usize), f64>,
    chain_end: &BTreeSet<(usize, usize)>,
    params: &HierarchicalParams,
) -> f64 {
    let p = undirected(u, v);
    if twins.contains(&p) {
        params.twin_spine_boost
    } else if chain_end.contains(&p) {
        params.chain_end_boost
    } else {
        hop_boost.get(&p).copied().unwrap_or(1.0)
    }
}

fn desired_weight(
    u: usize,
    v: usize,
    base: f64,
    weight: f64,
    twins: &BTreeSet<(usize, usize)>,
    hop_boost: &BTreeMap<(usize, usize), f64>,
    chain_end: &BTreeSet<(usize, usize)>,
    params: &HierarchicalParams,
) -> f64 {
    let mut w = base;
    if twins.contains(&undirected(u, v)) {
        w *= params.twin_spine_boost;
    }
    if chain_end.contains(&undirected(u, v)) {
        w *= params.chain_end_boost;
    }
    w *= hop_boost.get(&undirected(u, v)).copied().unwrap_or(1.0);
    w *= weight;
    w
}

/// Jacobi step of `min Σ w((x+off)_u − (x+off)_v)² + λ Σ (x_h − center_h)²`
/// with neighbors frozen. Twin / exclusive-spine / fan-mass hops carry their
/// boost in `w`. Fan-in targets stay unboosted so λ_sym writes the sink.
fn unconstrained_l2_step(
    x: &[f64],
    nbs: &[Vec<Nb>],
    hubs: &[usize],
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    twins: &BTreeSet<(usize, usize)>,
    hop_boost: &BTreeMap<(usize, usize), f64>,
    chain_end: &BTreeSet<(usize, usize)>,
    mass: &[f64],
    params: &HierarchicalParams,
) -> Vec<f64> {
    let n = x.len();
    let mut x_unc = x.to_vec();
    for e in 0..n {
        let mut num = 0.0;
        let mut den = 0.0;
        for &(nb, base, w, delta) in &nbs[e] {
            let dw = base * w * align_boost(e, nb, twins, hop_boost, chain_end, params);
            num += dw * (x[nb] + delta);
            den += dw;
        }
        if den > 1e-12 {
            x_unc[e] = num / den;
        }
    }
    let lambda_sym = params.lambda_sym;
    if lambda_sym > 1e-12 {
        for &h in hubs {
            if stem_fan_out_keep_j(h, down_nbs, up_nbs, down_deg) {
                continue;
            }
            let c = center_h(x, h, down_nbs, up_nbs, down_deg, up_deg, mass);
            let mut num = 0.0;
            let mut den = 0.0;
            for &(nb, base, w, delta) in &nbs[h] {
                let dw = base * w * align_boost(h, nb, twins, hop_boost, chain_end, params);
                num += dw * (x[nb] + delta);
                den += dw;
            }
            num += lambda_sym * c;
            den += lambda_sym;
            if den > 1e-12 {
                x_unc[h] = num / den;
            }
        }
    }
    x_unc
}

fn weighted_median_desired(
    e: usize,
    x: &[f64],
    nbs: &[Vec<Nb>],
    twins: &BTreeSet<(usize, usize)>,
    hop_boost: &BTreeMap<(usize, usize), f64>,
    chain_end: &BTreeSet<(usize, usize)>,
    params: &HierarchicalParams,
    layer_pos: &[usize],
    plan: &PlanGraph,
) -> f64 {
    let mut weighted: Vec<(f64, f64, usize)> = Vec::new();
    for &(nb, base, w, delta) in &nbs[e] {
        let dw = desired_weight(e, nb, base, w, twins, hop_boost, chain_end, params);
        weighted.push((x[nb] + delta, dw, nb));
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

/// Fan center: fan-out uses mass-weighted barycenter (equal masses → median);
/// fan-in stays unweighted median of parents.
fn center_h(
    x: &[f64],
    h: usize,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    mass: &[f64],
) -> f64 {
    let mut parts = Vec::new();
    if down_deg[h] >= 2 {
        parts.push(fan_axis_from_children(&down_nbs[h], x, mass));
    }
    if up_deg[h] >= 2 {
        parts.push(axis_from_neighbors(&up_nbs[h], x));
    }
    if parts.is_empty() {
        return x[h];
    }
    parts.iter().sum::<f64>() / parts.len() as f64
}

#[allow(clippy::too_many_arguments)]
fn objective_j(
    plan: &PlanGraph,
    x: &[f64],
    seg_offs: &[(f64, f64)],
    hubs: &[usize],
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    twins: &BTreeSet<(usize, usize)>,
    hop_boost: &BTreeMap<(usize, usize), f64>,
    chain_end: &BTreeSet<(usize, usize)>,
    mass: &[f64],
    params: &HierarchicalParams,
) -> f64 {
    let mut j = 0.0;
    for (i, s) in plan.segments.iter().enumerate() {
        if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
            continue;
        }
        let w = edge_weight_base(&plan.elems[s.from], &plan.elems[s.to])
            * align_boost(s.from, s.to, twins, hop_boost, chain_end, params);
        let (off_from, off_to) = seg_offs[i];
        j += w * ((x[s.from] + off_from) - (x[s.to] + off_to)).abs();
    }
    for &h in hubs {
        if stem_fan_out_keep_j(h, down_nbs, up_nbs, down_deg) {
            continue;
        }
        let c = center_h(x, h, down_nbs, up_nbs, down_deg, up_deg, mass);
        j += params.lambda_sym * (x[h] - c).abs();
    }
    j
}

/// Unique down-neighbor of parent *and* unique up-neighbor of child: a stem
/// that should be one port column. Exclusive chain stops at hubs, so the
/// continuation of a packed leaf was never rewritten. Parent writes, child
/// follows.
///
/// Skip when the child is itself a fan-out hub: pulling that hub onto the
/// parent yanks its packed leaves, and pulling the parent onto the child
/// hits same-layer sep. Those stems keep J on the child instead (see snap
/// axis). Chain-end leaves stay on the corridor.
#[allow(clippy::too_many_arguments)]
fn weld_unique_stems(
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    deltas: &AlignDeltas,
    chain_end_leaves: &BTreeSet<usize>,
    desired: &mut [f64],
    weights: &mut [f64],
) {
    let n = plan.elems.len();
    let mut stems: Vec<(usize, usize)> = Vec::new();
    for p in 0..n {
        if plan.elems[p].key.is_virtual() || plan.elems[p].key.is_zero_width() {
            continue;
        }
        if down_nbs[p].len() != 1 {
            continue;
        }
        let c = down_nbs[p][0];
        if plan.elems[c].key.is_virtual()
            || plan.elems[c].key.is_zero_width()
            || chain_end_leaves.contains(&c)
            || chain_end_leaves.contains(&p)
        {
            continue;
        }
        if up_nbs[c].len() != 1 || up_nbs[c][0] != p {
            continue;
        }
        if down_deg[c] >= 2 {
            continue;
        }
        stems.push((p, c));
    }
    stems.sort_by(|&(a, b), &(c, d)| {
        plan.elems[a]
            .rank
            .cmp(&plan.elems[c].rank)
            .then(a.cmp(&c))
            .then(b.cmp(&d))
    });
    for (p, c) in stems {
        desired[c] = desired[p] + deltas.get(p, c);
        weights[c] = weights[c].max(REAL_WEIGHT * 32.0);
    }
}

/// After VPSC, force port-collinear equalities on real–real hops whose ports
/// already sit within `node_gap/4` (notes §4.2(d)). Stem weld writes a soft
/// desired; the least-squares projection leaves a 2–5px shelf (mech n1–n2).
/// Hard `x_to = x_from + δ` lets whichever end has slack move. Intentional
/// fan shelves (`n6–n10`, `n5–n23`) are larger than ε and stay bent.
/// Infeasible equalities are dropped — the pre-adsorb placement is kept.
fn adsorb_near_collinear(
    n: usize,
    mut placed: Vec<f64>,
    plan: &PlanGraph,
    deltas: &AlignDeltas,
    chain_end_leaves: &BTreeSet<usize>,
    hard: &[Constraint],
    node_gap: f64,
) -> Result<Vec<f64>, VpscError> {
    let eps = (node_gap * 0.25).max(1.0);
    // A move can expose a new sub-ε shelf on a neighbor hop. Same writer,
    // at most a couple of projections.
    for _ in 0..3 {
        let pairs = near_collinear_pairs(plan, deltas, chain_end_leaves, &placed, eps);
        if pairs.is_empty() {
            break;
        }
        let mut extra = hard.to_vec();
        extra.reserve(pairs.len() * 2);
        for &(a, b, d) in &pairs {
            extra.push(Constraint::new(a, b, d));
            extra.push(Constraint::new(b, a, -d));
        }
        let weights = vec![1.0; n];
        match solve_once(n, &placed, &weights, &extra) {
            Ok(v) => placed = v,
            Err(VpscError::Infeasible { .. }) => break,
            Err(e) => return Err(e),
        }
    }
    Ok(placed)
}

fn near_collinear_pairs(
    plan: &PlanGraph,
    deltas: &AlignDeltas,
    chain_end_leaves: &BTreeSet<usize>,
    x: &[f64],
    eps: f64,
) -> Vec<(usize, usize, f64)> {
    let mut seen: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut out = Vec::new();
    for s in &plan.segments {
        if s.edge_id.starts_with("gb:") || s.edge_id.starts_with("pb:") {
            continue;
        }
        let (a, b) = (s.from, s.to);
        if a == b
            || !matches!(plan.elems[a].key, ElemKey::Real(_))
            || !matches!(plan.elems[b].key, ElemKey::Real(_))
            || chain_end_leaves.contains(&a)
            || chain_end_leaves.contains(&b)
            || plan.elems[a].rank.abs_diff(plan.elems[b].rank) != 1
        {
            continue;
        }
        if !seen.insert(undirected(a, b)) {
            continue;
        }
        let d = deltas.get(a, b);
        let jog = (x[b] - x[a] - d).abs();
        if jog < 1e-9 || jog >= eps {
            continue;
        }
        out.push((a, b, d));
    }
    out.sort_by(|p, q| p.0.cmp(&q.0).then(p.1.cmp(&q.1)));
    out
}

/// Pure fan-in sink (no fan-out) sits on `center_h` of current parents.
/// Runs after stem weld so the span is the straightened parent columns.
/// Hubs whose column J already kept are not rewritten.
#[allow(clippy::too_many_arguments)]
fn recenter_fan_in_sinks(
    plan: &PlanGraph,
    hubs: &[usize],
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    keep_j: &BTreeSet<usize>,
    lambda_sym: f64,
    mass: &[f64],
    desired: &mut [f64],
    weights: &mut [f64],
) {
    let mut hs = hubs.to_vec();
    hs.sort_unstable();
    for h in hs {
        if down_deg[h] >= 2 || up_deg[h] < 2 || keep_j.contains(&h) {
            continue;
        }
        if plan.elems[h].key.is_virtual() || plan.elems[h].key.is_zero_width() {
            continue;
        }
        let axis = center_h(desired, h, down_nbs, up_nbs, down_deg, up_deg, mass);
        desired[h] = axis;
        weights[h] = weights[h].max(REAL_WEIGHT * (1.0 + lambda_sym) * 16.0);
    }
}

fn snap_spine_peer(
    hub: usize,
    peer: usize,
    axis: f64,
    deltas: &AlignDeltas,
    twins: &BTreeSet<(usize, usize)>,
    params: &HierarchicalParams,
    desired: &mut [f64],
    weights: &mut [f64],
) {
    if !twins.contains(&undirected(hub, peer)) {
        return;
    }
    desired[peer] = axis + deltas.get(hub, peer);
    // IPSEP: twin stays in J only — a hard lock glued a fan hub onto its
    // parent. Median still 1e6-locks the twin column (legacy packer).
    if params.symmetry_place == SymmetryPlace::Median {
        weights[peer] = weights[peer].max(1.0e6);
    }
}

/// Pull exclusive 1:1 real spines onto `axis` (replaces RigidColumnClass walk).
#[allow(clippy::too_many_arguments)]
fn pull_exclusive_chain(
    start: usize,
    toward_up: bool,
    axis: f64,
    plan: &PlanGraph,
    deltas: &AlignDeltas,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    desired: &mut [f64],
    weights: &mut [f64],
    skip: &BTreeSet<usize>,
    chain_end_leaves: &BTreeSet<usize>,
) {
    let mut cur = start;
    // The axis is a port column, not a center column: each hop shifts it by the
    // two ports' offsets so the spine stays straight through multi-port faces.
    let mut axis = axis;
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
        if skip.contains(&next) || chain_end_leaves.contains(&next) {
            break;
        }
        if plan.elems[next].key.is_virtual() || plan.elems[next].key.is_zero_width() {
            break;
        }
        if down_deg[next] >= 2 || up_deg[next] >= 2 {
            // Unique parent of a fan-out hub follows the child's keep-J
            // column (ticket-triage handle above resolve_gate). Not a
            // unique-child-into-fan-in glue.
            if toward_up && up_nbs[cur].len() == 1 && down_deg[next] == 1 {
                desired[next] = axis + deltas.get(cur, next);
                weights[next] = weights[next].max(REAL_WEIGHT * 32.0);
            }
            break;
        }
        let deg = down_deg[next] + up_deg[next];
        if deg > 2 {
            break;
        }
        axis += deltas.get(cur, next);
        desired[next] = axis;
        weights[next] = weights[next].max(REAL_WEIGHT * 8.0);
        cur = next;
    }
}

#[allow(clippy::too_many_arguments)]
fn pull_spine_to_axis(
    hub: usize,
    axis: f64,
    plan: &PlanGraph,
    deltas: &AlignDeltas,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    chain_end_leaves: &BTreeSet<usize>,
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
            deltas,
            down_nbs,
            up_nbs,
            down_deg,
            up_deg,
            desired,
            weights,
            &skip,
            chain_end_leaves,
        );
    }
}

/// Per-end port anchoring, with a **single writer** for the chain column
/// when a midpoint 4-bend Z would otherwise appear (yfiles/01 §4.5).
///
/// Chain identity binds ≥2 dummies into one VPSC variable. Equal-weight
/// writes from both non-leaf ends park that variable at the midpoint.
/// A single writer is used when at least one non-leaf end is a fan hub
/// and the chain has ≥2 dummies:
///
/// - exactly one hub → the **non-hub** writes (fan dummies track the child)
/// - both hubs → the **fan-out** end writes, unless peripheral ownership
///   splits strongly (|Δperipheral| ≥ 0.5: one end alone on its rank with
///   nowhere to park vs an end on the rank edge that already owns an
///   exterior column) — then the peripheral end writes
///
/// Neither hub, or a single dummy: keep per-end writes. Closer-to-x on
/// neither-hub long reverses drifted D2.
///
/// Chain-end leaves follow the dummy column afterwards.
fn apply_port_anchor_desired(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
    main: &[f64],
    owner_x: &[f64],
    segs_by_edge: &BTreeMap<String, Vec<usize>>,
    chain_end_leaves: &BTreeSet<usize>,
    hubs: &[usize],
    down_deg: &[usize],
    desired: &mut [f64],
    weights: &mut [f64],
) {
    let frame_of = |e: usize| -> Rect {
        let s = size_of(e);
        Rect::new(owner_x[e] - s.width / 2.0, main[e], s.width, s.height)
    };
    let hub_set: BTreeSet<usize> = hubs.iter().copied().collect();
    let mut leaf_pulls: Vec<(usize, usize)> = Vec::new();
    for e in &graph.edges {
        let Some(rp) = ports.get(&e.edge_id) else {
            continue;
        };
        let Some(idxs) = segs_by_edge.get(&e.edge_id) else {
            continue;
        };
        let mut dummies = BTreeSet::new();
        for &i in idxs {
            let s = &plan.segments[i];
            if plan.elems[s.from].key.is_virtual() {
                dummies.insert(s.from);
            }
            if plan.elems[s.to].key.is_virtual() {
                dummies.insert(s.to);
            }
        }
        if dummies.is_empty() {
            continue;
        }
        let mut nonleaf: Vec<(usize, ResolvedPort, usize)> = Vec::new();
        for (real_idx, port) in [
            (e.original_target, rp.target),
            (e.original_source, rp.source),
        ] {
            let real_elem = plan.index_of[&ElemKey::Real(graph.ids[real_idx].clone())];
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
            if chain_end_leaves.contains(&real_elem) {
                leaf_pulls.push((real_elem, nb));
                continue;
            }
            nonleaf.push((real_elem, port, nb));
        }
        let ends: Vec<(usize, ResolvedPort)> = nonleaf.iter().map(|&(r, p, _)| (r, p)).collect();
        let single_writer = match ends.as_slice() {
            [_] => true,
            &[a, b] if dummies.len() >= 2 => hub_set.contains(&a.0) || hub_set.contains(&b.0),
            _ => false,
        };
        if single_writer {
            if let Some((real_elem, port)) = pick_chain_anchor(&ends, &hub_set, down_deg, plan) {
                let ax = port_anchor(frame_of(real_elem), port).x;
                for &d in &dummies {
                    desired[d] = ax;
                    weights[d] = weights[d].max(VIRTUAL_WEIGHT * 16.0);
                }
            }
        } else {
            for &(real_elem, port, nb) in &nonleaf {
                desired[nb] = port_anchor(frame_of(real_elem), port).x;
                weights[nb] = weights[nb].max(VIRTUAL_WEIGHT * 16.0);
            }
        }
    }
    for (real_elem, nb) in leaf_pulls {
        desired[real_elem] = desired[nb];
        weights[real_elem] = weights[real_elem].max(REAL_WEIGHT * 16.0);
    }
}

fn pick_chain_anchor(
    ends: &[(usize, ResolvedPort)],
    hubs: &BTreeSet<usize>,
    down_deg: &[usize],
    plan: &PlanGraph,
) -> Option<(usize, ResolvedPort)> {
    match ends {
        [] => None,
        &[one] => Some(one),
        &[a, b] => {
            let ha = hubs.contains(&a.0);
            let hb = hubs.contains(&b.0);
            if ha != hb {
                return Some(if ha { b } else { a });
            }
            // Peripheral ownership only overrides fan-out on a strong split
            // (≥0.5): one end alone on its rank (nowhere to park) vs an end
            // on the rank edge (already owns an exterior column). Mild
            // differences keep the fan-out writer.
            let pa = layer_peripheral(a.0, plan);
            let pb = layer_peripheral(b.0, plan);
            if (pa - pb).abs() >= 0.5 {
                return Some(if pa > pb { a } else { b });
            }
            let fa = down_deg[a.0] >= 2;
            let fb = down_deg[b.0] >= 2;
            if fa != fb {
                return Some(if fa { a } else { b });
            }
            Some(more_peripheral_end(a, b, plan))
        }
        more => more.iter().copied().max_by(|a, b| {
            layer_peripheral(a.0, plan)
                .total_cmp(&layer_peripheral(b.0, plan))
                .then(a.0.cmp(&b.0))
        }),
    }
}

/// |layer_frac − 0.5|: a node alone on its rank scores 0; the leftmost or
/// rightmost real on a populated rank scores 0.5. Used when both chain ends
/// are hubs so the corridor parks on the side that already has an exterior
/// column, not the midpoint that packs the intervening layer.
fn layer_peripheral(elem: usize, plan: &PlanGraph) -> f64 {
    (layer_frac(elem, plan) - 0.5).abs()
}

fn layer_frac(elem: usize, plan: &PlanGraph) -> f64 {
    let rank = plan.elems[elem].rank as usize;
    let Some(layer) = plan.layers.get(rank) else {
        return 0.5;
    };
    let reals: Vec<usize> = layer
        .iter()
        .copied()
        .filter(|&e| matches!(plan.elems[e].key, ElemKey::Real(_)))
        .collect();
    if reals.len() <= 1 {
        return 0.5;
    }
    let idx = reals.iter().position(|&e| e == elem).unwrap_or(0);
    idx as f64 / (reals.len() - 1) as f64
}

fn more_peripheral_end(
    a: (usize, ResolvedPort),
    b: (usize, ResolvedPort),
    plan: &PlanGraph,
) -> (usize, ResolvedPort) {
    let pa = layer_peripheral(a.0, plan);
    let pb = layer_peripheral(b.0, plan);
    if pa.total_cmp(&pb).then(a.0.cmp(&b.0)).is_ge() {
        a
    } else {
        b
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
                &[],
                false,
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
            Size::new(
                if plan.elems[i].key.is_zero_width() {
                    0.0
                } else {
                    10.0
                },
                10.0,
            )
        };
        let constraints = hard_constraints(
            &plan,
            &size_of,
            24.0,
            &[],
            &[],
            false,
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

    /// Linear segment (yfiles/01 §4.5): adjacent-dummy equalities bind one
    /// long edge's chain into a single variable on every hard tier that keeps
    /// `chain_hard`; the final degradation floor drops them.
    #[test]
    fn chain_identity_constraints_bind_long_edge_dummies() {
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e1".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e1".into(),
                    ordinal: 1,
                },
                group_path: Vec::new(),
                rank: 2,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 3,
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
            decl_index: vec![0, 1, 2, 3],
            segments: Vec::new(),
            layers: vec![vec![0], vec![1], vec![2], vec![3]],
            ..Default::default()
        };
        let size_of = |_i: usize| Size::new(0.0, 10.0);
        let chain_pairs = [(1usize, 2usize)];
        let with_chain = hard_constraints(
            &plan,
            &size_of,
            24.0,
            &[],
            &chain_pairs,
            true,
            &BTreeSet::new(),
            false,
            false,
            None,
        );
        assert!(
            with_chain
                .iter()
                .any(|c| c.left == 1 && c.right == 2 && c.gap == 0.0)
                && with_chain
                    .iter()
                    .any(|c| c.left == 2 && c.right == 1 && c.gap == 0.0),
            "chain identity binds adjacent dummies both ways"
        );
        let floor = hard_constraints(
            &plan,
            &size_of,
            24.0,
            &[],
            &chain_pairs,
            false,
            &BTreeSet::new(),
            false,
            false,
            None,
        );
        assert!(
            !floor
                .iter()
                .any(|c| (c.left, c.right) == (1, 2) && c.gap == 0.0),
            "degradation floor drops chain identity"
        );
    }
}
