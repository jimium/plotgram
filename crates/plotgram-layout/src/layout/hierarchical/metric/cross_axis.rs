//! P4.2 cross axis: Brandes–Köpf ideal + one **global** (cross-layer) VPSC
//! solve, run twice (roadmap phase A). Replaces the MVP's damped-barycenter
//! ideal + per-layer VPSC (notes/2026-08-02-mvp-scope.md §2.3):
//!
//! - Hard collinearity for BK primary blocks — but only over **virtual
//!   member pairs**. Real nodes stay soft: they are pulled toward the chain
//!   via their BK ideal in pass 1, and pass 2's port-anchor pull must not
//!   drag real nodes around. The dummy-chain trunk stays straight either
//!   way, which is the staircase fix.
//! - Everything else is soft: `desired` = 4-candidate merged BK ideal,
//!   dummies weighted higher so long edges keep their columns.
//! - Pass 2 expands port anchors ([`super::anchor`], Metric's own expansion
//!   — never read from Ink) and re-solves with chain-end dummies pulled
//!   onto their anchors. On single-dummy chains both ends address the same
//!   elem; the source anchor wins (target applied first, source overwrites)
//!   — a fixed, deterministic rule.
//!
//! Feasibility: equality pairs are a subset of one BK alignment's edges,
//! whose blocks never cross — so the constraint system is feasible by
//! construction. A VPSC failure is therefore a bug, not a layout
//! contingency, and fails hard (architecture.md §3.4); there is no packed
//! fallback anymore.

use std::collections::BTreeMap;

use plotgram_algo::orientation::Size;
use plotgram_algo::vpsc::{self, Constraint, Variable, VpscError};
use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::metric::bk;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

const REAL_WEIGHT: f64 = 1.0;
const VIRTUAL_WEIGHT: f64 = 4.0;

/// Per-elem canonical cross-axis center coordinate.
///
/// `main` is the main-axis (layer-top) position per elem, needed to expand
/// port anchors for the second pass.
pub fn assign_cross_axis(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
    main: &[f64],
    node_gap: f64,
) -> Result<Vec<f64>, VpscError> {
    let n = plan.elems.len();
    if n == 0 {
        return Ok(Vec::new());
    }

    let bk = bk::bk_ideal(plan, size_of, node_gap);
    let constraints = build_constraints(plan, size_of, node_gap, &bk.primary_blocks);
    let weights: Vec<f64> = (0..n)
        .map(|e| {
            if plan.elems[e].key.is_virtual() {
                VIRTUAL_WEIGHT
            } else {
                REAL_WEIGHT
            }
        })
        .collect();

    // Pass 1: node centers toward the merged BK ideal.
    let pass1 = solve_once(n, &bk.ideal, &weights, &constraints)?;

    // Pass 2: pull chain-end dummies onto their port anchors. Target ends
    // are applied first so the source anchor wins on single-dummy chains.
    let mut desired = bk.ideal;
    let frame_of = |e: usize| -> Rect {
        let s = size_of(e);
        Rect::new(pass1[e] - s.width / 2.0, main[e], s.width, s.height)
    };
    for e in &graph.edges {
        let rp = &ports[&e.edge_id];
        for (real_idx, port) in [(e.original_target, rp.target), (e.original_source, rp.source)] {
            let real_elem = plan.index_of[&ElemKey::Real(graph.ids[real_idx].clone())];
            let Some(nb) = chain_neighbor(plan, &e.edge_id, real_elem) else {
                continue;
            };
            if !plan.elems[nb].key.is_virtual() {
                continue; // single-hop edge: nothing to pull
            }
            desired[nb] = port_anchor(frame_of(real_elem), port).x;
        }
    }
    solve_once(n, &desired, &weights, &constraints)
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

/// Intra-layer separation + hard collinearity over the virtual member pairs
/// of each BK primary block (equality = opposing zero-gap constraint pairs,
/// which `vpsc::solve` supports directly).
fn build_constraints(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    blocks: &[Vec<usize>],
) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    for layer in &plan.layers {
        for i in 0..layer.len().saturating_sub(1) {
            let (l, r) = (layer[i], layer[i + 1]);
            let gap = size_of(l).width / 2.0 + size_of(r).width / 2.0 + node_gap;
            constraints.push(Constraint::new(l, r, gap));
        }
    }
    for block in blocks {
        for pair in block.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if plan.elems[a].key.is_virtual() && plan.elems[b].key.is_virtual() {
                constraints.push(Constraint::new(a, b, 0.0));
                constraints.push(Constraint::new(b, a, 0.0));
            }
        }
    }
    constraints
}

