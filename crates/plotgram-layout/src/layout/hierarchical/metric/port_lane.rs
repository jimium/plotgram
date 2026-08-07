//! PortLaneWriter — absolute cross-axis columns for twin N/S corridors.
//!
//! Contract: [`docs/design/layout/hierarchical/phases/port-lanes.md`]
//!
//! After node frames are known, twin short request/response edges on N/S
//! get their `AlongSpec` rewritten from `Ordered` to `LocalOffset` so both
//! ends share one world-space x (yFiles PortAlignmentIds; **not** grid).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::Side;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::AlongSpec;

use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

const PORT_MARGIN: f64 = 0.0;

/// Rewrite twin N/S corridor ports onto absolute lanes (`axis ± port_pitch`).
pub fn apply_port_lanes(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &mut BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    port_pitch: f64,
) {
    let twin_pairs = twin_undirected_pairs(graph);
    for &(a, b) in &twin_pairs {
        let Some(ea) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[a].clone()))
            .copied()
        else {
            continue;
        };
        let Some(eb) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[b].clone()))
            .copied()
        else {
            continue;
        };
        let span = (plan.elems[ea].rank as usize).abs_diff(plan.elems[eb].rank as usize);
        if span != 1 {
            continue;
        }

        let mut corridor: Vec<(String, u32)> = Vec::new();
        for e in &graph.edges {
            let pair = undirected_pair(e.original_source, e.original_target);
            if pair != (a, b) {
                continue;
            }
            let Some(ep) = ports.get(&e.edge_id) else {
                continue;
            };
            if !is_ns(ep.source.side) || !is_ns(ep.target.side) {
                continue;
            }
            let order = corridor_order(ep);
            corridor.push((e.edge_id.clone(), order));
        }
        if corridor.len() < 2 {
            continue;
        }
        corridor.sort_by(|x, y| x.1.cmp(&y.1).then(x.0.cmp(&y.0)));

        let axis = (center_x(&frames[ea]) + center_x(&frames[eb])) / 2.0;
        let left = axis - port_pitch;
        let right = axis + port_pitch;

        for (i, (edge_id, _)) in corridor.iter().enumerate() {
            let lane = if i == 0 { left } else { right };
            let edge = graph
                .edges
                .iter()
                .find(|ed| ed.edge_id == *edge_id)
                .expect("corridor edge");
            let src_elem =
                plan.index_of[&ElemKey::Real(graph.ids[edge.original_source].clone())];
            let tgt_elem =
                plan.index_of[&ElemKey::Real(graph.ids[edge.original_target].clone())];
            let Some(ep) = ports.get_mut(edge_id) else {
                continue;
            };
            ep.source.along = local_on_side(&frames[src_elem], ep.source.side, lane);
            ep.target.along = local_on_side(&frames[tgt_elem], ep.target.side, lane);
        }
    }
}

fn twin_undirected_pairs(graph: &RealGraph) -> BTreeSet<(usize, usize)> {
    let mut forward = BTreeSet::new();
    let mut reversed = BTreeSet::new();
    for e in &graph.edges {
        let p = undirected_pair(e.original_source, e.original_target);
        if e.reversed {
            reversed.insert(p);
        } else {
            forward.insert(p);
        }
    }
    forward.intersection(&reversed).copied().collect()
}

fn undirected_pair(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn is_ns(side: Side) -> bool {
    matches!(side, Side::North | Side::South)
}

fn corridor_order(ep: &EdgePorts) -> u32 {
    ordered_order(ep.source.along).min(ordered_order(ep.target.along))
}

fn ordered_order(along: AlongSpec) -> u32 {
    match along {
        AlongSpec::Ordered { order, .. } => order,
        AlongSpec::LocalOffset(_) => u32::MAX,
    }
}

fn center_x(frame: &Rect) -> f64 {
    frame.x + frame.width / 2.0
}

fn local_on_side(frame: &Rect, side: Side, lane_x: f64) -> AlongSpec {
    let x = lane_x.clamp(frame.x + PORT_MARGIN, frame.right() - PORT_MARGIN);
    let y = match side {
        Side::North => 0.0,
        Side::South => frame.height,
        Side::West | Side::East => frame.height / 2.0,
    };
    AlongSpec::LocalOffset(Point {
        x: x - frame.x,
        y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::ports::ResolvedPort;
    use crate::layout::hierarchical::metric::anchor::port_anchor;
    use crate::layout::hierarchical::model::{Elem, RealEdge};

    fn real(id: &str, rank: u32) -> Elem {
        Elem {
            key: ElemKey::Real(id.into()),
            group_path: Vec::new(),
            rank,
        }
    }

    fn graph_and_plan() -> (RealGraph, PlanGraph, BTreeMap<String, EdgePorts>, Vec<Rect>) {
        // client=0 (narrow), api=1 (wide); forward + reverse twin.
        let elems = vec![real("client", 0), real("api", 1)];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1],
            segments: vec![crate::layout::hierarchical::model::Segment {
                edge_id: "fwd".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            }],
            layers: vec![vec![0], vec![1]],
        };
        let graph = RealGraph {
            ids: vec!["client".into(), "api".into()],
            index_of: [("client".into(), 0usize), ("api".into(), 1)]
                .into_iter()
                .collect(),
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![
                RealEdge {
                    edge_id: "fwd".into(),
                    original_source: 0,
                    original_target: 1,
                    working_source: 0,
                    working_target: 1,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
                RealEdge {
                    edge_id: "rev".into(),
                    original_source: 1,
                    original_target: 0,
                    working_source: 0,
                    working_target: 1,
                    reversed: true,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
            ],
            self_loops: Vec::new(),
        };
        let mut ports = BTreeMap::new();
        ports.insert(
            "fwd".into(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 0, count: 2 },
                },
                target: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 0, count: 2 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );
        ports.insert(
            "rev".into(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 1, count: 2 },
                },
                target: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 1, count: 2 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );
        // Centers both at 50; widths 40 vs 80 → Ordered would diverge.
        let frames = vec![Rect::new(30.0, 0.0, 40.0, 20.0), Rect::new(10.0, 40.0, 80.0, 20.0)];
        (graph, plan, ports, frames)
    }

    #[test]
    fn twin_unequal_width_ports_share_lane_x() {
        let (graph, plan, mut ports, frames) = graph_and_plan();
        let pitch = 12.0;
        apply_port_lanes(&plan, &graph, &mut ports, &frames, pitch);

        let fwd = &ports["fwd"];
        let rev = &ports["rev"];
        let fwd_sx = port_anchor(frames[0], fwd.source).x;
        let fwd_tx = port_anchor(frames[1], fwd.target).x;
        let rev_sx = port_anchor(frames[1], rev.source).x;
        let rev_tx = port_anchor(frames[0], rev.target).x;

        assert!(
            (fwd_sx - fwd_tx).abs() < 1e-9,
            "forward ends must share x: {fwd_sx} vs {fwd_tx}"
        );
        assert!(
            (rev_sx - rev_tx).abs() < 1e-9,
            "reverse ends must share x: {rev_sx} vs {rev_tx}"
        );
        // Lanes straddle the shared center (50).
        assert!((fwd_sx - (50.0 - pitch)).abs() < 1e-9);
        assert!((rev_sx - (50.0 + pitch)).abs() < 1e-9);
    }
}
