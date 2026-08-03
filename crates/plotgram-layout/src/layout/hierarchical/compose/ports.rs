//! P7 port finalize: FREE ports inferred from rank direction (canonical
//! North/South only — this runs *inside* the TB-canonical core, see
//! `orient.rs`); FIXED side/slot honored as authored (any of the 4 physical
//! sides — converted into canonical TB space here via
//! `Orientation::to_tb_side`, mirroring the `from_tb_side` pass in `mod.rs`
//! on the way out; architecture.md §9.3).
//!
//! Slot order within a (node, side) group: opposite endpoint's
//! `(neighbor_rank, neighbor_order, EdgeId)` (ports-and-channel.md §2 step 4).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::{Orientation as AlgoOrientation, Side};

use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};
use crate::layout::hierarchical::orient::to_algo_side;

/// A finalized port, still in canonical (TB) space — `mod.rs`'s final
/// orientation pass converts `side` to the physical `plotgram_model::port::Side`.
/// `count` is the number of ports sharing this (node, side), needed by Ink to
/// place the pixel anchor along the side.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedPort {
    pub side: Side,
    pub slot: u32,
    pub count: u32,
}

pub struct EdgePorts {
    pub source: ResolvedPort,
    pub target: ResolvedPort,
}

fn positions_within_layer(plan: &PlanGraph) -> Vec<usize> {
    let mut pos = vec![0usize; plan.elems.len()];
    for layer in &plan.layers {
        for (i, &e) in layer.iter().enumerate() {
            pos[e] = i;
        }
    }
    pos
}

/// Find the elem adjacent to `real_idx` along `edge_id`'s chain (its only
/// segment neighbor — dummy or the other real endpoint).
fn immediate_neighbor(plan: &PlanGraph, edge_id: &str, real_idx: usize) -> usize {
    plan.segments
        .iter()
        .find(|s| s.edge_id == edge_id && (s.from == real_idx || s.to == real_idx))
        .map(|s| if s.from == real_idx { s.to } else { s.from })
        .expect("real endpoint must have exactly one adjacent segment for its own edge")
}

/// One end (source or target) of one edge, resolved to a canonical side
/// before slot assignment.
struct EndPoint {
    edge_id: String,
    is_source_end: bool,
    node_real: usize,
    side: Side,
    fixed_slot: Option<u32>,
    sort_key: (u32, usize, String), // (neighbor_rank, neighbor_order, edge_id)
}

