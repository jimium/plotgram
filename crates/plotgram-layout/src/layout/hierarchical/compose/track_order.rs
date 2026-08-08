//! D1.1 TrackOrder (L3): colour overlapping spans on each substrate track.
//!
//! End-bus BundlePlan members are skipped (intentional bus collinearity).
//! Cross-track lane counts feed LayerGap Demand via the track's rank-gap line.
//! Main corridors colour by **rank occupancy** (half-open `[min_rank, max_rank+1)`);
//! Cross corridors colour by cross-axis pixel span with **outer-first** nest
//! keys (`min(order, count-1-order)`) so fan-out horizontals nest symmetrically.

use std::collections::BTreeMap;

use plotgram_algo::interval_color::{color_intervals, Interval};
use plotgram_model::geometry::Rect;
use plotgram_model::port::AlongSpec;

use crate::layout::hierarchical::channel::{
    ChannelPath, ChannelRoutePlan, RouteTopology, TrackId, TrackOrient,
};
use crate::layout::hierarchical::compose::bundle::{end_bus_edge_ids, BundlePlan};
use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

/// L3 assignment for one edge on one substrate track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HopTrack {
    pub track: TrackId,
    pub track_index: u32,
}

/// TrackOrder plan (indices only — pixels are Metric).
#[derive(Debug, Default, Clone)]
pub struct TrackOrderPlan {
    /// `(edge_id, track_id)` → lane index on that substrate track.
    pub assignments: BTreeMap<(String, TrackId), HopTrack>,
    /// Substrate track → number of lanes.
    pub track_counts: BTreeMap<TrackId, usize>,
    /// RankGap(r) → max concurrent lanes on Cross line r+1 (for Demand).
    pub rank_gap_track_counts: BTreeMap<u32, usize>,
}

fn endpoint_cross(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    edge_id: &str,
    at_source: bool,
) -> f64 {
    let edge = graph
        .edges
        .iter()
        .find(|e| e.edge_id == edge_id)
        .expect("edge must exist");
    let node_idx = if at_source {
        edge.original_source
    } else {
        edge.original_target
    };
    let id = &graph.ids[node_idx];
    let elem = plan.index_of[&ElemKey::Real(id.clone())];
    let rp = &ports[edge_id];
    let port = if at_source { rp.source } else { rp.target };
    port_anchor(frames[elem], port).x
}

