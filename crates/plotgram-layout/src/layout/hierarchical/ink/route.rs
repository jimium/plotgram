//! P5 Ink: pure expansion of Plan + Metric into orthogonal polylines. Reads
//! the dummy-chain waypoints and finalized ports; never invents a port side
//! or a new bend beyond the mechanical elbow needed to connect two points
//! that don't already share an axis (ink-and-verification.md §1, §4).

use std::collections::BTreeMap;

use plotgram_algo::path_ortho::{normalize_orthogonal, NormalizeOptions};
use plotgram_model::geometry::{Point, Rect};

use crate::layout::hierarchical::compose::ports::{EdgePorts, ResolvedPort};
use crate::layout::hierarchical::model::{PlanGraph, RealGraph, Segment};
use crate::layout::hierarchical::orient::{from_algo_point, to_algo_point};

/// One routed edge, still in canonical (TB) space.
pub struct CanonicalEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub path: Vec<Point>,
    pub from_port: ResolvedPort,
    pub to_port: ResolvedPort,
}

pub(crate) fn port_anchor(frame: Rect, port: ResolvedPort) -> Point {
    use plotgram_algo::orientation::Side::*;
    let t = (port.slot as f64 + 1.0) / (port.count as f64 + 1.0);
    match port.side {
        North => Point {
            x: frame.x + t * frame.width,
            y: frame.y,
        },
        South => Point {
            x: frame.x + t * frame.width,
            y: frame.bottom(),
        },
        West => Point {
            x: frame.x,
            y: frame.y + t * frame.height,
        },
        East => Point {
            x: frame.right(),
            y: frame.y + t * frame.height,
        },
    }
}

/// Elem indices along `edge_id`'s dummy chain in **original** source->target
/// order (properify stores segments in *working* direction; this un-swaps
/// them when the edge was reversed by FAS — composition.md's "FAS 方向不变量").
fn chain_in_original_order(plan: &PlanGraph, edge_id: &str, reversed: bool) -> Vec<usize> {
    let mut segs: Vec<&Segment> = plan
        .segments
        .iter()
        .filter(|s| s.edge_id == edge_id)
        .collect();
    segs.sort_by_key(|s| s.ordinal);
    let mut chain = Vec::with_capacity(segs.len() + 1);
    chain.push(segs[0].from);
    chain.extend(segs.iter().map(|s| s.to));
    if reversed {
        chain.reverse();
    }
    chain
}

/// Jog between `from` and `to` when they don't already share an axis, via
/// **two** bends at the vertical midpoint rather than one. A single bend can
/// only keep one endpoint's stub vertical (the other is then approached
/// horizontally); every real port here is North/South (main-axis, see
/// `ports.rs`), so both the departure *and* the arrival leg must be vertical
/// (ink-and-verification.md §4: "Port 出线方向由 side 决定"). Two bends at
/// the midpoint give a symmetric down-jog-down (or up-jog-up) that satisfies
/// both ends at once.
fn append_bend(path: &mut Vec<Point>, to: Point) {
    let from = *path.last().unwrap();
    if (from.x - to.x).abs() > 1e-9 && (from.y - to.y).abs() > 1e-9 {
        let mid_y = (from.y + to.y) / 2.0;
        path.push(Point {
            x: from.x,
            y: mid_y,
        });
        path.push(Point { x: to.x, y: mid_y });
    }
    path.push(to);
}

pub fn route_edges(
    graph: &RealGraph,
    plan: &PlanGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
) -> Vec<CanonicalEdge> {
    let mut out = Vec::with_capacity(graph.edges.len());
    for e in &graph.edges {
        let chain = chain_in_original_order(plan, &e.edge_id, e.reversed);
        let rp = &ports[&e.edge_id];

        let last_i = chain.len() - 1;
        let waypoints: Vec<Point> = chain
            .iter()
            .enumerate()
            .map(|(i, &elem)| {
                if i == 0 {
                    port_anchor(frames[elem], rp.source)
                } else if i == last_i {
                    port_anchor(frames[elem], rp.target)
                } else {
                    frames[elem].center()
                }
            })
            .collect();

        let mut path = vec![waypoints[0]];
        for &wp in &waypoints[1..] {
            append_bend(&mut path, wp);
        }

        let algo_pts: Vec<_> = path.iter().map(|&p| to_algo_point(p)).collect();
        let normalized = normalize_orthogonal(&algo_pts, &NormalizeOptions::default());
        let path: Vec<Point> = normalized.into_iter().map(from_algo_point).collect();

        out.push(CanonicalEdge {
            id: e.edge_id.clone(),
            source: graph.ids[e.original_source].clone(),
            target: graph.ids[e.original_target].clone(),
            path,
            from_port: rp.source,
            to_port: rp.target,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey, RealEdge};
    use plotgram_algo::orientation::Side;

    fn simple_setup() -> (RealGraph, PlanGraph, BTreeMap<String, EdgePorts>, Vec<Rect>) {
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let edges = vec![RealEdge {
            edge_id: "e0".into(),
            original_source: 0,
            original_target: 1,
            working_source: 0,
            working_target: 1,
            reversed: false,
            from_port: None,
            to_port: None,
        }];
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
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
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let segments = vec![Segment {
            edge_id: "e0".into(),
            ordinal: 0,
            from: 0,
            to: 1,
        }];
        let layers = vec![vec![0], vec![1]];
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1],
            segments,
            layers,
        };

        let mut ports = BTreeMap::new();
        ports.insert(
            "e0".to_string(),
            EdgePorts {
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
            },
        );

        let frames = vec![
            Rect::new(0.0, 0.0, 20.0, 10.0),
            Rect::new(30.0, 40.0, 20.0, 10.0),
        ];
        (graph, plan, ports, frames)
    }

    #[test]
    fn straight_when_ports_share_x() {
        let (graph, plan, ports, mut frames) = simple_setup();
        frames[1] = Rect::new(0.0, 40.0, 20.0, 10.0); // same center x as frames[0]
        let routed = route_edges(&graph, &plan, &ports, &frames);
        let e = &routed[0];
        assert_eq!(e.path.first().unwrap().x, e.path.last().unwrap().x);
    }

    #[test]
    fn elbow_inserted_when_axes_differ() {
        let (graph, plan, ports, frames) = simple_setup();
        let routed = route_edges(&graph, &plan, &ports, &frames);
        let e = &routed[0];
        assert!(
            e.path.len() >= 3,
            "expected an elbow bend, got {:?}",
            e.path
        );
        assert_eq!(e.path.first().copied().unwrap().y, 10.0); // south of a
        assert_eq!(e.path.last().copied().unwrap().y, 40.0); // north of b
    }

    #[test]
    fn path_endpoints_are_exact_port_anchors() {
        let (graph, plan, ports, frames) = simple_setup();
        let routed = route_edges(&graph, &plan, &ports, &frames);
        let e = &routed[0];
        assert_eq!(
            *e.path.first().unwrap(),
            port_anchor(frames[0], ports["e0"].source)
        );
        assert_eq!(
            *e.path.last().unwrap(),
            port_anchor(frames[1], ports["e0"].target)
        );
    }
}
