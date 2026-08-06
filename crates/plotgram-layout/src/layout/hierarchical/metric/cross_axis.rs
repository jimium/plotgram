//! P4.2 cross axis: Brandes–Köpf ideal + one **global** (cross-layer) VPSC
//! solve, run twice (roadmap phase A). Replaces the MVP's damped-barycenter
//! ideal + per-layer VPSC (notes/2026-08-02-mvp-scope.md §2.3):
//!
//! - Hard collinearity for BK primary blocks — over **same-type member
//!   pairs**: virtual-virtual keeps long-edge trunks straight, and
//!   real-real straightens 1:1 real chains (a spine must not zigzag,
//!   and a leaf must land under its only neighbor). A real pair is only
//!   hardened when both sides are directionally 1:1; any fan of ≥ 2
//!   real neighbors stays soft so pass 2 can place the junction on the
//!   fan's center — BK's layer sweep welds a fan-out hub to its
//!   *leftmost* child under left bias (each child has one upper
//!   neighbor), so odd fans are not "already centered" by hardening a
//!   median block pair. Pairs only harden when *neither* member aligned
//!   with a dummy anywhere in the primary alignment: a real node that
//!   won a dummy as its median belongs to that chain's column battle,
//!   and hardening it would drag the whole chain off its corridor. All
//!   other pairs stay soft: pass 2's port-anchor pull on chain-end
//!   dummies must not drag real nodes around.
//! - Everything else is soft: `desired` = 4-candidate merged BK ideal,
//!   dummies weighted higher so long edges keep their columns.
//! - Pass 2 expands two upstream decisions (Metric's own expansions —
//!   never read from Ink): fan junctions (≥ 2 real neighbors on a side)
//!   get their desired rewritten to the fan center of their neighbors'
//!   pass-1 positions (odd → single median; even → midpoint of the two
//!   middle neighbors); exclusive degree-1 neighbors on the **non-fan**
//!   side follow that axis (spine above/below a fan); and chain-end
//!   dummies are pulled onto their port anchors. On single-dummy chains
//!   both ends address the same elem; the source anchor wins (target
//!   applied first, source overwrites) — a fixed, deterministic rule.
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
/// port anchors for the second pass. Clustered ends share one `PortPoint`
/// (yFiles bus-style — no pitch spread).
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
    let dummy_aligned = dummy_aligned_reals(plan, &bk.primary_blocks);
    let (down_deg, up_deg) = real_degrees(plan);
    let constraints = build_constraints(
        plan,
        size_of,
        node_gap,
        &bk.primary_blocks,
        &dummy_aligned,
        &down_deg,
        &up_deg,
    );
    // Critical-marked edges (edge-parameters §2.5): their dummy chains hold
    // their desired with doubled VPSC soft weight, pulling the corridor
    // harder toward the port anchors set in pass 2.
    let critical_edges: std::collections::BTreeSet<&str> = graph
        .edges
        .iter()
        .filter(|e| e.critical)
        .map(|e| e.edge_id.as_str())
        .collect();
    let weights: Vec<f64> = (0..n)
        .map(|e| match &plan.elems[e].key {
            ElemKey::Virtual { edge_id, .. } if critical_edges.contains(edge_id.as_str()) => {
                VIRTUAL_WEIGHT * 2.0
            }
            ElemKey::Virtual { .. } => VIRTUAL_WEIGHT,
            ElemKey::Real(_) => REAL_WEIGHT,
        })
        .collect();

    // Pass 1: node centers toward the merged BK ideal.
    let pass1 = solve_once(n, &bk.ideal, &weights, &constraints)?;

    // Pass 2: fan junctions center over their fan; exclusive degree-1
    // chain neighbors follow that axis; chain-end dummies are pulled
    // onto their port anchors. Target ends are applied first so the
    // source anchor wins on single-dummy chains.
    let mut desired = bk.ideal;
    let fan_targets = fan_centers(plan, &pass1, &down_deg, &up_deg);
    for &(e, center) in &fan_targets {
        desired[e] = center;
    }
    pull_fan_chain_followers(plan, &fan_targets, &down_deg, &up_deg, &mut desired);
    let frame_of = |e: usize| -> Rect {
        let s = size_of(e);
        Rect::new(pass1[e] - s.width / 2.0, main[e], s.width, s.height)
    };
    for e in &graph.edges {
        let rp = &ports[&e.edge_id];
        for (real_idx, port, _cluster) in [
            (e.original_target, rp.target, rp.target_cluster),
            (e.original_source, rp.source, rp.source_cluster),
        ] {
            let real_elem = plan.index_of[&ElemKey::Real(graph.ids[real_idx].clone())];
            let Some(nb) = chain_neighbor(plan, &e.edge_id, real_elem) else {
                continue;
            };
            if !plan.elems[nb].key.is_virtual() {
                continue; // single-hop edge: nothing to pull
            }
            // Clustered ends share one PortPoint — pull the chain-end dummy
            // onto that shared anchor (no member pitch offset).
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

/// Real block members that sit next to a virtual member in the primary
/// alignment — i.e., real nodes whose median was a dummy. They keep the
/// soft treatment (see module doc).
fn dummy_aligned_reals(plan: &PlanGraph, blocks: &[Vec<usize>]) -> Vec<bool> {
    let mut marked = vec![false; plan.elems.len()];
    for block in blocks {
        for pair in block.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let a_virtual = plan.elems[a].key.is_virtual();
            if a_virtual != plan.elems[b].key.is_virtual() {
                if !a_virtual {
                    marked[a] = true;
                } else {
                    marked[b] = true;
                }
            }
        }
    }
    marked
}

/// Per real elem: how many real neighbors it has one rank below / above
/// (real-real segments only — dummies belong to their edge's chain).
fn real_degrees(plan: &PlanGraph) -> (Vec<usize>, Vec<usize>) {
    let n = plan.elems.len();
    let (mut down, mut up) = (vec![0usize; n], vec![0usize; n]);
    for s in &plan.segments {
        if !plan.elems[s.from].key.is_virtual() && !plan.elems[s.to].key.is_virtual() {
            down[s.from] += 1;
            up[s.to] += 1;
        }
    }
    (down, up)
}

/// Fan-centering targets for pass 2: a real junction with ≥ 2 real
/// neighbors on one side wants the fan center of those neighbors'
/// pass-1 positions — odd count → single median; even → midpoint of the
/// two middle neighbors. (BK left-bias welds a fan-out hub to its
/// leftmost child, so odd fans need an explicit target just like even
/// ones.) A junction fanned on both sides takes the mean of both side
/// centers. Neighbor order: pass-1 position, elem index tie-break.
fn fan_centers(
    plan: &PlanGraph,
    pass1: &[f64],
    down_deg: &[usize],
    up_deg: &[usize],
) -> Vec<(usize, f64)> {
    // e -> real neighbors below / above (segment adjacency).
    let n = plan.elems.len();
    let (mut down_nbs, mut up_nbs) = (vec![Vec::new(); n], vec![Vec::new(); n]);
    for s in &plan.segments {
        if !plan.elems[s.from].key.is_virtual() && !plan.elems[s.to].key.is_virtual() {
            down_nbs[s.from].push(s.to);
            up_nbs[s.to].push(s.from);
        }
    }
    let fan_center = |nbs: &mut Vec<usize>| {
        nbs.sort_by(|a, b| {
            pass1[*a]
                .partial_cmp(&pass1[*b])
                .unwrap()
                .then(a.cmp(b))
        });
        let m = nbs.len() / 2;
        if nbs.len() % 2 == 1 {
            pass1[nbs[m]]
        } else {
            (pass1[nbs[m - 1]] + pass1[nbs[m]]) / 2.0
        }
    };
    let mut targets = Vec::new();
    for e in 0..n {
        if plan.elems[e].key.is_virtual() {
            continue;
        }
        let mut centers: Vec<f64> = Vec::new();
        if down_deg[e] >= 2 {
            centers.push(fan_center(&mut down_nbs[e]));
        }
        if up_deg[e] >= 2 {
            centers.push(fan_center(&mut up_nbs[e]));
        }
        if !centers.is_empty() {
            targets.push((e, centers.iter().sum::<f64>() / centers.len() as f64));
        }
    }
    targets
}

/// Intra-layer separation + hard collinearity over the same-type member
/// pairs of each BK primary block (equality = opposing zero-gap constraint
/// pairs, which `vpsc::solve` supports directly). A real pair touching a
/// fan of ≥ 2 real neighbors stays soft so pass 2 can place the junction
/// on the fan center. Pairs touching a dummy-aligned member stay soft too
/// (chain-drag guard, module doc).
fn build_constraints(
    plan: &PlanGraph,
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    blocks: &[Vec<usize>],
    dummy_aligned: &[bool],
    down_deg: &[usize],
    up_deg: &[usize],
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
            let a_virtual = plan.elems[a].key.is_virtual();
            if a_virtual == plan.elems[b].key.is_virtual() {
                if !a_virtual {
                    // Any fan junction (deg≥2 on either side of either end)
                    // stays soft for pass-2 centering — including a hub
                    // whose fan is opposite the BK block edge (e.g. submit
                    // welded to review while review fans downward).
                    if !hardenable_real_pair(
                        dummy_aligned[a] || dummy_aligned[b],
                        down_deg[a],
                        up_deg[a],
                        down_deg[b],
                        up_deg[b],
                    ) {
                        continue;
                    }
                }
                constraints.push(Constraint::new(a, b, 0.0));
                constraints.push(Constraint::new(b, a, 0.0));
            }
        }
    }
    constraints
}

