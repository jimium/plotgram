//! Symmetry helpers shared by the cross-axis objective solver (P4).
//!
//! Fan adjacency is **real endpoints** on [`RealGraph`]: after P1 the
//! `reversed` bit is only a direction, so reversed edges count like forward
//! ones (working source → working target); long edges still count as one
//! hop. Twin pairs (2-cycle) and exclusive 1:1 stems feed soft weights /
//! snap desired in [`super::symmetry_objective`]. Stem-flow J: exclusive
//! hops scale with dummy-inclusive length; fan-out hops share weight by
//! descendant mass; fan-in hops stay unboosted. Unique-min-span "primary
//! arm" identity was dropped (it glued a fan hub onto its shortest child).
//! The old SymmetryPlan tables (claimed / FanPack / RigidColumnClass) were
//! deleted in P4-S4.

use std::collections::BTreeSet;

use plotgram_algo::orientation::Size;

use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

/// Real–real adjacency: long edges count as one hop between endpoints;
/// dummies do not hide a fan. Reversed edges participate like forward ones
/// — after P1, `reversed` is only a direction bit (yfiles/01 §1/§3) — but
/// only when local (adjacent ranks): a long reversed edge's geometry rides
/// its dummy chain (bound into one variable by R2), and counting the remote
/// endpoint turns spine heads into hubs / breaks exclusive-spine collinearity
/// (a long reverse whose far real is counted as a local neighbor).
pub fn forward_real_adjacency(
    plan: &PlanGraph,
    graph: &RealGraph,
) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let n = plan.elems.len();
    let (mut down, mut up) = (vec![Vec::new(); n], vec![Vec::new(); n]);
    for e in &graph.edges {
        let src_id = &graph.ids[e.working_source];
        let tgt_id = &graph.ids[e.working_target];
        let Some(&src) = plan.index_of.get(&ElemKey::Real(src_id.clone())) else {
            continue;
        };
        let Some(&tgt) = plan.index_of.get(&ElemKey::Real(tgt_id.clone())) else {
            continue;
        };
        if e.reversed && plan.elems[tgt].rank != plan.elems[src].rank + 1 {
            continue;
        }
        down[src].push(tgt);
        up[tgt].push(src);
    }
    for v in &mut down {
        v.sort_unstable();
        v.dedup();
    }
    for v in &mut up {
        v.sort_unstable();
        v.dedup();
    }
    (down, up)
}

pub fn degrees_of(nbs: &[Vec<usize>]) -> Vec<usize> {
    nbs.iter().map(|v| v.len()).collect()
}

fn undirected_real_pair(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Undirected original-endpoint pairs that carry both a forward and a reversed
/// edge (2-cycle / req–resp twin).
pub(crate) fn twin_real_pairs(graph: &RealGraph) -> BTreeSet<(usize, usize)> {
    let mut fwd = BTreeSet::new();
    let mut rev = BTreeSet::new();
    for e in &graph.edges {
        let p = undirected_real_pair(e.original_source, e.original_target);
        if e.reversed {
            rev.insert(p);
        } else {
            fwd.insert(p);
        }
    }
    fwd.into_iter().filter(|p| rev.contains(p)).collect()
}

/// Map twin pairs from RealGraph id indices → PlanGraph elem indices.
pub(crate) fn twin_plan_pairs(plan: &PlanGraph, graph: &RealGraph) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for (a, b) in twin_real_pairs(graph) {
        let Some(&ea) = plan.index_of.get(&ElemKey::Real(graph.ids[a].clone())) else {
            continue;
        };
        let Some(&eb) = plan.index_of.get(&ElemKey::Real(graph.ids[b].clone())) else {
            continue;
        };
        out.insert(undirected_real_pair(ea, eb));
    }
    out
}

/// Downstream flow mass: `1 + Σ_c mass(c) / max(up_deg[c], 1)`.
/// High rank first so children are ready; virtuals stay 0.
pub(crate) fn descendant_mass(
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_deg: &[usize],
) -> Vec<f64> {
    let n = plan.elems.len();
    let mut mass = vec![0.0; n];
    let mut order: Vec<usize> = (0..n)
        .filter(|&e| !plan.elems[e].key.is_virtual() && !plan.elems[e].key.is_zero_width())
        .collect();
    order.sort_by(|&a, &b| plan.elems[b].rank.cmp(&plan.elems[a].rank).then(a.cmp(&b)));
    for v in order {
        let mut s = 1.0;
        for &c in &down_nbs[v] {
            let child = if mass[c] > 0.0 { mass[c] } else { 1.0 };
            s += child / (up_deg[c].max(1) as f64);
        }
        mass[v] = s;
    }
    mass
}