pub fn assign_ports(
    graph: &RealGraph,
    plan: &PlanGraph,
    orientation: AlgoOrientation,
) -> BTreeMap<String, EdgePorts> {
    let pos = positions_within_layer(plan);
    let real_idx_of = |id: &str| plan.index_of[&ElemKey::Real(id.to_string())];

    let mut endpoints = Vec::with_capacity(graph.edges.len() * 2);
    for e in &graph.edges {
        for (is_source_end, node_id, constraint) in [
            (true, &graph.ids[e.original_source], e.from_port),
            (false, &graph.ids[e.original_target], e.to_port),
        ] {
            let node_real = real_idx_of(node_id);
            let neighbor = immediate_neighbor(plan, &e.edge_id, node_real);
            let own_rank = plan.elems[node_real].rank;
            let neighbor_rank = plan.elems[neighbor].rank;

            let (side, fixed_slot) = match constraint {
                // Authored sides are physical; the core is canonical TB.
                Some(c) => (orientation.to_tb_side(to_algo_side(c.side)), c.slot),
                None => {
                    let inferred = if neighbor_rank > own_rank {
                        Side::South
                    } else {
                        Side::North
                    };
                    (inferred, None)
                }
            };

            endpoints.push(EndPoint {
                edge_id: e.edge_id.clone(),
                is_source_end,
                node_real,
                side,
                fixed_slot,
                sort_key: (neighbor_rank, pos[neighbor], e.edge_id.clone()),
            });
        }
    }

    // Group by (node, side); assign slots within each group.
    let mut groups: BTreeMap<(usize, Side), Vec<usize>> = BTreeMap::new(); // -> indices into `endpoints`
    for (i, ep) in endpoints.iter().enumerate() {
        groups.entry((ep.node_real, ep.side)).or_default().push(i);
    }

    let mut slot_of: Vec<u32> = vec![0; endpoints.len()];
    let mut count_of: Vec<u32> = vec![0; endpoints.len()];
    for members in groups.into_values() {
        let taken: BTreeSet<u32> = members
            .iter()
            .filter_map(|&i| endpoints[i].fixed_slot)
            .collect();
        let mut order = members.clone();
        order.sort_by(|&a, &b| endpoints[a].sort_key.cmp(&endpoints[b].sort_key));

        let mut next_free = 0u32;
        let mut max_slot = 0u32;
        for &i in &order {
            let s = match endpoints[i].fixed_slot {
                Some(s) => s,
                None => {
                    while taken.contains(&next_free) {
                        next_free += 1;
                    }
                    let s = next_free;
                    next_free += 1;
                    s
                }
            };
            slot_of[i] = s;
            max_slot = max_slot.max(s);
        }
        let count = (max_slot + 1).max(order.len() as u32);
        for &i in &order {
            count_of[i] = count;
        }
    }

    let mut out: BTreeMap<String, EdgePorts> = BTreeMap::new();
    for (i, ep) in endpoints.iter().enumerate() {
        let port = ResolvedPort {
            side: ep.side,
            slot: slot_of[i],
            count: count_of[i],
        };
        let entry = out.entry(ep.edge_id.clone()).or_insert(EdgePorts {
            source: ResolvedPort {
                side: Side::South,
                slot: 0,
                count: 1,
            },
            target: ResolvedPort {
                side: Side::North,
                slot: 0,
                count: 1,
            },
        });
        if ep.is_source_end {
            entry.source = port;
        } else {
            entry.target = port;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, RealEdge, Segment};

    fn small_plan_and_graph() -> (RealGraph, PlanGraph) {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let edges = vec![
            RealEdge {
                edge_id: "e0".into(),
                original_source: 0,
                original_target: 1,
                working_source: 0,
                working_target: 1,
                reversed: false,
                from_port: None,
                to_port: None,
            },
            RealEdge {
                edge_id: "e1".into(),
                original_source: 0,
                original_target: 2,
                working_source: 0,
                working_target: 2,
                reversed: false,
                from_port: None,
                to_port: None,
            },
        ];
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 3],
            edges,
            self_loops: Vec::new(),
        };

        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 0,
                to: 2,
            },
        ];
        let layers = vec![vec![0], vec![1, 2]];
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1, 2],
            segments,
            layers,
        };
        (graph, plan)
    }

    #[test]
    fn free_ports_infer_south_for_downstream_north_for_upstream() {
        let (graph, plan) = small_plan_and_graph();
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb);
        assert_eq!(ports["e0"].source.side, Side::South);
        assert_eq!(ports["e0"].target.side, Side::North);
        assert_eq!(ports["e1"].source.side, Side::South);
        // a has two outgoing (South) edges to b (order0) and c (order1):
        // slots assigned by neighbor order.
        assert_eq!(ports["e0"].source.slot, 0);
        assert_eq!(ports["e1"].source.slot, 1);
    }

    #[test]
    fn fixed_constraint_is_honored_and_free_slots_avoid_it() {
        let (graph, mut plan) = small_plan_and_graph();
        let mut graph = graph;
        graph.edges[1].from_port = Some(plotgram_model::port::PortConstraint {
            side: plotgram_model::port::Side::South,
            slot: Some(0),
        });
        let _ = &mut plan;
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb);
        assert_eq!(ports["e1"].source.slot, 0);
        assert_eq!(
            ports["e0"].source.slot, 1,
            "free slot must skip the taken fixed slot 0"
        );
    }

    /// Regression: an authored (physical) fixed side must enter the canonical
    /// TB core through `to_tb_side` — with `Tb` the identity hid the bug.
    /// Table-driven over the four orientations (architecture.md §9.3).
    #[test]
    fn fixed_side_is_converted_into_canonical_space() {
        // Lr maps physical South → canonical East (point transform (x,y)→(y,x));
        // Bt flips North↔South; Rl ((x,y)→(y,-x)) maps South → East as well
        // but via the 4-cycle North→West→South→East; Tb is the identity.
        let cases = [
            (
                AlgoOrientation::Tb,
                plotgram_model::port::Side::South,
                Side::South,
            ),
            (
                AlgoOrientation::Lr,
                plotgram_model::port::Side::South,
                Side::East,
            ),
            (
                AlgoOrientation::Bt,
                plotgram_model::port::Side::South,
                Side::North,
            ),
            (
                AlgoOrientation::Rl,
                plotgram_model::port::Side::South,
                Side::East,
            ),
        ];
        for (orientation, authored, expected_canonical) in cases {
            let (mut graph, plan) = small_plan_and_graph();
            graph.edges[0].from_port = Some(plotgram_model::port::PortConstraint {
                side: authored,
                slot: None,
            });
            let ports = assign_ports(&graph, &plan, orientation);
            assert_eq!(
                ports["e0"].source.side,
                expected_canonical,
                "orientation {orientation:?}: authored {authored:?}"
            );
            // Round-trip through mod.rs's output transform must recover the
            // authored physical side.
            assert_eq!(
                orientation.from_tb_side(ports["e0"].source.side),
                to_algo_side(authored),
                "output transform must round-trip the authored side"
            );
        }
    }
}