/// Whether a same-type real–real BK block pair should receive hard
/// collinearity. Either endpoint that is a fan junction (`down≥2` or
/// `up≥2`) stays soft for pass-2 centering; dummy-aligned members stay
/// soft (chain-drag guard).
pub(crate) fn hardenable_real_pair(
    either_dummy_aligned: bool,
    a_down: usize,
    a_up: usize,
    b_down: usize,
    b_up: usize,
) -> bool {
    if either_dummy_aligned {
        return false;
    }
    let is_fan = |down: usize, up: usize| down >= 2 || up >= 2;
    !is_fan(a_down, a_up) && !is_fan(b_down, b_up)
}

/// After fan hubs get a target, pull exclusive degree-1 real neighbors on
/// the **non-fan** side onto the same axis (e.g. `submit` above a fan-out
/// `review`). Fan members themselves are never pulled — that would collapse
/// the pack toward the hub.
fn pull_fan_chain_followers(
    plan: &PlanGraph,
    fan_targets: &[(usize, f64)],
    down_deg: &[usize],
    up_deg: &[usize],
    desired: &mut [f64],
) {
    let n = plan.elems.len();
    let (mut down_nbs, mut up_nbs) = (vec![Vec::new(); n], vec![Vec::new(); n]);
    for s in &plan.segments {
        if !plan.elems[s.from].key.is_virtual() && !plan.elems[s.to].key.is_virtual() {
            down_nbs[s.from].push(s.to);
            up_nbs[s.to].push(s.from);
        }
    }
    let is_fan = |e: usize| down_deg[e] >= 2 || up_deg[e] >= 2;
    for &(hub, center) in fan_targets {
        let mut candidates = Vec::new();
        // Fan below → exclusive parents may follow; fan above → exclusive children.
        if down_deg[hub] >= 2 {
            candidates.extend(up_nbs[hub].iter().copied());
        }
        if up_deg[hub] >= 2 {
            candidates.extend(down_nbs[hub].iter().copied());
        }
        for nb in candidates {
            if is_fan(nb) {
                continue;
            }
            if down_deg[nb] + up_deg[nb] == 1 {
                desired[nb] = center;
            }
        }
    }
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
            shapes: vec![plotgram_model::NodeShape::DEFAULT; ids.len()],
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
                    critical: false,
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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

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

    /// All-real 1:1 chain with a leaf at the end (the commit→…→prod→done
    /// shape): same-type hard collinearity must place every member — leaf
    /// included — on one column, even though the packed-center ideals
    /// differ per width.
    #[test]
    fn real_chain_and_leaf_share_one_column() {
        let elems = vec![real("a", 0), real("b", 1), real("c", 2), real("done", 3)];
        let layers = vec![vec![0], vec![1], vec![2], vec![3]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 1, 2),
            seg("e2", 0, 2, 3),
        ];
        let plan = build_plan(elems, layers, segments);
        let graph = real_graph(
            &["a", "b", "c", "done"],
            &[("e0", 0, 1), ("e1", 1, 2), ("e2", 2, 3)],
        );
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
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
            10.0,
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

    #[test]
    fn hardenable_real_pair_matrix() {
        // (dummy, a_down, a_up, b_down, b_up, expect_harden)
        let cases: &[(bool, usize, usize, usize, usize, bool)] = &[
            (false, 1, 1, 1, 1, true),  // 1:1 chain
            (false, 2, 1, 1, 1, false), // a fan-out
            (false, 3, 1, 1, 1, false), // a odd fan-out
            (false, 1, 1, 1, 2, false), // b fan-in
            (false, 1, 1, 1, 3, false), // b odd fan-in
            (false, 1, 1, 3, 1, false), // b fan-out (opposite BK edge)
            (true, 1, 1, 1, 1, false),  // chain-drag guard
            (false, 0, 1, 1, 0, true),  // leaf / non-fan
        ];
        for (i, &(dummy, ad, au, bd, bu, want)) in cases.iter().enumerate() {
            assert_eq!(
                hardenable_real_pair(dummy, ad, au, bd, bu),
                want,
                "case {i}"
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
        let elems = vec![
            real("gw", 0),
            real("a", 1),
            real("b", 1),
            real("db", 2),
        ];
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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

        let coords =
            assign_cross_axis(&plan, &graph, &ports, &|_| Size::new(20.0, 10.0), &main_of(&plan), 10.0)
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
    /// left bias (auto_edge_grouping.pgm / orch asymmetry).
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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

        let coords =
            assign_cross_axis(&plan, &graph, &ports, &|_| Size::new(20.0, 10.0), &main_of(&plan), 10.0)
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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

        let coords =
            assign_cross_axis(&plan, &graph, &ports, &|_| Size::new(20.0, 10.0), &main_of(&plan), 10.0)
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
            &[("e0", 0, 1), ("e1", 0, 2), ("e2", 0, 3), ("e3", 0, 4), ("e4", 2, 5)],
        );
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

        let coords = assign_cross_axis(&plan, &graph, &ports, &|_| Size::new(20.0, 10.0), &main_of(&plan), 10.0)
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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
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
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            &vec![Size::new(20.0, 10.0); graph.ids.len()],
            false,
        )
        .unwrap()
        .ports;

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
