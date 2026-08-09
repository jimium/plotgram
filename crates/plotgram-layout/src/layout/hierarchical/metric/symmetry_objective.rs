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
use crate::layout::hierarchical::group_frame::GROUP_PAD;
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::metric::bk;
use crate::layout::hierarchical::metric::symmetry::{
    axis_from_neighbors, degrees_of, fan_pitch, forward_real_adjacency, slot_multipliers,
    twin_plan_pairs, unique_min_span_primary,
};
use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph, RealGraph};
use crate::layout::hierarchical::params::HierarchicalParams;

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
    // cannot be disjoint anyway and we degrade gracefully.
    let chains = [
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &twins,
            true,
            true,
        ),
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &twins,
            false,
            true,
        ),
        hard_constraints(
            plan,
            size_of,
            params.node_gap,
            &bk.primary_blocks,
            &twins,
            false,
            false,
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

    snap_fan_pack_style(
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
    )
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
            ElemKey::GroupBoundary { .. } | ElemKey::OrderPad { .. } => 4.0,
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
) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    // Group-boundary clamps participate in the hard separation chain: with a
    // `GROUP_PAD` extra on boundary-adjacent pairs, each clamp sits exactly on
    // the frame edge its rank implies (finalize draws frames at member ± pad),
    // and clamp-to-clamp across sibling groups reserves the frame gap.
    for layer in &plan.layers {
        let geometric: Vec<usize> = layer
            .iter()
            .copied()
            .filter(|&e| !matches!(&plan.elems[e].key, ElemKey::OrderPad { .. }))
            .collect();
        for i in 0..geometric.len().saturating_sub(1) {
            let (l, r) = (geometric[i], geometric[i + 1]);
            let extra = if plan.elems[l].key.is_group_boundary()
                || plan.elems[r].key.is_group_boundary()
            {
                GROUP_PAD
            } else {
                node_gap
            };
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
        for s in &plan.segments {
            if s.edge_id.starts_with("gb:") {
                constraints.push(Constraint::new(s.from, s.to, 0.0));
                constraints.push(Constraint::new(s.to, s.from, 0.0));
            }
        }
    }
    constraints
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
        if s.edge_id.starts_with("gb:") {
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
        if s.edge_id.starts_with("gb:") {
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
