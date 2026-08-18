//! Cross-axis writer: Brandes–Köpf ideal + symmetry objective (P4).
//!
//! Sole consumer of [`super::symmetry_objective`]: iterative weighted-median
//! + VPSC under layer separation and VV equalities. Legacy SymmetryPlan
//! two-pass (claimed / FanPack / rigid class) was removed in P4-S4.

use std::collections::BTreeMap;

use tautcore_algo::orientation::Size;
use tautcore_algo::vpsc::VpscError;

use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::metric::symmetry_objective;
use crate::layout::hierarchical::model::PlanGraph;
use crate::layout::hierarchical::model::RealGraph;
use crate::layout::hierarchical::params::HierarchicalParams;

/// Per-elem canonical cross-axis center coordinate.
///
/// `main` is the main-axis (layer-top) position per elem, needed to expand
/// port anchors. Clustered ends share one `PortPoint` (yFiles bus-style).
pub fn assign_cross_axis(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    size_of: &dyn Fn(usize) -> Size,
    main: &[f64],
    params: &HierarchicalParams,
) -> Result<Vec<f64>, VpscError> {
    symmetry_objective::solve_symmetry_objective(plan, graph, ports, size_of, main, params)
}

/// Push a same-layer dummy's soft desired outside neighbors that already
/// carry absolute desired, matching layer order + sep gap.
pub(crate) fn exteriorize_dummy_desired(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    desired: &[f64],
    dummy: usize,
    mut anchor: f64,
) -> f64 {
    let rank = plan.elems[dummy].rank as usize;
    let Some(layer) = plan.layers.get(rank) else {
        return anchor;
    };
    let Some(pos) = layer.iter().position(|&e| e == dummy) else {
        return anchor;
    };
    if pos > 0 {
        let left = layer[pos - 1];
        let gap = size_of(left).width / 2.0 + size_of(dummy).width / 2.0 + node_gap;
        anchor = anchor.max(desired[left] + gap);
    }
    if pos + 1 < layer.len() {
        let right = layer[pos + 1];
        let gap = size_of(dummy).width / 2.0 + size_of(right).width / 2.0 + node_gap;
        anchor = anchor.min(desired[right] - gap);
    }
    anchor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::ports::assign_ports;
    use crate::layout::hierarchical::model::{Elem, ElemKey, RealEdge, Segment};
    use tautcore_algo::orientation::Orientation as AlgoOrientation;

    fn gap_params(node_gap: f64) -> HierarchicalParams {
        HierarchicalParams {
            node_gap,
            ..HierarchicalParams::default()
        }
    }

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
            shapes: vec![tautcore_model::NodeShape::DEFAULT; ids.len()],
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
                    weight: 1.0,
                    ..Default::default()
                })
                .collect(),
            self_loops: Vec::new(),
            ..Default::default()
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
            ..Default::default()
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
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

    /// All-real 1:1 chain with a leaf at the end (the commit→…→prod→done
    /// shape): same-type hard collinearity must place every member — leaf
    /// included — on one column, even though the packed-center ideals
    /// differ per width.
    #[test]
    fn real_chain_and_leaf_share_one_column() {
        let elems = vec![real("a", 0), real("b", 1), real("c", 2), real("done", 3)];
        let layers = vec![vec![0], vec![1], vec![2], vec![3]];
        let segments = vec![seg("e0", 0, 0, 1), seg("e1", 0, 1, 2), seg("e2", 0, 2, 3)];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["a", "b", "c", "done"],
            &[("e0", 0, 1), ("e1", 1, 2), ("e2", 2, 3)],
        );
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        // Unequal widths make the packed-center ideals differ per elem, so
        // only the hard equality (not the soft ideal) can keep them aligned.
        let widths = [20.0, 60.0, 40.0, 30.0];
        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|e| Size::new(widths[e], 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
        .expect("feasible");
        for i in 1..4 {
            assert!(
                (coords[i] - coords[0]).abs() < 1e-6,
                "elem {i} must share the chain column: {} vs {}",
                coords[i],
                coords[0]
            );
        }
    }

    /// Multi-hop spine above a binary fan must share the fan axis
    /// (`gw → api → worker → {m, db}`), including degree-2 `api`.
    #[test]
    fn multi_hop_spine_collinear_on_fan_axis() {
        let elems = vec![
            real("gw", 0),
            real("api", 1),
            real("worker", 2),
            real("m", 3),
            real("db", 3),
        ];
        let layers = vec![vec![0], vec![1], vec![2], vec![3, 4]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 1, 2),
            seg("e2", 0, 2, 3),
            seg("e3", 0, 2, 4),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["gw", "api", "worker", "m", "db"],
            &[("e0", 0, 1), ("e1", 1, 2), ("e2", 2, 3), ("e3", 2, 4)],
        );
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
        .expect("feasible");
        let axis = (coords[3] + coords[4]) / 2.0;
        for (i, name) in [(0, "gw"), (1, "api"), (2, "worker")] {
            assert!(
                (coords[i] - axis).abs() < 1e-6,
                "{name} must sit on fan axis: {} vs {axis}",
                coords[i]
            );
        }
    }

    /// Binary fan (gateway → {A, B} → database): both the source hub and
    /// the sink must sit at the midpoint of the two middle-layer nodes —
    /// degree-2 must not stay welded to the left child
    /// (smoke.flat-gateway-fanout asymmetry).
    #[test]
    fn binary_fan_diamond_centers_hub_and_sink() {
        // gw=0, a=1, b=2, db=3
        let elems = vec![real("gw", 0), real("a", 1), real("b", 1), real("db", 2)];
        let layers = vec![vec![0], vec![1, 2], vec![3]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 0, 2),
            seg("e2", 0, 1, 3),
            seg("e3", 0, 2, 3),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["gw", "a", "b", "db"],
            &[("e0", 0, 1), ("e1", 0, 2), ("e2", 1, 3), ("e3", 2, 3)],
        );
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
        .expect("feasible");
        let mid = (coords[1] + coords[2]) / 2.0;
        assert!(
            (coords[0] - mid).abs() < 1e-6,
            "gw must center over {{a,b}}: {} vs {mid}",
            coords[0]
        );
        assert!(
            (coords[3] - mid).abs() < 1e-6,
            "db must center under {{a,b}}: {} vs {mid}",
            coords[3]
        );
    }

    /// Odd fan-out/in (hub → {a,b,c} → sink): both junctions sit on the
    /// middle child's column — not welded to the leftmost child by BK
    /// left bias (auto_edge_grouping.taut / orch asymmetry).
    #[test]
    fn odd_fan_diamond_centers_hub_and_sink() {
        // hub=0, a=1, b=2, c=3, sink=4
        let elems = vec![
            real("hub", 0),
            real("a", 1),
            real("b", 1),
            real("c", 1),
            real("sink", 2),
        ];
        let layers = vec![vec![0], vec![1, 2, 3], vec![4]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 0, 2),
            seg("e2", 0, 0, 3),
            seg("e3", 0, 1, 4),
            seg("e4", 0, 2, 4),
            seg("e5", 0, 3, 4),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["hub", "a", "b", "c", "sink"],
            &[
                ("e0", 0, 1),
                ("e1", 0, 2),
                ("e2", 0, 3),
                ("e3", 1, 4),
                ("e4", 2, 4),
                ("e5", 3, 4),
            ],
        );
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
        .expect("feasible");
        let mid = coords[2]; // median child `b`
        assert!(
            (coords[0] - mid).abs() < 1e-6,
            "hub must sit on median child: {} vs {mid}",
            coords[0]
        );
        assert!(
            (coords[4] - mid).abs() < 1e-6,
            "sink must sit on median parent: {} vs {mid}",
            coords[4]
        );
        // Outer span midpoint equals median for equal-width packing.
        let outer = (coords[1] + coords[3]) / 2.0;
        assert!(
            (coords[0] - outer).abs() < 1e-6,
            "hub must also be the fan's geometric middle: {} vs {outer}",
            coords[0]
        );
    }

    /// Spine above an odd fan (`submit → review → {L,M,R} → notify`) must
    /// follow the fan axis — not stay welded left by BK + hard collinearity
    /// (product.symmetric-fanout centering).
    #[test]
    fn fan_spine_chain_follows_median() {
        // submit=0, review=1, L=2, M=3, R=4, notify=5
        let elems = vec![
            real("submit", 0),
            real("review", 1),
            real("L", 2),
            real("M", 2),
            real("R", 2),
            real("notify", 3),
        ];
        let layers = vec![vec![0], vec![1], vec![2, 3, 4], vec![5]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 1, 2),
            seg("e2", 0, 1, 3),
            seg("e3", 0, 1, 4),
            seg("e4", 0, 2, 5),
            seg("e5", 0, 3, 5),
            seg("e6", 0, 4, 5),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["submit", "review", "L", "M", "R", "notify"],
            &[
                ("e0", 0, 1),
                ("e1", 1, 2),
                ("e2", 1, 3),
                ("e3", 1, 4),
                ("e4", 2, 5),
                ("e5", 3, 5),
                ("e6", 4, 5),
            ],
        );
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
        .expect("feasible");
        let mid = coords[3]; // median child M
        for (i, name) in [(0, "submit"), (1, "review"), (5, "notify")] {
            assert!(
                (coords[i] - mid).abs() < 1e-6,
                "{name} must sit on median fan axis: {} vs {mid}",
                coords[i]
            );
        }
    }

    /// Fan-out junction (the API gateway over 4 services shape): the
    /// junction must not be welded to one median child — it centers over
    /// its fan, and a child's own 1:1 chain below stays straight.
    #[test]
    fn fan_out_junction_centers_over_its_children() {
        // hub -> {a, b, c, d}; b continues to b2 (1:1 below the fan).
        let elems = vec![
            real("hub", 0),
            real("a", 1),
            real("b", 1),
            real("c", 1),
            real("d", 1),
            real("b2", 2),
        ];
        let layers = vec![vec![0], vec![1, 2, 3, 4], vec![5]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 0, 2),
            seg("e2", 0, 0, 3),
            seg("e3", 0, 0, 4),
            seg("e4", 0, 2, 5),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["hub", "a", "b", "c", "d", "b2"],
            &[
                ("e0", 0, 1),
                ("e1", 0, 2),
                ("e2", 0, 3),
                ("e3", 0, 4),
                ("e4", 2, 5),
            ],
        );
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
        .expect("feasible");
        // Even fan of four equal-width children: hub sits at the midpoint
        // of the two middle children (= midpoint of the outer two).
        let expected = (coords[2] + coords[3]) / 2.0;
        assert!(
            (coords[0] - expected).abs() < 1e-6,
            "hub must center over its fan: {} vs {expected}",
            coords[0]
        );
        let outer = (coords[1] + coords[4]) / 2.0;
        assert!(
            (coords[0] - outer).abs() < 1e-6,
            "hub must sit at the fan's middle: {} vs {outer}",
            coords[0]
        );
        // b's own chain below the fan stays straight.
        assert!(
            (coords[5] - coords[2]).abs() < 1e-6,
            "b2 must stay under b: {} vs {}",
            coords[5],
            coords[2]
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let coords = assign_cross_axis(
            &plan,
            &graph,
            &ports,
            &|_| Size::new(20.0, 10.0),
            &main_of(&plan),
            &gap_params(10.0),
        )
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

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
            &gap_params(10.0),
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;

        let run = || {
            assign_cross_axis(
                &plan,
                &graph,
                &ports,
                &|_| Size::new(20.0, 10.0),
                &main_of(&plan),
                &gap_params(10.0),
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
