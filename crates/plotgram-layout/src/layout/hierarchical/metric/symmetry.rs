//! Symmetry helpers shared by the cross-axis objective solver (P4).
//!
//! Fan adjacency is **forward real endpoints** on [`RealGraph`] (non-reversed
//! edges): long edges still count as one hop. Twin pairs (2-cycle) and
//! unique-min-span primary arms feed soft weights / snap desired in
//! [`super::symmetry_objective`]. The old SymmetryPlan tables (claimed /
//! FanPack / RigidColumnClass) were deleted in P4-S4.

use std::collections::BTreeSet;

use plotgram_algo::orientation::Size;

use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

/// Forward (non-reversed) real–real adjacency: long edges count as one hop
/// between endpoints; dummies do not hide a fan.
pub fn forward_real_adjacency(
    plan: &PlanGraph,
    graph: &RealGraph,
) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let n = plan.elems.len();
    let (mut down, mut up) = (vec![Vec::new(); n], vec![Vec::new(); n]);
    for e in &graph.edges {
        if e.reversed {
            continue;
        }
        let src_id = &graph.ids[e.working_source];
        let tgt_id = &graph.ids[e.working_target];
        let Some(&src) = plan.index_of.get(&ElemKey::Real(src_id.clone())) else {
            continue;
        };
        let Some(&tgt) = plan.index_of.get(&ElemKey::Real(tgt_id.clone())) else {
            continue;
        };
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

fn is_twin_peer(hub: usize, peer: usize, twins: &BTreeSet<(usize, usize)>) -> bool {
    twins.contains(&undirected_real_pair(hub, peer))
}

/// Unique min-span non-twin real neighbor on one side — primary arm on spine.
/// Tied minima → `None` (keep even/odd fan-out mirror).
pub(crate) fn unique_min_span_primary(
    hub: usize,
    neighbors: &[usize],
    plan: &PlanGraph,
    twins: &BTreeSet<(usize, usize)>,
) -> Option<usize> {
    let cands = min_span_candidates(hub, neighbors, plan, twins);
    match cands.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// Fan-in primary: unique min-span, or on an **odd** tie the median parent by
/// layer order. Even ties → `None` so the hub stays on the fan midpoint
/// (D3 multi-rank backedge / even mirror).
pub(crate) fn min_span_primary_median_tie(
    hub: usize,
    neighbors: &[usize],
    plan: &PlanGraph,
    twins: &BTreeSet<(usize, usize)>,
    layer_pos: &[usize],
) -> Option<usize> {
    let mut cands = min_span_candidates(hub, neighbors, plan, twins);
    match cands.len() {
        0 => None,
        1 => Some(cands[0]),
        n if n % 2 == 0 => None,
        _ => {
            cands.sort_by(|&a, &b| layer_pos[a].cmp(&layer_pos[b]).then(a.cmp(&b)));
            Some(cands[cands.len() / 2])
        }
    }
}

fn min_span_candidates(
    hub: usize,
    neighbors: &[usize],
    plan: &PlanGraph,
    twins: &BTreeSet<(usize, usize)>,
) -> Vec<usize> {
    let hub_rank = plan.elems[hub].rank as i64;
    let mut best_span: Option<i64> = None;
    let mut cands = Vec::new();
    for &nb in neighbors {
        if plan.elems[nb].key.is_virtual() || is_twin_peer(hub, nb, twins) {
            continue;
        }
        let span = (plan.elems[nb].rank as i64 - hub_rank).abs();
        match best_span {
            None => {
                best_span = Some(span);
                cands = vec![nb];
            }
            Some(s) if span < s => {
                best_span = Some(span);
                cands = vec![nb];
            }
            Some(s) if span == s => cands.push(nb),
            _ => {}
        }
    }
    cands
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

    #[test]
    fn unique_min_span_primary_picks_nearest() {
        let elems = vec![
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("h".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("near".into()),
                group_path: Vec::new(),
                rank: 1,
            },
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("far".into()),
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
            decl_index: vec![0, 1, 2],
            segments: Vec::new(),
            layers: vec![vec![0], vec![1], vec![], vec![2]],
            ..Default::default()
        };
        let twins = BTreeSet::new();
        assert_eq!(unique_min_span_primary(0, &[1, 2], &plan, &twins), Some(1));
        // Tied spans → None
        let elems2 = vec![
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("h".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 2,
            },
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 2,
            },
        ];
        let index_of = elems2
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan2 = PlanGraph {
            elems: elems2,
            index_of,
            decl_index: vec![0, 1, 2],
            segments: Vec::new(),
            layers: vec![vec![0], vec![], vec![1, 2]],
            ..Default::default()
        };
        assert_eq!(unique_min_span_primary(0, &[1, 2], &plan2, &twins), None);
        let layer_pos = [0usize, 0, 1];
        assert_eq!(
            min_span_primary_median_tie(0, &[1, 2], &plan2, &twins, &layer_pos),
            None,
            "even fan-in tie keeps midpoint (no primary)"
        );
        // Odd tie → median by layer order.
        let elems3 = vec![
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("h".into()),
                group_path: Vec::new(),
                rank: 1,
            },
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            crate::layout::hierarchical::model::Elem {
                key: ElemKey::Real("c".into()),
                group_path: Vec::new(),
                rank: 0,
            },
        ];
        let index_of = elems3
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan3 = PlanGraph {
            elems: elems3,
            index_of,
            decl_index: vec![0, 1, 2, 3],
            segments: Vec::new(),
            layers: vec![vec![1, 2, 3], vec![0]],
            ..Default::default()
        };
        let layer_pos3 = [0usize, 0, 1, 2];
        assert_eq!(
            min_span_primary_median_tie(0, &[1, 2, 3], &plan3, &twins, &layer_pos3),
            Some(2),
            "odd fan-in tie picks layer-order median parent"
        );
    }
}
