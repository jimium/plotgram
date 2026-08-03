//! P4.2 cross axis: iterative neighbor-median relaxation for an "ideal"
//! position (straightens dummy chains), then a per-layer VPSC solve for the
//! final non-overlapping coordinate (separation by order, soft-pulled to
//! ideal — dummies weighted higher so long edges stay straighter). See
//! `docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md` §2.3.

use plotgram_algo::orientation::Size;
use plotgram_algo::vpsc::{self, Constraint, Variable};

use crate::layout::hierarchical::model::PlanGraph;

const RELAX_ITERS: usize = 6;
const RELAX_DAMPING: f64 = 0.5;
const REAL_WEIGHT: f64 = 1.0;
const VIRTUAL_WEIGHT: f64 = 4.0;

/// Per-elem canonical cross-axis center coordinate.
pub fn assign_cross_axis(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
) -> Vec<f64> {
    let n = plan.elems.len();
    if n == 0 {
        return Vec::new();
    }

    let (up, down) = build_adjacency(plan);

    // 1) initial packed position (also the fallback if VPSC ever fails).
    let mut ideal = vec![0.0; n];
    for layer in &plan.layers {
        let mut cursor = 0.0;
        for &e in layer {
            let w = size_of(e).width;
            ideal[e] = cursor + w / 2.0;
            cursor += w + node_gap;
        }
    }
    let packed = ideal.clone();

    // 2) neighbor-median relaxation.
    for _ in 0..RELAX_ITERS {
        let mut next = ideal.clone();
        for e in 0..n {
            let mut sum = 0.0;
            let mut count = 0usize;
            for &nb in up[e].iter().chain(down[e].iter()) {
                sum += ideal[nb];
                count += 1;
            }
            if count > 0 {
                let mean = sum / count as f64;
                next[e] = ideal[e] + RELAX_DAMPING * (mean - ideal[e]);
            }
        }
        ideal = next;
    }

    // 3) per-layer VPSC: order-based separation, pulled toward `ideal`.
    let mut result = vec![0.0; n];
    for layer in &plan.layers {
        if layer.is_empty() {
            continue;
        }
        let vars: Vec<Variable> = layer
            .iter()
            .map(|&e| {
                let w = if plan.elems[e].key.is_virtual() {
                    VIRTUAL_WEIGHT
                } else {
                    REAL_WEIGHT
                };
                Variable {
                    desired: ideal[e],
                    weight: w,
                }
            })
            .collect();
        let mut constraints = Vec::with_capacity(layer.len().saturating_sub(1));
        for i in 0..layer.len().saturating_sub(1) {
            let (l, r) = (layer[i], layer[i + 1]);
            let gap = size_of(l).width / 2.0 + size_of(r).width / 2.0 + node_gap;
            constraints.push(Constraint::new(i, i + 1, gap));
        }
        match vpsc::solve(&vars, &constraints) {
            Ok(positions) => {
                for (i, &e) in layer.iter().enumerate() {
                    result[e] = positions[i];
                }
            }
            Err(_) => {
                // Defensive fallback: a simple adjacent-chain of positive-gap
                // constraints is always feasible, so this should not trigger.
                for &e in layer {
                    result[e] = packed[e];
                }
            }
        }
    }

    result
}

fn build_adjacency(plan: &PlanGraph) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let n = plan.elems.len();
    let mut up = vec![Vec::new(); n];
    let mut down = vec![Vec::new(); n];
    for s in &plan.segments {
        down[s.from].push(s.to);
        up[s.to].push(s.from);
    }
    (up, down)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey, Segment};

    fn simple_plan() -> PlanGraph {
        // layer0: a(w20) b(w20); layer1: c(w20) d(w20); a-c, b-d (no crossing)
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("d".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
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
        PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1, 2, 3],
            segments,
            layers,
        }
    }

    #[test]
    fn no_overlap_within_layer() {
        let plan = simple_plan();
        let coords = assign_cross_axis(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        assert!(
            coords[1] - coords[0] >= 30.0 - 1e-6,
            "b must be at least gap+width away from a"
        );
    }

    #[test]
    fn aligned_chain_stays_straight() {
        let plan = simple_plan();
        let coords = assign_cross_axis(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        // a-c and b-d are 1:1 non-crossing chains with identical widths on
        // both layers: relaxation should keep each pair aligned.
        assert!(
            (coords[0] - coords[2]).abs() < 1e-6,
            "a/c should align: {} vs {}",
            coords[0],
            coords[2]
        );
        assert!(
            (coords[1] - coords[3]).abs() < 1e-6,
            "b/d should align: {} vs {}",
            coords[1],
            coords[3]
        );
    }

    #[test]
    fn deterministic_rerun() {
        let plan = simple_plan();
        let c1 = assign_cross_axis(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        let c2 = assign_cross_axis(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        assert_eq!(c1, c2);
    }
}
