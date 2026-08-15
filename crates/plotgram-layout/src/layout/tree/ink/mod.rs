//! Ink: expand tree / extra edges. Orthogonal 3-segment or straight.

use std::collections::{BTreeMap, BTreeSet};

use plotgram_engine_api::{EdgeGeometryMode, LayoutError};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::Graph;
use plotgram_model::port::{AlongSpec, PortRef, Side};
use plotgram_model::result::{EdgePath, EdgePlacement};

use super::params::{TreeParams, TreeRoutingStyle};
use super::plan::TreePlan;

pub fn expand(
    graph: &Graph,
    plan: &TreePlan,
    frames: &BTreeMap<String, Rect>,
    params: &TreeParams,
    mode: EdgeGeometryMode,
) -> Result<Vec<EdgePlacement>, LayoutError> {
    let tree_set: BTreeSet<&str> = plan.tree_edge_ids.iter().map(|s| s.as_str()).collect();
    let mut out = Vec::new();
    for edge in graph.edges_in_declaration_order() {
        let Some(src) = frames.get(&edge.source) else {
            continue;
        };
        let Some(tgt) = frames.get(&edge.target) else {
            continue;
        };
        let from_port = PortRef {
            side: Side::South,
            along: AlongSpec::Ordered { order: 0, count: 1 },
        };
        let to_port = PortRef {
            side: Side::North,
            along: AlongSpec::Ordered { order: 0, count: 1 },
        };
        let path = if mode == EdgeGeometryMode::DeferToRouter {
            EdgePath::polyline(Vec::new())
        } else if tree_set.contains(edge.id.as_str())
            && matches!(params.routing_style, TreeRoutingStyle::Orthogonal)
        {
            let a = Point {
                x: src.center().x,
                y: src.bottom(),
            };
            let b = Point {
                x: tgt.center().x,
                y: tgt.y,
            };
            let mid_y = (a.y + b.y) / 2.0;
            EdgePath::polyline(vec![
                a,
                Point { x: a.x, y: mid_y },
                Point { x: b.x, y: mid_y },
                b,
            ])
        } else {
            EdgePath::polyline(vec![
                Point {
                    x: src.center().x,
                    y: src.bottom(),
                },
                Point {
                    x: tgt.center().x,
                    y: tgt.y,
                },
            ])
        };
        out.push(EdgePlacement {
            id: edge.id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            path,
            from_port: Some(from_port),
            to_port: Some(to_port),
        });
    }
    Ok(out)
}