/// The elem adjacent to `real_elem` along `edge_id`'s chain (its only
/// segment neighbor — dummy or the other real endpoint).
fn chain_neighbor(plan: &PlanGraph, edge_id: &str, real_elem: usize) -> Option<usize> {
    plan.segments
        .iter()
        .find(|s| s.edge_id == edge_id && (s.from == real_elem || s.to == real_elem))
        .map(|s| if s.from == real_elem { s.to } else { s.from })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::ports::assign_ports;
    use crate::layout::hierarchical::model::{Elem, RealEdge, Segment};
    use plotgram_algo::orientation::Orientation as AlgoOrientation;

    fn real(id: &str, rank: u32) -> Elem {
        Elem {
            key: ElemKey::Real(id.into()),
            group_path: Vec::new(),
            rank,
        }
    }

    fn virt(edge_id: &str, ordinal: u32, rank: u32) -> Elem {
        Elem {
            key: ElemKey::Virtual {
                edge_id: edge_id.into(),
                ordinal,
            },
            group_path: Vec::new(),
            rank,
        }
    }

    fn seg(edge_id: &str, ordinal: u32, from: usize, to: usize) -> Segment {
        Segment {
            edge_id: edge_id.into(),
            ordinal,
            from,
            to,
        }
    }

    fn real_graph(ids: &[&str], edges: &[(&str, usize, usize)]) -> RealGraph {
        RealGraph {
            ids: ids.iter().map(|s| s.to_string()).collect(),
            index_of: ids
                .iter()
                .enumerate()
                .map(|(i, s)| (s.to_string(), i))
                .collect(),
            group_path: vec![Vec::new(); ids.len()],
            edges: edges
                .iter()
                .map(|(id, s, t)| RealEdge {
                    edge_id: (*id).into(),
                    original_source: *s,
                    original_target: *t,
                    working_source: *s,
                    working_target: *t,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                })
                .collect(),
            self_loops: Vec::new(),
        }
    }

    fn build_plan(elems: Vec<Elem>, layers: Vec<Vec<usize>>, segments: Vec<Segment>) -> PlanGraph {
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = (0..elems.len()).collect();
        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
        }
    }

    fn main_of(plan: &PlanGraph) -> Vec<f64> {
        plan.elems.iter().map(|e| e.rank as f64 * 100.0).collect()
    }

    /// a-c / b-d on two layers: 1:1 non-crossing chains keep their columns,
    /// intra-layer separation holds.
    #[test]
    fn non_crossing_pairs_stay_separated_and_aligned() {
        let elems = vec![real("a", 0), real("b", 0), real("c", 1), real("d", 1)];
        let layers = vec![vec![0, 1], vec![2, 3]];
        let segments = vec![seg("e0", 0, 0, 2), seg("e1", 0, 1, 3)];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(&["a", "b", "c", "d"], &[("e0", 0, 2), ("e1", 1, 3)]);
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb);

        let coords = assign_cross_axis(&plan, &graph, &ports, &|_| Size::new(20.0, 10.0), &main_of(&plan), 10.0)
            .expect("feasible");
        assert!(
            coords[1] - coords[0] >= 30.0 - 1e-6,
            "b must keep gap+width away from a"
        );
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

    /// Span-3 chain: the virtual pair is hard-collinear and settles at the
    /// mean of the two (single-port ⇒ center) endpoint anchors.
    #[test]
    fn long_chain_is_straight_between_endpoint_centers() {
        let elems = vec![
            real("a", 0),
            virt("e0", 0, 1),
            virt("e0", 1, 2),
            real("b", 3),
        ];
        let layers = vec![vec![0], vec![1], vec![2], vec![3]];
        let segments = vec![seg("e0", 0, 0, 1), seg("e0", 1, 1, 2), seg("e0", 2, 2, 3)];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(&["a", "b"], &[("e0", 0, 1)]);
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb);

        let coords = assign_cross_axis(&plan, &graph, &ports, &|_| Size::new(20.0, 10.0), &main_of(&plan), 10.0)
            .expect("feasible");
        assert!(
            (coords[1] - coords[2]).abs() < 1e-9,
            "dummies must be hard-collinear: {} vs {}",
            coords[1],
            coords[2]
        );
        let expected = (coords[0] + coords[3]) / 2.0;
        assert!(
            (coords[1] - expected).abs() < 1e-9,
            "chain column must sit at the mean of the endpoint anchors: {} vs {expected}",
            coords[1]
        );
    }

    /// Two crossing span-3 chains: BK's r-boundary keeps the blocks
    /// non-crossing, so the equality system stays feasible and separation
    /// holds in every layer.
    #[test]
    fn crossing_long_chains_stay_feasible_and_separated() {
        // a=0 b=1 db1=2 da1=3 da2=4 db2=5 ca=6 cb=7
        let elems = vec![
            real("a", 0),
            real("b", 0),
            virt("eb", 0, 1),
            virt("ea", 0, 1),
            virt("ea", 1, 2),
            virt("eb", 1, 2),
            real("ca", 3),
            real("cb", 3),
        ];
        let layers = vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]];
        let segments = vec![
            seg("ea", 0, 0, 3),
            seg("ea", 1, 3, 4),
            seg("ea", 2, 4, 6),
            seg("eb", 0, 1, 2),
            seg("eb", 1, 2, 5),
            seg("eb", 2, 5, 7),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(&["a", "b", "ca", "cb"], &[("ea", 0, 2), ("eb", 1, 3)]);
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb);

        let coords = assign_cross_axis(&plan, &graph, &ports, &|_| Size::new(20.0, 10.0), &main_of(&plan), 10.0)
            .expect("crossing chains must not make the equality system infeasible");
        // L1 order [db1, da1]; L2 order [da2, db2] — dummies are width 0.
        assert!(coords[3] - coords[2] >= 10.0 - 1e-6, "L1 separation");
        assert!(coords[5] - coords[4] >= 10.0 - 1e-6, "L2 separation");
        // L3: real nodes, width 20 + gap 10.
        assert!(coords[7] - coords[6] >= 30.0 - 1e-6, "L3 separation");
    }

    /// Three edges fan out of a wide node (3 south ports). Each chain's
    /// single dummy must land exactly on its port's slot column — not the
    /// node center — once nothing blocks it.
    #[test]
    fn end_dummies_are_pulled_onto_port_anchor_columns() {
        let elems = vec![
            real("A", 0),
            virt("e0", 0, 1),
            virt("e1", 0, 1),
            virt("e2", 0, 1),
            real("B0", 2),
            real("B1", 2),
            real("B2", 2),
        ];
        let layers = vec![vec![0], vec![1, 2, 3], vec![4, 5, 6]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e0", 1, 1, 4),
            seg("e1", 0, 0, 2),
            seg("e1", 1, 2, 5),
            seg("e2", 0, 0, 3),
            seg("e2", 1, 3, 6),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["A", "B0", "B1", "B2"],
            &[("e0", 0, 1), ("e1", 0, 2), ("e2", 0, 3)],
        );
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb);

        let width = |e: usize| {
            if plan.elems[e].key.is_virtual() {
                0.0 // virtual elems are zero-width, as in the real pipeline
            } else if e == 0 {
                90.0
            } else {
                20.0
            }
        };
        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|e| Size::new(width(e), 10.0),
            &main_of(&plan),
            10.0,
        )
        .expect("feasible");

        // 3 south ports on A (width 90): anchors at 1/4, 2/4, 3/4 of the
        // width, i.e. center − 22.5 / center / center + 22.5.
        let center = coords[0];
        assert!(
            (coords[1] - (center - 22.5)).abs() < 1e-9,
            "e0 dummy must sit on slot 0's anchor column: {}",
            coords[1]
        );
        assert!(
            (coords[2] - center).abs() < 1e-9,
            "e1 dummy must sit on the middle anchor (= center): {}",
            coords[2]
        );
        assert!(
            (coords[3] - (center + 22.5)).abs() < 1e-9,
            "e2 dummy must sit on slot 2's anchor column: {}",
            coords[3]
        );
    }

    #[test]
    fn deterministic_bit_identical_reruns() {
        let elems = vec![
            real("A", 0),
            virt("e0", 0, 1),
            virt("e0", 1, 2),
            real("B", 3),
        ];
        let layers = vec![vec![0], vec![1], vec![2], vec![3]];
        let segments = vec![seg("e0", 0, 0, 1), seg("e0", 1, 1, 2), seg("e0", 2, 2, 3)];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(&["A", "B"], &[("e0", 0, 1)]);
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb);

        let run = || {
            assign_cross_axis(
                &plan,
                &graph,
                &ports,
                &|_| Size::new(20.0, 10.0),
                &main_of(&plan),
                10.0,
            )
            .expect("feasible")
        };
        let c1 = run();
        let c2 = run();
        let b1: Vec<u64> = c1.iter().map(|f| f.to_bits()).collect();
        let b2: Vec<u64> = c2.iter().map(|f| f.to_bits()).collect();
        assert_eq!(b1, b2, "cross-axis output must be bit-identical");
    }
}