/// Assign L3 lanes from Channel paths + pixel spans.
pub fn assign_track_order(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    bundles: &[BundlePlan],
    route_plan: &ChannelRoutePlan,
) -> TrackOrderPlan {
    let bus_edges = end_bus_edge_ids(bundles);

    // Group edge spans by substrate track (Cross tracks only for L3 pitch
    // demand; Main tracks get lanes too for Ink vertical corridors).
    let mut by_track: BTreeMap<TrackId, Vec<(String, f64, f64)>> = BTreeMap::new();
    for (edge_id, topo) in &route_plan.routes {
        if bus_edges.contains(edge_id) {
            continue;
        }
        let RouteTopology::Orthogonal(ChannelPath { tracks, .. }) = topo;
        let ax = endpoint_cross(plan, graph, ports, frames, edge_id, true);
        let bx = endpoint_cross(plan, graph, ports, frames, edge_id, false);
        let lo = ax.min(bx);
        let hi = ax.max(bx);
        for &tid in tracks {
            let Some(t) = route_plan.substrate.track(tid) else {
                continue;
            };
            match t.orient {
                TrackOrient::Cross => {
                    // Horizontal corridor occupancy along the cross axis
                    // (including zero-width / straight vertical jogs).
                    by_track
                        .entry(tid)
                        .or_default()
                        .push((edge_id.clone(), lo, hi));
                }
                TrackOrient::Main => {
                    // Vertical corridor: conflict when rank bands overlap.
                    // Half-open [lo, hi+1) as closed [lo, hi+1] so shared
                    // endpoint ranks collide; abutting bands may share a lane.
                    let (lo_r, hi_r) = endpoint_ranks(plan, graph, edge_id);
                    by_track.entry(tid).or_default().push((
                        edge_id.clone(),
                        lo_r as f64,
                        (hi_r + 1) as f64,
                    ));
                }
            }
        }
    }

    let mut assignments = BTreeMap::new();
    let mut track_counts = BTreeMap::new();
    let mut rank_gap_track_counts: BTreeMap<u32, usize> = BTreeMap::new();

    for (tid, members) in &by_track {
        let is_cross = route_plan
            .substrate
            .track(*tid)
            .is_some_and(|t| matches!(t.orient, TrackOrient::Cross));
        let colors = if is_cross {
            color_cross_outer_first(members, ports)
        } else {
            let intervals: Vec<Interval> = members
                .iter()
                .map(|(_, lo, hi)| Interval::new(*lo, *hi))
                .collect();
            color_intervals(&intervals, 0.0)
        };
        let count = colors.iter().copied().max().map_or(0, |m| m + 1);
        track_counts.insert(*tid, count);
        for (i, (edge_id, _, _)) in members.iter().enumerate() {
            assignments.insert(
                (edge_id.clone(), *tid),
                HopTrack {
                    track: *tid,
                    track_index: colors[i] as u32,
                },
            );
        }
        if let Some(t) = route_plan.substrate.track(*tid) {
            if matches!(t.orient, TrackOrient::Cross) {
                // Interior Cross only: line k∈[1, rank_count) sits in RankGap(k-1).
                // Outer lines 0 / rank_count must not inflate LayerGap (D1.3.4).
                if t.line >= 1 && t.line < route_plan.index.rank_count {
                    let gap = (t.line - 1) as u32;
                    let e = rank_gap_track_counts.entry(gap).or_insert(0);
                    *e = (*e).max(count);
                }
            }
        }
    }

    TrackOrderPlan {
        assignments,
        track_counts,
        rank_gap_track_counts,
    }
}

fn endpoint_ranks(plan: &PlanGraph, graph: &RealGraph, edge_id: &str) -> (usize, usize) {
    let edge = graph
        .edges
        .iter()
        .find(|e| e.edge_id == edge_id)
        .expect("edge must exist");
    let src = plan.index_of[&ElemKey::Real(graph.ids[edge.original_source].clone())];
    let tgt = plan.index_of[&ElemKey::Real(graph.ids[edge.original_target].clone())];
    let sr = plan.elems[src].rank as usize;
    let tr = plan.elems[tgt].rank as usize;
    (sr.min(tr), sr.max(tr))
}

/// `(nest, order)` from the Ordered end with `count ≥ 2`; both ends fan →
/// smaller nest. Non-fan → `(0, 0)`.
fn fan_nest(ports: &BTreeMap<String, EdgePorts>, edge_id: &str) -> (u32, u32) {
    let Some(ep) = ports.get(edge_id) else {
        return (0, 0);
    };
    let pick = |along: AlongSpec| match along {
        AlongSpec::Ordered { order, count } if count >= 2 => {
            Some((order.min(count - 1 - order), order))
        }
        _ => None,
    };
    match (pick(ep.source.along), pick(ep.target.along)) {
        (Some(a), Some(b)) => {
            if a.0 < b.0 || (a.0 == b.0 && a.1 <= b.1) {
                a
            } else {
                b
            }
        }
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => (0, 0),
    }
}

