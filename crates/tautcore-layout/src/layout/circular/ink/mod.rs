//! Ink: expand CircRoute skeletons. Zero new decisions.

use tautcore_engine_api::EdgeGeometryMode;
use tautcore_model::graph::Graph;
use tautcore_model::port::{AlongSpec, PortRef};
use tautcore_model::result::{EdgePath, EdgePlacement};

use super::geom::side_of;
use super::plan::{CircMetric, CircPlan};

pub fn expand(
    graph: &Graph,
    _plan: &CircPlan,
    metric: &CircMetric,
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
            let a = super::geom::rect_boundary_toward(src, tgt.center());
            let b = super::geom::rect_boundary_toward(tgt, src.center());
            (
                EdgePath::polyline(vec![a, b]),
                Some(port(super::geom::side_of(a, src.center()))),
                Some(port(super::geom::side_of(b, tgt.center()))),
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

fn port(side: tautcore_model::port::Side) -> PortRef {
    PortRef {
        side,
        along: AlongSpec::Ordered { order: 0, count: 1 },
    }
}
