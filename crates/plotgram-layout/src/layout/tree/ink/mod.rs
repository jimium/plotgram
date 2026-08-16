//! Ink: expand TreeRoute skeletons. Zero new decisions.

use plotgram_engine_api::EdgeGeometryMode;
use plotgram_model::geometry::Point;
use plotgram_model::graph::Graph;
use plotgram_model::port::{AlongSpec, PortRef, Side};
use plotgram_model::result::{EdgePath, EdgePlacement};

use super::metric::TreeMetric;
use super::plan::TreePlan;

pub fn expand(
    graph: &Graph,
    _plan: &TreePlan,
    metric: &TreeMetric,
    mode: EdgeGeometryMode,
) -> Vec<EdgePlacement> {
    let mut out = Vec::new();
    for edge in graph.edges_in_declaration_order() {
        let Some(src) = metric.frames.get(&edge.source) else {
            continue;
        };
        let Some(tgt) = metric.frames.get(&edge.target) else {
            continue;
        };
        let (path, from_port, to_port) = if let Some(route) = metric.routes.get(&edge.id) {
            let from = side_of(route.start(), src.center());
            let to = side_of(route.end(), tgt.center());
            let path = if mode == EdgeGeometryMode::DeferToRouter {
                EdgePath::polyline(Vec::new())
            } else {
                EdgePath::polyline(route.points())
            };
            (path, Some(port(from)), Some(port(to)))
        } else if mode == EdgeGeometryMode::DeferToRouter {
            (EdgePath::polyline(Vec::new()), None, None)
        } else {
            // Extra (non-tree) edges have no TreeRoute; straight between frames.
            let a = Point {
                x: src.center().x,
                y: src.bottom(),
            };
            let b = Point {
                x: tgt.center().x,
                y: tgt.y,
            };
            (
                EdgePath::polyline(vec![a, b]),
                Some(port(Side::South)),
                Some(port(Side::North)),
            )
        };
        out.push(EdgePlacement {
            id: edge.id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            path,
            from_port,
            to_port,
        });
    }
    out
}

fn port(side: Side) -> PortRef {
    PortRef {
        side,
        along: AlongSpec::Ordered { order: 0, count: 1 },
    }
}

fn side_of(p: Point, center: Point) -> Side {
    let dx = p.x - center.x;
    let dy = p.y - center.y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            Side::East
        } else {
            Side::West
        }
    } else if dy >= 0.0 {
        Side::South
    } else {
        Side::North
    }
}