/// Exclusive-stem length gain: `1 + α log2(L)`, capped at 2.
pub(crate) fn stem_psi(len: usize, gain: f64) -> f64 {
    if len < 2 || gain <= 0.0 {
        1.0
    } else {
        (1.0 + gain * (len as f64).log2()).min(2.0)
    }
}

/// Fan-out axis: mass-weighted barycenter, or unweighted median when
/// sibling masses are equal (D3 even/odd mirror).
pub(crate) fn fan_axis_from_children(neighbors: &[usize], x: &[f64], mass: &[f64]) -> f64 {
    if neighbors.is_empty() {
        return 0.0;
    }
    let mut min_m = f64::INFINITY;
    let mut max_m = 0.0_f64;
    for &c in neighbors {
        let m = mass[c].max(1e-12);
        min_m = min_m.min(m);
        max_m = max_m.max(m);
    }
    if max_m <= min_m * (1.0 + 1e-9) {
        return axis_from_neighbors(neighbors, x);
    }
    let mut num = 0.0;
    let mut den = 0.0;
    for &c in neighbors {
        let m = mass[c].max(1e-12);
        num += m * x[c];
        den += m;
    }
    num / den
}

/// Odd → median neighbor center; even → midpoint of two middle neighbors.
pub fn axis_from_neighbors(neighbors: &[usize], pass1: &[f64]) -> f64 {
    let mut nbs: Vec<usize> = neighbors.to_vec();
    nbs.sort_by(|&a, &b| pass1[a].partial_cmp(&pass1[b]).unwrap().then(a.cmp(&b)));
    let m = nbs.len() / 2;
    if nbs.len() % 2 == 1 {
        pass1[nbs[m]]
    } else {
        (pass1[nbs[m - 1]] + pass1[nbs[m]]) / 2.0
    }
}

/// Relative slot offsets: `i - (n-1)/2` in pitch units (odd median 0; even ±0.5…).
pub fn slot_multipliers(n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let center = (n as f64 - 1.0) / 2.0;
    (0..n).map(|i| i as f64 - center).collect()
}

/// Pitch large enough for layer sep and not tighter than pass-1 adjacent gaps.
pub fn fan_pitch(
    leaves: &[usize],
    pass1: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
) -> f64 {
    if leaves.len() < 2 {
        return node_gap;
    }
    let mut width_need = 0.0_f64;
    let mut pass1_gaps = Vec::with_capacity(leaves.len() - 1);
    for w in leaves.windows(2) {
        let (a, b) = (w[0], w[1]);
        width_need = width_need.max(node_gap + size_of(a).width / 2.0 + size_of(b).width / 2.0);
        pass1_gaps.push((pass1[b] - pass1[a]).abs());
    }
    pass1_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = pass1_gaps.len() / 2;
    let from_pass1 = if pass1_gaps.len() % 2 == 1 {
        pass1_gaps[mid]
    } else {
        (pass1_gaps[mid - 1] + pass1_gaps[mid]) / 2.0
    };
    width_need.max(from_pass1).max(node_gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_formula_odd_and_even() {
        let pass1 = [0.0, 10.0, 20.0, 30.0, 40.0];
        assert!((axis_from_neighbors(&[0, 1, 2], &pass1) - 10.0).abs() < 1e-9);
        assert!((axis_from_neighbors(&[1, 2], &pass1) - 15.0).abs() < 1e-9);
        assert!((axis_from_neighbors(&[0, 1, 2, 3], &pass1) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn slot_multipliers_odd_even() {
        assert_eq!(slot_multipliers(2), vec![-0.5, 0.5]);
        assert_eq!(slot_multipliers(3), vec![-1.0, 0.0, 1.0]);
        assert_eq!(slot_multipliers(4), vec![-1.5, -0.5, 0.5, 1.5]);
    }
}