/// Cross L3: outer-first first-fit (channel-d1 §5.1.4). Not left-edge greedy —
/// that flips right-half outer/inner nesting on fan-outs.
fn color_cross_outer_first(
    members: &[(String, f64, f64)],
    ports: &BTreeMap<String, EdgePorts>,
) -> Vec<usize> {
    let n = members.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let (na, oa) = fan_nest(ports, &members[a].0);
        let (nb, ob) = fan_nest(ports, &members[b].0);
        let wa = members[a].2 - members[a].1;
        let wb = members[b].2 - members[b].1;
        na.cmp(&nb)
            .then(oa.cmp(&ob))
            .then(wb.total_cmp(&wa))
            .then(members[a].0.cmp(&members[b].0))
    });

    // First-fit over full occupancy: colour order is not left-to-right.
    let mut tracks: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut result = vec![0usize; n];
    for &i in &order {
        let (lo, hi) = (members[i].1, members[i].2);
        let track = tracks
            .iter()
            .position(|ivs| ivs.iter().all(|&(a, b)| b <= lo || hi <= a))
            .unwrap_or_else(|| {
                tracks.push(Vec::new());
                tracks.len() - 1
            });
        tracks[track].push((lo, hi));
        result[i] = track;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::channel::{
        substrate::derive_root_substrate, ChannelPath, ChannelRoutePlan, RouteTopology, TrackOrient,
    };
    use crate::layout::hierarchical::compose::ports::{assign_ports, EdgePorts};
    use crate::layout::hierarchical::model::{Elem, ElemKey, RealEdge, RealGraph, Segment};
    use plotgram_algo::orientation::{Orientation as AlgoOrientation, Size};

    #[test]
    fn track_order_is_deterministic_and_assigns_lanes() {
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: vec![],
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("d".into()),
                group_path: vec![],
                rank: 1,
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
            decl_index: (0..4).collect(),
            segments: vec![
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
            ],
            layers: vec![vec![0, 1], vec![2, 3]],
        };
        let mut ids = BTreeMap::new();
        for (i, id) in ["a", "b", "c", "d"].iter().enumerate() {
            ids.insert((*id).into(), i);
        }
        let graph = RealGraph {
            ids: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            index_of: ids,
            group_path: vec![vec![]; 4],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 4],
            edges: vec![
                RealEdge {
                    edge_id: "e0".into(),
                    original_source: 0,
                    original_target: 2,
                    working_source: 0,
                    working_target: 2,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
                RealEdge {
                    edge_id: "e1".into(),
                    original_source: 1,
                    original_target: 3,
                    working_source: 1,
                    working_target: 3,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
            ],
            self_loops: vec![],
        };
        let (sub, idx) = derive_root_substrate(&plan);
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            false,
        )
        .unwrap()
        .ports;
        // Adjacent-layer: one Cross track between ranks.
        let cross = idx.cross_at(1, 0).expect("cross gap");
        let mut routes = BTreeMap::new();
        routes.insert(
            "e0".into(),
            RouteTopology::Orthogonal(ChannelPath {
                tracks: vec![cross],
                gates: vec![],
            }),
        );
        routes.insert(
            "e1".into(),
            RouteTopology::Orthogonal(ChannelPath {
                tracks: vec![cross],
                gates: vec![],
            }),
        );
        let route_plan = ChannelRoutePlan {
            substrate: sub,
            index: idx,
            routes,
            bundles: vec![],
            relaxations: vec![],
            ripup_rounds: 0,
            used_gates: false,
            route_order: vec![],
        };
        let frames = vec![
            Rect::new(0.0, 0.0, 20.0, 10.0),
            Rect::new(40.0, 0.0, 20.0, 10.0),
            Rect::new(0.0, 50.0, 20.0, 10.0),
            Rect::new(40.0, 50.0, 20.0, 10.0),
        ];
        let a = assign_track_order(&plan, &graph, &ports, &frames, &[], &route_plan);
        let b = assign_track_order(&plan, &graph, &ports, &frames, &[], &route_plan);
        assert_eq!(a.assignments, b.assignments);
        assert!(a.track_counts.get(&cross).copied().unwrap_or(0) >= 1);
        let _ = TrackOrient::Cross;
        let _: BTreeMap<String, EdgePorts> = ports;
    }

    /// Stacked same-Main reverses that share a rank must get distinct lanes
    /// (three-tier parallel returns); abutting rank bands may share.
    #[test]
    fn main_lanes_split_on_shared_rank_not_abutting() {
        // Ranks 0-1-2, single column. Two reverse edges on the outer Main:
        // e_low: 2→1, e_high: 1→0 — share rank 1 → 2 lanes.
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec![],
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: vec![],
                rank: 2,
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
            decl_index: (0..3).collect(),
            segments: vec![
                Segment {
                    edge_id: "e_low".into(),
                    ordinal: 0,
                    from: 2,
                    to: 1,
                },
                Segment {
                    edge_id: "e_high".into(),
                    ordinal: 0,
                    from: 1,
                    to: 0,
                },
            ],
            layers: vec![vec![0], vec![1], vec![2]],
        };
        let mut ids = BTreeMap::new();
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            ids.insert((*id).into(), i);
        }
        let graph = RealGraph {
            ids: vec!["a".into(), "b".into(), "c".into()],
            index_of: ids,
            group_path: vec![vec![]; 3],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 3],
            edges: vec![
                RealEdge {
                    edge_id: "e_low".into(),
                    original_source: 2,
                    original_target: 1,
                    working_source: 2,
                    working_target: 1,
                    reversed: true,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
                RealEdge {
                    edge_id: "e_high".into(),
                    original_source: 1,
                    original_target: 0,
                    working_source: 1,
                    working_target: 0,
                    reversed: true,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
            ],
            self_loops: vec![],
        };
        let (sub, idx) = derive_root_substrate(&plan);
        let ports = assign_ports(
            &graph,
            &plan,
            AlgoOrientation::Tb,
            false,
        )
        .unwrap()
        .ports;
        // Outer Main: order_gap == column count == 1 → line 1.
        let main_id = sub
            .tracks()
            .find(|t| t.orient == TrackOrient::Main && t.line == 1)
            .map(|t| t.id)
            .expect("outer Main");
        let mut routes = BTreeMap::new();
        for eid in ["e_low", "e_high"] {
            routes.insert(
                eid.into(),
                RouteTopology::Orthogonal(ChannelPath {
                    tracks: vec![main_id],
                    gates: vec![],
                }),
            );
        }
        let route_plan = ChannelRoutePlan {
            substrate: sub,
            index: idx,
            routes,
            bundles: vec![],
            relaxations: vec![],
            ripup_rounds: 0,
            used_gates: false,
            route_order: vec![],
        };
        let frames = vec![
            Rect::new(0.0, 0.0, 20.0, 10.0),
            Rect::new(0.0, 40.0, 20.0, 10.0),
            Rect::new(0.0, 80.0, 20.0, 10.0),
        ];
        let order = assign_track_order(&plan, &graph, &ports, &frames, &[], &route_plan);
        assert_eq!(
            order.track_counts.get(&main_id).copied(),
            Some(2),
            "shared rank must force 2 Main lanes"
        );
        let i0 = order.assignments[&("e_low".into(), main_id)].track_index;
        let i1 = order.assignments[&("e_high".into(), main_id)].track_index;
        assert_ne!(i0, i1);
    }

    /// Four-way fan-out: outer leaves share low track_index; left-edge greedy
    /// would put the right outer on the inner rail.
    #[test]
    fn cross_fan_out_nests_outer_first() {
        use crate::layout::hierarchical::compose::ports::ResolvedPort;
        use plotgram_algo::orientation::Side as AlgoSide;
        use plotgram_model::port::AlongSpec;

        // hub @ x∈[90,110]; leaves L→R: b,c,d,a with Ordered 0..3.
        // Spans: b[10,100], c[40,100], d[100,160], a[100,200].
        let elems = vec![
            Elem {
                key: ElemKey::Real("hub".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("svc_b".into()),
                group_path: vec![],
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("svc_c".into()),
                group_path: vec![],
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("svc_d".into()),
                group_path: vec![],
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("svc_a".into()),
                group_path: vec![],
                rank: 1,
            },
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let edge_ids = ["e_b", "e_c", "e_d", "e_a"];
        let targets = [1usize, 2, 3, 4];
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: (0..5).collect(),
            segments: edge_ids
                .iter()
                .zip(targets)
                .map(|(eid, to)| Segment {
                    edge_id: (*eid).into(),
                    ordinal: 0,
                    from: 0,
                    to,
                })
                .collect(),
            layers: vec![vec![0], vec![1, 2, 3, 4]],
        };
        let mut ids = BTreeMap::new();
        for (i, id) in ["hub", "svc_b", "svc_c", "svc_d", "svc_a"]
            .iter()
            .enumerate()
        {
            ids.insert((*id).into(), i);
        }
        let graph = RealGraph {
            ids: vec![
                "hub".into(),
                "svc_b".into(),
                "svc_c".into(),
                "svc_d".into(),
                "svc_a".into(),
            ],
            index_of: ids,
            group_path: vec![vec![]; 5],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 5],
            edges: edge_ids
                .iter()
                .zip(targets)
                .map(|(eid, tgt)| RealEdge {
                    edge_id: (*eid).into(),
                    original_source: 0,
                    original_target: tgt,
                    working_source: 0,
                    working_target: tgt,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                })
                .collect(),
            self_loops: vec![],
        };
        let mut ports = BTreeMap::new();
        for (i, eid) in edge_ids.iter().enumerate() {
            let order = i as u32;
            ports.insert(
                (*eid).into(),
                EdgePorts {
                    source: ResolvedPort {
                        side: AlgoSide::South,
                        along: AlongSpec::Ordered { order, count: 4 },
                    },
                    target: ResolvedPort {
                        side: AlgoSide::North,
                        along: AlongSpec::Ordered { order: 0, count: 1 },
                    },
                    source_cluster: None,
                    target_cluster: None,
                },
            );
        }
        let (sub, idx) = derive_root_substrate(&plan);
        let cross = idx.cross_at(1, 0).expect("cross gap");
        let mut routes = BTreeMap::new();
        for eid in edge_ids {
            routes.insert(
                eid.into(),
                RouteTopology::Orthogonal(ChannelPath {
                    tracks: vec![cross],
                    gates: vec![],
                }),
            );
        }
        let route_plan = ChannelRoutePlan {
            substrate: sub,
            index: idx,
            routes,
            bundles: vec![],
            relaxations: vec![],
            ripup_rounds: 0,
            used_gates: false,
            route_order: vec![],
        };
        // Frames: hub centre x=100; leaves at 10,40,160,200 centres.
        let frames = vec![
            Rect::new(90.0, 0.0, 20.0, 10.0),
            Rect::new(0.0, 50.0, 20.0, 10.0),
            Rect::new(30.0, 50.0, 20.0, 10.0),
            Rect::new(150.0, 50.0, 20.0, 10.0),
            Rect::new(190.0, 50.0, 20.0, 10.0),
        ];
        let order = assign_track_order(&plan, &graph, &ports, &frames, &[], &route_plan);
        let lane = |eid: &str| order.assignments[&(eid.into(), cross)].track_index;
        let lb = lane("e_b");
        let lc = lane("e_c");
        let ld = lane("e_d");
        let la = lane("e_a");
        assert_eq!(lb, la, "outer leaves must share a lane");
        assert_eq!(lc, ld, "inner leaves must share a lane");
        assert!(
            lb < lc,
            "outer nest must get lower track_index than inner (got outer={lb} inner={lc})"
        );
        assert_eq!(
            order.track_counts.get(&cross).copied(),
            Some(2),
            "four nested spans need exactly 2 lanes"
        );
    }
}
